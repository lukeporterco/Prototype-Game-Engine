use std::process::ExitCode;

use engine::{run_app_with_hooks, LoopRuntimeHooks};
use tracing::error;

use super::bootstrap::AppWiring;

pub(crate) fn run(app: AppWiring) -> ExitCode {
    let AppWiring {
        config,
        scene_a,
        scene_b,
        dev_thruport,
    } = app;

    let hooks = LoopRuntimeHooks {
        remote_console_pump: Some(Box::new(dev_thruport)),
    };

    if let Err(err) = run_app_with_hooks(config, scene_a, scene_b, hooks) {
        error!(error = %err, "startup_failed");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
