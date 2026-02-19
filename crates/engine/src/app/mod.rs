mod input;
mod loop_runner;
mod metrics;
mod rendering;
mod scene;

pub use input::InputAction;
pub use loop_runner::{run_app, run_app_with_metrics, AppError, LoopConfig, SLOW_FRAME_ENV_VAR};
pub use metrics::{LoopMetricsSnapshot, MetricsHandle};
pub use rendering::{world_to_screen, Renderer, Viewport};
pub use scene::{
    Camera2D, Entity, EntityId, InputSnapshot, RenderableDesc, RenderableKind, Scene, SceneCommand,
    SceneKey, SceneWorld, Transform, Vec2,
};
