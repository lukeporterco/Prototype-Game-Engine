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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
enum ActiveFloor {
    Rooftop,
    #[default]
    Main,
    Basement,
}

impl ActiveFloor {
    fn from_engine_floor(value: engine::FloorId) -> Self {
        match value {
            engine::FloorId::Rooftop => Self::Rooftop,
            engine::FloorId::Main => Self::Main,
            engine::FloorId::Basement => Self::Basement,
        }
    }

    fn to_engine_floor(self) -> engine::FloorId {
        match self {
            Self::Rooftop => engine::FloorId::Rooftop,
            Self::Main => engine::FloorId::Main,
            Self::Basement => engine::FloorId::Basement,
        }
    }

    fn as_token(self) -> &'static str {
        match self {
            Self::Rooftop => "rooftop",
            Self::Main => "main",
            Self::Basement => "basement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PawnControlRole {
    PlayerPawn,
    Settler,
    Npc,
}

impl PawnControlRole {
    fn is_orderable(self) -> bool {
        matches!(self, Self::PlayerPawn | Self::Settler)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct JobId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobKind {
    MoveToPoint,
    UseInteractable,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum JobTarget {
    WorldPoint(Vec2),
    TargetSaveId(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobState {
    Open,
    Reserved,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobPhase {
    Idle,
    Navigating,
    Interacting,
}

#[derive(Debug, Clone, PartialEq)]
struct JobRecord {
    id: JobId,
    kind: JobKind,
    target: JobTarget,
    priority: i32,
    reserved_by: Option<EntityId>,
    state: JobState,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct JobBoard {
    next_job_id: u64,
    jobs_by_id: std::collections::BTreeMap<JobId, JobRecord>,
    assigned_job_by_entity: HashMap<EntityId, JobId>,
}

impl JobBoard {
    fn clear(&mut self) {
        self.next_job_id = 0;
        self.jobs_by_id.clear();
        self.assigned_job_by_entity.clear();
    }

    fn create_job(&mut self, kind: JobKind, target: JobTarget, priority: i32) -> JobId {
        let id = JobId(self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);
        self.jobs_by_id.insert(
            id,
            JobRecord {
                id,
                kind,
                target,
                priority,
                reserved_by: None,
                state: JobState::Open,
            },
        );
        id
    }

    fn assign_job_to_entity(&mut self, job_id: JobId, actor_id: EntityId) -> bool {
        let Some(job) = self.jobs_by_id.get_mut(&job_id) else {
            return false;
        };
        job.reserved_by = Some(actor_id);
        job.state = JobState::Reserved;
        self.assigned_job_by_entity.insert(actor_id, job_id);
        true
    }

    fn assigned_job_id(&self, actor_id: EntityId) -> Option<JobId> {
        self.assigned_job_by_entity.get(&actor_id).copied()
    }

    fn job(&self, job_id: JobId) -> Option<&JobRecord> {
        self.jobs_by_id.get(&job_id)
    }

    fn mark_job_in_progress(&mut self, job_id: JobId) {
        if let Some(job) = self.jobs_by_id.get_mut(&job_id) {
            job.state = JobState::InProgress;
        }
    }

    fn mark_job_state(&mut self, job_id: JobId, state: JobState) {
        if let Some(job) = self.jobs_by_id.get_mut(&job_id) {
            job.state = state;
            if matches!(state, JobState::Completed | JobState::Failed) {
                job.reserved_by = None;
            }
        }
    }

    fn clear_assignment_for_entity(&mut self, actor_id: EntityId) -> Option<JobId> {
        self.assigned_job_by_entity.remove(&actor_id)
    }

    fn retain_live_entities(&mut self, live_ids: &HashSet<EntityId>) {
        self.assigned_job_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));
        for job in self.jobs_by_id.values_mut() {
            if let Some(actor_id) = job.reserved_by {
                if !live_ids.contains(&actor_id) {
                    job.reserved_by = None;
                    if matches!(job.state, JobState::Reserved | JobState::InProgress) {
                        job.state = JobState::Failed;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SavedInteractableKind {
    ResourcePile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SavedFloorId {
    Rooftop,
    Main,
    Basement,
}

impl SavedFloorId {
    fn from_engine_floor(value: engine::FloorId) -> Self {
        match value {
            engine::FloorId::Rooftop => Self::Rooftop,
            engine::FloorId::Main => Self::Main,
            engine::FloorId::Basement => Self::Basement,
        }
    }

    fn to_engine_floor(self) -> engine::FloorId {
        match self {
            Self::Rooftop => engine::FloorId::Rooftop,
            Self::Main => engine::FloorId::Main,
            Self::Basement => engine::FloorId::Basement,
        }
    }
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
    #[serde(default)]
    floor: Option<SavedFloorId>,
    selectable: bool,
    actor: bool,
    #[serde(default)]
    archetype_def_name: Option<String>,
    move_target_world: Option<SavedVec2>,
    interaction_target_save_id: Option<u64>,
    job_state: SavedJobState,
    interactable: Option<SavedInteractableRuntime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SaveGame {
    save_version: u32,
    scene_key: SavedSceneKey,
    #[serde(default)]
    active_floor: Option<SavedFloorId>,
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
    active_floor: ActiveFloor,
}

impl<'a> WorldView<'a> {
    fn new(world: &'a SceneWorld, active_floor: ActiveFloor) -> Self {
        Self { world, active_floor }
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
            .pick_topmost_interactable_at_cursor(
                cursor_position_px,
                window_size,
                Some(self.active_floor.to_engine_floor()),
            )
    }

    fn pick_topmost_selectable_at_cursor(
        &self,
        cursor_position_px: Vec2,
        window_size: (u32, u32),
    ) -> Option<EntityId> {
        self.world
            .pick_topmost_selectable_at_cursor(
                cursor_position_px,
                window_size,
                Some(self.active_floor.to_engine_floor()),
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StatusId(&'static str);

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
struct Health {
    current: u32,
    max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EffectiveCombatAiParams {
    health_max: u32,
    base_damage: u32,
    aggro_radius: f32,
    attack_range: f32,
    attack_cooldown_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveStatus {
    status_id: StatusId,
    remaining_seconds: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct StatusSet {
    active: Vec<ActiveStatus>,
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
    fn from_home_position(home_position: Vec2, params: EffectiveCombatAiParams) -> Self {
        Self {
            state: AiState::Idle,
            home_position,
            wander_target: None,
            aggro_radius: params.aggro_radius,
            attack_range: params.attack_range,
            cooldown_seconds: params.attack_cooldown_seconds,
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
        duration_seconds: f32,
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

