use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;

use engine::{
    resolve_app_paths, run_app, screen_to_world_px, ContentPlanRequest, DebugInfoSnapshot,
    DebugJobState, DebugMarker, DebugMarkerKind, EntityArchetype, EntityDefId, EntityId,
    InputAction, InputSnapshot, Interactable, InteractableKind, LoopConfig, OrderState,
    RenderableDesc, RenderableKind, Scene, SceneCommand, SceneDebugCommand,
    SceneDebugCommandResult, SceneDebugContext, SceneKey, SceneWorld, Tilemap, Transform, Vec2,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplaySystemId {
    InputIntent,
    Interaction,
    AI,
    CombatResolution,
    StatusEffects,
    Cleanup,
}

impl GameplaySystemId {
    #[cfg(test)]
    fn name(self) -> &'static str {
        match self {
            Self::InputIntent => "InputIntent",
            Self::Interaction => "Interaction",
            Self::AI => "AI",
            Self::CombatResolution => "CombatResolution",
            Self::StatusEffects => "StatusEffects",
            Self::Cleanup => "Cleanup",
        }
    }
}

const GAMEPLAY_SYSTEM_ORDER: [GameplaySystemId; 6] = [
    GameplaySystemId::InputIntent,
    GameplaySystemId::Interaction,
    GameplaySystemId::AI,
    GameplaySystemId::CombatResolution,
    GameplaySystemId::StatusEffects,
    GameplaySystemId::Cleanup,
];

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
        target_save_id: u64,
        remaining_time: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SavedEntityRuntime {
    save_id: u64,
    position: SavedVec2,
    rotation_radians: Option<f32>,
    selectable: bool,
    actor: bool,
    move_target_world: Option<SavedVec2>,
    interaction_target_save_id: Option<u64>,
    job_state: SavedJobState,
    interactable: Option<SavedInteractableRuntime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SaveGame {
    save_version: u32,
    scene_key: SavedSceneKey,
    camera_position: SavedVec2,
    camera_zoom: f32,
    selected_entity_save_id: Option<u64>,
    player_entity_save_id: Option<u64>,
    next_save_id: u64,
    resource_count: u32,
    entities: Vec<SavedEntityRuntime>,
}

type SaveLoadResult<T> = Result<T, String>;

#[derive(Clone, Copy)]
struct WorldView<'a> {
    world: &'a SceneWorld,
}

impl<'a> WorldView<'a> {
    fn new(world: &'a SceneWorld) -> Self {
        Self { world }
    }

    fn camera(&self) -> &engine::Camera2D {
        self.world.camera()
    }

    fn find_entity(&self, id: EntityId) -> Option<&engine::Entity> {
        self.world.find_entity(id)
    }

