use engine::{
    run_app, InputSnapshot, LoopConfig, RenderableDesc, RenderableKind, Scene, SceneCommand,
    SceneKey, SceneMachine, SceneWorld, Transform, Vec2,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

struct SceneA;
struct SceneB;

impl Scene for SceneA {
    fn load(&mut self, world: &mut SceneWorld) {
        for idx in 0..3 {
            world.spawn(
                Transform {
                    position: Vec2 {
                        x: idx as f32,
                        y: 0.0,
                    },
                    rotation_radians: None,
                },
                RenderableDesc {
                    kind: RenderableKind::Placeholder,
                    debug_name: "scene_a_entity",
                },
            );
        }
        world.apply_pending();
        info!(
            scene = "A",
            entity_count = world.entity_count(),
            "scene_loaded"
        );
    }

    fn update(
        &mut self,
        _fixed_dt_seconds: f32,
        input: &InputSnapshot,
        _world: &mut SceneWorld,
    ) -> SceneCommand {
        if input.switch_scene_pressed() {
            SceneCommand::SwitchTo(SceneKey::B)
        } else {
            SceneCommand::None
        }
    }

    fn render(&mut self, _world: &SceneWorld) {}

    fn unload(&mut self, world: &mut SceneWorld) {
        info!(
            scene = "A",
            entity_count = world.entity_count(),
            "scene_unload"
        );
    }
}

impl Scene for SceneB {
    fn load(&mut self, world: &mut SceneWorld) {
        for idx in 0..5 {
            world.spawn(
                Transform {
                    position: Vec2 {
                        x: idx as f32,
                        y: 1.0,
                    },
                    rotation_radians: Some(0.0),
                },
                RenderableDesc {
                    kind: RenderableKind::Placeholder,
                    debug_name: "scene_b_entity",
                },
            );
        }
        world.apply_pending();
        info!(
            scene = "B",
            entity_count = world.entity_count(),
            "scene_loaded"
        );
    }

    fn update(
        &mut self,
        _fixed_dt_seconds: f32,
        input: &InputSnapshot,
        _world: &mut SceneWorld,
    ) -> SceneCommand {
        if input.switch_scene_pressed() {
            SceneCommand::SwitchTo(SceneKey::A)
        } else {
            SceneCommand::None
        }
    }

    fn render(&mut self, _world: &SceneWorld) {}

    fn unload(&mut self, world: &mut SceneWorld) {
        info!(
            scene = "B",
            entity_count = world.entity_count(),
            "scene_unload"
        );
    }
}

fn main() {
    init_tracing();
    info!("=== Proto GE Startup ===");

    let scenes = SceneMachine::new(Box::new(SceneA), Box::new(SceneB), SceneKey::A);
    if let Err(err) = run_app(LoopConfig::default(), scenes) {
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
