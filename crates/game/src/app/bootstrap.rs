use engine::{ContentPlanRequest, LoopConfig, Scene};
use tracing::info;
use tracing_subscriber::EnvFilter;

use super::gameplay;

const ENABLED_MODS_ENV_VAR: &str = "PROTOGE_ENABLED_MODS";

pub(crate) struct AppWiring {
    pub(crate) config: LoopConfig,
    pub(crate) scene_a: Box<dyn Scene>,
    pub(crate) scene_b: Box<dyn Scene>,
}

pub(crate) fn build_app() -> AppWiring {
    init_tracing();
    info!("=== Proto GE Startup ===");

    let (scene_a, scene_b) = gameplay::build_scene_pair();
    let config = LoopConfig {
        content_plan_request: ContentPlanRequest {
            enabled_mods: parse_enabled_mods_from_env(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            game_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        ..LoopConfig::default()
    };

    AppWiring {
        config,
        scene_a,
        scene_b,
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

fn parse_enabled_mods_from_env() -> Vec<String> {
    std::env::var(ENABLED_MODS_ENV_VAR)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
