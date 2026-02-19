use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use engine::{
    resolve_app_paths, run_app, screen_to_world_px, ContentPlanRequest, DebugInfoSnapshot,
    DebugJobState, EntityArchetype, EntityId, InputAction, InputSnapshot, Interactable,
    InteractableKind, JobState, LoopConfig, RenderableDesc, RenderableKind, Scene, SceneCommand,
    SceneKey, SceneWorld, Tilemap, Transform, Vec2,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

const CAMERA_SPEED_UNITS_PER_SECOND: f32 = 6.0;
const MOVE_ARRIVAL_THRESHOLD: f32 = 0.1;
const JOB_DURATION_SECONDS: f32 = 2.0;
const RESOURCE_PILE_INTERACTION_RADIUS: f32 = 0.75;
const RESOURCE_PILE_STARTING_USES: u32 = 3;
const ENABLED_MODS_ENV_VAR: &str = "PROTOGE_ENABLED_MODS";
const SAVE_VERSION: u32 = 1;
const SCENE_A_SAVE_FILE: &str = "scene_a.save.json";
const SCENE_B_SAVE_FILE: &str = "scene_b.save.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SavedSceneKey {
    A,
    B,
}

impl SavedSceneKey {
    fn from_scene_key(scene_key: SceneKey) -> Self {
        match scene_key {
            SceneKey::A => Self::A,
            SceneKey::B => Self::B,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct SavedVec2 {
    x: f32,
    y: f32,
}

impl SavedVec2 {
    fn from_vec2(value: Vec2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }

    fn to_vec2(self) -> Vec2 {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SavedInteractableKind {
    ResourcePile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct SavedInteractableRuntime {
    kind: SavedInteractableKind,
    interaction_radius: f32,
    remaining_uses: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum SavedJobState {
    Idle,
    Working {
        target_index: usize,
        remaining_time: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SavedEntityRuntime {
    position: SavedVec2,
    rotation_radians: Option<f32>,
    selectable: bool,
    actor: bool,
    move_target_world: Option<SavedVec2>,
    interaction_target_index: Option<usize>,
    job_state: SavedJobState,
    interactable: Option<SavedInteractableRuntime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SaveGame {
    save_version: u32,
    scene_key: SavedSceneKey,
    camera_position: SavedVec2,
    selected_entity_index: Option<usize>,
    player_entity_index: Option<usize>,
    resource_count: u32,
    entities: Vec<SavedEntityRuntime>,
}

type SaveLoadResult<T> = Result<T, String>;

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

    fn scene_key(&self) -> SceneKey {
        match self.switch_target {
            SceneKey::A => SceneKey::B,
            SceneKey::B => SceneKey::A,
        }
    }

    fn save_file_path(&self) -> SaveLoadResult<PathBuf> {
        let app_paths =
            resolve_app_paths().map_err(|error| format!("resolve app paths: {error}"))?;
        let file_name = match self.scene_key() {
            SceneKey::A => SCENE_A_SAVE_FILE,
            SceneKey::B => SCENE_B_SAVE_FILE,
        };
        Ok(app_paths.cache_dir.join("saves").join(file_name))
    }

    fn save_to_disk(&self, world: &SceneWorld) -> SaveLoadResult<PathBuf> {
        let save = self.build_save_game(world);
        let path = self.save_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create save dir '{}': {error}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&save)
            .map_err(|error| format!("encode save json: {error}"))?;
        fs::write(&path, json)
            .map_err(|error| format!("write save '{}': {error}", path.display()))?;
        Ok(path)
    }

    fn load_and_validate_save(&self, expected_scene: SavedSceneKey) -> SaveLoadResult<SaveGame> {
        let path = self.save_file_path()?;
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("read save '{}': {error}", path.display()))?;
        let save: SaveGame =
            serde_json::from_str(&raw).map_err(|error| format!("parse save json: {error}"))?;
        Self::validate_save_game(&save, expected_scene)?;
        Ok(save)
    }

    fn validate_save_game(save: &SaveGame, expected_scene: SavedSceneKey) -> SaveLoadResult<()> {
        if save.save_version != SAVE_VERSION {
            return Err(format!(
                "save version mismatch: expected {}, got {}",
                SAVE_VERSION, save.save_version
            ));
        }
        if save.scene_key != expected_scene {
            return Err(format!(
                "save scene mismatch: expected {:?}, got {:?}",
                expected_scene, save.scene_key
            ));
        }

        let len = save.entities.len();
        let validate_idx = |label: &str, index: Option<usize>| -> SaveLoadResult<()> {
            if let Some(idx) = index {
                if idx >= len {
                    return Err(format!(
                        "invalid {label} index {idx} for entity count {len}"
                    ));
                }
            }
            Ok(())
        };

        validate_idx("selected_entity", save.selected_entity_index)?;
        validate_idx("player_entity", save.player_entity_index)?;
        for (entity_index, entity) in save.entities.iter().enumerate() {
            validate_idx("interaction_target", entity.interaction_target_index)?;
            if let SavedJobState::Working { target_index, .. } = entity.job_state {
                if target_index >= len {
                    return Err(format!(
                        "invalid job target index {target_index} on entity {entity_index} for entity count {len}"
                    ));
                }
            }
        }

        Ok(())
    }

    fn build_save_game(&self, world: &SceneWorld) -> SaveGame {
        let mut index_by_id = HashMap::new();
        for (index, entity) in world.entities().iter().enumerate() {
            index_by_id.insert(entity.id, index);
        }

        let entities = world
            .entities()
            .iter()
            .map(|entity| SavedEntityRuntime {
                position: SavedVec2::from_vec2(entity.transform.position),
                rotation_radians: entity.transform.rotation_radians,
                selectable: entity.selectable,
                actor: entity.actor,
                move_target_world: entity.move_target_world.map(SavedVec2::from_vec2),
                interaction_target_index: entity
                    .interaction_target
                    .and_then(|id| index_by_id.get(&id).copied()),
                job_state: match entity.job_state {
                    JobState::Idle => SavedJobState::Idle,
                    JobState::Working {
                        target,
                        remaining_time,
                    } => index_by_id
                        .get(&target)
                        .copied()
                        .map(|target_index| SavedJobState::Working {
                            target_index,
                            remaining_time,
                        })
                        .unwrap_or(SavedJobState::Idle),
                },
                interactable: entity
                    .interactable
                    .map(|interactable| SavedInteractableRuntime {
                        kind: match interactable.kind {
                            InteractableKind::ResourcePile => SavedInteractableKind::ResourcePile,
                        },
                        interaction_radius: interactable.interaction_radius,
                        remaining_uses: interactable.remaining_uses,
                    }),
            })
            .collect();

        SaveGame {
            save_version: SAVE_VERSION,
            scene_key: SavedSceneKey::from_scene_key(self.scene_key()),
            camera_position: SavedVec2::from_vec2(world.camera().position),
            selected_entity_index: self
                .selected_entity
                .and_then(|id| index_by_id.get(&id).copied()),
            player_entity_index: self.player_id.and_then(|id| index_by_id.get(&id).copied()),
            resource_count: self.resource_count,
            entities,
        }
    }

    fn apply_save_game(&mut self, save: SaveGame, world: &mut SceneWorld) -> SaveLoadResult<()> {
        let needs_actor_archetype = save.entities.iter().any(|entity| entity.actor);
        let needs_pile_archetype = save
            .entities
            .iter()
            .any(|entity| entity.interactable.is_some());
        let player_archetype = if needs_actor_archetype {
            Some(try_resolve_player_archetype(world)?)
        } else {
            None
        };
        let pile_archetype = if needs_pile_archetype {
            Some(try_resolve_resource_pile_archetype(world)?)
        } else {
            None
        };
        if let Some(archetype) = &player_archetype {
            self.player_move_speed = archetype.move_speed;
        }

        world.clear();
        self.interactable_cache.clear();
        self.completed_target_ids.clear();
        world.camera_mut().position = save.camera_position.to_vec2();

        let mut spawned_ids = Vec::with_capacity(save.entities.len());
        for saved_entity in &save.entities {
            let renderable_kind = if saved_entity.interactable.is_some() {
                pile_archetype
                    .as_ref()
                    .map(|archetype| archetype.renderable.clone())
                    .unwrap_or(RenderableKind::Placeholder)
            } else if saved_entity.actor {
                player_archetype
                    .as_ref()
                    .map(|archetype| archetype.renderable.clone())
                    .unwrap_or(RenderableKind::Placeholder)
            } else {
                RenderableKind::Placeholder
            };
            let id = world.spawn(
                Transform {
                    position: saved_entity.position.to_vec2(),
                    rotation_radians: saved_entity.rotation_radians,
                },
                RenderableDesc {
                    kind: renderable_kind,
                    debug_name: "saved",
                },
            );
            spawned_ids.push(id);
        }
        world.apply_pending();

        for (index, saved_entity) in save.entities.iter().enumerate() {
            let id = spawned_ids[index];
            let Some(entity) = world.find_entity_mut(id) else {
                return Err(format!("spawned entity missing at index {index}"));
            };

            entity.transform.position = saved_entity.position.to_vec2();
            entity.transform.rotation_radians = saved_entity.rotation_radians;
            entity.selectable = saved_entity.selectable;
            entity.actor = saved_entity.actor;
            entity.move_target_world = saved_entity.move_target_world.map(SavedVec2::to_vec2);
            entity.interactable = saved_entity.interactable.map(|interactable| Interactable {
                kind: match interactable.kind {
                    SavedInteractableKind::ResourcePile => InteractableKind::ResourcePile,
                },
                interaction_radius: interactable.interaction_radius,
                remaining_uses: interactable.remaining_uses,
            });
            entity.interaction_target = saved_entity
                .interaction_target_index
                .and_then(|target_index| spawned_ids.get(target_index).copied());
            entity.job_state = match saved_entity.job_state {
                SavedJobState::Idle => JobState::Idle,
                SavedJobState::Working {
                    target_index,
                    remaining_time,
                } => spawned_ids
                    .get(target_index)
                    .copied()
                    .map(|target| JobState::Working {
                        target,
                        remaining_time,
                    })
                    .unwrap_or(JobState::Idle),
            };
        }

        self.selected_entity = save
            .selected_entity_index
            .and_then(|index| spawned_ids.get(index).copied());
        self.player_id = save
            .player_entity_index
            .and_then(|index| spawned_ids.get(index).copied());
        self.resource_count = save.resource_count;
        Ok(())
    }
}

impl Scene for GameplayScene {
    fn load(&mut self, world: &mut SceneWorld) {
        let player_archetype = resolve_player_archetype(world);
        let pile_archetype = resolve_resource_pile_archetype(world);
        world.set_tilemap(build_ground_tilemap(self.scene_key()));
        self.player_move_speed = player_archetype.move_speed;
        let player_id = world.spawn_actor(
            Transform {
                position: self.player_spawn,
                rotation_radians: None,
            },
            RenderableDesc {
                kind: player_archetype.renderable.clone(),
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
                kind: player_archetype.renderable.clone(),
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
                kind: player_archetype.renderable.clone(),
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
                kind: pile_archetype.renderable.clone(),
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
        if input.save_pressed() {
            match self.save_to_disk(world) {
                Ok(path) => info!(
                    scene = self.scene_name,
                    path = %path.display(),
                    "save_written"
                ),
                Err(error) => warn!(
                    scene = self.scene_name,
                    error = %error,
                    "save_failed"
                ),
            }
        }

        if input.load_pressed() {
            let expected_scene = SavedSceneKey::from_scene_key(self.scene_key());
            match self.load_and_validate_save(expected_scene) {
                Ok(save) => {
                    if let Err(error) = self.apply_save_game(save, world) {
                        warn!(
                            scene = self.scene_name,
                            error = %error,
                            "load_apply_failed"
                        );
                    } else {
                        info!(scene = self.scene_name, "save_loaded");
                    }
                }
                Err(error) => warn!(
                    scene = self.scene_name,
                    error = %error,
                    "load_failed"
                ),
            }
        }

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

    fn debug_info_snapshot(&self, world: &SceneWorld) -> Option<DebugInfoSnapshot> {
        let entity_count = world.entity_count();
        let actor_count = world
            .entities()
            .iter()
            .filter(|entity| entity.actor)
            .count();
        let interactable_count = world
            .entities()
            .iter()
            .filter(|entity| entity.interactable.is_some())
            .count();

        let mut selected_position_world = None;
        let mut selected_order_world = None;
        let mut selected_job_state = DebugJobState::None;

        if let Some(selected_id) = self.selected_entity {
            if let Some(entity) = world.find_entity(selected_id) {
                selected_position_world = Some(entity.transform.position);
                selected_order_world = entity.move_target_world.or_else(|| {
                    entity
                        .interaction_target
                        .and_then(|target_id| world.find_entity(target_id))
                        .map(|target| target.transform.position)
                });
                selected_job_state = match entity.job_state {
                    JobState::Idle => DebugJobState::Idle,
                    JobState::Working { remaining_time, .. } => {
                        DebugJobState::Working { remaining_time }
                    }
                };
            }
        }

        Some(DebugInfoSnapshot {
            selected_entity: self.selected_entity,
            selected_position_world,
            selected_order_world,
            selected_job_state,
            entity_count,
            actor_count,
            interactable_count,
            resource_count: self.resource_count,
        })
    }
}

fn build_ground_tilemap(scene_key: SceneKey) -> Tilemap {
    let width = 16u32;
    let height = 12u32;
    let mut tiles = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = ((x + y) % 2) as u16;
            let tile_id = match scene_key {
                SceneKey::A => checker,
                SceneKey::B => 1u16.saturating_sub(checker),
            };
            tiles.push(tile_id);
        }
    }
    Tilemap::new(
        width,
        height,
        Vec2 {
            x: -(width as f32) / 2.0,
            y: -(height as f32) / 2.0,
        },
        tiles,
    )
    .expect("static tilemap shape is valid")
}

fn resolve_player_archetype(world: &SceneWorld) -> EntityArchetype {
    try_resolve_player_archetype(world).unwrap_or_else(|error| panic!("{error}"))
}

fn try_resolve_player_archetype(world: &SceneWorld) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let player_id = def_db
        .entity_def_id_by_name("proto.player")
        .ok_or_else(|| {
            "missing EntityDef 'proto.player'; add it to assets/base or enabled mods and fix XML compile errors"
                .to_string()
        })?;
    def_db
        .entity_def(player_id)
        .ok_or_else(|| "EntityDef id for 'proto.player' is missing from DefDatabase".to_string())
        .cloned()
}

fn resolve_resource_pile_archetype(world: &SceneWorld) -> EntityArchetype {
    try_resolve_resource_pile_archetype(world).unwrap_or_else(|error| panic!("{error}"))
}

fn try_resolve_resource_pile_archetype(world: &SceneWorld) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let pile_id = def_db
        .entity_def_id_by_name("proto.resource_pile")
        .ok_or_else(|| {
            "missing EntityDef 'proto.resource_pile'; add it to assets/base or enabled mods and fix XML compile errors"
                .to_string()
        })?;
    let pile = def_db
        .entity_def(pile_id)
        .ok_or_else(|| {
            "EntityDef id for 'proto.resource_pile' is missing from DefDatabase".to_string()
        })?
        .clone();
    let has_interactable_tag = pile.tags.iter().any(|tag| tag == "interactable");
    let has_resource_pile_tag = pile.tags.iter().any(|tag| tag == "resource_pile");
    if !(has_interactable_tag && has_resource_pile_tag) {
        return Err(
            "EntityDef 'proto.resource_pile' must include tags 'interactable' and 'resource_pile'"
                .to_string(),
        );
    }
    Ok(pile)
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

    fn seed_def_database(world: &mut SceneWorld) {
        let paths = resolve_app_paths().expect("app paths");
        let request = ContentPlanRequest {
            enabled_mods: Vec::new(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            game_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let defs = engine::build_or_load_def_database(&paths, &request).expect("def db");
        world.set_def_database(defs);
    }

    fn sample_save_game(scene_key: SavedSceneKey) -> SaveGame {
        SaveGame {
            save_version: SAVE_VERSION,
            scene_key,
            camera_position: SavedVec2 { x: 3.0, y: -1.0 },
            selected_entity_index: Some(0),
            player_entity_index: Some(0),
            resource_count: 2,
            entities: vec![
                SavedEntityRuntime {
                    position: SavedVec2 { x: 1.0, y: 2.0 },
                    rotation_radians: None,
                    selectable: true,
                    actor: true,
                    move_target_world: Some(SavedVec2 { x: 4.0, y: 2.0 }),
                    interaction_target_index: Some(1),
                    job_state: SavedJobState::Working {
                        target_index: 1,
                        remaining_time: 1.5,
                    },
                    interactable: None,
                },
                SavedEntityRuntime {
                    position: SavedVec2 { x: 5.0, y: 6.0 },
                    rotation_radians: None,
                    selectable: false,
                    actor: false,
                    move_target_world: None,
                    interaction_target_index: None,
                    job_state: SavedJobState::Idle,
                    interactable: Some(SavedInteractableRuntime {
                        kind: SavedInteractableKind::ResourcePile,
                        interaction_radius: 0.75,
                        remaining_uses: 2,
                    }),
                },
            ],
        }
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

    #[test]
    fn job_remaining_time_pauses_while_inactive_and_completes_after_resume() {
        let mut scene_a = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world_a = SceneWorld::default();
        let actor = world_a.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let pile = spawn_interactable_pile(&mut world_a, Vec2 { x: 0.0, y: 0.0 }, 1);
        {
            let entity = world_a.find_entity_mut(actor).expect("actor");
            entity.interaction_target = Some(pile);
            entity.job_state = JobState::Working {
                target: pile,
                remaining_time: 1.0,
            };
        }

        let mut scene_b = GameplayScene::new("B", SceneKey::A, Vec2 { x: 5.0, y: 5.0 });
        let mut world_b = SceneWorld::default();
        world_b.spawn_actor(
            Transform::default(),
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "b",
            },
        );
        world_b.apply_pending();

        scene_a.update(0.1, &InputSnapshot::empty(), &mut world_a);
        let before_pause = match world_a.find_entity(actor).expect("actor").job_state {
            JobState::Working { remaining_time, .. } => remaining_time,
            _ => panic!("expected working"),
        };

        for _ in 0..10 {
            scene_b.update(0.1, &InputSnapshot::empty(), &mut world_b);
            world_b.apply_pending();
        }

        let after_pause = match world_a.find_entity(actor).expect("actor").job_state {
            JobState::Working { remaining_time, .. } => remaining_time,
            _ => panic!("expected working"),
        };
        assert!((before_pause - after_pause).abs() < 0.0001);

        for _ in 0..20 {
            scene_a.update(0.1, &InputSnapshot::empty(), &mut world_a);
            world_a.apply_pending();
        }

        let actor_entity = world_a.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.job_state, JobState::Idle);
        assert_eq!(scene_a.resource_count, 1);
    }

    #[test]
    fn mid_move_state_persists_across_normal_switch() {
        let mut scene_a = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world_a = SceneWorld::default();
        let actor = world_a.spawn_actor(
            Transform {
                position: Vec2 { x: -1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let pile = spawn_interactable_pile(&mut world_a, Vec2 { x: 1.0, y: 0.0 }, 3);
        {
            let entity = world_a.find_entity_mut(actor).expect("actor");
            entity.selectable = true;
            entity.move_target_world = Some(Vec2 { x: 1.0, y: 0.0 });
            entity.interaction_target = Some(pile);
            entity.job_state = JobState::Idle;
        }
        scene_a.selected_entity = Some(actor);
        scene_a.resource_count = 2;

        let mut scene_b = GameplayScene::new("B", SceneKey::A, Vec2 { x: 8.0, y: 8.0 });
        let mut world_b = SceneWorld::default();
        world_b.spawn(
            Transform::default(),
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "b_obj",
            },
        );
        world_b.apply_pending();

        let before = world_a.find_entity(actor).expect("actor").clone();
        let before_items = scene_a.resource_count;
        let before_selection = scene_a.selected_entity;

        for _ in 0..15 {
            scene_b.update(0.1, &InputSnapshot::empty(), &mut world_b);
            world_b.apply_pending();
        }

        let after = world_a.find_entity(actor).expect("actor").clone();
        assert_eq!(scene_a.selected_entity, before_selection);
        assert_eq!(scene_a.resource_count, before_items);
        assert_eq!(after.transform.position, before.transform.position);
        assert_eq!(after.move_target_world, before.move_target_world);
        assert_eq!(after.interaction_target, before.interaction_target);
        assert_eq!(after.job_state, before.job_state);
    }

    #[test]
    fn debug_info_snapshot_reports_selected_entity_fields_and_counts() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 1.0, y: 2.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let pile = spawn_interactable_pile(&mut world, Vec2 { x: 3.0, y: 4.0 }, 2);
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.selectable = true;
            entity.move_target_world = Some(Vec2 { x: 5.0, y: 6.0 });
            entity.job_state = JobState::Working {
                target: pile,
                remaining_time: 1.2,
            };
        }
        scene.selected_entity = Some(actor);
        scene.resource_count = 7;

        let snapshot = scene
            .debug_info_snapshot(&world)
            .expect("debug snapshot exists");
        assert_eq!(snapshot.selected_entity, Some(actor));
        assert_eq!(
            snapshot.selected_position_world,
            Some(Vec2 { x: 1.0, y: 2.0 })
        );
        assert_eq!(snapshot.selected_order_world, Some(Vec2 { x: 5.0, y: 6.0 }));
        assert_eq!(
            snapshot.selected_job_state,
            DebugJobState::Working {
                remaining_time: 1.2
            }
        );
        assert_eq!(snapshot.entity_count, 2);
        assert_eq!(snapshot.actor_count, 1);
        assert_eq!(snapshot.interactable_count, 1);
        assert_eq!(snapshot.resource_count, 7);
    }

    #[test]
    fn debug_info_snapshot_handles_missing_selected_entity() {
        let scene = GameplayScene {
            selected_entity: Some(EntityId(999)),
            ..GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 })
        };
        let world = SceneWorld::default();
        let snapshot = scene
            .debug_info_snapshot(&world)
            .expect("debug snapshot exists");
        assert_eq!(snapshot.selected_entity, Some(EntityId(999)));
        assert_eq!(snapshot.selected_position_world, None);
        assert_eq!(snapshot.selected_order_world, None);
        assert_eq!(snapshot.selected_job_state, DebugJobState::None);
    }

    #[test]
    fn save_game_roundtrip_json_preserves_runtime_fields() {
        let save = sample_save_game(SavedSceneKey::A);
        let json = serde_json::to_string(&save).expect("serialize");
        let decoded: SaveGame = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, save);
    }

    #[test]
    fn load_validation_rejects_bad_version_or_scene_without_mutation() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();
        scene.selected_entity = Some(world.entities()[0].id);
        let before_resource_count = scene.resource_count;
        let before_entity_count = world.entity_count();
        let before_first_pos = world.entities()[0].transform.position;

        let mut bad_version = sample_save_game(SavedSceneKey::A);
        bad_version.save_version = SAVE_VERSION + 1;
        assert!(GameplayScene::validate_save_game(&bad_version, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(world.entities()[0].transform.position, before_first_pos);
        assert_eq!(scene.resource_count, before_resource_count);

        let bad_scene = sample_save_game(SavedSceneKey::B);
        assert!(GameplayScene::validate_save_game(&bad_scene, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(world.entities()[0].transform.position, before_first_pos);
        assert_eq!(scene.resource_count, before_resource_count);
    }

    #[test]
    fn index_based_remap_restores_refs_correctly() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);

        let save = sample_save_game(SavedSceneKey::A);
        GameplayScene::validate_save_game(&save, SavedSceneKey::A).expect("valid");
        scene.apply_save_game(save, &mut world).expect("apply");

        let entities = world.entities();
        assert_eq!(entities.len(), 2);
        let actor = &entities[0];
        let target = &entities[1];
        assert!(actor.actor);
        assert_eq!(scene.selected_entity, Some(actor.id));
        assert_eq!(scene.player_id, Some(actor.id));
        assert_eq!(actor.interaction_target, Some(target.id));
        assert_eq!(
            actor.job_state,
            JobState::Working {
                target: target.id,
                remaining_time: 1.5
            }
        );
        assert_eq!(scene.resource_count, 2);
    }

    #[test]
    fn save_mid_move_then_load_restores_resumable_state() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);

        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.selectable = true;
            entity.move_target_world = Some(Vec2 { x: 2.0, y: 0.0 });
        }
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);
        scene.resource_count = 5;
        world.camera_mut().position = Vec2 { x: 1.0, y: -1.0 };

        let save = scene.build_save_game(&world);
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.transform.position = Vec2 { x: 9.0, y: 9.0 };
            entity.move_target_world = None;
        }
        scene.selected_entity = None;
        scene.resource_count = 0;
        world.camera_mut().position = Vec2 { x: -4.0, y: 7.0 };

        GameplayScene::validate_save_game(&save, SavedSceneKey::A).expect("valid");
        scene.apply_save_game(save, &mut world).expect("apply");

        let restored_actor = world
            .find_entity(scene.player_id.expect("player"))
            .expect("actor");
        assert_eq!(scene.selected_entity, Some(restored_actor.id));
        assert_eq!(scene.resource_count, 5);
        assert_eq!(world.camera().position, Vec2 { x: 1.0, y: -1.0 });
        assert_eq!(restored_actor.transform.position, Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(
            restored_actor.move_target_world,
            Some(Vec2 { x: 2.0, y: 0.0 })
        );
        let restored_actor_id = restored_actor.id;

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        let advanced_actor = world.find_entity(restored_actor_id).expect("actor");
        assert!(advanced_actor.transform.position.x > 0.0);
    }
}