    fn pick_topmost_interactable_at_cursor(
        &self,
        cursor_position_px: Vec2,
        window_size: (u32, u32),
    ) -> Option<EntityId> {
        self.world
            .pick_topmost_interactable_at_cursor(cursor_position_px, window_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StatusId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InteractionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveInteractionKind {
    Use,
    Attack,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveInteraction {
    actor_id: EntityId,
    target_id: EntityId,
    interaction_id: InteractionId,
    kind: ActiveInteractionKind,
    interaction_range: f32,
    duration_seconds: f32,
    remaining_seconds: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiState {
    Idle,
    Wander,
    Chase,
    UseInteraction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiAgent {
    state: AiState,
    home_position: Vec2,
    wander_target: Option<Vec2>,
    aggro_radius: f32,
    attack_range: f32,
    cooldown_seconds: f32,
    cooldown_remaining_seconds: f32,
}

impl AiAgent {
    fn from_home_position(home_position: Vec2) -> Self {
        Self {
            state: AiState::Idle,
            home_position,
            wander_target: None,
            aggro_radius: AI_AGGRO_RADIUS_UNITS,
            attack_range: AI_ATTACK_RANGE_UNITS,
            cooldown_seconds: AI_ATTACK_COOLDOWN_SECONDS,
            cooldown_remaining_seconds: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AiStateCounts {
    idle: u32,
    wander: u32,
    chase: u32,
    use_interaction: u32,
}

impl AiStateCounts {
    fn record(&mut self, state: AiState) {
        match state {
            AiState::Idle => self.idle = self.idle.saturating_add(1),
            AiState::Wander => self.wander = self.wander.saturating_add(1),
            AiState::Chase => self.chase = self.chase.saturating_add(1),
            AiState::UseInteraction => {
                self.use_interaction = self.use_interaction.saturating_add(1)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplayEvent {
    InteractionStarted {
        actor_id: EntityId,
        target_id: EntityId,
    },
    InteractionCompleted {
        actor_id: EntityId,
        target_id: EntityId,
    },
    EntityDamaged {
        entity_id: EntityId,
        amount: u32,
    },
    EntityDied {
        entity_id: EntityId,
    },
    StatusApplied {
        entity_id: EntityId,
        status_id: StatusId,
    },
    StatusExpired {
        entity_id: EntityId,
        status_id: StatusId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplayEventKind {
    InteractionStarted,
    InteractionCompleted,
    EntityDamaged,
    EntityDied,
    StatusApplied,
    StatusExpired,
}

impl GameplayEvent {
    fn kind(self) -> GameplayEventKind {
        match self {
            Self::InteractionStarted { .. } => GameplayEventKind::InteractionStarted,
            Self::InteractionCompleted { .. } => GameplayEventKind::InteractionCompleted,
            Self::EntityDamaged { .. } => GameplayEventKind::EntityDamaged,
            Self::EntityDied { .. } => GameplayEventKind::EntityDied,
            Self::StatusApplied { .. } => GameplayEventKind::StatusApplied,
            Self::StatusExpired { .. } => GameplayEventKind::StatusExpired,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GameplayEventCounts {
    total: u32,
    interaction_started: u32,
    interaction_completed: u32,
    entity_damaged: u32,
    entity_died: u32,
    status_applied: u32,
    status_expired: u32,
}

impl GameplayEventCounts {
    fn record(&mut self, kind: GameplayEventKind) {
        self.total = self.total.saturating_add(1);
        match kind {
            GameplayEventKind::InteractionStarted => {
                self.interaction_started = self.interaction_started.saturating_add(1)
            }
            GameplayEventKind::InteractionCompleted => {
                self.interaction_completed = self.interaction_completed.saturating_add(1)
            }
            GameplayEventKind::EntityDamaged => {
                self.entity_damaged = self.entity_damaged.saturating_add(1)
            }
            GameplayEventKind::EntityDied => self.entity_died = self.entity_died.saturating_add(1),
            GameplayEventKind::StatusApplied => {
                self.status_applied = self.status_applied.saturating_add(1)
            }
            GameplayEventKind::StatusExpired => {
                self.status_expired = self.status_expired.saturating_add(1)
            }
        }
    }
}

#[derive(Default)]
struct GameplayEventBus {
    current_tick_events: Vec<GameplayEvent>,
    last_tick_counts: GameplayEventCounts,
}

impl GameplayEventBus {
    fn clear_current_tick(&mut self) {
        self.current_tick_events.clear();
    }

    fn emit(&mut self, event: GameplayEvent) {
        self.current_tick_events.push(event);
    }

    fn iter_emitted_so_far(&self) -> impl Iterator<Item = &GameplayEvent> {
        self.current_tick_events.iter()
    }

    fn finish_tick_rollover(&mut self) {
        let mut counts = GameplayEventCounts::default();
        for event in &self.current_tick_events {
            counts.record(event.kind());
        }
        self.last_tick_counts = counts;
        self.current_tick_events.clear();
    }

    fn last_tick_counts(&self) -> GameplayEventCounts {
        self.last_tick_counts
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum GameplayIntent {
    SpawnByArchetypeId {
        archetype_id: EntityDefId,
        position: Vec2,
    },
    SetMoveTarget {
        actor_id: EntityId,
        point: Vec2,
    },
    DespawnEntity {
        entity_id: EntityId,
    },
    ApplyDamage {
        entity_id: EntityId,
        amount: u32,
    },
    AddStatus {
        entity_id: EntityId,
        status_id: StatusId,
    },
    RemoveStatus {
        entity_id: EntityId,
        status_id: StatusId,
    },
    StartInteraction {
        actor_id: EntityId,
        target_id: EntityId,
    },
    CancelInteraction {
        actor_id: EntityId,
    },
    CompleteInteraction {
        actor_id: EntityId,
        target_id: EntityId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplayIntentKind {
    SpawnByArchetypeId,
    SetMoveTarget,
    DespawnEntity,
    ApplyDamage,
    AddStatus,
    RemoveStatus,
    StartInteraction,
    CancelInteraction,
    CompleteInteraction,
}

impl GameplayIntent {
    fn kind(self) -> GameplayIntentKind {
        match self {
            Self::SpawnByArchetypeId { .. } => GameplayIntentKind::SpawnByArchetypeId,
            Self::SetMoveTarget { .. } => GameplayIntentKind::SetMoveTarget,
            Self::DespawnEntity { .. } => GameplayIntentKind::DespawnEntity,
            Self::ApplyDamage { .. } => GameplayIntentKind::ApplyDamage,
            Self::AddStatus { .. } => GameplayIntentKind::AddStatus,
            Self::RemoveStatus { .. } => GameplayIntentKind::RemoveStatus,
            Self::StartInteraction { .. } => GameplayIntentKind::StartInteraction,
            Self::CancelInteraction { .. } => GameplayIntentKind::CancelInteraction,
            Self::CompleteInteraction { .. } => GameplayIntentKind::CompleteInteraction,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GameplayIntentApplyStats {
    total: u32,
    spawn_by_archetype_id: u32,
    set_move_target: u32,
    despawn_entity: u32,
    apply_damage: u32,
    add_status: u32,
    remove_status: u32,
    start_interaction: u32,
    cancel_interaction: u32,
    complete_interaction: u32,
    invalid_target_count: u32,
    spawned_entity_ids: Vec<EntityId>,
}

impl GameplayIntentApplyStats {
    fn record_intent(&mut self, kind: GameplayIntentKind) {
        self.total = self.total.saturating_add(1);
        match kind {
            GameplayIntentKind::SpawnByArchetypeId => {
                self.spawn_by_archetype_id = self.spawn_by_archetype_id.saturating_add(1)
            }
            GameplayIntentKind::SetMoveTarget => {
                self.set_move_target = self.set_move_target.saturating_add(1)
            }
            GameplayIntentKind::DespawnEntity => {
                self.despawn_entity = self.despawn_entity.saturating_add(1)
            }
            GameplayIntentKind::ApplyDamage => {
                self.apply_damage = self.apply_damage.saturating_add(1)
            }
            GameplayIntentKind::AddStatus => self.add_status = self.add_status.saturating_add(1),
            GameplayIntentKind::RemoveStatus => {
                self.remove_status = self.remove_status.saturating_add(1)
            }
            GameplayIntentKind::StartInteraction => {
                self.start_interaction = self.start_interaction.saturating_add(1)
            }
            GameplayIntentKind::CancelInteraction => {
                self.cancel_interaction = self.cancel_interaction.saturating_add(1)
            }
            GameplayIntentKind::CompleteInteraction => {
                self.complete_interaction = self.complete_interaction.saturating_add(1)
            }
        }
    }

    fn record_invalid_target(&mut self) {
        self.invalid_target_count = self.invalid_target_count.saturating_add(1);
    }
}

#[derive(Default)]
struct GameplayIntentQueue {
    intents: Vec<GameplayIntent>,
    last_tick_apply_stats: GameplayIntentApplyStats,
}

impl GameplayIntentQueue {
    fn enqueue(&mut self, intent: GameplayIntent) {
        self.intents.push(intent);
    }

    fn drain_current_tick(&mut self) -> Vec<GameplayIntent> {
        std::mem::take(&mut self.intents)
    }

    fn set_last_tick_apply_stats(&mut self, stats: GameplayIntentApplyStats) {
        self.last_tick_apply_stats = stats;
    }

    fn last_tick_apply_stats(&self) -> &GameplayIntentApplyStats {
        &self.last_tick_apply_stats
    }
}

struct GameplaySystemContext<'a> {
    fixed_dt_seconds: f32,
    world_view: WorldView<'a>,
    input: &'a InputSnapshot,
    player_id: Option<EntityId>,
    selected_entity: Option<EntityId>,
    ai_agents_by_entity: &'a mut HashMap<EntityId, AiAgent>,
    active_interactions_by_actor: &'a mut HashMap<EntityId, ActiveInteraction>,
    next_interaction_id: &'a mut u64,
    selected_completion_enqueued_this_tick: &'a mut bool,
    events: &'a mut GameplayEventBus,
    intents: &'a mut GameplayIntentQueue,
}

#[derive(Default)]
struct GameplaySystemsHost {
    last_tick_order: Vec<GameplaySystemId>,
}

impl GameplaySystemsHost {
    fn run_once_per_tick(
        &mut self,
        fixed_dt_seconds: f32,
        world_view: WorldView<'_>,
        input: &InputSnapshot,
        player_id: Option<EntityId>,
        selected_entity: Option<EntityId>,
        ai_agents_by_entity: &mut HashMap<EntityId, AiAgent>,
        active_interactions_by_actor: &mut HashMap<EntityId, ActiveInteraction>,
        next_interaction_id: &mut u64,
        selected_completion_enqueued_this_tick: &mut bool,
        events: &mut GameplayEventBus,
        intents: &mut GameplayIntentQueue,
    ) {
        self.last_tick_order.clear();
        for system_id in GAMEPLAY_SYSTEM_ORDER {
            self.last_tick_order.push(system_id);
            let mut context = GameplaySystemContext {
                fixed_dt_seconds,
                world_view,
                input,
                player_id,
                selected_entity,
                ai_agents_by_entity,
                active_interactions_by_actor,
                next_interaction_id,
                selected_completion_enqueued_this_tick,
                events,
                intents,
            };
            self.run_system(system_id, &mut context);
        }
    }

    fn alloc_interaction_id(next_interaction_id: &mut u64) -> InteractionId {
        let id = *next_interaction_id;
        *next_interaction_id = next_interaction_id.saturating_add(1);
        InteractionId(id)
    }

    fn interaction_duration_seconds_for_use_target(target: &engine::Entity) -> f32 {
        if target.interactable.is_some() {
            JOB_DURATION_SECONDS
        } else {
            0.0
        }
    }

    fn interaction_range_for_use_target(target: &engine::Entity) -> Option<f32> {
        target
            .interactable
            .map(|interactable| interactable.interaction_radius)
    }

    fn within_distance_range(actor: &engine::Entity, target: &engine::Entity, range: f32) -> bool {
        let dx = target.transform.position.x - actor.transform.position.x;
        let dy = target.transform.position.y - actor.transform.position.y;
        let range_sq = range * range;
        dx * dx + dy * dy <= range_sq
    }

    fn order_state_indicates_interaction(order_state: OrderState) -> bool {
        matches!(
            order_state,
            OrderState::Interact { .. } | OrderState::Working { .. }
        )
    }

    fn deterministic_wander_target(
        home_position: Vec2,
        actor_id: EntityId,
        current_target: Option<Vec2>,
    ) -> Vec2 {
        let direction = if actor_id.0 % 2 == 0 { 1.0 } else { -1.0 };
        let primary = Vec2 {
            x: home_position.x + direction * AI_WANDER_OFFSET_UNITS,
            y: home_position.y,
        };
        let secondary = Vec2 {
            x: home_position.x - direction * AI_WANDER_OFFSET_UNITS,
            y: home_position.y,
        };

        if let Some(target) = current_target {
            let dx = target.x - primary.x;
            let dy = target.y - primary.y;
            if dx * dx + dy * dy <= 0.01 {
                return secondary;
            }
        }
        primary
    }

    fn run_input_intent_system(&self, context: &mut GameplaySystemContext<'_>) {
        if !context.input.right_click_pressed() {
            return;
        }
        let Some(actor_id) = context.selected_entity else {
            return;
        };
        let Some(cursor_px) = context.input.cursor_position_px() else {
            return;
        };
        let window_size = context.input.window_size();
        let Some(target_id) = context
            .world_view
            .pick_topmost_interactable_at_cursor(cursor_px, window_size)
        else {
            return;
        };

        let Some(actor) = context.world_view.find_entity(actor_id) else {
            return;
        };
        if !actor.actor {
            return;
        }
        let Some(target) = context.world_view.find_entity(target_id) else {
            return;
        };
        if target.interactable.is_none() {
            return;
        }

        if context.active_interactions_by_actor.contains_key(&actor_id) {
            context
                .intents
                .enqueue(GameplayIntent::CancelInteraction { actor_id });
        }

        let interaction_id = Self::alloc_interaction_id(context.next_interaction_id);
        let Some(interaction_range) = Self::interaction_range_for_use_target(target) else {
            return;
        };
        let duration_seconds = Self::interaction_duration_seconds_for_use_target(target);
        context.active_interactions_by_actor.insert(
            actor_id,
            ActiveInteraction {
                actor_id,
                target_id,
                interaction_id,
                kind: ActiveInteractionKind::Use,
                interaction_range,
                duration_seconds,
                remaining_seconds: None,
            },
        );
        context.events.emit(GameplayEvent::InteractionStarted {
            actor_id,
            target_id,
        });
        context.intents.enqueue(GameplayIntent::StartInteraction {
            actor_id,
            target_id,
        });
    }

    fn run_ai_system(&self, context: &mut GameplaySystemContext<'_>) {
        let mut actor_ids = context
            .ai_agents_by_entity
            .keys()
            .copied()
            .collect::<Vec<_>>();
        actor_ids.sort_by_key(|id| id.0);

        let player = context
            .player_id
            .and_then(|player_id| context.world_view.find_entity(player_id));

        for actor_id in actor_ids {
            let Some(mut agent) = context.ai_agents_by_entity.get(&actor_id).copied() else {
                continue;
            };
            let Some(actor) = context.world_view.find_entity(actor_id) else {
                context.ai_agents_by_entity.remove(&actor_id);
                continue;
            };
            if !actor.actor || Some(actor_id) == context.player_id {
                context.ai_agents_by_entity.remove(&actor_id);
                continue;
            }

            agent.cooldown_remaining_seconds =
                (agent.cooldown_remaining_seconds - context.fixed_dt_seconds).max(0.0);

            let has_runtime_interaction =
                context.active_interactions_by_actor.contains_key(&actor_id);
            let has_world_interaction = Self::order_state_indicates_interaction(actor.order_state);
            let movement_blocked = has_runtime_interaction || has_world_interaction;

            if let Some(player_entity) = player {
                let dx = player_entity.transform.position.x - actor.transform.position.x;
                let dy = player_entity.transform.position.y - actor.transform.position.y;
                let distance_sq = dx * dx + dy * dy;
                let aggro_sq = agent.aggro_radius * agent.aggro_radius;
                let in_aggro = distance_sq <= aggro_sq;
                let in_attack_range = distance_sq <= agent.attack_range * agent.attack_range;

                if in_aggro {
                    if in_attack_range {
                        agent.state = AiState::UseInteraction;
                        if !movement_blocked && agent.cooldown_remaining_seconds <= 0.0 {
                            let interaction_id =
                                Self::alloc_interaction_id(context.next_interaction_id);
                            context.active_interactions_by_actor.insert(
                                actor_id,
                                ActiveInteraction {
                                    actor_id,
                                    target_id: player_entity.id,
                                    interaction_id,
                                    kind: ActiveInteractionKind::Attack,
                                    interaction_range: agent.attack_range,
                                    duration_seconds: AI_ATTACK_INTERACTION_DURATION_SECONDS,
                                    remaining_seconds: None,
                                },
                            );
                            context.events.emit(GameplayEvent::InteractionStarted {
                                actor_id,
                                target_id: player_entity.id,
                            });
                            context.intents.enqueue(GameplayIntent::StartInteraction {
                                actor_id,
                                target_id: player_entity.id,
                            });
                            agent.cooldown_remaining_seconds = agent.cooldown_seconds;
                        }
                    } else {
                        agent.state = AiState::Chase;
                        if !movement_blocked {
                            context.intents.enqueue(GameplayIntent::SetMoveTarget {
                                actor_id,
                                point: player_entity.transform.position,
                            });
                        }
                    }

                    context.ai_agents_by_entity.insert(actor_id, agent);
                    continue;
                }
            }

            let wander_target = Self::deterministic_wander_target(
                agent.home_position,
                actor_id,
                agent.wander_target,
            );
            let dx = wander_target.x - actor.transform.position.x;
            let dy = wander_target.y - actor.transform.position.y;
            let arrived =
                dx * dx + dy * dy <= AI_WANDER_ARRIVAL_THRESHOLD * AI_WANDER_ARRIVAL_THRESHOLD;

            if arrived {
                agent.state = AiState::Idle;
                agent.wander_target = Some(wander_target);
            } else {
                agent.state = AiState::Wander;
                agent.wander_target = Some(wander_target);
                if !movement_blocked {
                    context.intents.enqueue(GameplayIntent::SetMoveTarget {
                        actor_id,
                        point: wander_target,
                    });
                }
            }

            context.ai_agents_by_entity.insert(actor_id, agent);
        }
    }

    fn run_interaction_system(&self, context: &mut GameplaySystemContext<'_>) {
        let mut actor_ids = context
            .active_interactions_by_actor
            .keys()
            .copied()
            .collect::<Vec<_>>();
        actor_ids.sort_by_key(|id| id.0);

        for actor_id in actor_ids {
            let Some(mut interaction) =
                context.active_interactions_by_actor.get(&actor_id).copied()
            else {
                continue;
            };
            let Some(actor) = context.world_view.find_entity(interaction.actor_id) else {
                context
                    .intents
                    .enqueue(GameplayIntent::CancelInteraction { actor_id });
                context.active_interactions_by_actor.remove(&actor_id);
                continue;
            };
            if !actor.actor {
                context
                    .intents
                    .enqueue(GameplayIntent::CancelInteraction { actor_id });
                context.active_interactions_by_actor.remove(&actor_id);
                continue;
            }
            let Some(target) = context.world_view.find_entity(interaction.target_id) else {
                context
                    .intents
                    .enqueue(GameplayIntent::CancelInteraction { actor_id });
                context.active_interactions_by_actor.remove(&actor_id);
                continue;
            };

            match interaction.kind {
                ActiveInteractionKind::Use => {
                    if target.interactable.is_none() {
                        context
                            .intents
                            .enqueue(GameplayIntent::CancelInteraction { actor_id });
                        context.active_interactions_by_actor.remove(&actor_id);
                        continue;
                    }
                    if let Some(range) = Self::interaction_range_for_use_target(target) {
                        interaction.interaction_range = range;
                    }
                }
                ActiveInteractionKind::Attack => {
                    if !target.actor {
                        context
                            .intents
                            .enqueue(GameplayIntent::CancelInteraction { actor_id });
                        context.active_interactions_by_actor.remove(&actor_id);
                        continue;
                    }
                }
            }

            let in_range =
                Self::within_distance_range(actor, target, interaction.interaction_range);
            if !in_range {
                if interaction.remaining_seconds.is_some() {
                    context
                        .intents
                        .enqueue(GameplayIntent::CancelInteraction { actor_id });
                    context.active_interactions_by_actor.remove(&actor_id);
                }
                continue;
            }

            if interaction.duration_seconds <= 0.0 {
                context.events.emit(GameplayEvent::InteractionCompleted {
                    actor_id: interaction.actor_id,
                    target_id: interaction.target_id,
                });
                context
                    .intents
                    .enqueue(GameplayIntent::CompleteInteraction {
                        actor_id: interaction.actor_id,
                        target_id: interaction.target_id,
                    });
                if context.selected_entity == Some(interaction.actor_id) {
                    *context.selected_completion_enqueued_this_tick = true;
                }
                context.active_interactions_by_actor.remove(&actor_id);
                continue;
            }

            let remaining = interaction
                .remaining_seconds
                .unwrap_or(interaction.duration_seconds)
                - context.fixed_dt_seconds;
            if remaining <= 0.0 {
                context.events.emit(GameplayEvent::InteractionCompleted {
                    actor_id: interaction.actor_id,
                    target_id: interaction.target_id,
                });
                context
                    .intents
                    .enqueue(GameplayIntent::CompleteInteraction {
                        actor_id: interaction.actor_id,
                        target_id: interaction.target_id,
                    });
                if context.selected_entity == Some(interaction.actor_id) {
                    *context.selected_completion_enqueued_this_tick = true;
                }
                context.active_interactions_by_actor.remove(&actor_id);
            } else {
                interaction.remaining_seconds = Some(remaining);
                context
                    .active_interactions_by_actor
                    .insert(actor_id, interaction);
            }
        }
    }

    fn run_system(&self, system_id: GameplaySystemId, context: &mut GameplaySystemContext<'_>) {
        match system_id {
            GameplaySystemId::InputIntent => {
                let _ = context.fixed_dt_seconds;
                let _ = context.world_view.camera().zoom;
                let _ = context.world_view.find_entity(EntityId(0));
                self.run_input_intent_system(context);
            }
            GameplaySystemId::Interaction => {
                self.run_interaction_system(context);
            }
            GameplaySystemId::AI => {
                self.run_ai_system(context);
            }
            GameplaySystemId::CombatResolution
            | GameplaySystemId::StatusEffects
            | GameplaySystemId::Cleanup => {
                let _ = context.input.quit_requested();
                if matches!(
                    system_id,
                    GameplaySystemId::StatusEffects | GameplaySystemId::Cleanup
                ) {
                    let _ = context.events.iter_emitted_so_far().count();
                }
            }
        }

        if cfg!(debug_assertions) {
            let event = match system_id {
                GameplaySystemId::InputIntent => GameplayEvent::StatusApplied {
                    entity_id: EntityId(0),
                    status_id: StatusId(1),
                },
                GameplaySystemId::Interaction => return,
                GameplaySystemId::AI => GameplayEvent::EntityDamaged {
                    entity_id: EntityId(1),
                    amount: 1,
                },
                GameplaySystemId::CombatResolution => GameplayEvent::EntityDied {
                    entity_id: EntityId(2),
                },
                GameplaySystemId::StatusEffects => GameplayEvent::StatusExpired {
                    entity_id: EntityId(0),
                    status_id: StatusId(1),
                },
                GameplaySystemId::Cleanup => return,
            };
            context.events.emit(event);
        }
    }
}

struct GameplayScene {
    scene_name: &'static str,
    switch_target: SceneKey,
    player_spawn: Vec2,
    player_id: Option<EntityId>,
    selected_entity: Option<EntityId>,
    player_move_speed: f32,
    resource_count: u32,
    interactable_cache: Vec<(EntityId, Vec2, f32)>,
    interactable_lookup_by_save_id: HashMap<u64, (EntityId, Vec2, f32)>,
    completed_target_ids: Vec<EntityId>,
    entity_save_ids: HashMap<EntityId, u64>,
    save_id_to_entity: HashMap<u64, EntityId>,
    next_save_id: u64,
    health_by_entity: HashMap<EntityId, u32>,
    status_ids_by_entity: HashMap<EntityId, Vec<StatusId>>,
    ai_agents_by_entity: HashMap<EntityId, AiAgent>,
    active_interactions_by_actor: HashMap<EntityId, ActiveInteraction>,
    next_interaction_id: u64,
    selected_completion_enqueued_this_tick: bool,
    systems_host: GameplaySystemsHost,
    system_events: GameplayEventBus,
    system_intents: GameplayIntentQueue,
    system_order_text: String,
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
            interactable_lookup_by_save_id: HashMap::new(),
            completed_target_ids: Vec::new(),
            entity_save_ids: HashMap::new(),
            save_id_to_entity: HashMap::new(),
            next_save_id: 0,
            health_by_entity: HashMap::new(),
            status_ids_by_entity: HashMap::new(),
            ai_agents_by_entity: HashMap::new(),
            active_interactions_by_actor: HashMap::new(),
            next_interaction_id: 0,
            selected_completion_enqueued_this_tick: false,
            systems_host: GameplaySystemsHost::default(),
            system_events: GameplayEventBus::default(),
            system_intents: GameplayIntentQueue::default(),
            system_order_text: String::new(),
        }
    }

    fn scene_key(&self) -> SceneKey {
        match self.switch_target {
            SceneKey::A => SceneKey::B,
            SceneKey::B => SceneKey::A,
        }
    }

    fn alloc_next_save_id(&mut self) -> SaveLoadResult<u64> {
        let save_id = self.next_save_id;
        self.next_save_id = self
            .next_save_id
            .checked_add(1)
            .ok_or_else(|| "save_id allocator overflow".to_string())?;
        Ok(save_id)
    }

    fn rebuild_reverse_save_id_map(&mut self) {
        self.save_id_to_entity.clear();
        for (entity_id, save_id) in &self.entity_save_ids {
            self.save_id_to_entity.insert(*save_id, *entity_id);
        }
    }

    fn remove_entity_save_mapping(&mut self, entity_id: EntityId) {
        if let Some(save_id) = self.entity_save_ids.remove(&entity_id) {
            self.save_id_to_entity.remove(&save_id);
        }
    }

    fn save_id_for_entity(&self, entity_id: EntityId) -> Option<u64> {
        self.entity_save_ids.get(&entity_id).copied()
    }

    fn resolve_runtime_target_id(
        &self,
        target_save_id: u64,
        world: &SceneWorld,
    ) -> Option<EntityId> {
        let target_id = self.save_id_to_entity.get(&target_save_id).copied()?;
        world.find_entity(target_id).map(|_| target_id)
    }

    fn sync_save_id_map_with_world(&mut self, world: &SceneWorld) -> SaveLoadResult<()> {
        let live_ids: Vec<EntityId> = world.entities().iter().map(|entity| entity.id).collect();
        let live_id_set: HashSet<EntityId> = live_ids.iter().copied().collect();
        self.entity_save_ids
            .retain(|entity_id, _| live_id_set.contains(entity_id));

        let mut missing_ids: Vec<EntityId> = live_ids
            .into_iter()
            .filter(|entity_id| !self.entity_save_ids.contains_key(entity_id))
            .collect();
        missing_ids.sort_by_key(|entity_id| entity_id.0);

        for entity_id in missing_ids {
            let save_id = self.alloc_next_save_id()?;
            self.entity_save_ids.insert(entity_id, save_id);
        }
        self.rebuild_reverse_save_id_map();
        Ok(())
    }

    fn rebuild_save_id_map_from_loaded(
        &mut self,
        world: &SceneWorld,
        spawned_ids_by_save_id: &HashMap<u64, EntityId>,
        loaded_next_save_id: u64,
    ) -> SaveLoadResult<()> {
        if let Some(max_used_save_id) = spawned_ids_by_save_id.keys().copied().max() {
            if loaded_next_save_id <= max_used_save_id {
                return Err(format!(
                    "invalid next_save_id {}: must be greater than max used save_id {}",
                    loaded_next_save_id, max_used_save_id
                ));
            }
        } else if loaded_next_save_id != 0 {
            return Err(format!(
                "invalid next_save_id {}: expected 0 for empty entity save",
                loaded_next_save_id
            ));
        }

        self.entity_save_ids.clear();
        for (save_id, entity_id) in spawned_ids_by_save_id {
            self.entity_save_ids.insert(*entity_id, *save_id);
        }
        self.rebuild_reverse_save_id_map();

        if self.entity_save_ids.len() != world.entity_count() {
            return Err(format!(
                "loaded save_id map size {} does not match world entity count {}",
                self.entity_save_ids.len(),
                world.entity_count()
            ));
        }

        self.next_save_id = loaded_next_save_id;
        Ok(())
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

    fn save_to_disk(&mut self, world: &SceneWorld) -> SaveLoadResult<PathBuf> {
        let save = self.build_save_game(world)?;
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
        let save = Self::parse_save_game_json(&raw)?;
        Self::validate_save_game(&save, expected_scene)?;
        Ok(save)
    }

    fn parse_save_game_json(raw: &str) -> SaveLoadResult<SaveGame> {
        let mut deserializer = serde_json::Deserializer::from_str(raw);
        match serde_path_to_error::deserialize::<_, SaveGame>(&mut deserializer) {
            Ok(save) => Ok(save),
            Err(error) => {
                let path = error.path().to_string();
                let source = error.into_inner();
                if path.is_empty() || path == "." {
                    Err(format!("parse save json: {source}"))
                } else {
                    Err(format!("parse save json at {path}: {source}"))
                }
            }
        }
    }

    fn validation_err(path: &str, message: impl Into<String>) -> String {
        format!("validation failed at {path}: {}", message.into())
    }

    fn expected_actual(path: &str, expected: impl Display, actual: impl Display) -> String {
        Self::validation_err(path, format!("expected {expected}, got {actual}"))
    }

    fn validate_save_game(save: &SaveGame, expected_scene: SavedSceneKey) -> SaveLoadResult<()> {
        if save.save_version != SAVE_VERSION {
            return Err(Self::expected_actual(
                "save_version",
                SAVE_VERSION,
                save.save_version,
            ));
        }
        if save.scene_key != expected_scene {
            return Err(Self::expected_actual(
                "scene_key",
                format!("{expected_scene:?}"),
                format!("{:?}", save.scene_key),
            ));
        }
        if !save.camera_position.x.is_finite() {
            return Err(Self::expected_actual(
                "camera_position.x",
                "finite number",
                save.camera_position.x,
            ));
        }
        if !save.camera_position.y.is_finite() {
            return Err(Self::expected_actual(
                "camera_position.y",
                "finite number",
                save.camera_position.y,
            ));
        }
        if !save.camera_zoom.is_finite() {
            return Err(Self::expected_actual(
                "camera_zoom",
                "finite number",
                save.camera_zoom,
            ));
        }

        let mut known_save_ids = HashMap::with_capacity(save.entities.len());
        for (index, entity) in save.entities.iter().enumerate() {
            let save_id_path = format!("entities[{index}].save_id");
            if let Some(first_index) = known_save_ids.insert(entity.save_id, index) {
                return Err(Self::validation_err(
                    &save_id_path,
                    format!(
                        "duplicate save_id {} (first seen at entities[{first_index}].save_id)",
                        entity.save_id
                    ),
                ));
            }

            let pos_x_path = format!("entities[{index}].position.x");
            let pos_y_path = format!("entities[{index}].position.y");
            if !entity.position.x.is_finite() {
                return Err(Self::expected_actual(
                    &pos_x_path,
                    "finite number",
                    entity.position.x,
                ));
            }
            if !entity.position.y.is_finite() {
                return Err(Self::expected_actual(
                    &pos_y_path,
                    "finite number",
                    entity.position.y,
                ));
            }

            if let Some(rotation_radians) = entity.rotation_radians {
                let path = format!("entities[{index}].rotation_radians");
                if !rotation_radians.is_finite() {
                    return Err(Self::expected_actual(
                        &path,
                        "finite number",
                        rotation_radians,
                    ));
                }
            }

            if let Some(move_target_world) = entity.move_target_world {
                let move_x_path = format!("entities[{index}].move_target_world.x");
                let move_y_path = format!("entities[{index}].move_target_world.y");
                if !move_target_world.x.is_finite() {
                    return Err(Self::expected_actual(
                        &move_x_path,
                        "finite number",
                        move_target_world.x,
                    ));
                }
                if !move_target_world.y.is_finite() {
                    return Err(Self::expected_actual(
                        &move_y_path,
                        "finite number",
                        move_target_world.y,
                    ));
                }
            }

            if let Some(interactable) = entity.interactable {
                let radius_path = format!("entities[{index}].interactable.interaction_radius");
                if !interactable.interaction_radius.is_finite() {
                    return Err(Self::expected_actual(
                        &radius_path,
                        "finite number",
                        interactable.interaction_radius,
                    ));
                }
                if interactable.interaction_radius < 0.0 {
                    return Err(Self::expected_actual(
                        &radius_path,
                        ">= 0",
                        interactable.interaction_radius,
                    ));
                }
            }

            if let SavedJobState::Working { remaining_time, .. } = entity.job_state {
                let path = format!("entities[{index}].job_state.remaining_time");
                if !remaining_time.is_finite() {
                    return Err(Self::expected_actual(
                        &path,
                        "finite number",
                        remaining_time,
                    ));
                }
                if remaining_time < 0.0 {
                    return Err(Self::expected_actual(&path, ">= 0", remaining_time));
                }
            }
        }
        let known_save_ids = known_save_ids.keys().copied().collect::<HashSet<_>>();

        if let Some(selected_save_id) = save.selected_entity_save_id {
            if !known_save_ids.contains(&selected_save_id) {
                return Err(Self::validation_err(
                    "selected_entity_save_id",
                    format!("references unknown save_id {selected_save_id}"),
                ));
            }
        }
        if let Some(player_save_id) = save.player_entity_save_id {
            if !known_save_ids.contains(&player_save_id) {
                return Err(Self::validation_err(
                    "player_entity_save_id",
                    format!("references unknown save_id {player_save_id}"),
                ));
            }
        }

        for (index, entity) in save.entities.iter().enumerate() {
            if let Some(target_save_id) = entity.interaction_target_save_id {
                if !known_save_ids.contains(&target_save_id) {
                    let path = format!("entities[{index}].interaction_target_save_id");
                    return Err(Self::validation_err(
                        &path,
                        format!("references unknown save_id {target_save_id}"),
                    ));
                }
            }
            if let SavedJobState::Working { target_save_id, .. } = entity.job_state {
                if !known_save_ids.contains(&target_save_id) {
                    let path = format!("entities[{index}].job_state.target_save_id");
                    return Err(Self::validation_err(
                        &path,
                        format!("references unknown save_id {target_save_id}"),
                    ));
                }
            }
        }

        match save.entities.iter().map(|entity| entity.save_id).max() {
            Some(max_used_save_id) => {
                if save.next_save_id <= max_used_save_id {
                    return Err(Self::validation_err(
                        "next_save_id",
                        format!(
                            "expected value greater than max used save_id {max_used_save_id}, got {}",
                            save.next_save_id
                        ),
                    ));
                }
            }
            None => {
                if save.next_save_id != 0 {
                    return Err(Self::expected_actual("next_save_id", 0, save.next_save_id));
                }
            }
        }

        Ok(())
    }

    fn saved_order_fields_from_runtime(
        order_state: OrderState,
    ) -> (Option<SavedVec2>, Option<u64>, SavedJobState) {
        match order_state {
            OrderState::Idle => (None, None, SavedJobState::Idle),
            OrderState::MoveTo { point } => {
                (Some(SavedVec2::from_vec2(point)), None, SavedJobState::Idle)
            }
            OrderState::Interact { target_save_id } => {
                (None, Some(target_save_id), SavedJobState::Idle)
            }
            OrderState::Working {
                target_save_id,
                remaining_time,
            } => (
                None,
                Some(target_save_id),
                SavedJobState::Working {
                    target_save_id,
                    remaining_time,
                },
            ),
        }
    }

    fn runtime_order_state_from_saved(entity: &SavedEntityRuntime) -> OrderState {
        match entity.job_state {
            SavedJobState::Working {
                target_save_id,
                remaining_time,
            } => OrderState::Working {
                target_save_id,
                remaining_time,
            },
            SavedJobState::Idle => {
                if let Some(target_save_id) = entity.interaction_target_save_id {
                    OrderState::Interact { target_save_id }
                } else if let Some(point) = entity.move_target_world {
                    OrderState::MoveTo {
                        point: point.to_vec2(),
                    }
                } else {
                    OrderState::Idle
                }
            }
        }
    }

    fn build_save_game(&mut self, world: &SceneWorld) -> SaveLoadResult<SaveGame> {
        self.sync_save_id_map_with_world(world)?;

        let entities = world
            .entities()
            .iter()
            .map(|entity| {
                let save_id = self
                    .entity_save_ids
                    .get(&entity.id)
                    .copied()
                    .ok_or_else(|| {
                        format!("missing save_id mapping for entity id {}", entity.id.0)
                    })?;
                let (move_target_world, interaction_target_save_id, job_state) =
                    Self::saved_order_fields_from_runtime(entity.order_state);

                Ok(SavedEntityRuntime {
                    save_id,
                    position: SavedVec2::from_vec2(entity.transform.position),
                    rotation_radians: entity.transform.rotation_radians,
                    selectable: entity.selectable,
                    actor: entity.actor,
                    move_target_world,
                    interaction_target_save_id,
                    job_state,
                    interactable: entity.interactable.map(|interactable| {
                        SavedInteractableRuntime {
                            kind: match interactable.kind {
                                InteractableKind::ResourcePile => {
                                    SavedInteractableKind::ResourcePile
                                }
                            },
                            interaction_radius: interactable.interaction_radius,
                            remaining_uses: interactable.remaining_uses,
                        }
                    }),
                })
            })
            .collect::<SaveLoadResult<Vec<_>>>()?;

        Ok(SaveGame {
            save_version: SAVE_VERSION,
            scene_key: SavedSceneKey::from_scene_key(self.scene_key()),
            camera_position: SavedVec2::from_vec2(world.camera().position),
            camera_zoom: world.camera().zoom,
            selected_entity_save_id: self
                .selected_entity
                .and_then(|id| self.entity_save_ids.get(&id).copied()),
            player_entity_save_id: self
                .player_id
                .and_then(|id| self.entity_save_ids.get(&id).copied()),
            next_save_id: self.next_save_id,
            resource_count: self.resource_count,
            entities,
        })
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
        self.interactable_lookup_by_save_id.clear();
        self.completed_target_ids.clear();
        self.entity_save_ids.clear();
        self.save_id_to_entity.clear();
        self.reset_runtime_component_stores();
        world.camera_mut().position = save.camera_position.to_vec2();
        world.camera_mut().set_zoom_clamped(save.camera_zoom);

        let mut spawned_ids = Vec::with_capacity(save.entities.len());
        let mut spawned_ids_by_save_id = HashMap::with_capacity(save.entities.len());
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
            if spawned_ids_by_save_id
                .insert(saved_entity.save_id, id)
                .is_some()
            {
                return Err(format!(
                    "duplicate save_id {} encountered while applying save",
                    saved_entity.save_id
                ));
            }
        }
        world.apply_pending();

        for (saved_entity, id) in save.entities.iter().zip(spawned_ids.into_iter()) {
            let Some(entity) = world.find_entity_mut(id) else {
                return Err(format!(
                    "spawned entity missing for save_id {}",
                    saved_entity.save_id
                ));
            };

            entity.transform.position = saved_entity.position.to_vec2();
            entity.transform.rotation_radians = saved_entity.rotation_radians;
            entity.selectable = saved_entity.selectable;
            entity.actor = saved_entity.actor;
            entity.order_state = Self::runtime_order_state_from_saved(saved_entity);
            entity.interactable = saved_entity.interactable.map(|interactable| Interactable {
                kind: match interactable.kind {
                    SavedInteractableKind::ResourcePile => InteractableKind::ResourcePile,
                },
                interaction_radius: interactable.interaction_radius,
                remaining_uses: interactable.remaining_uses,
            });
        }

        self.selected_entity = save
            .selected_entity_save_id
            .and_then(|save_id| spawned_ids_by_save_id.get(&save_id).copied());
        self.player_id = save
            .player_entity_save_id
            .and_then(|save_id| spawned_ids_by_save_id.get(&save_id).copied());
        self.rebuild_save_id_map_from_loaded(world, &spawned_ids_by_save_id, save.next_save_id)?;
        self.resource_count = save.resource_count;
        self.sync_runtime_component_stores_with_world(world);
        self.rebuild_active_interactions_from_world_order(world);
        self.rebuild_ai_agents_from_world(world);
        Ok(())
    }
}

impl GameplayScene {
    fn reset_runtime_component_stores(&mut self) {
        self.health_by_entity.clear();
        self.status_ids_by_entity.clear();
        self.ai_agents_by_entity.clear();
        self.active_interactions_by_actor.clear();
        self.next_interaction_id = 0;
    }

    fn sync_runtime_component_stores_with_world(&mut self, world: &SceneWorld) {
        let live_ids: HashSet<EntityId> = world.entities().iter().map(|entity| entity.id).collect();
        self.health_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));
        self.status_ids_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));

        for entity in world.entities() {
            self.health_by_entity.entry(entity.id).or_insert(100);
            self.status_ids_by_entity.entry(entity.id).or_default();
        }
    }

    fn rebuild_ai_agents_from_world(&mut self, world: &SceneWorld) {
        self.ai_agents_by_entity.clear();
        for entity in world.entities() {
            if !entity.actor || Some(entity.id) == self.player_id {
                continue;
            }
            self.ai_agents_by_entity.insert(
                entity.id,
                AiAgent::from_home_position(entity.transform.position),
            );
        }
    }

    fn ai_state_counts(&self) -> AiStateCounts {
        let mut counts = AiStateCounts::default();
        for agent in self.ai_agents_by_entity.values() {
            counts.record(agent.state);
        }
        counts
    }

    fn rebuild_active_interactions_from_world_order(&mut self, world: &SceneWorld) {
        self.active_interactions_by_actor.clear();
        for entity in world.entities() {
            if !entity.actor {
                continue;
            }

            let (target_save_id, remaining_seconds) = match entity.order_state {
                OrderState::Interact { target_save_id } => (target_save_id, None),
                OrderState::Working {
                    target_save_id,
                    remaining_time,
                } => (target_save_id, Some(remaining_time.max(0.0))),
                _ => continue,
            };
            let Some(target_id) = self.resolve_runtime_target_id(target_save_id, world) else {
                continue;
            };
            let Some(target) = world.find_entity(target_id) else {
                continue;
            };
            let (kind, duration_seconds, interaction_range) = if target.interactable.is_some() {
                (
                    ActiveInteractionKind::Use,
                    GameplaySystemsHost::interaction_duration_seconds_for_use_target(target),
                    GameplaySystemsHost::interaction_range_for_use_target(target)
                        .unwrap_or(RESOURCE_PILE_INTERACTION_RADIUS),
                )
            } else if target.actor {
                (
                    ActiveInteractionKind::Attack,
                    AI_ATTACK_INTERACTION_DURATION_SECONDS,
                    AI_ATTACK_RANGE_UNITS,
                )
            } else {
                continue;
            };
            let interaction_id =
                GameplaySystemsHost::alloc_interaction_id(&mut self.next_interaction_id);
            self.active_interactions_by_actor.insert(
                entity.id,
                ActiveInteraction {
                    actor_id: entity.id,
                    target_id,
                    interaction_id,
                    kind,
                    interaction_range,
                    duration_seconds,
                    remaining_seconds,
                },
            );
        }
    }

    fn run_gameplay_systems_once(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &SceneWorld,
    ) {
        self.system_events.clear_current_tick();
        self.selected_completion_enqueued_this_tick = false;
        self.systems_host.run_once_per_tick(
            fixed_dt_seconds,
            WorldView::new(world),
            input,
            self.player_id,
            self.selected_entity,
            &mut self.ai_agents_by_entity,
            &mut self.active_interactions_by_actor,
            &mut self.next_interaction_id,
            &mut self.selected_completion_enqueued_this_tick,
            &mut self.system_events,
            &mut self.system_intents,
        );
    }

    fn apply_gameplay_intents_at_safe_point(
        &mut self,
        intents: Vec<GameplayIntent>,
        world: &mut SceneWorld,
    ) -> GameplayIntentApplyStats {
        let mut stats = GameplayIntentApplyStats::default();

        for intent in intents {
            stats.record_intent(intent.kind());
            match intent {
                GameplayIntent::SpawnByArchetypeId {
                    archetype_id,
                    position,
                } => {
                    let Some(def_db) = world.def_database() else {
                        stats.record_invalid_target();
                        continue;
                    };
                    let Some(archetype) = def_db.entity_def(archetype_id).cloned() else {
                        stats.record_invalid_target();
                        continue;
                    };

                    let has_actor_tag = archetype.tags.iter().any(|tag| tag == "actor");
                    let has_interactable_tag =
                        archetype.tags.iter().any(|tag| tag == "interactable");
                    let entity_id = if has_actor_tag {
                        world.spawn_actor(
                            Transform {
                                position,
                                rotation_radians: None,
                            },
                            RenderableDesc {
                                kind: archetype.renderable,
                                debug_name: "intent_spawn_actor",
                            },
                        )
                    } else {
                        world.spawn_selectable(
                            Transform {
                                position,
                                rotation_radians: None,
                            },
                            RenderableDesc {
                                kind: archetype.renderable,
                                debug_name: "intent_spawn",
                            },
                        )
                    };
                    world.apply_pending();
                    stats.spawned_entity_ids.push(entity_id);

                    match self.alloc_next_save_id() {
                        Ok(save_id) => {
                            self.entity_save_ids.insert(entity_id, save_id);
                            self.save_id_to_entity.insert(save_id, entity_id);
                        }
                        Err(_) => stats.record_invalid_target(),
                    }
                    self.health_by_entity.entry(entity_id).or_insert(100);
                    self.status_ids_by_entity.entry(entity_id).or_default();

                    if has_interactable_tag {
                        if let Some(entity) = world.find_entity_mut(entity_id) {
                            entity.interactable = Some(Interactable {
                                kind: InteractableKind::ResourcePile,
                                interaction_radius: RESOURCE_PILE_INTERACTION_RADIUS,
                                remaining_uses: RESOURCE_PILE_STARTING_USES,
                            });
                        }
                    }

                    if has_actor_tag && Some(entity_id) != self.player_id {
                        self.ai_agents_by_entity
                            .insert(entity_id, AiAgent::from_home_position(position));
                    }
                }
                GameplayIntent::SetMoveTarget { actor_id, point } => {
                    let Some(actor) = world.find_entity_mut(actor_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    if !actor.actor {
                        stats.record_invalid_target();
                        continue;
                    }
                    actor.order_state = OrderState::MoveTo { point };
                }
                GameplayIntent::DespawnEntity { entity_id } => {
                    if !world.despawn(entity_id) {
                        stats.record_invalid_target();
                        continue;
                    }
                    self.remove_entity_save_mapping(entity_id);
                    if self.selected_entity == Some(entity_id) {
                        self.selected_entity = None;
                    }
                    if self.player_id == Some(entity_id) {
                        self.player_id = None;
                    }
                    self.health_by_entity.remove(&entity_id);
                    self.status_ids_by_entity.remove(&entity_id);
                    self.ai_agents_by_entity.remove(&entity_id);
                    self.active_interactions_by_actor.remove(&entity_id);
                }
                GameplayIntent::ApplyDamage { entity_id, amount } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    let Some(health) = self.health_by_entity.get_mut(&entity_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    *health = health.saturating_sub(amount);
                }
                GameplayIntent::AddStatus {
                    entity_id,
                    status_id,
                } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    let statuses = self.status_ids_by_entity.entry(entity_id).or_default();
                    if !statuses.contains(&status_id) {
                        statuses.push(status_id);
                    }
                }
                GameplayIntent::RemoveStatus {
                    entity_id,
                    status_id,
                } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    if let Some(statuses) = self.status_ids_by_entity.get_mut(&entity_id) {
                        statuses.retain(|id| *id != status_id);
                    }
                }
                GameplayIntent::StartInteraction {
                    actor_id,
                    target_id,
                } => {
                    if world.find_entity(actor_id).is_none()
                        || world.find_entity(target_id).is_none()
                    {
                        stats.record_invalid_target();
                        continue;
                    }
                    let Some(target_save_id) = self.save_id_for_entity(target_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    let Some(actor) = world.find_entity_mut(actor_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    if !actor.actor {
                        stats.record_invalid_target();
                        continue;
                    }
                    actor.order_state = OrderState::Interact { target_save_id };
                }
                GameplayIntent::CancelInteraction { actor_id } => {
                    let Some(actor) = world.find_entity_mut(actor_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    if !actor.actor {
                        stats.record_invalid_target();
                        continue;
                    }
                    actor.order_state = OrderState::Idle;
                }
                GameplayIntent::CompleteInteraction {
                    actor_id,
                    target_id,
                } => {
                    if world.find_entity(actor_id).is_none()
                        || world.find_entity(target_id).is_none()
                    {
                        stats.record_invalid_target();
                        continue;
                    }
                    let Some(actor) = world.find_entity_mut(actor_id) else {
                        stats.record_invalid_target();
                        continue;
                    };
                    if !actor.actor {
                        stats.record_invalid_target();
                        continue;
                    }
                    actor.order_state = OrderState::Idle;

                    // Mechanical interaction outcomes for resource piles:
                    // successful completion grants one item and consumes one use.
                    let mut should_despawn_target = false;
                    if let Some(target) = world.find_entity_mut(target_id) {
                        if let Some(interactable) = target.interactable.as_mut() {
                            if interactable.remaining_uses > 0 {
                                interactable.remaining_uses -= 1;
                                self.resource_count = self.resource_count.saturating_add(1);
                            }
                            should_despawn_target = interactable.remaining_uses == 0;
                        }
                    }

                    if should_despawn_target {
                        if world.despawn(target_id) {
                            self.remove_entity_save_mapping(target_id);
                        }
                        self.health_by_entity.remove(&target_id);
                        self.status_ids_by_entity.remove(&target_id);
                        self.ai_agents_by_entity.remove(&target_id);
                        self.active_interactions_by_actor.remove(&target_id);
                    }
                }
            }
        }

        stats
    }

    fn apply_system_outputs(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &mut SceneWorld,
    ) {
        let intents = self.system_intents.drain_current_tick();
        let stats = self.apply_gameplay_intents_at_safe_point(intents, world);
        self.system_intents.set_last_tick_apply_stats(stats);
        self.ensure_authoritative_player_exists_if_missing(world);
        self.apply_gameplay_tick_at_safe_point(fixed_dt_seconds, input, world);
        self.system_events.finish_tick_rollover();
    }

    fn ensure_authoritative_player_exists_if_missing(&mut self, world: &mut SceneWorld) {
        if let Some(player_id) = self.player_id {
            if world.find_entity(player_id).is_some()
                || self.entity_save_ids.contains_key(&player_id)
            {
                self.ai_agents_by_entity.remove(&player_id);
                return;
            }
            self.player_id = None;
        }
        if world.def_database().is_none() {
            return;
        }

        let player_archetype = resolve_player_archetype(world);
        let player_id = world.spawn_actor(
            Transform {
                position: self.player_spawn,
                rotation_radians: None,
            },
            RenderableDesc {
                kind: player_archetype.renderable,
                debug_name: "player_auto",
            },
        );

        match self.alloc_next_save_id() {
            Ok(save_id) => {
                self.entity_save_ids.insert(player_id, save_id);
                self.save_id_to_entity.insert(save_id, player_id);
            }
            Err(error) => {
                warn!(
                    scene = self.scene_name,
                    error = %error,
                    "failed_to_allocate_save_id_for_auto_player"
                );
            }
        }

        self.health_by_entity.entry(player_id).or_insert(100);
        self.status_ids_by_entity.entry(player_id).or_default();
        self.ai_agents_by_entity.remove(&player_id);
        self.player_id = Some(player_id);
        info!(
            scene = self.scene_name,
            player_id = player_id.0,
            "authoritative_player_auto_spawned"
        );
    }

    fn apply_gameplay_tick_at_safe_point(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &mut SceneWorld,
    ) {
        world
            .camera_mut()
            .apply_zoom_steps(input.zoom_delta_steps());
        world.tick_debug_markers(fixed_dt_seconds);
        let hovered_interactable = input.cursor_position_px().and_then(|cursor_px| {
            world.pick_topmost_interactable_at_cursor(cursor_px, input.window_size())
        });

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
                let interactable_target = hovered_interactable.and_then(|id| {
                    world
                        .find_entity(id)
                        .map(|entity| entity.transform.position)
                });

                let mut marker_position = None::<Vec2>;
                if let Some(entity) = world.find_entity_mut(selected_id) {
                    if entity.actor {
                        if let Some(target_world) = interactable_target {
                            marker_position = Some(target_world);
                        } else {
                            entity.order_state = OrderState::MoveTo {
                                point: ground_target,
                            };
                            marker_position = Some(ground_target);
                        }
                    }
                }
                if let Some(position_world) = marker_position {
                    world.push_debug_marker(DebugMarker {
                        kind: DebugMarkerKind::Order,
                        position_world,
                        ttl_seconds: ORDER_MARKER_TTL_SECONDS,
                    });
                }
            }
        }

        world.set_hovered_interactable_visual(hovered_interactable);

        if let Some(current_selected) = world.visual_state().selected_actor {
            let stale_or_non_actor = match world.find_entity(current_selected) {
                Some(entity) => !entity.actor,
                None => true,
            };
            if stale_or_non_actor {
                world.set_selected_actor_visual(None);
            }
        }
        let selected_actor_visual = self.selected_entity.and_then(|id| {
            world
                .find_entity(id)
                .filter(|entity| entity.actor)
                .map(|_| id)
        });
        world.set_selected_actor_visual(selected_actor_visual);

        if let Some(player_id) = self.player_id {
            if let Some(player) = world.find_entity_mut(player_id) {
                let delta = movement_delta(input, fixed_dt_seconds, self.player_move_speed);
                player.transform.position.x += delta.x;
                player.transform.position.y += delta.y;
            }
        }

        self.interactable_cache.clear();
        self.interactable_lookup_by_save_id.clear();
        for entity in world.entities() {
            if let Some(interactable) = entity.interactable {
                if interactable.remaining_uses > 0 {
                    self.interactable_cache.push((
                        entity.id,
                        entity.transform.position,
                        interactable.interaction_radius,
                    ));
                    if let Some(target_save_id) = self.entity_save_ids.get(&entity.id).copied() {
                        self.interactable_lookup_by_save_id.insert(
                            target_save_id,
                            (
                                entity.id,
                                entity.transform.position,
                                interactable.interaction_radius,
                            ),
                        );
                    }
                }
            }
        }

        self.completed_target_ids.clear();
        let completed_jobs = 0u32;
        for entity in world.entities_mut() {
            if !entity.actor {
                continue;
            }

            match entity.order_state {
                OrderState::Idle => {}
                OrderState::MoveTo { point } => {
                    let (next, arrived) = step_toward(
                        entity.transform.position,
                        point,
                        self.player_move_speed,
                        fixed_dt_seconds,
                        MOVE_ARRIVAL_THRESHOLD,
                    );
                    entity.transform.position = next;
                    if arrived {
                        entity.order_state = OrderState::Idle;
                    }
                }
                OrderState::Interact { target_save_id } => {
                    if let Some((_, target_world, radius)) = self
                        .interactable_lookup_by_save_id
                        .get(&target_save_id)
                        .copied()
                    {
                        let dx = target_world.x - entity.transform.position.x;
                        let dy = target_world.y - entity.transform.position.y;
                        if dx * dx + dy * dy <= radius * radius {
                            entity.order_state = OrderState::Working {
                                target_save_id,
                                remaining_time: JOB_DURATION_SECONDS,
                            };
                        } else {
                            let (next, _) = step_toward(
                                entity.transform.position,
                                target_world,
                                self.player_move_speed,
                                fixed_dt_seconds,
                                MOVE_ARRIVAL_THRESHOLD,
                            );
                            entity.transform.position = next;
                        }
                    } else {
                        entity.order_state = OrderState::Idle;
                    }
                }
                OrderState::Working { target_save_id, .. } => {
                    if self
                        .interactable_lookup_by_save_id
                        .get(&target_save_id)
                        .copied()
                        .is_none()
                    {
                        entity.order_state = OrderState::Idle;
                    }
                }
            }
        }

        self.resource_count = self.resource_count.saturating_add(completed_jobs);
        for index in 0..self.completed_target_ids.len() {
            let target_id = self.completed_target_ids[index];
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
                self.remove_entity_save_mapping(target_id);
            }
        }

        let camera_delta = camera_delta(input, fixed_dt_seconds, CAMERA_SPEED_UNITS_PER_SECOND);
        world.camera_mut().position.x += camera_delta.x;
        world.camera_mut().position.y += camera_delta.y;
    }
}

impl Scene for GameplayScene {
    fn load(&mut self, world: &mut SceneWorld) {
        self.entity_save_ids.clear();
        self.save_id_to_entity.clear();
        self.next_save_id = 0;
        self.reset_runtime_component_stores();
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
        self.interactable_lookup_by_save_id.clear();
        self.completed_target_ids.clear();
        self.system_order_text = GAMEPLAY_SYSTEM_ORDER_TEXT.to_string();
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
        self.sync_save_id_map_with_world(world)
            .expect("initial save_id assignment should not fail");
        self.sync_runtime_component_stores_with_world(world);
        self.rebuild_ai_agents_from_world(world);
        info!(
            scene = self.scene_name,
            entity_count = world.entity_count(),
            sys = %self.system_order_text,
            "scene_loaded"
        );
        info!(scene = self.scene_name, "sys: {}", self.system_order_text);
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

        self.run_gameplay_systems_once(fixed_dt_seconds, input, world);
        self.apply_system_outputs(fixed_dt_seconds, input, world);

        SceneCommand::None
    }

    fn execute_debug_command(
        &mut self,
        command: SceneDebugCommand,
        context: SceneDebugContext,
        world: &mut SceneWorld,
    ) -> SceneDebugCommandResult {
        match command {
            SceneDebugCommand::Spawn { def_name, position } => {
                let archetype = match try_resolve_archetype_by_name(world, &def_name) {
                    Ok(archetype) => archetype,
                    Err(error) => return SceneDebugCommandResult::Error(error),
                };
                let spawn_position = position
                    .map(|(x, y)| Vec2 { x, y })
                    .or(context.cursor_world)
                    .or_else(|| {
                        self.player_id
                            .and_then(|id| world.find_entity(id))
                            .map(|entity| entity.transform.position)
                    })
                    .unwrap_or(Vec2 { x: 0.0, y: 0.0 });
                self.system_intents
                    .enqueue(GameplayIntent::SpawnByArchetypeId {
                        archetype_id: archetype.id,
                        position: spawn_position,
                    });
                SceneDebugCommandResult::Success(format!(
                    "queued spawn '{}' at ({:.2}, {:.2})",
                    archetype.def_name, spawn_position.x, spawn_position.y
                ))
            }
            SceneDebugCommand::Despawn { entity_id } => {
                let runtime_id = EntityId(entity_id);
                if world.find_entity(runtime_id).is_none() {
                    SceneDebugCommandResult::Error(format!("entity {entity_id} not found"))
                } else {
                    self.system_intents.enqueue(GameplayIntent::DespawnEntity {
                        entity_id: runtime_id,
                    });
                    SceneDebugCommandResult::Success(format!("queued despawn entity {entity_id}"))
                }
            }
        }
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
        self.interactable_lookup_by_save_id.clear();
        self.completed_target_ids.clear();
        self.entity_save_ids.clear();
        self.save_id_to_entity.clear();
        self.next_save_id = 0;
        self.reset_runtime_component_stores();
        self.system_events = GameplayEventBus::default();
        self.system_intents = GameplayIntentQueue::default();
        self.system_order_text.clear();
        self.selected_completion_enqueued_this_tick = false;
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
        match entity.order_state {
            OrderState::Idle => None,
            OrderState::MoveTo { point } => Some(point),
            OrderState::Interact { target_save_id }
            | OrderState::Working { target_save_id, .. } => self
                .resolve_runtime_target_id(target_save_id, world)
                .and_then(|target_id| world.find_entity(target_id))
                .map(|target| target.transform.position),
        }
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
                selected_order_world = match entity.order_state {
                    OrderState::Idle => None,
                    OrderState::MoveTo { point } => Some(point),
                    OrderState::Interact { target_save_id }
                    | OrderState::Working { target_save_id, .. } => self
                        .resolve_runtime_target_id(target_save_id, world)
                        .and_then(|target_id| world.find_entity(target_id))
                        .map(|target| target.transform.position),
                };
                selected_job_state = match entity.order_state {
                    OrderState::Working { remaining_time, .. } => {
                        DebugJobState::Working { remaining_time }
                    }
                    _ => DebugJobState::Idle,
                };
            }
        }

        let event_counts = self.system_events.last_tick_counts();
        let intent_stats = self.system_intents.last_tick_apply_stats();
        let ai_counts = self.ai_state_counts();
        let interaction_line = if let Some(selected_id) = self.selected_entity {
            if let Some(active) = self.active_interactions_by_actor.get(&selected_id) {
                let rem_text = match active.remaining_seconds {
                    Some(remaining) => format!("{remaining:.2}"),
                    None => "ready".to_string(),
                };
                format!(
                    "ix: a:{} t:{} iid:{} rem:{}",
                    active.actor_id.0, active.target_id.0, active.interaction_id.0, rem_text
                )
            } else {
                "ix: none".to_string()
            }
        } else {
            "ix: none".to_string()
        };
        let interaction_probe_line = if let Some(selected_id) = self.selected_entity {
            if let Some(active) = self.active_interactions_by_actor.get(&selected_id) {
                let in_range = world
                    .find_entity(active.actor_id)
                    .zip(world.find_entity(active.target_id))
                    .map(|(actor, target)| {
                        GameplaySystemsHost::within_distance_range(
                            actor,
                            target,
                            active.interaction_range,
                        )
                    })
                    .unwrap_or(false);
                let remaining = match active.remaining_seconds {
                    Some(value) => format!("{value:.2}"),
                    None => "ready".to_string(),
                };
                format!(
                    "ixd: act:1 dur:{:.2} in:{} rem:{} ciq:{}",
                    active.duration_seconds,
                    if in_range { 1 } else { 0 },
                    remaining,
                    if self.selected_completion_enqueued_this_tick {
                        1
                    } else {
                        0
                    }
                )
            } else {
                format!(
                    "ixd: act:0 dur:none in:na rem:none ciq:{}",
                    if self.selected_completion_enqueued_this_tick {
                        1
                    } else {
                        0
                    }
                )
            }
        } else {
            format!(
                "ixd: act:0 dur:none in:na rem:none ciq:{}",
                if self.selected_completion_enqueued_this_tick {
                    1
                } else {
                    0
                }
            )
        };
        let extra_debug_lines = vec![
            format!("ev: {}", event_counts.total),
            format!(
                "evk: is:{} ic:{} dm:{} dd:{} sa:{} se:{}",
                event_counts.interaction_started,
                event_counts.interaction_completed,
                event_counts.entity_damaged,
                event_counts.entity_died,
                event_counts.status_applied,
                event_counts.status_expired
            ),
            format!("in: {}", intent_stats.total),
            format!(
                "ink: sp:{} mt:{} de:{} dmg:{} add:{} rem:{} si:{} ci:{} ca:{}",
                intent_stats.spawn_by_archetype_id,
                intent_stats.set_move_target,
                intent_stats.despawn_entity,
                intent_stats.apply_damage,
                intent_stats.add_status,
                intent_stats.remove_status,
                intent_stats.start_interaction,
                intent_stats.complete_interaction,
                intent_stats.cancel_interaction,
            ),
            format!("in_bad: {}", intent_stats.invalid_target_count),
            format!(
                "ai: id:{} wa:{} ch:{} use:{}",
                ai_counts.idle, ai_counts.wander, ai_counts.chase, ai_counts.use_interaction
            ),
            interaction_line,
            interaction_probe_line,
        ];

        Some(DebugInfoSnapshot {
            selected_entity: self.selected_entity,
            selected_position_world,
            selected_order_world,
            selected_job_state,
            entity_count,
            actor_count,
            interactable_count,
            resource_count: self.resource_count,
            system_order: self.system_order_text.clone(),
            extra_debug_lines: Some(extra_debug_lines),
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

fn try_resolve_archetype_by_name(
    world: &SceneWorld,
    def_name: &str,
) -> SaveLoadResult<EntityArchetype> {
    let def_db = world
        .def_database()
        .ok_or_else(|| "DefDatabase not set on SceneWorld before scene load".to_string())?;
    let def_id = def_db
        .entity_def_id_by_name(def_name)
        .ok_or_else(|| format!("unknown entity def '{def_name}'"))?;
    def_db
        .entity_def(def_id)
        .ok_or_else(|| format!("EntityDef id for '{def_name}' is missing from DefDatabase"))
        .cloned()
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
    use serde_json::json;

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
            camera_zoom: 1.4,
            selected_entity_save_id: Some(10),
            player_entity_save_id: Some(10),
            next_save_id: 21,
            resource_count: 2,
            entities: vec![
                SavedEntityRuntime {
                    save_id: 10,
                    position: SavedVec2 { x: 1.0, y: 2.0 },
                    rotation_radians: None,
                    selectable: true,
                    actor: true,
                    move_target_world: Some(SavedVec2 { x: 4.0, y: 2.0 }),
                    interaction_target_save_id: Some(20),
                    job_state: SavedJobState::Working {
                        target_save_id: 20,
                        remaining_time: 1.5,
                    },
                    interactable: None,
                },
                SavedEntityRuntime {
                    save_id: 20,
                    position: SavedVec2 { x: 5.0, y: 6.0 },
                    rotation_radians: None,
                    selectable: false,
                    actor: false,
                    move_target_world: None,
                    interaction_target_save_id: None,
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

    fn assert_vec2_close(actual: Vec2, expected: Vec2, epsilon: f32) {
        assert!(
            (actual.x - expected.x).abs() <= epsilon,
            "x {} vs {}",
            actual.x,
            expected.x
        );
        assert!(
            (actual.y - expected.y).abs() <= epsilon,
            "y {} vs {}",
            actual.y,
            expected.y
        );
    }

    fn interactable_entity_count(world: &SceneWorld) -> usize {
        world
            .entities()
            .iter()
            .filter(|entity| entity.interactable.is_some())
            .count()
    }

    fn missing_entity_id_from_world(world: &SceneWorld) -> EntityId {
        let max_id = world
            .entities()
            .iter()
            .map(|entity| entity.id.0)
            .max()
            .unwrap_or(0);
        EntityId(max_id.saturating_add(1))
    }

    fn first_non_player_actor_id(scene: &GameplayScene, world: &SceneWorld) -> EntityId {
        world
            .entities()
            .iter()
            .find(|entity| entity.actor && Some(entity.id) != scene.player_id)
            .map(|entity| entity.id)
            .expect("non-player actor")
    }

    fn advance(scene: &mut GameplayScene, world: &mut SceneWorld, steps: usize, fixed_dt: f32) {
        for _ in 0..steps {
            scene.update(fixed_dt, &InputSnapshot::empty(), world);
            world.apply_pending();
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EntityKindTag {
        Actor,
        Interactable,
        Other,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OrderDigest {
        Idle,
        MoveTo {
            x_bits: u32,
            y_bits: u32,
        },
        Interact {
            target_save_id: u64,
        },
        Working {
            target_save_id: u64,
            remaining_time_bits: u32,
        },
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct EntityDigest {
        save_id: u64,
        entity_kind: EntityKindTag,
        x_bits: u32,
        y_bits: u32,
        order: OrderDigest,
        interactable_remaining_uses: Option<u32>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SimDigest {
        camera_x_bits: u32,
        camera_y_bits: u32,
        camera_zoom_bits: u32,
        selected_save_id: Option<u64>,
        resource_count: u32,
        entities: Vec<EntityDigest>,
    }

    #[derive(Debug, Clone, Copy)]
    enum TickAction {
        Noop,
        SelectWorld(Vec2),
        RightClickWorld(Vec2),
    }

    #[derive(Debug, Clone, Copy)]
    struct ScriptCheckpoint {
        tick: usize,
        label: &'static str,
    }

    fn order_digest(order_state: OrderState) -> OrderDigest {
        match order_state {
            OrderState::Idle => OrderDigest::Idle,
            OrderState::MoveTo { point } => OrderDigest::MoveTo {
                x_bits: point.x.to_bits(),
                y_bits: point.y.to_bits(),
            },
            OrderState::Interact { target_save_id } => OrderDigest::Interact { target_save_id },
            OrderState::Working {
                target_save_id,
                remaining_time,
            } => OrderDigest::Working {
                target_save_id,
                remaining_time_bits: remaining_time.to_bits(),
            },
        }
    }

    fn input_for_action(
        world: &SceneWorld,
        action: TickAction,
        window_size: (u32, u32),
    ) -> InputSnapshot {
        match action {
            TickAction::Noop => InputSnapshot::empty().with_window_size(window_size),
            TickAction::SelectWorld(position_world) => {
                let (x, y) =
                    engine::world_to_screen_px(world.camera(), window_size, position_world);
                InputSnapshot::empty()
                    .with_window_size(window_size)
                    .with_left_click_pressed(true)
                    .with_cursor_position_px(Some(Vec2 {
                        x: x as f32,
                        y: y as f32,
                    }))
            }
            TickAction::RightClickWorld(position_world) => {
                let (x, y) =
                    engine::world_to_screen_px(world.camera(), window_size, position_world);
                InputSnapshot::empty()
                    .with_window_size(window_size)
                    .with_right_click_pressed(true)
                    .with_cursor_position_px(Some(Vec2 {
                        x: x as f32,
                        y: y as f32,
                    }))
            }
        }
    }

    fn capture_sim_digest(scene: &GameplayScene, world: &SceneWorld) -> SimDigest {
        let mut entities = world
            .entities()
            .iter()
            .map(|entity| {
                let save_id = scene
                    .entity_save_ids
                    .get(&entity.id)
                    .copied()
                    .unwrap_or_else(|| {
                        panic!("missing save_id mapping for entity {}", entity.id.0)
                    });
                let entity_kind = if entity.actor {
                    EntityKindTag::Actor
                } else if entity.interactable.is_some() {
                    EntityKindTag::Interactable
                } else {
                    EntityKindTag::Other
                };
                EntityDigest {
                    save_id,
                    entity_kind,
                    x_bits: entity.transform.position.x.to_bits(),
                    y_bits: entity.transform.position.y.to_bits(),
                    order: order_digest(entity.order_state),
                    interactable_remaining_uses: entity
                        .interactable
                        .map(|interactable| interactable.remaining_uses),
                }
            })
            .collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.save_id);

        let camera = world.camera();
        SimDigest {
            camera_x_bits: camera.position.x.to_bits(),
            camera_y_bits: camera.position.y.to_bits(),
            camera_zoom_bits: camera.zoom.to_bits(),
            selected_save_id: scene
                .selected_entity
                .and_then(|id| scene.entity_save_ids.get(&id).copied()),
            resource_count: scene.resource_count,
            entities,
        }
    }

    fn run_script_and_capture(
        scene: &mut GameplayScene,
        world: &mut SceneWorld,
        fixed_dt: f32,
        steps: usize,
        script_actions: &[(usize, TickAction)],
        checkpoints: &[ScriptCheckpoint],
        window_size: (u32, u32),
    ) -> Vec<(&'static str, SimDigest)> {
        let mut snapshots = Vec::new();
        for tick in 0..steps {
            let action = script_actions
                .iter()
                .find(|(action_tick, _)| *action_tick == tick)
                .map(|(_, action)| *action)
                .unwrap_or(TickAction::Noop);
            let input = input_for_action(world, action, window_size);
            scene.update(fixed_dt, &input, world);
            world.apply_pending();

            for checkpoint in checkpoints {
                if checkpoint.tick == tick {
                    snapshots.push((checkpoint.label, capture_sim_digest(scene, world)));
                }
            }
        }
        snapshots
    }

    fn make_move_fixture() -> (GameplayScene, SceneWorld, u64) {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        world.camera_mut().position = Vec2 { x: 0.0, y: 0.0 };
        world.camera_mut().set_zoom_clamped(1.0);

        let actor_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "det_actor",
            },
        );
        world.apply_pending();
        world.find_entity_mut(actor_id).expect("actor").selectable = true;
        scene.player_id = Some(actor_id);
        scene.selected_entity = None;
        scene
            .sync_save_id_map_with_world(&world)
            .expect("sync deterministic move fixture");
        let actor_save_id = scene
            .entity_save_ids
            .get(&actor_id)
            .copied()
            .expect("actor save id");
        (scene, world, actor_save_id)
    }

    fn make_interact_fixture() -> (GameplayScene, SceneWorld, u64, u64) {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        world.camera_mut().position = Vec2 { x: 0.0, y: 0.0 };
        world.camera_mut().set_zoom_clamped(1.0);

        let actor_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: -2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "det_actor",
            },
        );
        world.apply_pending();
        world.find_entity_mut(actor_id).expect("actor").selectable = true;
        let target_id = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        scene.player_id = Some(actor_id);
        scene.selected_entity = None;
        scene
            .sync_save_id_map_with_world(&world)
            .expect("sync deterministic interact fixture");

        let actor_save_id = scene
            .entity_save_ids
            .get(&actor_id)
            .copied()
            .expect("actor save id");
        let target_save_id = scene
            .entity_save_ids
            .get(&target_id)
            .copied()
            .expect("target save id");
        (scene, world, actor_save_id, target_save_id)
    }

    #[test]
    fn gameplay_system_order_is_stable_and_expected_names() {
        let names: Vec<&'static str> = GAMEPLAY_SYSTEM_ORDER
            .iter()
            .map(|system_id| system_id.name())
            .collect();
        assert_eq!(
            names,
            vec![
                "InputIntent",
                "Interaction",
                "AI",
                "CombatResolution",
                "StatusEffects",
                "Cleanup",
            ]
        );
        assert_eq!(
            GAMEPLAY_SYSTEM_ORDER_TEXT,
            "InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup"
        );
    }

    #[test]
    fn gameplay_systems_host_one_tick_executes_without_panic() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        scene.system_order_text = GAMEPLAY_SYSTEM_ORDER_TEXT.to_string();
        let mut world = SceneWorld::default();
        let input = InputSnapshot::empty().with_window_size((1280, 720));

        scene.update(0.1, &input, &mut world);
        world.apply_pending();

        assert_eq!(
            scene.systems_host.last_tick_order.len(),
            GAMEPLAY_SYSTEM_ORDER.len()
        );
        for (actual, expected) in scene
            .systems_host
            .last_tick_order
            .iter()
            .zip(GAMEPLAY_SYSTEM_ORDER.iter())
        {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn event_bus_emit_and_rollover_counts_are_correct() {
        let mut bus = GameplayEventBus::default();

        bus.emit(GameplayEvent::InteractionStarted {
            actor_id: EntityId(1),
            target_id: EntityId(2),
        });
        bus.emit(GameplayEvent::EntityDamaged {
            entity_id: EntityId(2),
            amount: 3,
        });
        bus.emit(GameplayEvent::StatusApplied {
            entity_id: EntityId(1),
            status_id: StatusId(7),
        });
        bus.emit(GameplayEvent::StatusExpired {
            entity_id: EntityId(1),
            status_id: StatusId(7),
        });
        bus.emit(GameplayEvent::EntityDied {
            entity_id: EntityId(2),
        });
        bus.emit(GameplayEvent::InteractionCompleted {
            actor_id: EntityId(1),
            target_id: EntityId(2),
        });

        assert_eq!(bus.iter_emitted_so_far().count(), 6);
        bus.finish_tick_rollover();

        let counts = bus.last_tick_counts();
        assert_eq!(counts.total, 6);
        assert_eq!(counts.interaction_started, 1);
        assert_eq!(counts.interaction_completed, 1);
        assert_eq!(counts.entity_damaged, 1);
        assert_eq!(counts.entity_died, 1);
        assert_eq!(counts.status_applied, 1);
        assert_eq!(counts.status_expired, 1);
        assert_eq!(bus.iter_emitted_so_far().count(), 0);

        bus.emit(GameplayEvent::StatusApplied {
            entity_id: EntityId(9),
            status_id: StatusId(2),
        });
        bus.finish_tick_rollover();
        let next_counts = bus.last_tick_counts();
        assert_eq!(next_counts.total, 1);
        assert_eq!(next_counts.status_applied, 1);
        assert_eq!(next_counts.interaction_started, 0);
        assert_eq!(next_counts.interaction_completed, 0);
        assert_eq!(next_counts.entity_damaged, 0);
        assert_eq!(next_counts.entity_died, 0);
        assert_eq!(next_counts.status_expired, 0);
    }

    #[test]
    fn gameplay_systems_dev_probe_emits_nonzero_last_tick_events() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let input = InputSnapshot::empty().with_window_size((1280, 720));

        scene.update(0.1, &input, &mut world);
        world.apply_pending();

        let counts = scene.system_events.last_tick_counts();
        assert!(counts.total > 0);
    }

    #[test]
    fn intent_apply_order_is_deterministic() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let entity_id = world.entities()[0].id;
        let status_id = StatusId(42);
        let intents = vec![
            GameplayIntent::AddStatus {
                entity_id,
                status_id,
            },
            GameplayIntent::RemoveStatus {
                entity_id,
                status_id,
            },
        ];

        let stats = scene.apply_gameplay_intents_at_safe_point(intents, &mut world);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.add_status, 1);
        assert_eq!(stats.remove_status, 1);
        let statuses = scene
            .status_ids_by_entity
            .get(&entity_id)
            .expect("status store entry");
        assert!(!statuses.contains(&status_id));
    }

    #[test]
    fn intent_apply_spawn_and_despawn_hooks_run_in_order() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let victim_id = world
            .entities()
            .iter()
            .find(|entity| Some(entity.id) != scene.player_id)
            .map(|entity| entity.id)
            .expect("non-player victim");
        let before_count = world.entity_count();
        let archetype_id = world
            .def_database()
            .expect("def database")
            .entity_def_id_by_name("proto.player")
            .expect("player archetype id");
        let intents = vec![
            GameplayIntent::SpawnByArchetypeId {
                archetype_id,
                position: Vec2 { x: 123.0, y: 456.0 },
            },
            GameplayIntent::DespawnEntity {
                entity_id: victim_id,
            },
        ];

        let stats = scene.apply_gameplay_intents_at_safe_point(intents, &mut world);
        world.apply_pending();

        assert_eq!(stats.total, 2);
        assert_eq!(stats.spawn_by_archetype_id, 1);
        assert_eq!(stats.despawn_entity, 1);
        assert_eq!(stats.invalid_target_count, 0);
        assert_eq!(stats.spawned_entity_ids.len(), 1);
        let spawned_id = stats.spawned_entity_ids[0];
        assert!(world.find_entity(victim_id).is_none());
        assert!(world.find_entity(spawned_id).is_some());
        assert_eq!(world.entity_count(), before_count);
        assert!(!scene.entity_save_ids.contains_key(&victim_id));
        assert!(scene.entity_save_ids.contains_key(&spawned_id));
    }

    #[test]
    fn bad_entity_id_intent_does_not_panic() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let missing_id = missing_entity_id_from_world(&world);
        let intents = vec![GameplayIntent::ApplyDamage {
            entity_id: missing_id,
            amount: 5,
        }];
        let stats = scene.apply_gameplay_intents_at_safe_point(intents, &mut world);

        assert_eq!(stats.total, 1);
        assert_eq!(stats.apply_damage, 1);
        assert_eq!(stats.invalid_target_count, 1);
    }

    #[test]
    fn debug_spawn_and_despawn_are_queued_intents() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let before_spawn_count = world.entity_count();
        let spawn_result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((50.0, -20.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_result, SceneDebugCommandResult::Success(_)));
        assert_eq!(world.entity_count(), before_spawn_count);
        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        assert_eq!(world.entity_count(), before_spawn_count + 1);

        let victim_id = world
            .entities()
            .iter()
            .find(|entity| Some(entity.id) != scene.player_id)
            .map(|entity| entity.id)
            .expect("non-player victim");
        let before_despawn_count = world.entity_count();
        let despawn_result = scene.execute_debug_command(
            SceneDebugCommand::Despawn {
                entity_id: victim_id.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(
            despawn_result,
            SceneDebugCommandResult::Success(_)
        ));
        assert_eq!(world.entity_count(), before_despawn_count);
        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        assert!(world.entity_count() <= before_despawn_count);
        assert!(world.find_entity(victim_id).is_none());
    }

    #[test]
    fn debug_spawn_unknown_def_returns_error() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let before_count = world.entity_count();
        let before_map_len = scene.entity_save_ids.len();

        let result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.unknown_def".to_string(),
                position: None,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Error(_)));
        assert_eq!(world.entity_count(), before_count);
        assert_eq!(scene.entity_save_ids.len(), before_map_len);
    }

    #[test]
    fn debug_despawn_failure_path_returns_error() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let failure = scene.execute_debug_command(
            SceneDebugCommand::Despawn {
                entity_id: missing_entity_id_from_world(&world).0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(failure, SceneDebugCommandResult::Error(_)));
    }

    #[test]
    fn default_content_pack_contains_new_microticket_defs() {
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        let def_db = world.def_database().expect("def database");
        for def_name in [
            "proto.npc_chaser",
            "proto.npc_dummy",
            "proto.stockpile_small",
            "proto.door_dummy",
        ] {
            assert!(
                def_db.entity_def_id_by_name(def_name).is_some(),
                "missing def {def_name}"
            );
        }
    }

    #[test]
    fn spawn_by_archetype_tags_drive_actor_and_interactable_runtime_roles() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let def_db = world.def_database().expect("def database");
        let npc_chaser_id = def_db
            .entity_def_id_by_name("proto.npc_chaser")
            .expect("npc chaser def");
        let npc_dummy_id = def_db
            .entity_def_id_by_name("proto.npc_dummy")
            .expect("npc dummy def");
        let stockpile_small_id = def_db
            .entity_def_id_by_name("proto.stockpile_small")
            .expect("stockpile small def");
        let door_dummy_id = def_db
            .entity_def_id_by_name("proto.door_dummy")
            .expect("door dummy def");

        let intents = vec![
            GameplayIntent::SpawnByArchetypeId {
                archetype_id: npc_chaser_id,
                position: Vec2 { x: 10.0, y: 0.0 },
            },
            GameplayIntent::SpawnByArchetypeId {
                archetype_id: npc_dummy_id,
                position: Vec2 { x: 11.0, y: 0.0 },
            },
            GameplayIntent::SpawnByArchetypeId {
                archetype_id: stockpile_small_id,
                position: Vec2 { x: 12.0, y: 0.0 },
            },
            GameplayIntent::SpawnByArchetypeId {
                archetype_id: door_dummy_id,
                position: Vec2 { x: 13.0, y: 0.0 },
            },
        ];

        let stats = scene.apply_gameplay_intents_at_safe_point(intents, &mut world);
        world.apply_pending();

        assert_eq!(stats.invalid_target_count, 0);
        assert_eq!(stats.spawn_by_archetype_id, 4);
        assert_eq!(stats.spawned_entity_ids.len(), 4);

        let spawned = stats
            .spawned_entity_ids
            .iter()
            .map(|id| {
                let entity = world.find_entity(*id).expect("spawned entity");
                (
                    entity.transform.position,
                    entity.actor,
                    entity.interactable.is_some(),
                )
            })
            .collect::<Vec<_>>();
        let chaser = spawned
            .iter()
            .find(|(pos, _, _)| (pos.x - 10.0).abs() < 0.001)
            .expect("chaser spawn");
        assert!(chaser.1);
        assert!(!chaser.2);
        let dummy = spawned
            .iter()
            .find(|(pos, _, _)| (pos.x - 11.0).abs() < 0.001)
            .expect("dummy spawn");
        assert!(dummy.1);
        assert!(!dummy.2);
        let stockpile = spawned
            .iter()
            .find(|(pos, _, _)| (pos.x - 12.0).abs() < 0.001)
            .expect("stockpile spawn");
        assert!(!stockpile.1);
        assert!(stockpile.2);
        let door = spawned
            .iter()
            .find(|(pos, _, _)| (pos.x - 13.0).abs() < 0.001)
            .expect("door spawn");
        assert!(!door.1);
        assert!(door.2);
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

        let target = world.find_entity(actor).expect("actor");
        let target = match target.order_state {
            OrderState::MoveTo { point } => point,
            _ => panic!("expected move order"),
        };
        assert!((target.x - 1.0).abs() < 0.0001);
        assert!(target.y.abs() < 0.0001);
    }

    #[test]
    fn zoom_steps_apply_before_right_click_screen_to_world_targeting() {
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
        scene.selected_entity = Some(actor);

        let input = InputSnapshot::empty()
            .with_right_click_pressed(true)
            .with_zoom_delta_steps(1)
            .with_cursor_position_px(Some(Vec2 { x: 672.0, y: 360.0 }))
            .with_window_size((1280, 720));
        scene.update(1.0 / 60.0, &input, &mut world);

        let target = world.find_entity(actor).expect("actor");
        let target = match target.order_state {
            OrderState::MoveTo { point } => point,
            _ => panic!("expected move order"),
        };
        assert!((world.camera().zoom - 1.1).abs() < 0.0001);
        assert!((target.x - (32.0 / (32.0 * 1.1))).abs() < 0.0001);
        assert!(target.y.abs() < 0.0001);
    }

    #[test]
    fn right_click_selected_actor_creates_order_marker_with_ttl() {
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

        let markers = world.debug_markers();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].kind, engine::DebugMarkerKind::Order);
        assert!((markers[0].position_world.x - 1.0).abs() < 0.0001);
        assert!(markers[0].position_world.y.abs() < 0.0001);
        assert!((markers[0].ttl_seconds - ORDER_MARKER_TTL_SECONDS).abs() < 0.0001);
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
        assert_eq!(
            world.find_entity(actor).expect("actor").order_state,
            OrderState::Idle
        );
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
        assert_eq!(
            world.find_entity(non_actor).expect("non_actor").order_state,
            OrderState::Idle
        );
    }

    #[test]
    fn selected_visual_clears_when_stale_or_non_actor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        world.set_selected_actor_visual(Some(EntityId(9999)));
        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        assert_eq!(world.visual_state().selected_actor, None);

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
        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        assert_eq!(world.visual_state().selected_actor, None);
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
            entity.order_state = OrderState::MoveTo {
                point: Vec2 { x: 0.2, y: 0.0 },
            };
        }

        for _ in 0..10 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
        }

        let entity = world.find_entity(actor).expect("actor");
        assert_eq!(entity.order_state, OrderState::Idle);
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);

        let updated = world.find_entity(actor).expect("actor");
        let target_save_id = scene
            .entity_save_ids
            .get(&pile)
            .copied()
            .expect("pile save id");
        assert_eq!(updated.order_state, OrderState::Interact { target_save_id });
    }

    #[test]
    fn timed_interaction_completes_with_expected_fixed_ticks() {
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
        let _pile = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.selected_entity = Some(actor);
        world.find_entity_mut(actor).expect("actor").selectable = true;
        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();

        let mut saw_completed_event = false;
        let mut saw_completed_intent = false;
        for _ in 0..40 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let last_events = scene.system_events.last_tick_counts();
            if last_events.interaction_completed > 0 {
                saw_completed_event = true;
            }
            let last_intents = scene.system_intents.last_tick_apply_stats();
            if last_intents.complete_interaction > 0 {
                saw_completed_intent = true;
            }
        }

        let actor_entity = world.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.order_state, OrderState::Idle);
        assert!(saw_completed_event);
        assert!(saw_completed_intent);
    }

    #[test]
    fn stockpile_interaction_from_out_of_range_completes_within_expected_ticks() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let actor_id = scene.player_id.expect("player id");
        scene.selected_entity = Some(actor_id);

        let stockpile = world
            .entities()
            .iter()
            .find(|entity| entity.interactable.is_some())
            .map(|entity| entity.id)
            .expect("stockpile");
        let stockpile_pos = world
            .find_entity(stockpile)
            .expect("stockpile entity")
            .transform
            .position;
        let (sx, sy) = engine::world_to_screen_px(world.camera(), (1280, 720), stockpile_pos);
        let click = right_click_snapshot(
            Vec2 {
                x: sx as f32,
                y: sy as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();

        let mut saw_start = scene.system_events.last_tick_counts().interaction_started > 0;
        let mut saw_complete = false;
        for _ in 0..80 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let counts = scene.system_events.last_tick_counts();
            if counts.interaction_started > 0 {
                saw_start = true;
            }
            if counts.interaction_completed > 0 {
                saw_complete = true;
            }
        }
        assert!(saw_start);
        assert!(saw_complete);
        assert_eq!(
            world.find_entity(actor_id).expect("actor").order_state,
            OrderState::Idle
        );
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
        scene.save_id_to_entity.insert(9999, EntityId(9999));
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.order_state = OrderState::Working {
                target_save_id: 9999,
                remaining_time: 1.0,
            };
        }

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        let actor_entity = world.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.order_state, OrderState::Idle);
    }

    #[test]
    fn interaction_cancellation_uses_cancel_intent_not_complete() {
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
        scene_a
            .sync_save_id_map_with_world(&world_a)
            .expect("save-id sync");
        scene_a.active_interactions_by_actor.insert(
            actor,
            ActiveInteraction {
                actor_id: actor,
                target_id: pile,
                interaction_id: InteractionId(99),
                kind: ActiveInteractionKind::Use,
                interaction_range: RESOURCE_PILE_INTERACTION_RADIUS,
                duration_seconds: JOB_DURATION_SECONDS,
                remaining_seconds: Some(1.0),
            },
        );

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

        world_a
            .find_entity_mut(actor)
            .expect("actor")
            .transform
            .position = Vec2 { x: 9.0, y: 0.0 };
        scene_a.update(0.1, &InputSnapshot::empty(), &mut world_a);
        world_a.apply_pending();

        let actor_entity = world_a.find_entity(actor).expect("actor");
        assert_eq!(actor_entity.order_state, OrderState::Idle);
        assert!(scene_a.active_interactions_by_actor.is_empty());
        let last_intents = scene_a.system_intents.last_tick_apply_stats();
        assert_eq!(last_intents.cancel_interaction, 1);
        assert_eq!(last_intents.complete_interaction, 0);

        scene_b.update(0.1, &InputSnapshot::empty(), &mut world_b);
        world_b.apply_pending();
    }

    #[test]
    fn interaction_state_machine_start_tick_complete_and_cancel_out_of_range() {
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();
        let started_counts = scene.system_events.last_tick_counts();
        assert_eq!(started_counts.interaction_started, 1);
        assert!(scene.active_interactions_by_actor.contains_key(&actor));

        world
            .find_entity_mut(actor)
            .expect("actor")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        assert!(scene
            .active_interactions_by_actor
            .get(&actor)
            .and_then(|ix| ix.remaining_seconds)
            .is_some());

        world
            .find_entity_mut(actor)
            .expect("actor")
            .transform
            .position = Vec2 { x: 9.0, y: 0.0 };
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        let canceled_stats = scene.system_intents.last_tick_apply_stats();
        assert_eq!(canceled_stats.cancel_interaction, 1);
        assert!(scene.active_interactions_by_actor.get(&actor).is_none());

        world
            .find_entity_mut(actor)
            .expect("actor")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();
        let mut saw_completed_event = false;
        let mut saw_completed_intent = false;
        for _ in 0..25 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let completed_counts = scene.system_events.last_tick_counts();
            if completed_counts.interaction_completed > 0 {
                saw_completed_event = true;
            }
            let completed_stats = scene.system_intents.last_tick_apply_stats();
            if completed_stats.complete_interaction > 0 {
                saw_completed_intent = true;
            }
        }
        assert!(saw_completed_event);
        assert!(saw_completed_intent);
        assert_eq!(
            world.find_entity(actor).expect("actor").order_state,
            OrderState::Idle
        );
        assert!(world.find_entity(pile).is_some());
    }

    #[test]
    fn immediate_interaction_completion_is_emitted_by_interaction_system_only() {
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
        let target = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        scene.active_interactions_by_actor.insert(
            actor,
            ActiveInteraction {
                actor_id: actor,
                target_id: target,
                interaction_id: InteractionId(777),
                kind: ActiveInteractionKind::Use,
                interaction_range: RESOURCE_PILE_INTERACTION_RADIUS,
                duration_seconds: 0.0,
                remaining_seconds: None,
            },
        );

        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let completion_events = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| matches!(event, GameplayEvent::InteractionCompleted { .. }))
            .count();
        let start_events = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| matches!(event, GameplayEvent::InteractionStarted { .. }))
            .count();
        assert_eq!(completion_events, 1);
        assert_eq!(start_events, 0);

        let intents = scene.system_intents.drain_current_tick();
        let complete_count = intents
            .iter()
            .filter(|intent| matches!(intent, GameplayIntent::CompleteInteraction { .. }))
            .count();
        let start_count = intents
            .iter()
            .filter(|intent| matches!(intent, GameplayIntent::StartInteraction { .. }))
            .count();
        assert_eq!(complete_count, 1);
        assert_eq!(start_count, 0);
    }

