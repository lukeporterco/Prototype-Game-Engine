mod app;

use std::process::ExitCode;

fn main() -> ExitCode {
    let app_wiring = app::bootstrap::build_app();
    app::loop_runner::run(app_wiring)
}
