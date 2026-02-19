use std::env;
use std::thread;
use std::time::{Duration, Instant};

use pixels::Error as PixelsError;
use thiserror::Error;
use tracing::{info, warn};
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use crate::{
    build_or_load_def_database, resolve_app_paths, ContentPipelineError, ContentPlanRequest,
    StartupError,
};

use super::metrics::MetricsAccumulator;
use super::scene::SceneMachine;
use super::{
    InputAction, InputSnapshot, MetricsHandle, OverlayData, Renderer, Scene, SceneCommand,
    SceneKey, SceneWorld,
};

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
    pub content_plan_request: ContentPlanRequest,
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
            content_plan_request: ContentPlanRequest::default(),
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
    #[error("failed to initialize renderer: {0}")]
    CreateRenderer(#[source] PixelsError),
    #[error("failed to build or load content database: {0}")]
    ContentPipeline(#[from] ContentPipelineError),
    #[error("event loop failed: {0}")]
    EventLoopRun(#[source] EventLoopError),
}

pub fn run_app(
    config: LoopConfig,
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
) -> Result<(), AppError> {
    let metrics_handle = MetricsHandle::default();
    run_app_with_metrics(config, scene_a, scene_b, metrics_handle)
}

pub fn run_app_with_metrics(
    config: LoopConfig,
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
    metrics_handle: MetricsHandle,
) -> Result<(), AppError> {
    let mut scenes = SceneMachine::new(scene_a, scene_b, SceneKey::A);
    let app_paths = resolve_app_paths()?;
    info!(
        root = %app_paths.root.display(),
        base_content_dir = %app_paths.base_content_dir.display(),
        mods_dir = %app_paths.mods_dir.display(),
        cache_dir = %app_paths.cache_dir.display(),
        "startup"
    );
    let def_database = build_or_load_def_database(&app_paths, &config.content_plan_request)?;

    let event_loop = EventLoop::new().map_err(AppError::CreateEventLoop)?;
    let window: &'static winit::window::Window = Box::leak(Box::new(
        WindowBuilder::new()
            .with_title(config.window_title.clone())
            .with_inner_size(LogicalSize::new(
                config.window_width as f64,
                config.window_height as f64,
            ))
            .build(&event_loop)
            .map_err(AppError::CreateWindow)?,
    ));
    let window_for_renderer = window;
    let window_for_loop = window;
    let mut renderer = Renderer::new(window_for_renderer).map_err(AppError::CreateRenderer)?;

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
    let mut input_collector = InputCollector::new(config.window_width, config.window_height);
    let mut world = SceneWorld::default();
    world.set_def_database(def_database);

    scenes.load_active(&mut world);
    world.apply_pending();
    info!(
        scene = ?scenes.active_scene(),
        entity_count = world.entity_count(),
        "scene_loaded"
    );

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
    let mut last_applied_title: Option<String> = None;
    let mut overlay_visible = true;

    event_loop
        .run(move |event, window_target| match event {
            Event::WindowEvent { window_id, event } if window_id == window_for_loop.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        input_collector.mark_quit_requested();
                        info!(reason = "window_close", "shutdown_requested");
                        window_target.exit();
                    }
                    WindowEvent::Resized(new_size) => {
                        input_collector.set_window_size(new_size.width, new_size.height);
                        if let Err(error) = renderer.resize(new_size.width, new_size.height) {
                            warn!(error = %error, "renderer_resize_failed");
                            window_target.exit();
                        }
                    }
                    WindowEvent::ScaleFactorChanged { .. } => {
                        let size = window_for_loop.inner_size();
                        input_collector.set_window_size(size.width, size.height);
                        if let Err(error) = renderer.resize(size.width, size.height) {
                            warn!(error = %error, "renderer_resize_failed");
                            window_target.exit();
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        input_collector
                            .set_cursor_position_px(position.x as f32, position.y as f32);
                    }
                    WindowEvent::CursorLeft { .. } => {
                        input_collector.clear_cursor_position();
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        input_collector.handle_mouse_input(button, state);
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        input_collector.handle_keyboard_input(&event);
                        if input_collector.quit_requested {
                            info!(reason = "escape_key", "shutdown_requested");
                            window_target.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if input_collector.take_overlay_toggle_pressed() {
                            overlay_visible = !overlay_visible;
                            info!(overlay_visible, "overlay_toggled");
                        }

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
                            let input_snapshot = input_collector.snapshot_for_tick();
                            let command =
                                scenes.update_active(fixed_dt_seconds, &input_snapshot, &mut world);
                            world.apply_pending();

                            if let SceneCommand::SwitchTo(next_scene) = command {
                                if scenes.switch_to(next_scene, &mut world) {
                                    world.apply_pending();
                                    info!(
                                        scene = ?scenes.active_scene(),
                                        entity_count = world.entity_count(),
                                        "scene_switched"
                                    );
                                }
                            }
                            metrics_accumulator.record_tick();
                        }
                        accumulator = step_plan.remaining_accumulator;

                        if step_plan.dropped_backlog > Duration::ZERO {
                            warn!(
                                dropped_backlog_ms = step_plan.dropped_backlog.as_millis() as u64,
                                max_ticks_per_frame, "sim_clamp_triggered"
                            );
                        }

                        scenes.render_active(&world);
                        let overlay = overlay_visible.then(|| OverlayData {
                            metrics: metrics_handle.snapshot(),
                            entity_count: world.entity_count(),
                            content_status: "loaded",
                            selected_entity: scenes.debug_selected_entity_active(),
                            selected_target: scenes.debug_selected_target_active(&world),
                        });
                        if let Err(error) = renderer.render_world(&world, overlay.as_ref()) {
                            warn!(error = %error, "renderer_draw_failed");
                            window_target.exit();
                        }
                        let next_title = scenes.debug_title_active(&world);
                        if next_title != last_applied_title {
                            if let Some(title) = &next_title {
                                window_for_loop.set_title(title);
                            } else {
                                window_for_loop.set_title(&config.window_title);
                            }
                            last_applied_title = next_title;
                        }
                        metrics_accumulator.record_frame(raw_frame_dt);

                        if let Some(snapshot) = metrics_accumulator.maybe_snapshot(now) {
                            metrics_handle.publish(snapshot);
                            info!(
                                fps = snapshot.fps,
                                tps = snapshot.tps,
                                frame_time_ms = snapshot.frame_time_ms,
                                entity_count = world.entity_count(),
                                scene = ?scenes.active_scene(),
                                "loop_metrics"
                            );
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                window_for_loop.request_redraw();
            }
            Event::LoopExiting => {
                scenes.unload_active(&mut world);
                world.clear();
                info!("shutdown");
            }
            _ => {}
        })
        .map_err(AppError::EventLoopRun)
}

#[derive(Debug, Default)]
struct InputCollector {
    quit_requested: bool,
    tab_is_down: bool,
    switch_scene_pressed_edge: bool,
    overlay_toggle_is_down: bool,
    overlay_toggle_pressed_edge: bool,
    action_states: super::input::ActionStates,
    cursor_position_px: Option<super::Vec2>,
    left_mouse_is_down: bool,
    left_click_pressed_edge: bool,
    right_mouse_is_down: bool,
    right_click_pressed_edge: bool,
    window_width: u32,
    window_height: u32,
}

impl InputCollector {
    fn new(window_width: u32, window_height: u32) -> Self {
        Self {
            window_width,
            window_height,
            ..Self::default()
        }
    }

    fn mark_quit_requested(&mut self) {
        self.quit_requested = true;
    }

    fn handle_keyboard_input(&mut self, key_event: &winit::event::KeyEvent) {
        self.update_action_state_from_key_event(key_event);
        self.handle_key_state(is_tab_key(key_event), key_event.state);
        self.handle_overlay_toggle_key_state(is_overlay_toggle_key(key_event), key_event.state);
    }

    fn handle_key_state(&mut self, is_tab: bool, state: ElementState) {
        if !is_tab {
            return;
        }

        match state {
            ElementState::Pressed => {
                if !self.tab_is_down {
                    self.switch_scene_pressed_edge = true;
                }
                self.tab_is_down = true;
            }
            ElementState::Released => self.tab_is_down = false,
        }
    }

    fn snapshot_for_tick(&mut self) -> InputSnapshot {
        let snapshot = InputSnapshot::new(
            self.quit_requested,
            self.switch_scene_pressed_edge,
            self.action_states,
            self.cursor_position_px,
            self.left_click_pressed_edge,
            self.right_click_pressed_edge,
            self.window_width,
            self.window_height,
        );
        self.switch_scene_pressed_edge = false;
        self.left_click_pressed_edge = false;
        self.right_click_pressed_edge = false;
        snapshot
    }

    fn take_overlay_toggle_pressed(&mut self) -> bool {
        let was_pressed = self.overlay_toggle_pressed_edge;
        self.overlay_toggle_pressed_edge = false;
        was_pressed
    }

    fn update_action_state_from_key_event(&mut self, key_event: &winit::event::KeyEvent) {
        let is_pressed = key_event.state == ElementState::Pressed;
        self.update_action_state_from_physical_key(key_event.physical_key, is_pressed);
    }

    fn handle_overlay_toggle_key_state(&mut self, is_toggle_key: bool, state: ElementState) {
        if !is_toggle_key {
            return;
        }

        match state {
            ElementState::Pressed => {
                if !self.overlay_toggle_is_down {
                    self.overlay_toggle_pressed_edge = true;
                }
                self.overlay_toggle_is_down = true;
            }
            ElementState::Released => self.overlay_toggle_is_down = false,
        }
    }

    fn update_action_state_from_physical_key(&mut self, key: PhysicalKey, is_pressed: bool) {
        match key {
            PhysicalKey::Code(KeyCode::KeyW) | PhysicalKey::Code(KeyCode::ArrowUp) => {
                self.action_states.set(InputAction::MoveUp, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyS) | PhysicalKey::Code(KeyCode::ArrowDown) => {
                self.action_states.set(InputAction::MoveDown, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyA) | PhysicalKey::Code(KeyCode::ArrowLeft) => {
                self.action_states.set(InputAction::MoveLeft, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyD) | PhysicalKey::Code(KeyCode::ArrowRight) => {
                self.action_states.set(InputAction::MoveRight, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyI) => {
                self.action_states.set(InputAction::CameraUp, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyK) => {
                self.action_states.set(InputAction::CameraDown, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyJ) => {
                self.action_states.set(InputAction::CameraLeft, is_pressed);
            }
            PhysicalKey::Code(KeyCode::KeyL) => {
                self.action_states.set(InputAction::CameraRight, is_pressed);
            }
            PhysicalKey::Code(KeyCode::F3) => {
                self.action_states
                    .set(InputAction::ToggleOverlay, is_pressed);
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                self.action_states.set(InputAction::Quit, is_pressed);
                if is_pressed {
                    self.mark_quit_requested();
                }
            }
            _ => {}
        }
    }

    fn set_window_size(&mut self, width: u32, height: u32) {
        self.window_width = width;
        self.window_height = height;
    }

    fn set_cursor_position_px(&mut self, x: f32, y: f32) {
        self.cursor_position_px = Some(super::Vec2 { x, y });
    }

    fn clear_cursor_position(&mut self) {
        self.cursor_position_px = None;
    }

    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState) {
        match button {
            MouseButton::Left => match state {
                ElementState::Pressed => {
                    if !self.left_mouse_is_down {
                        self.left_click_pressed_edge = true;
                    }
                    self.left_mouse_is_down = true;
                }
                ElementState::Released => self.left_mouse_is_down = false,
            },
            MouseButton::Right => match state {
                ElementState::Pressed => {
                    if !self.right_mouse_is_down {
                        self.right_click_pressed_edge = true;
                    }
                    self.right_mouse_is_down = true;
                }
                ElementState::Released => self.right_mouse_is_down = false,
            },
            _ => {}
        }
    }
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

fn is_tab_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab))
}

fn is_overlay_toggle_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::F3))
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

    #[test]
    fn tab_press_is_edge_triggered_for_single_tick() {
        let mut input = InputCollector::default();
        input.tab_is_down = false;
        input.switch_scene_pressed_edge = true;

        let first = input.snapshot_for_tick();
        let second = input.snapshot_for_tick();

        assert!(first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
    }

    #[test]
    fn no_retrigger_without_new_press() {
        let mut input = InputCollector::default();
        let first = input.snapshot_for_tick();
        let second = input.snapshot_for_tick();

        assert!(!first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
    }

    #[test]
    fn held_tab_does_not_spam_press_edges() {
        let mut input = InputCollector::default();

        input.handle_key_state(true, ElementState::Pressed);
        let first = input.snapshot_for_tick();

        input.handle_key_state(true, ElementState::Pressed);
        let second = input.snapshot_for_tick();

        input.handle_key_state(true, ElementState::Released);
        input.handle_key_state(true, ElementState::Pressed);
        let third = input.snapshot_for_tick();

        assert!(first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
        assert!(third.switch_scene_pressed());
    }

    #[test]
    fn wasd_and_arrow_keys_map_to_actions() {
        let mut input = InputCollector::default();

        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyW), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::ArrowLeft), true);

        let snapshot = input.snapshot_for_tick();
        assert!(snapshot.is_down(InputAction::MoveUp));
        assert!(snapshot.is_down(InputAction::MoveLeft));
    }

    #[test]
    fn key_release_clears_action_state() {
        let mut input = InputCollector::default();
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyD), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyD), false);

        let snapshot = input.snapshot_for_tick();
        assert!(!snapshot.is_down(InputAction::MoveRight));
    }

    #[test]
    fn camera_pan_keys_map_to_camera_actions() {
        let mut input = InputCollector::default();
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyI), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyL), true);
        let snapshot = input.snapshot_for_tick();
        assert!(snapshot.is_down(InputAction::CameraUp));
        assert!(snapshot.is_down(InputAction::CameraRight));
    }

    #[test]
    fn f3_toggle_is_edge_triggered() {
        let mut input = InputCollector::default();

        input.handle_overlay_toggle_key_state(true, ElementState::Pressed);
        assert!(input.take_overlay_toggle_pressed());

        input.handle_overlay_toggle_key_state(true, ElementState::Pressed);
        assert!(!input.take_overlay_toggle_pressed());

        input.handle_overlay_toggle_key_state(true, ElementState::Released);
        input.handle_overlay_toggle_key_state(true, ElementState::Pressed);
        assert!(input.take_overlay_toggle_pressed());
    }

    #[test]
    fn left_click_is_edge_triggered_for_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let first = input.snapshot_for_tick();
        let second = input.snapshot_for_tick();

        assert!(first.left_click_pressed());
        assert!(!second.left_click_pressed());
    }

    #[test]
    fn held_left_click_does_not_repeat_pressed_edge() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let first = input.snapshot_for_tick();
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let second = input.snapshot_for_tick();

        assert!(first.left_click_pressed());
        assert!(!second.left_click_pressed());
    }

    #[test]
    fn snapshot_carries_cursor_and_window_size() {
        let mut input = InputCollector::new(1280, 720);
        input.set_cursor_position_px(100.0, 200.0);
        let snapshot = input.snapshot_for_tick();

        assert_eq!(snapshot.window_size(), (1280, 720));
        let cursor = snapshot.cursor_position_px().expect("cursor");
        assert!((cursor.x - 100.0).abs() < 0.0001);
        assert!((cursor.y - 200.0).abs() < 0.0001);
    }

    #[test]
    fn right_click_is_edge_triggered_for_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let first = input.snapshot_for_tick();
        let second = input.snapshot_for_tick();

        assert!(first.right_click_pressed());
        assert!(!second.right_click_pressed());
    }

    #[test]
    fn held_right_click_does_not_repeat_pressed_edge() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let first = input.snapshot_for_tick();
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let second = input.snapshot_for_tick();

        assert!(first.right_click_pressed());
        assert!(!second.right_click_pressed());
    }
}
