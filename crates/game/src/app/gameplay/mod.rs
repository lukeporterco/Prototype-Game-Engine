use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use engine::ContentPlanRequest;
use engine::{
    resolve_app_paths, screen_to_world_px, DebugInfoSnapshot, DebugJobState, DebugMarker,
    DebugMarkerKind, EntityArchetype, EntityDefId, EntityId, FloorId, InputAction, InputSnapshot,
    Interactable, InteractableKind, OrderState, RenderableDesc, RenderableKind, Scene,
    SceneCommand, SceneDebugCommand, SceneDebugCommandResult, SceneDebugContext, SceneKey,
    SceneWorld, Tilemap, Transform, Vec2,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

const CAMERA_SPEED_UNITS_PER_SECOND: f32 = 6.0;
const MOVE_ARRIVAL_THRESHOLD: f32 = 0.1;
const JOB_DURATION_SECONDS: f32 = 2.0;
const RESOURCE_PILE_INTERACTION_RADIUS: f32 = 0.75;
const RESOURCE_PILE_STARTING_USES: u32 = 3;
const SAVE_VERSION: u32 = 3;
const SCENE_A_SAVE_FILE: &str = "scene_a.save.json";
const SCENE_B_SAVE_FILE: &str = "scene_b.save.json";
const ORDER_MARKER_TTL_SECONDS: f32 = 0.75;
const GAMEPLAY_SYSTEM_ORDER_TEXT: &str =
    "InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup";
const AI_AGGRO_RADIUS_UNITS: f32 = 6.0;
const AI_ATTACK_RANGE_UNITS: f32 = 0.9;
const AI_ATTACK_INTERACTION_DURATION_SECONDS: f32 = 0.5;
const AI_ATTACK_COOLDOWN_SECONDS: f32 = 1.0;
const AI_WANDER_OFFSET_UNITS: f32 = 1.5;
const AI_WANDER_ARRIVAL_THRESHOLD: f32 = 0.15;
const DEFAULT_MAX_HEALTH: u32 = 100;
const ATTACK_DAMAGE_PER_HIT: u32 = 25;
const STATUS_SLOW: StatusId = StatusId("status.slow");
const STATUS_SLOW_DURATION_SECONDS: f32 = 2.0;
const STATUS_SLOW_MULTIPLIER: f32 = 0.5;
const COMBAT_CHASER_PLAYER_POS: Vec2 = Vec2 { x: 0.0, y: 0.0 };
const COMBAT_CHASER_CHASER_POS: Vec2 = Vec2 { x: 0.75, y: 0.0 };
const COMBAT_CHASER_DUMMY_POS: Vec2 = Vec2 { x: 7.0, y: 0.0 };

include!("types.rs");
include!("systems.rs");
include!("scene_state.rs");
include!("scene_impl.rs");
include!("util.rs");

pub(crate) fn build_scene_pair() -> (Box<dyn Scene>, Box<dyn Scene>) {
    let scene_a = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
    let scene_b = GameplayScene::new("B", SceneKey::A, Vec2 { x: 2.0, y: 2.0 });
    (Box::new(scene_a), Box::new(scene_b))
}

#[cfg(test)]
mod tests {
    include!("tests.rs");
}
