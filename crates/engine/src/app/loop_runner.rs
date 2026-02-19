use std::env;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use tracing::{info, warn};
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use crate::{resolve_app_paths, StartupError};

use super::metrics::MetricsAccumulator;
use super::{MetricsHandle, Scene};

pub const SLOW_FRAME_ENV_VAR: &str = "PROTOGE_SLOW_FRAME_MS";

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub window_title: String,
    pub window_width: u32,
    pub window_height: u32,
    pub target_tps: u32,
    pub max_frame_delta: Duration,
    pub max_ticks_per_frame: u32,
    pub metrics_log_interval: Duration,
    pub simulated_slow_frame_ms: u64,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            window_title: "Proto GE".to_string(),
            window_width: 1280,
            window_height: 720,
            target_tps: 60,
            max_frame_delta: Duration::from_millis(250),
            max_ticks_per_frame: 5,
            metrics_log_interval: Duration::from_secs(1),
            simulated_slow_frame_ms: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Startup(#[from] StartupError),
    #[error("failed to create event loop: {0}")]
    CreateEventLoop(#[source] EventLoopError),
    #[error("failed to create application window: {0}")]
    CreateWindow(#[source] OsError),
    #[error("event loop failed: {0}")]
    EventLoopRun(#[source] EventLoopError),
}

pub fn run_app<S: Scene + 'static>(config: LoopConfig, scene: S) -> Result<(), AppError> {
    let metrics_handle = MetricsHandle::default();
    run_app_with_metrics(config, scene, metrics_handle)
}

pub fn run_app_with_metrics<S: Scene + 'static>(
    config: LoopConfig,
    mut scene: S,
    metrics_handle: MetricsHandle,
) -> Result<(), AppError> {
    let app_paths = resolve_app_paths()?;
    info!(
        root = %app_paths.root.display(),
        base_content_dir = %app_paths.base_content_dir.display(),
        mods_dir = %app_paths.mods_dir.display(),
        cache_dir = %app_paths.cache_dir.display(),
        "startup"
    );

    let event_loop = EventLoop::new().map_err(AppError::CreateEventLoop)?;
    let window = WindowBuilder::new()
        .with_title(config.window_title.clone())
        .with_inner_size(LogicalSize::new(
            config.window_width as f64,
            config.window_height as f64,
        ))
        .build(&event_loop)
        .map_err(AppError::CreateWindow)?;

    event_loop.set_control_flow(ControlFlow::Poll);

    let target_tps = config.target_tps.max(1);
    let max_frame_delta =
        normalize_non_zero_duration(config.max_frame_delta, Duration::from_millis(250));
    let max_ticks_per_frame = config.max_ticks_per_frame.max(1);
    let metrics_log_interval =
        normalize_non_zero_duration(config.metrics_log_interval, Duration::from_secs(1));
    let fixed_dt = Duration::from_secs_f64(1.0 / target_tps as f64);
    let fixed_dt_seconds = fixed_dt.as_secs_f32();
    let slow_frame_delay = resolve_slow_frame_delay(config.simulated_slow_frame_ms);

    info!(
        target_tps,
        max_frame_delta_ms = max_frame_delta.as_millis() as u64,
        max_ticks_per_frame,
        metrics_log_interval_ms = metrics_log_interval.as_millis() as u64,
        slow_frame_delay_ms = slow_frame_delay.as_millis() as u64,
        "loop_config"
    );

    let mut accumulator = Duration::ZERO;
    let mut last_frame_instant = Instant::now();
    let mut metrics_accumulator = MetricsAccumulator::new(metrics_log_interval);

    event_loop
        .run(move |event, window_target| match event {
            Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => {
                    info!(reason = "window_close", "shutdown_requested");
                    window_target.exit();
                }
                WindowEvent::KeyboardInput { event, .. } if is_escape_pressed(&event) => {
                    info!(reason = "escape_key", "shutdown_requested");
                    window_target.exit();
                }
                WindowEvent::RedrawRequested => {
                    if slow_frame_delay > Duration::ZERO {
                        thread::sleep(slow_frame_delay);
                    }

                    let now = Instant::now();
                    let raw_frame_dt = now.saturating_duration_since(last_frame_instant);
                    last_frame_instant = now;

                    let clamped_frame_dt = clamp_frame_delta(raw_frame_dt, max_frame_delta);
                    accumulator = accumulator.saturating_add(clamped_frame_dt);

                    let step_plan = plan_sim_steps(accumulator, fixed_dt, max_ticks_per_frame);
                    for _ in 0..step_plan.ticks_to_run {
                        scene.update(fixed_dt_seconds);
                        metrics_accumulator.record_tick();
                    }
                    accumulator = step_plan.remaining_accumulator;

                    if step_plan.dropped_backlog > Duration::ZERO {
                        warn!(
                            dropped_backlog_ms = step_plan.dropped_backlog.as_millis() as u64,
                            max_ticks_per_frame, "sim_clamp_triggered"
                        );
                    }

                    scene.render();
                    metrics_accumulator.record_frame(raw_frame_dt);

                    if let Some(snapshot) = metrics_accumulator.maybe_snapshot(now) {
                        metrics_handle.publish(snapshot);
                        info!(
                            fps = snapshot.fps,
                            tps = snapshot.tps,
                            frame_time_ms = snapshot.frame_time_ms,
                            "loop_metrics"
                        );
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                window.request_redraw();
            }
            Event::LoopExiting => {
                info!("shutdown");
            }
            _ => {}
        })
        .map_err(AppError::EventLoopRun)
}

#[derive(Debug, Clone, Copy)]
struct StepPlan {
    ticks_to_run: u32,
    remaining_accumulator: Duration,
    dropped_backlog: Duration,
}

fn plan_sim_steps(
    mut accumulator: Duration,
    fixed_dt: Duration,
    max_ticks_per_frame: u32,
) -> StepPlan {
    let mut ticks_to_run = 0u32;

    while accumulator >= fixed_dt && ticks_to_run < max_ticks_per_frame {
        accumulator = accumulator.saturating_sub(fixed_dt);
        ticks_to_run = ticks_to_run.saturating_add(1);
    }

    if accumulator >= fixed_dt {
        let dropped_backlog = accumulator;
        accumulator = Duration::ZERO;
        StepPlan {
            ticks_to_run,
            remaining_accumulator: accumulator,
            dropped_backlog,
        }
    } else {
        StepPlan {
            ticks_to_run,
            remaining_accumulator: accumulator,
            dropped_backlog: Duration::ZERO,
        }
    }
}

fn clamp_frame_delta(frame_dt: Duration, max_frame_delta: Duration) -> Duration {
    frame_dt.min(max_frame_delta)
}

fn normalize_non_zero_duration(value: Duration, fallback: Duration) -> Duration {
    if value.is_zero() {
        fallback
    } else {
        value
    }
}

fn resolve_slow_frame_delay(config_slow_frame_ms: u64) -> Duration {
    match env::var(SLOW_FRAME_ENV_VAR) {
        Ok(value) => match value.parse::<u64>() {
            Ok(ms) => Duration::from_millis(ms),
            Err(_) => {
                warn!(
                    env_var = SLOW_FRAME_ENV_VAR,
                    value = value.as_str(),
                    "invalid slow-frame env var value; falling back to config"
                );
                Duration::from_millis(config_slow_frame_ms)
            }
        },
        Err(env::VarError::NotPresent) => Duration::from_millis(config_slow_frame_ms),
        Err(err) => {
            warn!(
                env_var = SLOW_FRAME_ENV_VAR,
                error = %err,
                "unable to read slow-frame env var; falling back to config"
            );
            Duration::from_millis(config_slow_frame_ms)
        }
    }
}

fn is_escape_pressed(key_event: &winit::event::KeyEvent) -> bool {
    key_event.state == ElementState::Pressed
        && matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_frame_delta_caps_large_frame() {
        let max_frame_delta = Duration::from_millis(250);
        let raw_frame_dt = Duration::from_millis(600);

        assert_eq!(
            clamp_frame_delta(raw_frame_dt, max_frame_delta),
            max_frame_delta
        );
    }

    #[test]
    fn plan_sim_steps_runs_expected_ticks_without_drop() {
        let fixed_dt = Duration::from_millis(16);
        let result = plan_sim_steps(Duration::from_millis(48), fixed_dt, 5);

        assert_eq!(result.ticks_to_run, 3);
        assert_eq!(result.remaining_accumulator, Duration::ZERO);
        assert_eq!(result.dropped_backlog, Duration::ZERO);
    }

    #[test]
    fn plan_sim_steps_drops_backlog_when_tick_cap_hit() {
        let fixed_dt = Duration::from_millis(16);
        let result = plan_sim_steps(Duration::from_millis(120), fixed_dt, 3);

        assert_eq!(result.ticks_to_run, 3);
        assert_eq!(result.remaining_accumulator, Duration::ZERO);
        assert_eq!(result.dropped_backlog, Duration::from_millis(72));
    }
}
