mod input;
mod loop_runner;
mod metrics;
mod rendering;
mod scene;
mod tools;

pub use input::InputAction;
pub use loop_runner::{
    run_app, run_app_with_hooks, run_app_with_metrics, AppError, LoopConfig, LoopRuntimeHooks,
    RemoteConsoleLinePump, SLOW_FRAME_ENV_VAR,
};
pub use metrics::{LoopMetricsSnapshot, MetricsHandle};
pub use rendering::{
    screen_to_world_px, world_to_screen, world_to_screen_px, Renderer, Viewport, PIXELS_PER_WORLD,
    PLACEHOLDER_HALF_SIZE_PX,
};
pub use scene::{
    Camera2D, DebugInfoSnapshot, DebugJobState, DebugMarker, DebugMarkerKind, Entity, EntityId,
    InputSnapshot, Interactable, InteractableKind, OrderState, RenderableDesc, RenderableKind,
    Scene, SceneCommand, SceneDebugCommand, SceneDebugCommandResult, SceneDebugContext, SceneKey,
    SceneVisualState, SceneWorld, Tilemap, TilemapError, Transform, Vec2, CAMERA_ZOOM_DEFAULT,
    CAMERA_ZOOM_MAX, CAMERA_ZOOM_MIN, CAMERA_ZOOM_STEP,
};
pub(crate) use tools::{
    ConsoleCommandProcessor, ConsoleState, DebugCommand, OverlayData, PerfStats,
};