    #[test]
    fn ai_state_transitions_idle_wander_chase_useinteraction_with_cooldown() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id = scene.player_id.expect("player");
        let npc_id = first_non_player_actor_id(&scene, &world);
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            npc_id,
            AiAgent::from_home_position(world.find_entity(npc_id).expect("npc").transform.position),
        );

        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 20.0, y: 0.0 };
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let wander_agent = scene.ai_agents_by_entity.get(&npc_id).expect("agent");
        assert_eq!(wander_agent.state, AiState::Wander);
        let wander_intents = scene.system_intents.drain_current_tick();
        assert!(wander_intents.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::SetMoveTarget { actor_id, .. } if *actor_id == npc_id
            )
        }));
        scene.system_events.clear_current_tick();

        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 3.0, y: 0.0 };
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let chase_agent = scene.ai_agents_by_entity.get(&npc_id).expect("agent");
        assert_eq!(chase_agent.state, AiState::Chase);
        let chase_intents = scene.system_intents.drain_current_tick();
        assert!(chase_intents.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::SetMoveTarget { actor_id, .. } if *actor_id == npc_id
            )
        }));
        scene.system_events.clear_current_tick();

        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 0.5, y: 0.0 };
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let use_agent = scene.ai_agents_by_entity.get(&npc_id).expect("agent");
        assert_eq!(use_agent.state, AiState::UseInteraction);
        assert!(use_agent.cooldown_remaining_seconds > 0.0);
        let use_intents = scene.system_intents.drain_current_tick();
        assert!(use_intents.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::StartInteraction { actor_id, target_id }
                    if *actor_id == npc_id && *target_id == player_id
            )
        }));
    }

    #[test]
    fn ai_smoke_spawned_npc_reaches_attack_interaction_within_bounded_ticks() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id = scene.player_id.expect("player");
        let player_pos = world
            .find_entity(player_id)
            .expect("player")
            .transform
            .position;
        let spawn_result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((player_pos.x + 0.5, player_pos.y)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_result, SceneDebugCommandResult::Success(_)));

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        let spawned_ids = scene
            .system_intents
            .last_tick_apply_stats()
            .spawned_entity_ids
            .clone();
        assert_eq!(spawned_ids.len(), 1);
        let spawned_npc_id = spawned_ids[0];
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            spawned_npc_id,
            AiAgent::from_home_position(
                world
                    .find_entity(spawned_npc_id)
                    .expect("spawned npc")
                    .transform
                    .position,
            ),
        );

        let mut saw_start = false;
        let mut saw_terminal = false;
        for _ in 0..40 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let stats = scene.system_intents.last_tick_apply_stats();
            if stats.start_interaction > 0 {
                saw_start = true;
            }
            if stats.complete_interaction > 0 || stats.cancel_interaction > 0 {
                saw_terminal = true;
            }
            if saw_start && saw_terminal {
                break;
            }
        }

        assert!(saw_start);
        assert!(saw_terminal);
    }

    #[test]
    fn ai_does_not_enqueue_set_move_target_when_interaction_is_in_progress() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id = scene.player_id.expect("player");
        let npc_id = first_non_player_actor_id(&scene, &world);
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            npc_id,
            AiAgent::from_home_position(world.find_entity(npc_id).expect("npc").transform.position),
        );
        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 3.0, y: 0.0 };
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };

        scene.active_interactions_by_actor.insert(
            npc_id,
            ActiveInteraction {
                actor_id: npc_id,
                target_id: player_id,
                interaction_id: InteractionId(900),
                kind: ActiveInteractionKind::Attack,
                interaction_range: AI_ATTACK_RANGE_UNITS,
                duration_seconds: AI_ATTACK_INTERACTION_DURATION_SECONDS,
                remaining_seconds: None,
            },
        );
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let intents_with_runtime = scene.system_intents.drain_current_tick();
        assert!(!intents_with_runtime.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::SetMoveTarget { actor_id, .. } if *actor_id == npc_id
            )
        }));
        scene.active_interactions_by_actor.clear();
        scene.system_events.clear_current_tick();

        let player_save_id = scene.save_id_for_entity(player_id).expect("player save id");
        world.find_entity_mut(npc_id).expect("npc").order_state = OrderState::Interact {
            target_save_id: player_save_id,
        };
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let intents_with_world_order = scene.system_intents.drain_current_tick();
        assert!(!intents_with_world_order.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::SetMoveTarget { actor_id, .. } if *actor_id == npc_id
            )
        }));
    }

    #[test]
    fn spawn_proto_player_never_replaces_player_id() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let original_player_id = scene.player_id.expect("player");
        let spawn_result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((10.0, -4.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_result, SceneDebugCommandResult::Success(_)));

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        assert_eq!(scene.player_id, Some(original_player_id));
        let spawned_ids = scene
            .system_intents
            .last_tick_apply_stats()
            .spawned_entity_ids
            .clone();
        assert_eq!(spawned_ids.len(), 1);
        let spawned_id = spawned_ids[0];
        let spawned = world.find_entity(spawned_id).expect("spawned proto.player");
        assert!(spawned.actor);
        assert!(scene.ai_agents_by_entity.contains_key(&spawned_id));
    }

    #[test]
    fn auto_spawn_restores_authoritative_player_once_when_missing() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let original_player_id = scene.player_id.expect("player");
        let baseline_entity_count = world.entity_count();
        scene.system_intents.enqueue(GameplayIntent::DespawnEntity {
            entity_id: original_player_id,
        });
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        let restored_player_id = scene.player_id.expect("restored player");
        assert_ne!(restored_player_id, original_player_id);
        assert!(world.find_entity(restored_player_id).is_some());
        assert_eq!(world.entity_count(), baseline_entity_count);

        let after_restore_count = world.entity_count();
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        assert_eq!(scene.player_id, Some(restored_player_id));
        assert_eq!(world.entity_count(), after_restore_count);
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
        scene_a
            .sync_save_id_map_with_world(&world_a)
            .expect("save-id sync");
        let pile_save_id = scene_a
            .entity_save_ids
            .get(&pile)
            .copied()
            .expect("pile save id");
        {
            let entity = world_a.find_entity_mut(actor).expect("actor");
            entity.selectable = true;
            entity.order_state = OrderState::Interact {
                target_save_id: pile_save_id,
            };
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
        assert_eq!(after.order_state, before.order_state);
    }

    #[test]
    fn debug_info_snapshot_reports_selected_entity_fields_and_counts() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        scene.system_order_text = GAMEPLAY_SYSTEM_ORDER_TEXT.to_string();
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        let pile_save_id = scene
            .entity_save_ids
            .get(&pile)
            .copied()
            .expect("pile save id");
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.selectable = true;
            entity.order_state = OrderState::Working {
                target_save_id: pile_save_id,
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
        assert_eq!(snapshot.selected_order_world, Some(Vec2 { x: 3.0, y: 4.0 }));
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
        assert_eq!(snapshot.system_order, GAMEPLAY_SYSTEM_ORDER_TEXT);
        let extra = snapshot.extra_debug_lines.expect("extra debug lines");
        assert!(extra.iter().any(|line| line.starts_with("ev: ")));
        assert!(extra.iter().any(|line| line.starts_with("evk: ")));
        assert!(extra.iter().any(|line| line.starts_with("in: ")));
        assert!(extra
            .iter()
            .any(|line| line.starts_with("ink: ") && line.contains(" ca:")));
        assert!(extra.iter().any(|line| line.starts_with("in_bad: ")));
        assert!(extra.iter().any(|line| line.starts_with("ai: ")));
        assert!(extra.iter().any(|line| line.starts_with("ix: ")));
        assert!(extra.iter().any(|line| line.starts_with("ixd: ")));
    }

    #[test]
    fn debug_info_snapshot_handles_missing_selected_entity() {
        let mut scene = GameplayScene {
            selected_entity: Some(EntityId(999)),
            ..GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 })
        };
        scene.system_order_text = GAMEPLAY_SYSTEM_ORDER_TEXT.to_string();
        let world = SceneWorld::default();
        let snapshot = scene
            .debug_info_snapshot(&world)
            .expect("debug snapshot exists");
        assert_eq!(snapshot.selected_entity, Some(EntityId(999)));
        assert_eq!(snapshot.selected_position_world, None);
        assert_eq!(snapshot.selected_order_world, None);
        assert_eq!(snapshot.selected_job_state, DebugJobState::None);
        assert_eq!(snapshot.system_order, GAMEPLAY_SYSTEM_ORDER_TEXT);
        let extra = snapshot.extra_debug_lines.expect("extra debug lines");
        assert!(extra.iter().any(|line| line.starts_with("ev: ")));
        assert!(extra.iter().any(|line| line.starts_with("evk: ")));
        assert!(extra.iter().any(|line| line.starts_with("in: ")));
        assert!(extra
            .iter()
            .any(|line| line.starts_with("ink: ") && line.contains(" ca:")));
        assert!(extra.iter().any(|line| line.starts_with("in_bad: ")));
        assert!(extra.iter().any(|line| line.starts_with("ai: ")));
        assert!(extra.iter().any(|line| line.starts_with("ix: ")));
        assert!(extra.iter().any(|line| line.starts_with("ixd: ")));
    }

    #[test]
    fn save_game_roundtrip_json_preserves_runtime_fields() {
        let save = sample_save_game(SavedSceneKey::A);
        let json = serde_json::to_string(&save).expect("serialize");
        let decoded: SaveGame = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, save);
    }

    fn capture_scene_restore_state(
        scene: &GameplayScene,
        world: &SceneWorld,
    ) -> (usize, Vec2, Option<EntityId>, Option<EntityId>, u32) {
        (
            world.entity_count(),
            world.entities()[0].transform.position,
            scene.selected_entity,
            scene.player_id,
            scene.resource_count,
        )
    }

    #[test]
    fn parse_save_game_json_reports_missing_required_field_path() {
        let mut value = serde_json::to_value(sample_save_game(SavedSceneKey::A)).expect("to_value");
        let object = value.as_object_mut().expect("save object");
        object.remove("save_version");
        let raw = serde_json::to_string(&value).expect("json");

        let error =
            GameplayScene::parse_save_game_json(&raw).expect_err("missing field should fail");
        assert!(error.contains("parse save json"));
        assert!(error.contains("save_version"));
        assert!(error.contains("missing field"));
    }

    #[test]
    fn parse_save_game_json_reports_unknown_enum_tag_path() {
        let mut value = serde_json::to_value(sample_save_game(SavedSceneKey::A)).expect("to_value");
        value["entities"][0]["job_state"] = json!("Broken");
        let raw = serde_json::to_string(&value).expect("json");

        let error =
            GameplayScene::parse_save_game_json(&raw).expect_err("unknown enum tag should fail");
        assert!(error.contains("parse save json"));
        assert!(error.contains("entities[0].job_state"));
        assert!(error.contains("unknown variant"));
    }

    #[test]
    fn parse_save_game_json_reports_type_mismatch_path() {
        let mut value = serde_json::to_value(sample_save_game(SavedSceneKey::A)).expect("to_value");
        value["entities"][0]["save_id"] = json!("oops");
        let raw = serde_json::to_string(&value).expect("json");

        let error =
            GameplayScene::parse_save_game_json(&raw).expect_err("type mismatch should fail");
        assert!(error.contains("parse save json"));
        assert!(error.contains("entities[0].save_id"));
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

        let mut bad_reference = sample_save_game(SavedSceneKey::A);
        bad_reference.selected_entity_save_id = Some(9999);
        assert!(GameplayScene::validate_save_game(&bad_reference, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(world.entities()[0].transform.position, before_first_pos);
        assert_eq!(scene.resource_count, before_resource_count);

        let mut bad_next_save_id = sample_save_game(SavedSceneKey::A);
        bad_next_save_id.next_save_id = 20;
        assert!(GameplayScene::validate_save_game(&bad_next_save_id, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(world.entities()[0].transform.position, before_first_pos);
        assert_eq!(scene.resource_count, before_resource_count);
    }

    #[test]
    fn load_validation_rejects_non_finite_camera_zoom() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.camera_zoom = f32::NAN;
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());
    }

    #[test]
    fn validate_reports_field_path_for_dangling_target_refs() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].interaction_target_save_id = Some(9999);
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("dangling target");
        assert!(error.contains("entities[0].interaction_target_save_id"));
        assert!(error.contains("references unknown save_id 9999"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].job_state = SavedJobState::Working {
            target_save_id: 9999,
            remaining_time: 1.0,
        };
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("dangling job target");
        assert!(error.contains("entities[0].job_state.target_save_id"));
        assert!(error.contains("references unknown save_id 9999"));
    }

    #[test]
    fn validate_reports_field_paths_for_non_finite_and_invalid_numbers() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.camera_position.x = f32::NAN;
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("non-finite camera x");
        assert!(error.contains("camera_position.x"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].position.y = f32::INFINITY;
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("non-finite position y");
        assert!(error.contains("entities[0].position.y"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].move_target_world = Some(SavedVec2 {
            x: f32::NEG_INFINITY,
            y: 0.0,
        });
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("non-finite move target");
        assert!(error.contains("entities[0].move_target_world.x"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[1]
            .interactable
            .as_mut()
            .expect("interactable")
            .interaction_radius = -0.1;
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("negative interaction radius");
        assert!(error.contains("entities[1].interactable.interaction_radius"));
        assert!(error.contains("expected >= 0"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].job_state = SavedJobState::Working {
            target_save_id: 20,
            remaining_time: -0.1,
        };
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("negative remaining time");
        assert!(error.contains("entities[0].job_state.remaining_time"));
        assert!(error.contains("expected >= 0"));
    }

    #[test]
    fn validate_reports_next_save_id_path_and_expected_actual() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.next_save_id = 20;
        let error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("invalid next_save_id");
        assert!(error.contains("next_save_id"));
        assert!(error.contains("expected value greater than max used save_id"));
        assert!(error.contains("got 20"));
    }

    #[test]
    fn corrupted_json_parse_or_validation_never_mutates_world_or_scene_state() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();
        scene.selected_entity = Some(world.entities()[0].id);

        let before = capture_scene_restore_state(&scene, &world);

        let mut value = serde_json::to_value(sample_save_game(SavedSceneKey::A)).expect("to_value");
        value["entities"][0]["job_state"] = json!("Broken");
        let raw = serde_json::to_string(&value).expect("json");
        let parse_error = GameplayScene::parse_save_game_json(&raw).expect_err("parse should fail");
        assert!(parse_error.contains("entities[0].job_state"));

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].interaction_target_save_id = Some(9999);
        let validation_error = GameplayScene::validate_save_game(&save, SavedSceneKey::A)
            .expect_err("validation should fail");
        assert!(validation_error.contains("entities[0].interaction_target_save_id"));

        let after = capture_scene_restore_state(&scene, &world);
        assert_eq!(after, before);
    }

    #[test]
    fn save_id_based_remap_restores_refs_correctly() {
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
        let target_save_id = scene
            .entity_save_ids
            .get(&target.id)
            .copied()
            .expect("target save id");
        assert_eq!(
            actor.order_state,
            OrderState::Working {
                target_save_id,
                remaining_time: 1.5
            }
        );
        assert_eq!(scene.resource_count, 2);
    }

    #[test]
    fn reorder_entities_before_load_still_resolves_refs_by_save_id() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities.swap(0, 1);
        GameplayScene::validate_save_game(&save, SavedSceneKey::A).expect("valid");
        scene.apply_save_game(save, &mut world).expect("apply");

        let player_id = scene.player_id.expect("player");
        let selected_id = scene.selected_entity.expect("selected");
        assert_eq!(player_id, selected_id);

        let player = world.find_entity(player_id).expect("player entity");
        let target_save_id = match player.order_state {
            OrderState::Working { target_save_id, .. } => target_save_id,
            _ => panic!("expected working"),
        };
        let target_id = scene
            .save_id_to_entity
            .get(&target_save_id)
            .copied()
            .expect("interaction target");
        assert!(world
            .find_entity(target_id)
            .expect("target entity")
            .interactable
            .is_some());
        assert_eq!(
            player.order_state,
            OrderState::Working {
                target_save_id,
                remaining_time: 1.5
            }
        );
    }

    #[test]
    fn sync_save_id_map_assigns_only_missing_and_preserves_existing_ids() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let first = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "first",
            },
        );
        let second = world.spawn(
            Transform {
                position: Vec2 { x: 1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "second",
            },
        );
        world.apply_pending();

        scene.entity_save_ids.insert(first, 42);
        scene.next_save_id = 100;
        scene.sync_save_id_map_with_world(&world).expect("sync");
        assert_eq!(scene.entity_save_ids.get(&first).copied(), Some(42));
        assert_eq!(scene.entity_save_ids.get(&second).copied(), Some(100));
        assert_eq!(scene.next_save_id, 101);

        scene
            .sync_save_id_map_with_world(&world)
            .expect("sync again");
        assert_eq!(scene.entity_save_ids.get(&first).copied(), Some(42));
        assert_eq!(scene.entity_save_ids.get(&second).copied(), Some(100));
        assert_eq!(scene.next_save_id, 101);

        assert!(world.despawn(first));
        world.apply_pending();
        scene
            .sync_save_id_map_with_world(&world)
            .expect("sync remove");
        assert_eq!(scene.entity_save_ids.get(&first), None);
        assert_eq!(scene.entity_save_ids.get(&second).copied(), Some(100));
        assert_eq!(scene.next_save_id, 101);
    }

    #[test]
    fn validate_rejects_duplicate_save_ids() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[1].save_id = save.entities[0].save_id;
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());
    }

    #[test]
    fn validate_rejects_missing_save_id_references() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].interaction_target_save_id = Some(9999);
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities[0].job_state = SavedJobState::Working {
            target_save_id: 9999,
            remaining_time: 1.5,
        };
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());
    }

    #[test]
    fn validate_rejects_invalid_next_save_id() {
        let mut save = sample_save_game(SavedSceneKey::A);
        save.next_save_id = 20;
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());

        let mut save = sample_save_game(SavedSceneKey::A);
        save.entities.clear();
        save.selected_entity_save_id = None;
        save.player_entity_save_id = None;
        save.next_save_id = 1;
        assert!(GameplayScene::validate_save_game(&save, SavedSceneKey::A).is_err());
    }

    #[test]
    fn saved_runtime_order_precedence_working_then_interact_then_move() {
        let mut saved = SavedEntityRuntime {
            save_id: 10,
            position: SavedVec2 { x: 0.0, y: 0.0 },
            rotation_radians: None,
            selectable: true,
            actor: true,
            move_target_world: Some(SavedVec2 { x: 9.0, y: 9.0 }),
            interaction_target_save_id: Some(20),
            job_state: SavedJobState::Idle,
            interactable: None,
        };

        assert_eq!(
            GameplayScene::runtime_order_state_from_saved(&saved),
            OrderState::Interact { target_save_id: 20 }
        );

        saved.job_state = SavedJobState::Working {
            target_save_id: 20,
            remaining_time: 1.25,
        };
        assert_eq!(
            GameplayScene::runtime_order_state_from_saved(&saved),
            OrderState::Working {
                target_save_id: 20,
                remaining_time: 1.25
            }
        );
    }

    #[test]
    fn move_order_save_load_midway_matches_baseline_trajectory() {
        let mut baseline_scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut baseline_world = SceneWorld::default();
        seed_def_database(&mut baseline_world);
        let baseline_actor = baseline_world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        baseline_world.apply_pending();
        baseline_world
            .find_entity_mut(baseline_actor)
            .expect("actor")
            .selectable = true;
        baseline_scene.player_id = Some(baseline_actor);
        baseline_scene.selected_entity = Some(baseline_actor);

        let mut resumed_scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut resumed_world = SceneWorld::default();
        seed_def_database(&mut resumed_world);
        let resumed_actor = resumed_world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        resumed_world.apply_pending();
        resumed_world
            .find_entity_mut(resumed_actor)
            .expect("actor")
            .selectable = true;
        resumed_scene.player_id = Some(resumed_actor);
        resumed_scene.selected_entity = Some(resumed_actor);

        let click = right_click_snapshot(Vec2 { x: 672.0, y: 360.0 }, (1280, 720));
        baseline_scene.update(1.0 / 60.0, &click, &mut baseline_world);
        resumed_scene.update(1.0 / 60.0, &click, &mut resumed_world);

        advance(&mut baseline_scene, &mut baseline_world, 8, 0.1);
        advance(&mut resumed_scene, &mut resumed_world, 8, 0.1);

        let save = resumed_scene.build_save_game(&resumed_world).expect("save");
        {
            let entity = resumed_world.find_entity_mut(resumed_actor).expect("actor");
            entity.transform.position = Vec2 { x: 123.0, y: 456.0 };
            entity.order_state = OrderState::Idle;
        }
        resumed_scene
            .apply_save_game(save, &mut resumed_world)
            .expect("apply");

        advance(&mut baseline_scene, &mut baseline_world, 12, 0.1);
        advance(&mut resumed_scene, &mut resumed_world, 12, 0.1);

        let baseline = baseline_world
            .find_entity(baseline_actor)
            .expect("baseline");
        let resumed = resumed_world
            .find_entity(resumed_scene.player_id.expect("resumed player"))
            .expect("resumed");
        assert_vec2_close(
            resumed.transform.position,
            baseline.transform.position,
            0.0001,
        );
        assert_eq!(resumed.order_state, baseline.order_state);
    }

    #[test]
    fn interact_workflow_save_load_mid_work_matches_baseline_outcome() {
        let mut baseline_scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut baseline_world = SceneWorld::default();
        seed_def_database(&mut baseline_world);
        let baseline_actor = baseline_world.spawn_actor(
            Transform {
                position: Vec2 { x: -2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let _baseline_pile =
            spawn_interactable_pile(&mut baseline_world, Vec2 { x: 0.0, y: 0.0 }, 1);
        baseline_scene.player_id = Some(baseline_actor);
        baseline_scene.selected_entity = Some(baseline_actor);
        baseline_scene
            .sync_save_id_map_with_world(&baseline_world)
            .expect("sync");

        let mut resumed_scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut resumed_world = SceneWorld::default();
        seed_def_database(&mut resumed_world);
        let resumed_actor = resumed_world.spawn_actor(
            Transform {
                position: Vec2 { x: -2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        let _resumed_pile = spawn_interactable_pile(&mut resumed_world, Vec2 { x: 0.0, y: 0.0 }, 1);
        resumed_scene.player_id = Some(resumed_actor);
        resumed_scene.selected_entity = Some(resumed_actor);
        resumed_scene
            .sync_save_id_map_with_world(&resumed_world)
            .expect("sync");

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        baseline_scene.update(1.0 / 60.0, &click, &mut baseline_world);
        resumed_scene.update(1.0 / 60.0, &click, &mut resumed_world);

        let mut saw_working = false;
        for _ in 0..30 {
            advance(&mut baseline_scene, &mut baseline_world, 1, 0.1);
            advance(&mut resumed_scene, &mut resumed_world, 1, 0.1);
            let baseline_state = baseline_world
                .find_entity(baseline_actor)
                .expect("actor")
                .order_state;
            let resumed_state = resumed_world
                .find_entity(resumed_actor)
                .expect("actor")
                .order_state;
            if matches!(baseline_state, OrderState::Working { .. })
                && matches!(resumed_state, OrderState::Working { .. })
            {
                saw_working = true;
                break;
            }
        }
        assert!(saw_working, "expected both branches to enter working state");

        advance(&mut baseline_scene, &mut baseline_world, 3, 0.1);
        advance(&mut resumed_scene, &mut resumed_world, 3, 0.1);

        let save = resumed_scene.build_save_game(&resumed_world).expect("save");
        resumed_scene.resource_count = 99;
        resumed_scene
            .apply_save_game(save, &mut resumed_world)
            .expect("apply");

        advance(&mut baseline_scene, &mut baseline_world, 30, 0.1);
        advance(&mut resumed_scene, &mut resumed_world, 30, 0.1);

        let baseline_actor_entity = baseline_world.find_entity(baseline_actor).expect("actor");
        let resumed_actor_entity = resumed_world
            .find_entity(resumed_scene.player_id.expect("resumed player"))
            .expect("actor");
        assert_eq!(
            baseline_actor_entity.order_state,
            resumed_actor_entity.order_state
        );
        assert_eq!(resumed_scene.resource_count, baseline_scene.resource_count);
        assert_eq!(
            interactable_entity_count(&resumed_world),
            interactable_entity_count(&baseline_world)
        );
    }

    #[test]
    fn determinism_script_pure_move_digest_matches_replay() {
        let fixed_dt = 0.1;
        let steps = 20;
        let window_size = (1280, 720);
        let checkpoints = [
            ScriptCheckpoint {
                tick: 0,
                label: "after_select",
            },
            ScriptCheckpoint {
                tick: 1,
                label: "after_order",
            },
            ScriptCheckpoint {
                tick: 3,
                label: "mid_move",
            },
            ScriptCheckpoint {
                tick: 10,
                label: "settled",
            },
            ScriptCheckpoint {
                tick: 19,
                label: "final",
            },
        ];
        let script_actions = [
            (0usize, TickAction::SelectWorld(Vec2 { x: 0.0, y: 0.0 })),
            (1usize, TickAction::RightClickWorld(Vec2 { x: 1.5, y: 0.0 })),
        ];

        let (mut scene_a, mut world_a, actor_save_id_a) = make_move_fixture();
        let digest_a = run_script_and_capture(
            &mut scene_a,
            &mut world_a,
            fixed_dt,
            steps,
            &script_actions,
            &checkpoints,
            window_size,
        );

        let (mut scene_b, mut world_b, actor_save_id_b) = make_move_fixture();
        let digest_b = run_script_and_capture(
            &mut scene_b,
            &mut world_b,
            fixed_dt,
            steps,
            &script_actions,
            &checkpoints,
            window_size,
        );

        assert_eq!(actor_save_id_a, actor_save_id_b);
        assert_eq!(digest_a, digest_b);
    }

    #[test]
    fn determinism_script_interact_work_despawn_digest_matches_replay() {
        let fixed_dt = 0.1;
        let steps = 35;
        let window_size = (1280, 720);
        let checkpoints = [
            ScriptCheckpoint {
                tick: 1,
                label: "after_order",
            },
            ScriptCheckpoint {
                tick: 5,
                label: "working_started",
            },
            ScriptCheckpoint {
                tick: 26,
                label: "post_completion",
            },
            ScriptCheckpoint {
                tick: 34,
                label: "final",
            },
        ];
        let script_actions = [
            (0usize, TickAction::SelectWorld(Vec2 { x: -2.0, y: 0.0 })),
            (1usize, TickAction::RightClickWorld(Vec2 { x: 0.0, y: 0.0 })),
        ];

        let (mut scene_a, mut world_a, actor_save_id_a, target_save_id_a) = make_interact_fixture();
        let digest_a = run_script_and_capture(
            &mut scene_a,
            &mut world_a,
            fixed_dt,
            steps,
            &script_actions,
            &checkpoints,
            window_size,
        );

        let (mut scene_b, mut world_b, actor_save_id_b, target_save_id_b) = make_interact_fixture();
        let digest_b = run_script_and_capture(
            &mut scene_b,
            &mut world_b,
            fixed_dt,
            steps,
            &script_actions,
            &checkpoints,
            window_size,
        );

        assert_eq!(actor_save_id_a, actor_save_id_b);
        assert_eq!(target_save_id_a, target_save_id_b);
        assert_eq!(digest_a, digest_b);

        let (_, final_digest) = digest_a.last().expect("final checkpoint digest");
        assert_eq!(final_digest.resource_count, 1);
        assert!(
            !final_digest
                .entities
                .iter()
                .any(|entity| entity.save_id == target_save_id_a),
            "expected target save_id {} to be consumed by completion",
            target_save_id_a
        );
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
            entity.order_state = OrderState::MoveTo {
                point: Vec2 { x: 2.0, y: 0.0 },
            };
        }
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);
        scene.resource_count = 5;
        world.camera_mut().position = Vec2 { x: 1.0, y: -1.0 };
        world.camera_mut().set_zoom_clamped(1.6);

        let save = scene.build_save_game(&world).expect("save");
        {
            let entity = world.find_entity_mut(actor).expect("actor");
            entity.transform.position = Vec2 { x: 9.0, y: 9.0 };
            entity.order_state = OrderState::Idle;
        }
        scene.selected_entity = None;
        scene.resource_count = 0;
        world.camera_mut().position = Vec2 { x: -4.0, y: 7.0 };
        world.camera_mut().set_zoom_clamped(0.7);

        GameplayScene::validate_save_game(&save, SavedSceneKey::A).expect("valid");
        scene.apply_save_game(save, &mut world).expect("apply");

        let restored_actor = world
            .find_entity(scene.player_id.expect("player"))
            .expect("actor");
        assert_eq!(scene.selected_entity, Some(restored_actor.id));
        assert_eq!(scene.resource_count, 5);
        assert_eq!(world.camera().position, Vec2 { x: 1.0, y: -1.0 });
        assert!((world.camera().zoom - 1.6).abs() < 0.0001);
        assert_eq!(restored_actor.transform.position, Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(
            restored_actor.order_state,
            OrderState::MoveTo {
                point: Vec2 { x: 2.0, y: 0.0 }
            }
        );
        let restored_actor_id = restored_actor.id;

        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        let advanced_actor = world.find_entity(restored_actor_id).expect("actor");
        assert!(advanced_actor.transform.position.x > 0.0);
    }
}
