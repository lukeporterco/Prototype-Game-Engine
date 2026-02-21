use std::collections::VecDeque;
use std::env;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use pixels::Error as PixelsError;
use thiserror::Error;
use tracing::{info, warn};
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use crate::{
    build_or_load_def_database, resolve_app_paths, ContentPipelineError, ContentPlanRequest,
    StartupError,
};

use super::metrics::MetricsAccumulator;
use super::scene::SceneMachine;
use super::tools::console_commands::{InjectedInputEvent, InjectedKey, InjectedMouseButton};
use super::{
    ConsoleCommandProcessor, ConsoleState, DebugCommand, InputAction, InputSnapshot, MetricsHandle,
    OverlayData, PerfStats, Renderer, Scene, SceneCommand, SceneDebugCommand,
    SceneDebugCommandResult, SceneDebugContext, SceneKey,
};

pub const SLOW_FRAME_ENV_VAR: &str = "PROTOGE_SLOW_FRAME_MS";
const SOFT_BUDGET_CONSECUTIVE_BREACH_FRAMES: u32 = 3;
const MAX_PENDING_INJECTED_EVENTS: usize = 256;

pub trait RemoteConsoleLinePump: Send {
    fn poll_lines(&mut self, out: &mut Vec<String>);

    fn send_output_lines(&mut self, _lines: &[String]) {}

    fn take_disconnect_reset_requested(&mut self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct LoopRuntimeHooks {
    pub remote_console_pump: Option<Box<dyn RemoteConsoleLinePump>>,
}

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
    pub fps_cap: Option<u32>,
    pub sim_budget_ms: Option<f32>,
    pub render_budget_ms: Option<f32>,
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
            fps_cap: None,
            sim_budget_ms: None,
            render_budget_ms: None,
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
    run_app_with_metrics_and_hooks(
        config,
        scene_a,
        scene_b,
        metrics_handle,
        LoopRuntimeHooks::default(),
    )
}

pub fn run_app_with_hooks(
    config: LoopConfig,
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
    hooks: LoopRuntimeHooks,
) -> Result<(), AppError> {
    let metrics_handle = MetricsHandle::default();
    run_app_with_metrics_and_hooks(config, scene_a, scene_b, metrics_handle, hooks)
}

pub fn run_app_with_metrics(
    config: LoopConfig,
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
    metrics_handle: MetricsHandle,
) -> Result<(), AppError> {
    run_app_with_metrics_and_hooks(
        config,
        scene_a,
        scene_b,
        metrics_handle,
        LoopRuntimeHooks::default(),
    )
}

fn run_app_with_metrics_and_hooks(
    config: LoopConfig,
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
    metrics_handle: MetricsHandle,
    mut runtime_hooks: LoopRuntimeHooks,
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
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(config.window_title.clone())
            .with_inner_size(LogicalSize::new(
                config.window_width as f64,
                config.window_height as f64,
            ))
            .build(&event_loop)
            .map_err(AppError::CreateWindow)?,
    );
    let window_for_renderer = Arc::clone(&window);
    let window_for_loop = Arc::clone(&window);
    let asset_root = app_paths.root.join("assets");
    let mut renderer =
        Renderer::new(window_for_renderer, asset_root).map_err(AppError::CreateRenderer)?;

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
    let effective_render_cap = normalize_render_fps_cap(config.fps_cap);
    let sim_budget_ms = normalize_soft_budget_ms(config.sim_budget_ms);
    let render_budget_ms = normalize_soft_budget_ms(config.render_budget_ms);
    let mut sim_budget_gate = sim_budget_ms.map(|threshold| {
        SoftBudgetWarningGate::new(threshold, SOFT_BUDGET_CONSECUTIVE_BREACH_FRAMES)
    });
    let mut render_budget_gate = render_budget_ms.map(|threshold| {
        SoftBudgetWarningGate::new(threshold, SOFT_BUDGET_CONSECUTIVE_BREACH_FRAMES)
    });
    let render_frame_target = target_frame_duration(effective_render_cap);
    let mut input_collector = InputCollector::new(config.window_width, config.window_height);
    scenes.set_def_database_for_all(def_database);
    scenes.load_active();
    scenes.apply_pending_active();
    info!(
        scene = ?scenes.active_scene(),
        entity_count = scenes.active_world().entity_count(),
        "scene_loaded"
    );

    info!(
        target_tps,
        max_frame_delta_ms = max_frame_delta.as_millis() as u64,
        max_ticks_per_frame,
        metrics_log_interval_ms = metrics_log_interval.as_millis() as u64,
        slow_frame_delay_ms = slow_frame_delay.as_millis() as u64,
        render_fps_cap = %format_render_cap(effective_render_cap),
        "loop_config"
    );

