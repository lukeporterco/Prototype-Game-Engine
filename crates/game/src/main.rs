use engine::{run_app, LoopConfig, Scene};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

struct BootScene;

impl Scene for BootScene {
    fn update(&mut self, _fixed_dt_seconds: f32) {}

    fn render(&mut self) {}
}

fn main() {
    init_tracing();
    info!("=== Proto GE Startup ===");

    if let Err(err) = run_app(LoopConfig::default(), BootScene) {
        error!(error = %err, "startup_failed");
        std::process::exit(1);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_names(true)
        .compact()
        .init();
}
