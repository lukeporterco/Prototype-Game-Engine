mod input;
mod loop_runner;
mod metrics;
mod rendering;
mod scene;
mod tools;

pub use input::InputAction;
pub use loop_runner::{run_app, run_app_with_metrics, AppError, LoopConfig, SLOW_FRAME_ENV_VAR};
pub use metrics::{LoopMetricsSnapshot, MetricsHandle};
pub use rendering::{
    screen_to_world_px, world_to_screen, world_to_screen_px, Renderer, Viewport, PIXELS_PER_WORLD,
    PLACEHOLDER_HALF_SIZE_PX,
};
pub use scene::{
    Camera2D, Entity, EntityId, InputSnapshot, RenderableDesc, RenderableKind, Scene, SceneCommand,
    SceneKey, SceneWorld, Transform, Vec2,
};
pub(crate) use tools::OverlayData;