    let mut accumulator = Duration::ZERO;
    let mut last_frame_instant = Instant::now();
    let mut last_present_instant = Instant::now();
    let mut metrics_accumulator = MetricsAccumulator::new(metrics_log_interval);
    let mut perf_stats = PerfStats::new();
    let mut last_applied_title: Option<String> = None;
    let mut overlay_visible = true;
    let mut console = ConsoleState::default();
    let mut console_command_processor = ConsoleCommandProcessor::new();
    let mut drained_debug_commands = Vec::<DebugCommand>::new();
    let mut remote_console_lines = Vec::<String>::new();
    let mut remote_console_output_lines = Vec::<String>::new();
    info!(
        perf_stats_enabled_by_default = PerfStats::enabled_by_default(),
        perf_window_frames = PerfStats::window_len(),
        "perf_stats_config"
    );
    info!(
        sim_budget_ms,
        render_budget_ms,
        consecutive_breach_frames = SOFT_BUDGET_CONSECUTIVE_BREACH_FRAMES,
        "perf_budget_config"
    );

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
                        if !console.is_open() {
                            input_collector.handle_mouse_input(button, state);
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if !console.is_open() {
                            input_collector.handle_mouse_wheel(delta);
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        let route_to_console =
                            input_collector.handle_keyboard_input(&event, console.is_open());
                        if route_to_console {
                            console.handle_key_event(&event);
                            console.handle_text_input_from_key_event(&event);
                        }
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
                        if input_collector.take_console_toggle_pressed() {
                            console.toggle_open();
                            input_collector.reset_gameplay_inputs();
                            info!(console_open = console.is_open(), "console_toggled");
                        }
                        poll_remote_console_lines_into_console(
                            &mut runtime_hooks,
                            &mut console,
                            &mut remote_console_lines,
                        );
                        console_command_processor.process_pending_lines(&mut console);
                        drained_debug_commands.clear();
                        console_command_processor
                            .drain_pending_debug_commands_into(&mut drained_debug_commands);
                        if execute_drained_debug_commands(
                            &mut drained_debug_commands,
                            &mut scenes,
                            &mut console,
                            &mut input_collector,
                        ) {
                            info!(reason = "console_quit_command", "shutdown_requested");
                            window_target.exit();
                        }
                        forward_console_output_lines_to_remote(
                            &mut runtime_hooks,
                            &mut console,
                            &mut remote_console_output_lines,
                        );
                        mark_injected_reset_if_remote_disconnected(
                            &mut runtime_hooks,
                            &mut input_collector,
                        );

                        if slow_frame_delay > Duration::ZERO {
                            // Explicit debug perturbation only; this is not the FPS cap.
                            thread::sleep(slow_frame_delay);
                        }

                        let now = Instant::now();
                        let raw_frame_dt = now.saturating_duration_since(last_frame_instant);
                        last_frame_instant = now;

                        let clamped_frame_dt = clamp_frame_delta(raw_frame_dt, max_frame_delta);
                        accumulator = accumulator.saturating_add(clamped_frame_dt);

                        let step_plan = plan_sim_steps(accumulator, fixed_dt, max_ticks_per_frame);
                        // sim_ms boundary start:
                        // starts immediately before the fixed-step tick loop for this frame.
                        let sim_timer_start = Instant::now();
                        for _ in 0..step_plan.ticks_to_run {
                            let input_snapshot =
                                input_collector.snapshot_for_tick(console.is_open());
                            let command = scenes.update_active(fixed_dt_seconds, &input_snapshot);
                            scenes.apply_pending_active();

                            let switched = match command {
                                SceneCommand::SwitchTo(next_scene) => scenes.switch_to(next_scene),
                                SceneCommand::HardResetTo(next_scene) => {
                                    scenes.hard_reset_to(next_scene)
                                }
                                SceneCommand::None => false,
                            };
                            if switched {
                                scenes.apply_pending_active();
                                info!(
                                    scene = ?scenes.active_scene(),
                                    entity_count = scenes.active_world().entity_count(),
                                    "scene_switched"
                                );
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
                        // sim_ms boundary end:
                        // ends immediately after all tick work, scene switch handling,
                        // and sim backlog clamp handling are complete.
                        // Excludes cap sleep and all render work.
                        let sim_duration = sim_timer_start.elapsed();

                        // Single authoritative FPS cap sleep point for render pacing.
                        let elapsed_since_last_present =
                            Instant::now().saturating_duration_since(last_present_instant);
                        let cap_sleep =
                            compute_cap_sleep(elapsed_since_last_present, render_frame_target);
                        if cap_sleep > Duration::ZERO {
                            thread::sleep(cap_sleep);
                        }

                        let overlay = overlay_visible.then(|| OverlayData {
                            metrics: metrics_handle.snapshot(),
                            perf: perf_stats.snapshot(),
                            render_fps_cap: effective_render_cap,
                            slow_frame_delay_ms: slow_frame_delay.as_millis() as u64,
                            entity_count: scenes.active_world().entity_count(),
                            content_status: "loaded",
                            selected_entity: scenes.debug_selected_entity_active(),
                            selected_target: scenes.debug_selected_target_active(),
                            resource_count: scenes.debug_resource_count_active(),
                            debug_info: scenes.debug_info_snapshot_active(),
                        });

                        // render_ms boundary start:
                        // starts immediately before scene render preparation.
                        // Excludes cap sleep and non-render loop housekeeping.
                        let render_timer_start = Instant::now();
                        scenes.render_active();
                        let render_result = renderer.render_world(
                            scenes.active_world(),
                            overlay.as_ref(),
                            Some(&console),
                        );
                        // render_ms boundary end:
                        // ends immediately after renderer.render_world returns.
                        // Includes scenes.render_active + renderer.render_world only.
                        let render_duration = render_timer_start.elapsed();

                        perf_stats.record_frame(sim_duration, render_duration);
                        let perf_snapshot = perf_stats.snapshot();
                        maybe_warn_budget_breach("sim", &mut sim_budget_gate, perf_snapshot.sim);
                        maybe_warn_budget_breach(
                            "render",
                            &mut render_budget_gate,
                            perf_snapshot.ren,
                        );

                        if let Err(error) = render_result {
                            warn!(error = %error, "renderer_draw_failed");
                            window_target.exit();
                        }
                        last_present_instant = Instant::now();
                        let next_title = scenes.debug_title_active();
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
                                entity_count = scenes.active_world().entity_count(),
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
                scenes.shutdown_all();
                info!("shutdown");
            }
            _ => {}
        })
        .map_err(AppError::EventLoopRun)
}

#[derive(Debug, Default)]
struct InputCollector {
    quit_requested: bool,
    console_toggle_is_down: bool,
    console_toggle_pressed_edge: bool,
    tab_is_down: bool,
    switch_scene_pressed_edge: bool,
    overlay_toggle_is_down: bool,
    overlay_toggle_pressed_edge: bool,
    save_key_is_down: bool,
    save_pressed_edge: bool,
    load_key_is_down: bool,
    load_pressed_edge: bool,
    zoom_in_key_is_down: bool,
    zoom_out_key_is_down: bool,
    pending_zoom_steps: i32,
    action_states: super::input::ActionStates,
    cursor_position_px: Option<super::Vec2>,
    left_mouse_is_down: bool,
    left_click_pressed_edge: bool,
    right_mouse_is_down: bool,
    right_click_pressed_edge: bool,
    injected_pending_events: VecDeque<InjectedInputEvent>,
    injected_action_states: super::input::ActionStates,
    injected_left_mouse_is_down: bool,
    injected_left_click_pressed_edge: bool,
    injected_right_mouse_is_down: bool,
    injected_right_click_pressed_edge: bool,
    injected_disconnect_reset_pending: bool,
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

