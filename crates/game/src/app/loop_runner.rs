use std::process::ExitCode;

use engine::run_app;
use tracing::error;

use super::bootstrap::AppWiring;

pub(crate) fn run(app: AppWiring) -> ExitCode {
    if let Err(err) = run_app(app.config, app.scene_a, app.scene_b) {
        error!(error = %err, "startup_failed");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
