use engine::{
    run_app, screen_to_world_px, ContentPlanRequest, EntityArchetype, EntityId, InputAction,
    InputSnapshot, Interactable, InteractableKind, JobState, LoopConfig, RenderableDesc, Scene,
    SceneCommand, SceneKey, SceneWorld, Transform, Vec2,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

const CAMERA_SPEED_UNITS_PER_SECOND: f32 = 6.0;
const MOVE_ARRIVAL_THRESHOLD: f32 = 0.1;
const JOB_DURATION_SECONDS: f32 = 2.0;
const RESOURCE_PILE_INTERACTION_RADIUS: f32 = 0.75;
const RESOURCE_PILE_STARTING_USES: u32 = 3;
const ENABLED_MODS_ENV_VAR: &str = "PROTOGE_ENABLED_MODS";

struct GameplayScene {
    scene_name: &'static str,
    switch_target: SceneKey,
    player_spawn: Vec2,
    player_id: Option<EntityId>,
    selected_entity: Option<EntityId>,
    player_move_speed: f32,
    resource_count: u32,
    interactable_cache: Vec<(EntityId, Vec2, f32)>,
    completed_target_ids: Vec<EntityId>,
}

impl GameplayScene {
    fn new(scene_name: &'static str, switch_target: SceneKey, player_spawn: Vec2) -> Self {
        Self {
            scene_name,
            switch_target,
            player_spawn,
            player_id: None,
            selected_entity: None,
            player_move_speed: 5.0,
            resource_count: 0,
            interactable_cache: Vec::new(),
            completed_target_ids: Vec::new(),
        }
    }
}

impl Scene for GameplayScene {
    fn load(&mut self, world: &mut SceneWorld) {
        let player_archetype = resolve_player_archetype(world);
        let pile_archetype = resolve_resource_pile_archetype(world);
        self.player_move_speed = player_archetype.move_speed;
        let player_id = world.spawn_actor(
            Transform {
                position: self.player_spawn,
                rotation_radians: None,
            },
            RenderableDesc {
                kind: player_archetype.renderable,
                debug_name: "player",
            },
        );
        let unit_a = world.spawn_actor(
            Transform {
                position: Vec2 {
                    x: self.player_spawn.x + 2.0,
                    y: self.player_spawn.y,
                },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: player_archetype.renderable,
                debug_name: "unit_a",
            },
        );
        let unit_b = world.spawn_actor(
            Transform {
                position: Vec2 {
                    x: self.player_spawn.x - 2.0,
                    y: self.player_spawn.y + 1.0,
                },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: player_archetype.renderable,
                debug_name: "unit_b",
            },
        );
        let pile_id = world.spawn(
            Transform {
                position: Vec2 {
                    x: self.player_spawn.x + 4.0,
                    y: self.player_spawn.y,
                },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: pile_archetype.renderable,
                debug_name: "resource_pile",
            },
        );
        self.player_id = Some(player_id);
        self.selected_entity = None;
        self.resource_count = 0;
        self.interactable_cache.clear();
        self.completed_target_ids.clear();
        world.apply_pending();
        for id in [player_id, unit_a, unit_b] {
            if let Some(entity) = world.find_entity_mut(id) {
                entity.selectable = true;
            }
        }
        if let Some(pile) = world.find_entity_mut(pile_id) {
            pile.interactable = Some(Interactable {
                kind: InteractableKind::ResourcePile,
                interaction_radius: RESOURCE_PILE_INTERACTION_RADIUS,
                remaining_uses: RESOURCE_PILE_STARTING_USES,
            });
        }
        info!(
            scene = self.scene_name,
            entity_count = world.entity_count(),
            "scene_loaded"
        );
    }

    fn update(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &mut SceneWorld,
    ) -> SceneCommand {
        if input.switch_scene_pressed() {
            return SceneCommand::SwitchTo(self.switch_target);
        }

        if input.left_click_pressed() {
            self.selected_entity = input.cursor_position_px().and_then(|cursor_px| {
                world.pick_topmost_selectable_at_cursor(cursor_px, input.window_size())
            });
        }

        if input.right_click_pressed() {
            if let (Some(selected_id), Some(cursor_px)) =
                (self.selected_entity, input.cursor_position_px())
            {
                let window_size = input.window_size();
                let ground_target = screen_to_world_px(world.camera(), window_size, cursor_px);
                let interactable_target = world
                    .pick_topmost_interactable_at_cursor(cursor_px, window_size)
                    .and_then(|id| {
                        world
                            .find_entity(id)
                            .map(|entity| (id, entity.transform.position))
                    });

                if let Some(entity) = world.find_entity_mut(selected_id) {
                    if entity.actor {
                        entity.job_state = JobState::Idle;
                        if let Some((target_id, target_world)) = interactable_target {
                            entity.move_target_world = Some(target_world);
                            entity.interaction_target = Some(target_id);
                        } else {
                            entity.move_target_world = Some(ground_target);
                            entity.interaction_target = None;
                        }
                    }
                }
            }
        }

        if let Some(player_id) = self.player_id {
            if let Some(player) = world.find_entity_mut(player_id) {
                let delta = movement_delta(input, fixed_dt_seconds, self.player_move_speed);
                player.transform.position.x += delta.x;
                player.transform.position.y += delta.y;
            }
        }

        self.interactable_cache.clear();
        for entity in world.entities() {
            if let Some(interactable) = entity.interactable {
                if interactable.remaining_uses > 0 {
                    self.interactable_cache.push((
                        entity.id,
                        entity.transform.position,
                        interactable.interaction_radius,
                    ));
                }
            }
        }

        self.completed_target_ids.clear();
        let mut completed_jobs = 0u32;
        for entity in world.entities_mut() {
            if !entity.actor {
                continue;
            }

            if let Some(target) = entity.move_target_world {
                let (next, arrived) = step_toward(
                    entity.transform.position,
                    target,
                    self.player_move_speed,
                    fixed_dt_seconds,
                    MOVE_ARRIVAL_THRESHOLD,
                );
                entity.transform.position = next;
                if arrived {
                    entity.move_target_world = None;
                }
            }

            let mut started_work_this_tick = false;
            if let Some(interaction_target) = entity.interaction_target {
                if let Some((target_world, radius)) =
                    interactable_target_info(&self.interactable_cache, interaction_target)
                {
                    if matches!(entity.job_state, JobState::Idle) {
                        let dx = target_world.x - entity.transform.position.x;
                        let dy = target_world.y - entity.transform.position.y;
                        if dx * dx + dy * dy <= radius * radius {
                            entity.job_state = JobState::Working {
                                target: interaction_target,
                                remaining_time: JOB_DURATION_SECONDS,
                            };
                            entity.move_target_world = None;
                            started_work_this_tick = true;
                        }
                    }
                } else {
                    entity.interaction_target = None;
                    entity.job_state = JobState::Idle;
                    entity.move_target_world = None;
                }
            }

            if started_work_this_tick {
                continue;
            }

            if let JobState::Working {
                target,
                remaining_time,
            } = entity.job_state
            {
                if has_interactable_target(&self.interactable_cache, target) {
                    let next_remaining = remaining_time - fixed_dt_seconds;
                    if next_remaining <= 0.0 {
                        entity.job_state = JobState::Idle;
                        entity.interaction_target = None;
                        completed_jobs = completed_jobs.saturating_add(1);
                        self.completed_target_ids.push(target);
                    } else {
                        entity.job_state = JobState::Working {
                            target,
                            remaining_time: next_remaining,
                        };
                    }
                } else {
                    entity.job_state = JobState::Idle;
                    entity.interaction_target = None;
                }
            }
        }

        self.resource_count = self.resource_count.saturating_add(completed_jobs);
        for target_id in self.completed_target_ids.iter().copied() {
            let mut should_despawn = false;
            if let Some(target) = world.find_entity_mut(target_id) {
                if let Some(interactable) = target.interactable.as_mut() {
                    if interactable.remaining_uses > 0 {
                        interactable.remaining_uses -= 1;
                    }
                    should_despawn = interactable.remaining_uses == 0;
                }
            }
            if should_despawn {
                world.despawn(target_id);
            }
        }

        let camera_delta = camera_delta(input, fixed_dt_seconds, CAMERA_SPEED_UNITS_PER_SECOND);
        world.camera_mut().position.x += camera_delta.x;
        world.camera_mut().position.y += camera_delta.y;

        SceneCommand::None
    }

    fn render(&mut self, _world: &SceneWorld) {}

    fn unload(&mut self, world: &mut SceneWorld) {
        info!(
            scene = self.scene_name,
            entity_count = world.entity_count(),
            "scene_unload"
        );
        self.player_id = None;
        self.selected_entity = None;
        self.resource_count = 0;
        self.interactable_cache.clear();
        self.completed_target_ids.clear();
    }

    fn debug_title(&self, world: &SceneWorld) -> Option<String> {
        let player = self.player_id.and_then(|id| world.find_entity(id))?;
        let camera = world.camera();
        Some(format!(
            "Proto GE | Scene {} | Player ({:.2}, {:.2}) | Camera ({:.2}, {:.2}) | Entities {}",
            self.scene_name,
            player.transform.position.x,
            player.transform.position.y,
            camera.position.x,
            camera.position.y,
            world.entity_count()
        ))
    }

    fn debug_selected_entity(&self) -> Option<EntityId> {
        self.selected_entity
    }

    fn debug_selected_target(&self, world: &SceneWorld) -> Option<Vec2> {
        let selected = self.selected_entity?;
        let entity = world.find_entity(selected)?;
        if !entity.actor {
            return None;
        }
        if let Some(target) = entity.move_target_world {
            return Some(target);
        }
        if let Some(target_id) = entity.interaction_target {
            return world
                .find_entity(target_id)
                .map(|target| target.transform.position);
        }
        if let JobState::Working { target, .. } = entity.job_state {
            return world
                .find_entity(target)
                .map(|target| target.transform.position);
        }
        None
    }

    fn debug_resource_count(&self) -> Option<u32> {
        Some(self.resource_count)
    }
}

fn resolve_player_archetype(world: &SceneWorld) -> EntityArchetype {
    let def_db = world
        .def_database()
        .unwrap_or_else(|| panic!("DefDatabase not set on SceneWorld before scene load"));
    let player_id = def_db.entity_def_id_by_name("proto.player").unwrap_or_else(|| {
        panic!(
            "missing EntityDef 'proto.player'; add it to assets/base or enabled mods and fix XML compile errors"
        )
    });
    def_db
        .entity_def(player_id)
        .unwrap_or_else(|| panic!("EntityDef id for 'proto.player' is missing from DefDatabase"))
        .clone()
}

fn resolve_resource_pile_archetype(world: &SceneWorld) -> EntityArchetype {
    let def_db = world
        .def_database()
        .unwrap_or_else(|| panic!("DefDatabase not set on SceneWorld before scene load"));
    let pile_id = def_db
        .entity_def_id_by_name("proto.resource_pile")
        .unwrap_or_else(|| {
            panic!(
                "missing EntityDef 'proto.resource_pile'; add it to assets/base or enabled mods and fix XML compile errors"
            )
        });
    let pile = def_db
        .entity_def(pile_id)
        .unwrap_or_else(|| {
            panic!("EntityDef id for 'proto.resource_pile' is missing from DefDatabase")
        })
        .clone();
    let has_interactable_tag = pile.tags.iter().any(|tag| tag == "interactable");
    let has_resource_pile_tag = pile.tags.iter().any(|tag| tag == "resource_pile");
    assert!(
        has_interactable_tag && has_resource_pile_tag,
        "EntityDef 'proto.resource_pile' must include tags 'interactable' and 'resource_pile'"
    );
    pile
}

fn interactable_target_info(
    interactables: &[(EntityId, Vec2, f32)],
    target: EntityId,
) -> Option<(Vec2, f32)> {
    interactables
        .iter()
        .find(|(id, _, _)| *id == target)
        .map(|(_, position, radius)| (*position, *radius))
}

fn has_interactable_target(interactables: &[(EntityId, Vec2, f32)], target: EntityId) -> bool {
    interactables.iter().any(|(id, _, _)| *id == target)
}

fn movement_delta(input: &InputSnapshot, fixed_dt_seconds: f32, speed: f32) -> Vec2 {
    let mut x = 0.0f32;
    let mut y = 0.0f32;

    if input.is_down(InputAction::MoveRight) {
        x += 1.0;
    }
    if input.is_down(InputAction::MoveLeft) {
        x -= 1.0;
    }
    if input.is_down(InputAction::MoveUp) {
        y += 1.0;
    }
    if input.is_down(InputAction::MoveDown) {
        y -= 1.0;
    }

    let len_sq = x * x + y * y;
    if len_sq > 0.0 {
        let inv_len = len_sq.sqrt().recip();
        x *= inv_len;
        y *= inv_len;
    }

    Vec2 {
        x: x * speed * fixed_dt_seconds,
        y: y * speed * fixed_dt_seconds,
    }
}

fn camera_delta(input: &InputSnapshot, fixed_dt_seconds: f32, speed: f32) -> Vec2 {
    let mut x = 0.0f32;
    let mut y = 0.0f32;

    if input.is_down(InputAction::CameraRight) {
        x += 1.0;
    }
    if input.is_down(InputAction::CameraLeft) {
        x -= 1.0;
    }
    if input.is_down(InputAction::CameraUp) {
        y += 1.0;
    }
    if input.is_down(InputAction::CameraDown) {
        y -= 1.0;
    }

    let len_sq = x * x + y * y;
    if len_sq > 0.0 {
        let inv_len = len_sq.sqrt().recip();
        x *= inv_len;
        y *= inv_len;
    }

    Vec2 {
        x: x * speed * fixed_dt_seconds,
        y: y * speed * fixed_dt_seconds,
    }
}

fn step_toward(
    current: Vec2,
    target: Vec2,
    speed: f32,
    fixed_dt_seconds: f32,
    arrival_threshold: f32,
) -> (Vec2, bool) {
    let dx = target.x - current.x;
    let dy = target.y - current.y;
    let distance_sq = dx * dx + dy * dy;
    let threshold_sq = arrival_threshold * arrival_threshold;
    if distance_sq <= threshold_sq {
        return (target, true);
    }

    let distance = distance_sq.sqrt();
    let max_step = speed * fixed_dt_seconds;
    if max_step >= distance {
        return (target, true);
    }

    let inv_distance = distance.recip();
    (
        Vec2 {
            x: current.x + dx * inv_distance * max_step,
            y: current.y + dy * inv_distance * max_step,
        },
        false,
    )
}

fn main() {
    init_tracing();
    info!("=== Proto GE Startup ===");

    let scene_a = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
    let scene_b = GameplayScene::new("B", SceneKey::A, Vec2 { x: 2.0, y: 2.0 });
    let config = LoopConfig {
        content_plan_request: ContentPlanRequest {
            enabled_mods: parse_enabled_mods_from_env(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            game_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        ..LoopConfig::default()
    };

    if let Err(err) = run_app(config, Box::new(scene_a), Box::new(scene_b)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_from_actions(actions: &[InputAction]) -> InputSnapshot {
        let mut snapshot = InputSnapshot::empty();
        for action in actions {
            snapshot = snapshot.with_action_down(*action, true);
        }
        snapshot
    }

    fn click_snapshot(cursor_px: Vec2, window_size: (u32, u32)) -> InputSnapshot {
        InputSnapshot::empty()
            .with_left_click_pressed(true)
            .with_cursor_position_px(Some(cursor_px))
            .with_window_size(window_size)
    }

    fn right_click_snapshot(cursor_px: Vec2, window_size: (u32, u32)) -> InputSnapshot {
        InputSnapshot::empty()
            .with_right_click_pressed(true)
            .with_cursor_position_px(Some(cursor_px))
            .with_window_size(window_size)
    }

    fn spawn_interactable_pile(
        world: &mut SceneWorld,
        position: Vec2,
        remaining_uses: u32,
    ) -> EntityId {
        let pile_id = world.spawn(
            Transform {
                position,
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "pile",
            },
        );
        world.apply_pending();
        world.find_entity_mut(pile_id).expect("pile").interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: RESOURCE_PILE_INTERACTION_RADIUS,
            remaining_uses,
        });
        pile_id
    }

    #[test]
    fn movement_magnitude_is_speed_times_dt() {
        let input = snapshot_from_actions(&[InputAction::MoveRight]);
        let delta = movement_delta(&input, 0.5, 5.0);
        assert!((delta.x - 2.5).abs() < 0.0001);
        assert!((delta.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn diagonal_is_normalized() {
        let input = snapshot_from_actions(&[InputAction::MoveRight, InputAction::MoveUp]);
        let delta = movement_delta(&input, 1.0, 5.0);
        let magnitude = (delta.x * delta.x + delta.y * delta.y).sqrt();
        assert!((magnitude - 5.0).abs() < 0.0001);
    }

    #[test]
    fn opposite_directions_cancel() {
        let input = snapshot_from_actions(&[InputAction::MoveLeft, InputAction::MoveRight]);
        let delta = movement_delta(&input, 1.0, 5.0);
        assert!((delta.x - 0.0).abs() < 0.0001);
        assert!((delta.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn camera_delta_uses_camera_actions() {
        let input = snapshot_from_actions(&[InputAction::CameraUp, InputAction::CameraRight]);
        let delta = camera_delta(&input, 1.0, 6.0);
        let magnitude = (delta.x * delta.x + delta.y * delta.y).sqrt();
        assert!((magnitude - 6.0).abs() < 0.0001);
    }

    #[test]
    fn left_click_selects_entity_under_cursor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let target_id = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 2.0, y: 1.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "target",
            },
        );
        world.apply_pending();

        let (x, y) =
            engine::world_to_screen_px(world.camera(), (1280, 720), Vec2 { x: 2.0, y: 1.0 });
        let click = click_snapshot(
            Vec2 {
                x: x as f32,
                y: y as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click, &mut world);

        assert_eq!(scene.debug_selected_entity(), Some(target_id));
    }

    #[test]
    fn clicking_empty_clears_selection() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "target",
            },
        );
        world.apply_pending();

        let first_click = click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &first_click, &mut world);
        assert!(scene.debug_selected_entity().is_some());

        let empty_click = click_snapshot(Vec2 { x: 30.0, y: 30.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &empty_click, &mut world);
        assert_eq!(scene.debug_selected_entity(), None);
    }

    #[test]
    fn selection_swaps_between_entities() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let a = world.spawn_selectable(
            Transform {
                position: Vec2 { x: -2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "a",
            },
        );
        let b = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "b",
            },
        );
        world.apply_pending();

        let (ax, ay) =
            engine::world_to_screen_px(world.camera(), (1280, 720), Vec2 { x: -2.0, y: 0.0 });
        let click_a = click_snapshot(
            Vec2 {
                x: ax as f32,
                y: ay as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click_a, &mut world);
        assert_eq!(scene.debug_selected_entity(), Some(a));

        let (bx, by) =
            engine::world_to_screen_px(world.camera(), (1280, 720), Vec2 { x: 2.0, y: 0.0 });
        let click_b = click_snapshot(
            Vec2 {
                x: bx as f32,
                y: by as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click_b, &mut world);
        assert_eq!(scene.debug_selected_entity(), Some(b));
    }

    #[test]
    fn step_toward_moves_by_speed_times_dt_without_overshoot() {
        let (next, arrived) = step_toward(
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 10.0, y: 0.0 },
            2.0,
            0.5,
            0.1,
        );
        assert!(!arrived);
        assert!((next.x - 1.0).abs() < 0.0001);
        assert!((next.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn step_toward_arrives_and_snaps_at_threshold() {
        let (next, arrived) = step_toward(
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 0.05, y: 0.0 },
            5.0,
            1.0 / 60.0,
            0.1,
        );
        assert!(arrived);
        assert!((next.x - 0.05).abs() < 0.0001);
        assert!((next.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn right_click_selected_actor_sets_move_target() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        world.find_entity_mut(actor).expect("actor").selectable = true;
        scene.selected_entity = Some(actor);

        let click = right_click_snapshot(Vec2 { x: 672.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);

        let target = world
            .find_entity(actor)
            .expect("actor")
            .move_target_world
            .expect("target");
        assert!((target.x - 1.0).abs() < 0.0001);
        assert!(target.y.abs() < 0.0001);
    }

    #[test]
    fn right_click_with_no_selection_is_noop() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        assert!(world
            .find_entity(actor)
            .expect("actor")
            .move_target_world
            .is_none());
    }

    #[test]
    fn right_click_selected_non_actor_is_ignored() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let non_actor = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "non_actor",
            },
        );
        world.apply_pending();
        scene.selected_entity = Some(non_actor);

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        assert!(world
            .find_entity(non_actor)
            .expect("non_actor")
            .move_target_world
            .is_none());
    }

    #[test]
    fn actor_moves_to_target_and_clears_it_on_arrival() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.move_target_world = Some(Vec2 { x: 0.2, y: 0.0 });
        }

        for _ in 0..10 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
        }

        let entity = world.find_entity(actor).expect("actor");
        assert!(entity.move_target_world.is_none());
        assert!((entity.transform.position.x - 0.2).abs() <= MOVE_ARRIVAL_THRESHOLD);
    }

    #[test]
    fn right_click_interactable_sets_interaction_target() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: -2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let pile = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 3);
        world.find_entity_mut(actor).expect("actor").selectable = true;
        scene.selected_entity = Some(actor);

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);

        let updated = world.find_entity(actor).expect("actor");
        assert_eq!(updated.interaction_target, Some(pile));
        assert!(updated.move_target_world.is_some());
        assert_eq!(updated.job_state, JobState::Idle);
    }

    #[test]
    fn job_completion_increments_items_and_despawns_depleted_pile() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let pile = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.interaction_target = Some(pile);
            entity.job_state = JobState::Idle;
        }

        for _ in 0..40 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
        }

        let actor_entity = world.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.job_state, JobState::Idle);
        assert_eq!(actor_entity.interaction_target, None);
        assert_eq!(scene.resource_count, 1);
        assert!(world.find_entity(pile).is_none());
    }

    #[test]
    fn missing_interaction_target_clears_job_state_safely() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.interaction_target = Some(EntityId(9999));
            entity.job_state = JobState::Working {
                target: EntityId(9999),
                remaining_time: 1.0,
            };
        }

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        let actor_entity = world.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.job_state, JobState::Idle);
        assert_eq!(actor_entity.interaction_target, None);
    }
}