    fn handle_keyboard_input(
        &mut self,
        key_event: &winit::event::KeyEvent,
        console_open: bool,
    ) -> bool {
        self.handle_console_toggle_key_state(is_console_toggle_key(key_event), key_event.state);
        if console_open {
            return !is_console_toggle_key(key_event);
        }

        self.update_action_state_from_key_event(key_event);
        self.handle_key_state(is_tab_key(key_event), key_event.state);
        self.handle_overlay_toggle_key_state(is_overlay_toggle_key(key_event), key_event.state);
        self.handle_save_key_state(is_save_key(key_event), key_event.state);
        self.handle_load_key_state(is_load_key(key_event), key_event.state);
        self.handle_zoom_in_key_state(is_zoom_in_key(key_event), key_event.state);
        self.handle_zoom_out_key_state(is_zoom_out_key(key_event), key_event.state);
        false
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

    fn snapshot_for_tick(&mut self, console_open: bool) -> InputSnapshot {
        self.apply_injected_events_for_tick();
        let actions = self.merged_action_states();
        let left_click_pressed =
            self.left_click_pressed_edge || self.injected_left_click_pressed_edge;
        let right_click_pressed =
            self.right_click_pressed_edge || self.injected_right_click_pressed_edge;
        let snapshot = if console_open {
            InputSnapshot::new(
                false,
                false,
                super::input::ActionStates::default(),
                self.cursor_position_px,
                false,
                false,
                false,
                false,
                0,
                self.window_width,
                self.window_height,
            )
        } else {
            InputSnapshot::new(
                self.quit_requested,
                self.switch_scene_pressed_edge,
                actions,
                self.cursor_position_px,
                left_click_pressed,
                right_click_pressed,
                self.save_pressed_edge,
                self.load_pressed_edge,
                self.pending_zoom_steps,
                self.window_width,
                self.window_height,
            )
        };
        self.switch_scene_pressed_edge = false;
        self.left_click_pressed_edge = false;
        self.injected_left_click_pressed_edge = false;
        self.right_click_pressed_edge = false;
        self.injected_right_click_pressed_edge = false;
        self.save_pressed_edge = false;
        self.load_pressed_edge = false;
        self.pending_zoom_steps = 0;
        snapshot
    }

    fn take_overlay_toggle_pressed(&mut self) -> bool {
        let was_pressed = self.overlay_toggle_pressed_edge;
        self.overlay_toggle_pressed_edge = false;
        was_pressed
    }

    fn take_console_toggle_pressed(&mut self) -> bool {
        let was_pressed = self.console_toggle_pressed_edge;
        self.console_toggle_pressed_edge = false;
        was_pressed
    }

    fn reset_gameplay_inputs(&mut self) {
        self.action_states = super::input::ActionStates::default();
        self.injected_action_states = super::input::ActionStates::default();
        self.tab_is_down = false;
        self.switch_scene_pressed_edge = false;
        self.save_key_is_down = false;
        self.save_pressed_edge = false;
        self.load_key_is_down = false;
        self.load_pressed_edge = false;
        self.zoom_in_key_is_down = false;
        self.zoom_out_key_is_down = false;
        self.pending_zoom_steps = 0;
        self.left_mouse_is_down = false;
        self.left_click_pressed_edge = false;
        self.injected_left_mouse_is_down = false;
        self.injected_left_click_pressed_edge = false;
        self.right_mouse_is_down = false;
        self.right_click_pressed_edge = false;
        self.injected_right_mouse_is_down = false;
        self.injected_right_click_pressed_edge = false;
        self.injected_pending_events.clear();
        self.injected_disconnect_reset_pending = false;
        self.quit_requested = false;
    }

    fn enqueue_injected_event(&mut self, event: InjectedInputEvent) {
        if self.injected_pending_events.len() == MAX_PENDING_INJECTED_EVENTS {
            self.injected_pending_events.pop_front();
        }
        self.injected_pending_events.push_back(event);
    }

    fn mark_injected_disconnect_reset_pending(&mut self) {
        self.injected_disconnect_reset_pending = true;
    }

    fn clear_injected_held_inputs(&mut self) {
        self.injected_action_states = super::input::ActionStates::default();
        self.injected_left_mouse_is_down = false;
        self.injected_left_click_pressed_edge = false;
        self.injected_right_mouse_is_down = false;
        self.injected_right_click_pressed_edge = false;
    }

    fn apply_injected_events_for_tick(&mut self) {
        while let Some(event) = self.injected_pending_events.pop_front() {
            match event {
                InjectedInputEvent::KeyDown { key } => self.set_injected_key_state(key, true),
                InjectedInputEvent::KeyUp { key } => self.set_injected_key_state(key, false),
                InjectedInputEvent::MouseMove { x, y } => self.set_cursor_position_px(x, y),
                InjectedInputEvent::MouseDown { button } => {
                    self.handle_injected_mouse_input(button, ElementState::Pressed);
                }
                InjectedInputEvent::MouseUp { button } => {
                    self.handle_injected_mouse_input(button, ElementState::Released);
                }
            }
        }

        if self.injected_disconnect_reset_pending {
            self.clear_injected_held_inputs();
            self.injected_disconnect_reset_pending = false;
        }
    }

    fn merged_action_states(&self) -> super::input::ActionStates {
        let mut merged = self.action_states;
        const MERGEABLE_ACTIONS: [InputAction; 8] = [
            InputAction::MoveUp,
            InputAction::MoveDown,
            InputAction::MoveLeft,
            InputAction::MoveRight,
            InputAction::CameraUp,
            InputAction::CameraDown,
            InputAction::CameraLeft,
            InputAction::CameraRight,
        ];

        for action in MERGEABLE_ACTIONS {
            if self.injected_action_states.is_down(action) {
                merged.set(action, true);
            }
        }

        merged
    }

    fn set_injected_key_state(&mut self, key: InjectedKey, is_pressed: bool) {
        match key {
            InjectedKey::W | InjectedKey::Up => {
                self.injected_action_states
                    .set(InputAction::MoveUp, is_pressed);
            }
            InjectedKey::S | InjectedKey::Down => {
                self.injected_action_states
                    .set(InputAction::MoveDown, is_pressed);
            }
            InjectedKey::A | InjectedKey::Left => {
                self.injected_action_states
                    .set(InputAction::MoveLeft, is_pressed);
            }
            InjectedKey::D | InjectedKey::Right => {
                self.injected_action_states
                    .set(InputAction::MoveRight, is_pressed);
            }
            InjectedKey::I => {
                self.injected_action_states
                    .set(InputAction::CameraUp, is_pressed);
            }
            InjectedKey::K => {
                self.injected_action_states
                    .set(InputAction::CameraDown, is_pressed);
            }
            InjectedKey::J => {
                self.injected_action_states
                    .set(InputAction::CameraLeft, is_pressed);
            }
            InjectedKey::L => {
                self.injected_action_states
                    .set(InputAction::CameraRight, is_pressed);
            }
        }
    }

    fn handle_injected_mouse_input(&mut self, button: InjectedMouseButton, state: ElementState) {
        match button {
            InjectedMouseButton::Left => match state {
                ElementState::Pressed => {
                    if !self.injected_left_mouse_is_down {
                        self.injected_left_click_pressed_edge = true;
                    }
                    self.injected_left_mouse_is_down = true;
                }
                ElementState::Released => self.injected_left_mouse_is_down = false,
            },
            InjectedMouseButton::Right => match state {
                ElementState::Pressed => {
                    if !self.injected_right_mouse_is_down {
                        self.injected_right_click_pressed_edge = true;
                    }
                    self.injected_right_mouse_is_down = true;
                }
                ElementState::Released => self.injected_right_mouse_is_down = false,
            },
        }
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

    fn handle_console_toggle_key_state(&mut self, is_toggle_key: bool, state: ElementState) {
        if !is_toggle_key {
            return;
        }

        match state {
            ElementState::Pressed => {
                if !self.console_toggle_is_down {
                    self.console_toggle_pressed_edge = true;
                }
                self.console_toggle_is_down = true;
            }
            ElementState::Released => self.console_toggle_is_down = false,
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

    fn handle_save_key_state(&mut self, is_save_key: bool, state: ElementState) {
        if !is_save_key {
            return;
        }
        match state {
            ElementState::Pressed => {
                if !self.save_key_is_down {
                    self.save_pressed_edge = true;
                }
                self.save_key_is_down = true;
            }
            ElementState::Released => self.save_key_is_down = false,
        }
    }

    fn handle_load_key_state(&mut self, is_load_key: bool, state: ElementState) {
        if !is_load_key {
            return;
        }
        match state {
            ElementState::Pressed => {
                if !self.load_key_is_down {
                    self.load_pressed_edge = true;
                }
                self.load_key_is_down = true;
            }
            ElementState::Released => self.load_key_is_down = false,
        }
    }

    fn handle_zoom_in_key_state(&mut self, is_zoom_in_key: bool, state: ElementState) {
        if !is_zoom_in_key {
            return;
        }
        match state {
            ElementState::Pressed => {
                if !self.zoom_in_key_is_down {
                    self.pending_zoom_steps = self.pending_zoom_steps.saturating_add(1);
                }
                self.zoom_in_key_is_down = true;
            }
            ElementState::Released => self.zoom_in_key_is_down = false,
        }
    }

    fn handle_zoom_out_key_state(&mut self, is_zoom_out_key: bool, state: ElementState) {
        if !is_zoom_out_key {
            return;
        }
        match state {
            ElementState::Pressed => {
                if !self.zoom_out_key_is_down {
                    self.pending_zoom_steps = self.pending_zoom_steps.saturating_sub(1);
                }
                self.zoom_out_key_is_down = true;
            }
            ElementState::Released => self.zoom_out_key_is_down = false,
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

    fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let steps = zoom_steps_from_scroll_delta(delta);
        self.pending_zoom_steps = self.pending_zoom_steps.saturating_add(steps);
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

fn poll_remote_console_lines_into_console(
    hooks: &mut LoopRuntimeHooks,
    console: &mut ConsoleState,
    scratch: &mut Vec<String>,
) {
    let Some(pump) = hooks.remote_console_pump.as_mut() else {
        return;
    };

    scratch.clear();
    pump.poll_lines(scratch);
    for line in scratch.drain(..) {
        console.enqueue_pending_line(line);
    }
}

fn forward_console_output_lines_to_remote(
    hooks: &mut LoopRuntimeHooks,
    console: &mut ConsoleState,
    scratch: &mut Vec<String>,
) {
    let Some(pump) = hooks.remote_console_pump.as_mut() else {
        return;
    };

    scratch.clear();
    console.drain_new_output_lines_into(scratch);
    if !scratch.is_empty() {
        pump.send_output_lines(scratch);
    }
}

fn execute_drained_debug_commands(
    commands: &mut Vec<DebugCommand>,
    scenes: &mut SceneMachine,
    console: &mut ConsoleState,
    input_collector: &mut InputCollector,
) -> bool {
    let mut quit_requested = false;
    let mut should_apply_after_batch = false;

    for command in commands.drain(..) {
        match command {
            DebugCommand::Quit => {
                console.append_output_line("ok: quit requested");
                quit_requested = true;
            }
            DebugCommand::ResetScene => {
                let active = scenes.active_scene();
                let _ = scenes.hard_reset_to(active);
                scenes.apply_pending_active();
                console.append_output_line("ok: scene reset");
            }
            DebugCommand::SwitchScene { scene } => {
                if scenes.switch_to(scene) {
                    scenes.apply_pending_active();
                    console.append_output_line(format!(
                        "ok: switched to scene {}",
                        scene_key_token(scene)
                    ));
                } else {
                    console.append_output_line(format!(
                        "ok: scene {} already active",
                        scene_key_token(scene)
                    ));
                }
            }
            DebugCommand::Spawn { def_name, position } => {
                let context = SceneDebugContext {
                    cursor_world: cursor_world_from_input(scenes, input_collector),
                };
                let result = scenes.execute_debug_command_active(
                    SceneDebugCommand::Spawn { def_name, position },
                    context,
                );
                append_scene_debug_result(console, result);
                should_apply_after_batch = true;
            }
            DebugCommand::Despawn { entity_id } => {
                let context = SceneDebugContext {
                    cursor_world: cursor_world_from_input(scenes, input_collector),
                };
                let result = scenes.execute_debug_command_active(
                    SceneDebugCommand::Despawn { entity_id },
                    context,
                );
                append_scene_debug_result(console, result);
                should_apply_after_batch = true;
            }
            DebugCommand::InjectInput { event } => {
                input_collector.enqueue_injected_event(event);
                console.append_output_line(format!(
                    "ok: injected {}",
                    injected_event_debug_text(event)
                ));
            }
        }
    }

    if should_apply_after_batch {
        scenes.apply_pending_active();
    }

    quit_requested
}

fn mark_injected_reset_if_remote_disconnected(
    hooks: &mut LoopRuntimeHooks,
    input_collector: &mut InputCollector,
) {
    let Some(pump) = hooks.remote_console_pump.as_mut() else {
        return;
    };

    if pump.take_disconnect_reset_requested() {
        input_collector.mark_injected_disconnect_reset_pending();
    }
}

fn injected_event_debug_text(event: InjectedInputEvent) -> String {
    match event {
        InjectedInputEvent::KeyDown { key } => {
            format!("input.key_down {}", injected_key_token(key))
        }
        InjectedInputEvent::KeyUp { key } => {
            format!("input.key_up {}", injected_key_token(key))
        }
        InjectedInputEvent::MouseMove { x, y } => {
            format!("input.mouse_move {} {}", x, y)
        }
        InjectedInputEvent::MouseDown { button } => {
            format!("input.mouse_down {}", injected_mouse_button_token(button))
        }
        InjectedInputEvent::MouseUp { button } => {
            format!("input.mouse_up {}", injected_mouse_button_token(button))
        }
    }
}

fn injected_key_token(key: InjectedKey) -> &'static str {
    match key {
        InjectedKey::W => "w",
        InjectedKey::A => "a",
        InjectedKey::S => "s",
        InjectedKey::D => "d",
        InjectedKey::Up => "up",
        InjectedKey::Down => "down",
        InjectedKey::Left => "left",
        InjectedKey::Right => "right",
        InjectedKey::I => "i",
        InjectedKey::J => "j",
        InjectedKey::K => "k",
        InjectedKey::L => "l",
    }
}

fn injected_mouse_button_token(button: InjectedMouseButton) -> &'static str {
    match button {
        InjectedMouseButton::Left => "left",
        InjectedMouseButton::Right => "right",
    }
}

fn append_scene_debug_result(console: &mut ConsoleState, result: SceneDebugCommandResult) {
    match result {
        SceneDebugCommandResult::Unsupported => {
            console.append_output_line("error: active scene does not support this command");
        }
        SceneDebugCommandResult::Success(message) => {
            console.append_output_line(format!("ok: {message}"));
        }
        SceneDebugCommandResult::Error(message) => {
            console.append_output_line(format!("error: {message}"));
        }
    }
}

fn cursor_world_from_input(
    scenes: &SceneMachine,
    input_collector: &InputCollector,
) -> Option<super::Vec2> {
    let cursor_px = input_collector.cursor_position_px?;
    Some(super::screen_to_world_px(
        scenes.active_world().camera(),
        (input_collector.window_width, input_collector.window_height),
        cursor_px,
    ))
}

fn scene_key_token(scene: SceneKey) -> &'static str {
    match scene {
        SceneKey::A => "a",
        SceneKey::B => "b",
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

fn normalize_render_fps_cap(cap: Option<u32>) -> Option<u32> {
    cap.filter(|value| *value > 0)
}

fn normalize_soft_budget_ms(budget_ms: Option<f32>) -> Option<f32> {
    budget_ms.filter(|value| value.is_finite() && *value > 0.0)
}

fn target_frame_duration(fps_cap: Option<u32>) -> Option<Duration> {
    fps_cap.map(|fps| Duration::from_secs_f64(1.0 / fps as f64))
}

fn compute_cap_sleep(elapsed: Duration, target: Option<Duration>) -> Duration {
    match target {
        Some(frame_target) if elapsed < frame_target => frame_target - elapsed,
        _ => Duration::ZERO,
    }
}

fn format_render_cap(cap: Option<u32>) -> String {
    match cap {
        Some(value) => value.to_string(),
        None => "∞".to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SoftBudgetWarningGate {
    threshold_ms: f32,
    consecutive_required: u32,
    consecutive_breaches: u32,
    warned_in_current_streak: bool,
}

impl SoftBudgetWarningGate {
    fn new(threshold_ms: f32, consecutive_required: u32) -> Self {
        Self {
            threshold_ms,
            consecutive_required: consecutive_required.max(1),
            consecutive_breaches: 0,
            warned_in_current_streak: false,
        }
    }

    fn record_and_maybe_trigger(&mut self, last_ms: f32) -> Option<BudgetBreachEvent> {
        if last_ms > self.threshold_ms {
            self.consecutive_breaches = self.consecutive_breaches.saturating_add(1);
            if !self.warned_in_current_streak
                && self.consecutive_breaches >= self.consecutive_required
            {
                self.warned_in_current_streak = true;
                return Some(BudgetBreachEvent {
                    threshold_ms: self.threshold_ms,
                    consecutive_breaches: self.consecutive_breaches,
                    consecutive_required: self.consecutive_required,
                });
            }
            return None;
        }

        self.consecutive_breaches = 0;
        self.warned_in_current_streak = false;
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct BudgetBreachEvent {
    threshold_ms: f32,
    consecutive_breaches: u32,
    consecutive_required: u32,
}

fn maybe_warn_budget_breach(
    path: &'static str,
    gate: &mut Option<SoftBudgetWarningGate>,
    stats: super::tools::RollingMsStats,
) {
    let Some(active_gate) = gate.as_mut() else {
        return;
    };
    let Some(event) = active_gate.record_and_maybe_trigger(stats.last_ms) else {
        return;
    };

    warn!(
        path,
        threshold_ms = event.threshold_ms,
        consecutive_breaches = event.consecutive_breaches,
        consecutive_required = event.consecutive_required,
        last_ms = stats.last_ms,
        avg_ms = stats.avg_ms,
        max_ms = stats.max_ms,
        "perf_budget_exceeded"
    );
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

fn is_console_toggle_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(
        key_event.physical_key,
        PhysicalKey::Code(KeyCode::Backquote)
    )
}

fn is_save_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::F5))
}

fn is_load_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::F9))
}

fn is_zoom_in_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(
        key_event.physical_key,
        PhysicalKey::Code(KeyCode::Equal) | PhysicalKey::Code(KeyCode::NumpadAdd)
    )
}

fn is_zoom_out_key(key_event: &winit::event::KeyEvent) -> bool {
    matches!(
        key_event.physical_key,
        PhysicalKey::Code(KeyCode::Minus) | PhysicalKey::Code(KeyCode::NumpadSubtract)
    )
}

fn zoom_steps_from_scroll_delta(delta: MouseScrollDelta) -> i32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
        MouseScrollDelta::PixelDelta(position) => {
            if position.y > 0.0 {
                1
            } else if position.y < 0.0 {
                -1
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    struct NoopScene;

    impl Scene for NoopScene {
        fn load(&mut self, _world: &mut super::super::SceneWorld) {}

        fn update(
            &mut self,
            _fixed_dt_seconds: f32,
            _input: &InputSnapshot,
            _world: &mut super::super::SceneWorld,
        ) -> SceneCommand {
            SceneCommand::None
        }

        fn render(&mut self, _world: &super::super::SceneWorld) {}

        fn unload(&mut self, _world: &mut super::super::SceneWorld) {}
    }

    struct LoadQueuesOneEntityScene;

    impl Scene for LoadQueuesOneEntityScene {
        fn load(&mut self, world: &mut super::super::SceneWorld) {
            world.spawn(
                super::super::Transform::default(),
                super::super::RenderableDesc {
                    kind: super::super::RenderableKind::Placeholder,
                    debug_name: "queued_on_load",
                },
            );
        }

        fn update(
            &mut self,
            _fixed_dt_seconds: f32,
            _input: &InputSnapshot,
            _world: &mut super::super::SceneWorld,
        ) -> SceneCommand {
            SceneCommand::None
        }

        fn render(&mut self, _world: &super::super::SceneWorld) {}

        fn unload(&mut self, _world: &mut super::super::SceneWorld) {}
    }

    struct SceneWithDebugHook;

    impl Scene for SceneWithDebugHook {
        fn load(&mut self, _world: &mut super::super::SceneWorld) {}

        fn update(
            &mut self,
            _fixed_dt_seconds: f32,
            _input: &InputSnapshot,
            _world: &mut super::super::SceneWorld,
        ) -> SceneCommand {
            SceneCommand::None
        }

        fn render(&mut self, _world: &super::super::SceneWorld) {}

        fn unload(&mut self, _world: &mut super::super::SceneWorld) {}

        fn execute_debug_command(
            &mut self,
            command: SceneDebugCommand,
            _context: SceneDebugContext,
            world: &mut super::super::SceneWorld,
        ) -> SceneDebugCommandResult {
            match command {
                SceneDebugCommand::Spawn { .. } => {
                    world.spawn(
                        super::super::Transform::default(),
                        super::super::RenderableDesc {
                            kind: super::super::RenderableKind::Placeholder,
                            debug_name: "debug_spawn",
                        },
                    );
                    SceneDebugCommandResult::Success("spawned entity".to_string())
                }
                SceneDebugCommand::Despawn { entity_id } => {
                    if world.despawn(super::super::EntityId(entity_id)) {
                        SceneDebugCommandResult::Success("despawned entity".to_string())
                    } else {
                        SceneDebugCommandResult::Error("entity not found".to_string())
                    }
                }
            }
        }
    }

    struct SingleLinePump {
        emitted: bool,
    }

    impl RemoteConsoleLinePump for SingleLinePump {
        fn poll_lines(&mut self, out: &mut Vec<String>) {
            if !self.emitted {
                out.push("echo remote".to_string());
                self.emitted = true;
            }
        }
    }

    struct OutputCapturePump {
        captured: Arc<Mutex<Vec<String>>>,
    }

    impl RemoteConsoleLinePump for OutputCapturePump {
        fn poll_lines(&mut self, _out: &mut Vec<String>) {}

        fn send_output_lines(&mut self, lines: &[String]) {
            let mut captured = self.captured.lock().expect("lock");
            captured.extend(lines.iter().cloned());
        }
    }

    #[test]
    fn remote_pump_lines_are_enqueued_before_processing() {
        let mut hooks = LoopRuntimeHooks {
            remote_console_pump: Some(Box::new(SingleLinePump { emitted: false })),
        };
        let mut console = ConsoleState::default();
        let mut remote_lines = Vec::<String>::new();
        let mut processor = ConsoleCommandProcessor::new();

        poll_remote_console_lines_into_console(&mut hooks, &mut console, &mut remote_lines);
        processor.process_pending_lines(&mut console);

        assert_eq!(console.output_lines().collect::<Vec<_>>(), vec!["remote"]);
    }

    #[test]
    fn remote_output_lines_are_forwarded_after_console_append() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut hooks = LoopRuntimeHooks {
            remote_console_pump: Some(Box::new(OutputCapturePump {
                captured: Arc::clone(&captured),
            })),
        };
        let mut console = ConsoleState::default();
        let mut scratch = Vec::<String>::new();

        console.append_output_line("ok: one");
        console.append_output_line("error: two");
        forward_console_output_lines_to_remote(&mut hooks, &mut console, &mut scratch);

        let received = captured.lock().expect("lock");
        assert_eq!(
            *received,
            vec!["ok: one".to_string(), "error: two".to_string()]
        );
    }

    #[test]
    fn queueable_execution_emits_only_ok_or_error_lines() {
        let mut scenes = SceneMachine::new(
            Box::new(SceneWithDebugHook),
            Box::new(NoopScene),
            SceneKey::A,
        );
        scenes.load_active();
        scenes.apply_pending_active();

        let mut console = ConsoleState::default();
        let mut input_collector = InputCollector::new(1280, 720);
        let mut commands = vec![
            DebugCommand::Spawn {
                def_name: "proto.worker".to_string(),
                position: None,
            },
            DebugCommand::Despawn { entity_id: 999_999 },
        ];

        let quit = execute_drained_debug_commands(
            &mut commands,
            &mut scenes,
            &mut console,
            &mut input_collector,
        );

        assert!(!quit);
        let lines = console.output_lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert!(lines
            .iter()
            .all(|line| line.starts_with("ok:") || line.starts_with("error:")));
        assert!(lines.iter().all(|line| !line.starts_with("queued:")));
        assert_eq!(scenes.active_world().entity_count(), 1);
    }

    #[test]
    fn switch_scene_applies_immediately_after_active_scene_change() {
        let mut scenes = SceneMachine::new(
            Box::new(NoopScene),
            Box::new(LoadQueuesOneEntityScene),
            SceneKey::A,
        );
        scenes.load_active();
        scenes.apply_pending_active();

        let mut console = ConsoleState::default();
        let mut input_collector = InputCollector::new(1280, 720);
        let mut commands = vec![DebugCommand::SwitchScene { scene: SceneKey::B }];

        let _ = execute_drained_debug_commands(
            &mut commands,
            &mut scenes,
            &mut console,
            &mut input_collector,
        );

        assert_eq!(scenes.active_scene(), SceneKey::B);
        assert_eq!(scenes.active_world().entity_count(), 1);
        assert_eq!(
            console.output_lines().collect::<Vec<_>>(),
            vec!["ok: switched to scene b"]
        );
    }

    #[test]
    fn reset_scene_applies_immediately() {
        let mut scenes = SceneMachine::new(
            Box::new(LoadQueuesOneEntityScene),
            Box::new(NoopScene),
            SceneKey::A,
        );
        scenes.load_active();
        scenes.apply_pending_active();
        assert_eq!(scenes.active_world().entity_count(), 1);

        let mut console = ConsoleState::default();
        let mut input_collector = InputCollector::new(1280, 720);
        let mut commands = vec![DebugCommand::ResetScene];

        let _ = execute_drained_debug_commands(
            &mut commands,
            &mut scenes,
            &mut console,
            &mut input_collector,
        );

        assert_eq!(scenes.active_world().entity_count(), 1);
        assert_eq!(
            console.output_lines().collect::<Vec<_>>(),
            vec!["ok: scene reset"]
        );
    }

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

        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
    }

    #[test]
    fn no_retrigger_without_new_press() {
        let mut input = InputCollector::default();
        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(!first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
    }

    #[test]
    fn held_tab_does_not_spam_press_edges() {
        let mut input = InputCollector::default();

        input.handle_key_state(true, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);

        input.handle_key_state(true, ElementState::Pressed);
        let second = input.snapshot_for_tick(false);

        input.handle_key_state(true, ElementState::Released);
        input.handle_key_state(true, ElementState::Pressed);
        let third = input.snapshot_for_tick(false);

        assert!(first.switch_scene_pressed());
        assert!(!second.switch_scene_pressed());
        assert!(third.switch_scene_pressed());
    }

    #[test]
    fn wasd_and_arrow_keys_map_to_actions() {
        let mut input = InputCollector::default();

        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyW), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::ArrowLeft), true);

        let snapshot = input.snapshot_for_tick(false);
        assert!(snapshot.is_down(InputAction::MoveUp));
        assert!(snapshot.is_down(InputAction::MoveLeft));
    }

