use std::process::ExitCode;

use engine::run_app;
use tracing::error;

use super::bootstrap::AppWiring;

pub(crate) fn run(app: AppWiring) -> ExitCode {
    let AppWiring {
        config,
        scene_a,
        scene_b,
        dev_thruport: _dev_thruport,
    } = app;

    if let Err(err) = run_app(config, scene_a, scene_b) {
        error!(error = %err, "startup_failed");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