    #[test]
    fn key_release_clears_action_state() {
        let mut input = InputCollector::default();
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyD), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyD), false);

        let snapshot = input.snapshot_for_tick(false);
        assert!(!snapshot.is_down(InputAction::MoveRight));
    }

    #[test]
    fn camera_pan_keys_map_to_camera_actions() {
        let mut input = InputCollector::default();
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyI), true);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyL), true);
        let snapshot = input.snapshot_for_tick(false);
        assert!(snapshot.is_down(InputAction::CameraUp));
        assert!(snapshot.is_down(InputAction::CameraRight));
    }

    #[test]
    fn injected_event_queue_drains_into_snapshot() {
        let mut input = InputCollector::new(1280, 720);
        input.enqueue_injected_event(InjectedInputEvent::KeyDown {
            key: InjectedKey::W,
        });
        input.enqueue_injected_event(InjectedInputEvent::MouseMove { x: 12.0, y: 34.0 });
        input.enqueue_injected_event(InjectedInputEvent::MouseDown {
            button: InjectedMouseButton::Left,
        });

        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.is_down(InputAction::MoveUp));
        assert!(first.left_click_pressed());
        let cursor = first.cursor_position_px().expect("cursor");
        assert!((cursor.x - 12.0).abs() < 0.0001);
        assert!((cursor.y - 34.0).abs() < 0.0001);
        assert!(second.is_down(InputAction::MoveUp));
        assert!(!second.left_click_pressed());

        input.enqueue_injected_event(InjectedInputEvent::KeyUp {
            key: InjectedKey::W,
        });
        let third = input.snapshot_for_tick(false);
        assert!(!third.is_down(InputAction::MoveUp));
    }

    #[test]
    fn injected_disconnect_reset_clears_held_inputs() {
        let mut input = InputCollector::new(1280, 720);
        input.enqueue_injected_event(InjectedInputEvent::KeyDown {
            key: InjectedKey::D,
        });
        input.enqueue_injected_event(InjectedInputEvent::MouseDown {
            button: InjectedMouseButton::Right,
        });

        let first = input.snapshot_for_tick(false);
        assert!(first.is_down(InputAction::MoveRight));
        assert!(first.right_click_pressed());

        input.mark_injected_disconnect_reset_pending();
        let second = input.snapshot_for_tick(false);
        assert!(!second.is_down(InputAction::MoveRight));
        assert!(!second.right_click_pressed());
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
    fn backquote_console_toggle_is_edge_triggered() {
        let mut input = InputCollector::default();

        input.handle_console_toggle_key_state(true, ElementState::Pressed);
        assert!(input.take_console_toggle_pressed());

        input.handle_console_toggle_key_state(true, ElementState::Pressed);
        assert!(!input.take_console_toggle_pressed());

        input.handle_console_toggle_key_state(true, ElementState::Released);
        input.handle_console_toggle_key_state(true, ElementState::Pressed);
        assert!(input.take_console_toggle_pressed());
    }

    #[test]
    fn left_click_is_edge_triggered_for_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.left_click_pressed());
        assert!(!second.left_click_pressed());
    }

    #[test]
    fn held_left_click_does_not_repeat_pressed_edge() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        let second = input.snapshot_for_tick(false);

        assert!(first.left_click_pressed());
        assert!(!second.left_click_pressed());
    }

    #[test]
    fn snapshot_carries_cursor_and_window_size() {
        let mut input = InputCollector::new(1280, 720);
        input.set_cursor_position_px(100.0, 200.0);
        let snapshot = input.snapshot_for_tick(false);

        assert_eq!(snapshot.window_size(), (1280, 720));
        let cursor = snapshot.cursor_position_px().expect("cursor");
        assert!((cursor.x - 100.0).abs() < 0.0001);
        assert!((cursor.y - 200.0).abs() < 0.0001);
    }

    #[test]
    fn console_open_suppresses_gameplay_snapshot_inputs() {
        let mut input = InputCollector::new(1280, 720);
        input.update_action_state_from_physical_key(PhysicalKey::Code(KeyCode::KeyW), true);
        input.handle_key_state(true, ElementState::Pressed);
        input.handle_mouse_input(MouseButton::Left, ElementState::Pressed);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        input.handle_save_key_state(true, ElementState::Pressed);
        input.handle_load_key_state(true, ElementState::Pressed);
        input.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 2.0));

        let snapshot = input.snapshot_for_tick(true);
        assert!(!snapshot.is_down(InputAction::MoveUp));
        assert!(!snapshot.switch_scene_pressed());
        assert!(!snapshot.left_click_pressed());
        assert!(!snapshot.right_click_pressed());
        assert!(!snapshot.save_pressed());
        assert!(!snapshot.load_pressed());
        assert_eq!(snapshot.zoom_delta_steps(), 0);
        assert_eq!(snapshot.window_size(), (1280, 720));
    }

    #[test]
    fn right_click_is_edge_triggered_for_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.right_click_pressed());
        assert!(!second.right_click_pressed());
    }

    #[test]
    fn held_right_click_does_not_repeat_pressed_edge() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        input.handle_mouse_input(MouseButton::Right, ElementState::Pressed);
        let second = input.snapshot_for_tick(false);

        assert!(first.right_click_pressed());
        assert!(!second.right_click_pressed());
    }

    #[test]
    fn save_key_edge_is_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_save_key_state(true, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.save_pressed());
        assert!(!second.save_pressed());
    }

    #[test]
    fn load_key_edge_is_single_tick() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_load_key_state(true, ElementState::Pressed);
        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert!(first.load_pressed());
        assert!(!second.load_pressed());
    }

    #[test]
    fn held_save_load_do_not_retrigger_without_release() {
        let mut input = InputCollector::new(1280, 720);

        input.handle_save_key_state(true, ElementState::Pressed);
        assert!(input.snapshot_for_tick(false).save_pressed());
        input.handle_save_key_state(true, ElementState::Pressed);
        assert!(!input.snapshot_for_tick(false).save_pressed());
        input.handle_save_key_state(true, ElementState::Released);
        input.handle_save_key_state(true, ElementState::Pressed);
        assert!(input.snapshot_for_tick(false).save_pressed());

        input.handle_load_key_state(true, ElementState::Pressed);
        assert!(input.snapshot_for_tick(false).load_pressed());
        input.handle_load_key_state(true, ElementState::Pressed);
        assert!(!input.snapshot_for_tick(false).load_pressed());
        input.handle_load_key_state(true, ElementState::Released);
        input.handle_load_key_state(true, ElementState::Pressed);
        assert!(input.snapshot_for_tick(false).load_pressed());
    }

    #[test]
    fn zoom_keys_are_edge_triggered_only() {
        let mut input = InputCollector::new(1280, 720);

        input.handle_zoom_in_key_state(true, ElementState::Pressed);
        assert_eq!(input.snapshot_for_tick(false).zoom_delta_steps(), 1);

        input.handle_zoom_in_key_state(true, ElementState::Pressed);
        assert_eq!(input.snapshot_for_tick(false).zoom_delta_steps(), 0);

        input.handle_zoom_in_key_state(true, ElementState::Released);
        input.handle_zoom_in_key_state(true, ElementState::Pressed);
        assert_eq!(input.snapshot_for_tick(false).zoom_delta_steps(), 1);

        input.handle_zoom_out_key_state(true, ElementState::Pressed);
        assert_eq!(input.snapshot_for_tick(false).zoom_delta_steps(), -1);
    }

    #[test]
    fn mouse_wheel_adds_zoom_steps_and_snapshot_resets_pending() {
        let mut input = InputCollector::new(1280, 720);
        input.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
        input.handle_mouse_wheel(MouseScrollDelta::LineDelta(0.0, -2.0));

        let first = input.snapshot_for_tick(false);
        let second = input.snapshot_for_tick(false);

        assert_eq!(first.zoom_delta_steps(), -1);
        assert_eq!(second.zoom_delta_steps(), 0);
    }

    #[test]
    fn pixel_wheel_delta_maps_to_single_discrete_step_direction() {
        let positive = zoom_steps_from_scroll_delta(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 3.0),
        ));
        let negative = zoom_steps_from_scroll_delta(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -5.0),
        ));
        let none = zoom_steps_from_scroll_delta(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 0.0),
        ));

        assert_eq!(positive, 1);
        assert_eq!(negative, -1);
        assert_eq!(none, 0);
    }

    #[test]
    fn target_frame_duration_none_when_cap_off() {
        assert_eq!(target_frame_duration(None), None);
    }

    #[test]
    fn target_frame_duration_for_60hz_is_expected() {
        let duration = target_frame_duration(Some(60)).expect("duration");
        assert!((duration.as_secs_f64() - (1.0 / 60.0)).abs() < 0.000_001);
    }

    #[test]
    fn compute_cap_sleep_zero_when_over_budget() {
        let sleep = compute_cap_sleep(Duration::from_millis(20), target_frame_duration(Some(60)));
        assert_eq!(sleep, Duration::ZERO);
    }

    #[test]
    fn compute_cap_sleep_positive_when_under_budget() {
        let sleep = compute_cap_sleep(Duration::from_millis(5), target_frame_duration(Some(60)));
        assert!(sleep > Duration::ZERO);
    }

    #[test]
    fn normalize_render_fps_cap_disables_zero() {
        assert_eq!(normalize_render_fps_cap(Some(0)), None);
        assert_eq!(normalize_render_fps_cap(Some(60)), Some(60));
    }

    #[test]
    fn format_render_cap_uses_infinity_when_uncapped() {
        assert_eq!(format_render_cap(None), "∞");
        assert_eq!(format_render_cap(Some(60)), "60");
    }

    #[test]
    fn no_trigger_before_k_consecutive_breaches() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.5).is_none());
    }

    #[test]
    fn triggers_on_exact_kth_breach() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        let event = gate.record_and_maybe_trigger(6.0).expect("trigger event");
        assert_eq!(event.consecutive_breaches, 3);
        assert_eq!(event.consecutive_required, 3);
    }

    #[test]
    fn single_spike_then_recovery_does_not_trigger() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(4.9).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
    }

    #[test]
    fn warning_latches_within_same_streak() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_some());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
    }

    #[test]
    fn recovery_resets_latch_and_allows_future_warning() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_some());

        assert!(gate.record_and_maybe_trigger(4.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_none());
        assert!(gate.record_and_maybe_trigger(6.0).is_some());
    }

    #[test]
    fn at_or_below_threshold_not_counted_as_breach() {
        let mut gate = SoftBudgetWarningGate::new(5.0, 3);

        assert!(gate.record_and_maybe_trigger(5.0).is_none());
        assert!(gate.record_and_maybe_trigger(5.0).is_none());
        assert!(gate.record_and_maybe_trigger(5.1).is_none());
        assert!(gate.record_and_maybe_trigger(5.1).is_none());
    }

    #[test]
    fn normalize_soft_budget_disables_none_and_non_positive() {
        assert_eq!(normalize_soft_budget_ms(None), None);
        assert_eq!(normalize_soft_budget_ms(Some(0.0)), None);
        assert_eq!(normalize_soft_budget_ms(Some(-1.0)), None);
        assert_eq!(normalize_soft_budget_ms(Some(f32::NAN)), None);
        assert_eq!(normalize_soft_budget_ms(Some(f32::INFINITY)), None);
        assert_eq!(normalize_soft_budget_ms(Some(4.0)), Some(4.0));
    }
}
