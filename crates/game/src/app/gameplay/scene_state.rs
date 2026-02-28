struct GameplayScene {
    scene_name: &'static str,
    switch_target: SceneKey,
    player_spawn: Vec2,
    player_id: Option<EntityId>,
    selected_entity: Option<EntityId>,
    active_floor: ActiveFloor,
    player_move_speed: f32,
    resource_count: u32,
    interactable_cache: Vec<(EntityId, Vec2, f32)>,
    interactable_lookup_by_save_id: HashMap<u64, (EntityId, Vec2, f32)>,
    completed_target_ids: Vec<EntityId>,
    entity_save_ids: HashMap<EntityId, u64>,
    save_id_to_entity: HashMap<u64, EntityId>,
    next_save_id: u64,
    health_by_entity: HashMap<EntityId, Health>,
    damage_by_entity: HashMap<EntityId, u32>,
    status_sets_by_entity: HashMap<EntityId, StatusSet>,
    ai_agents_by_entity: HashMap<EntityId, AiAgent>,
    active_interactions_by_actor: HashMap<EntityId, ActiveInteraction>,
    completed_attack_pairs_this_tick: HashSet<(EntityId, EntityId)>,
    next_interaction_id: u64,
    reselect_player_on_respawn: bool,
    selected_completion_enqueued_this_tick: bool,
    systems_host: GameplaySystemsHost,
    system_events: GameplayEventBus,
    system_intents: GameplayIntentQueue,
    system_order_text: String,
    combat_chaser_scenario: CombatChaserScenarioSlot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CombatChaserScenarioSlot {
    chaser_id: Option<EntityId>,
    dummy_id: Option<EntityId>,
}

impl GameplayScene {
    fn new(scene_name: &'static str, switch_target: SceneKey, player_spawn: Vec2) -> Self {
        Self {
            scene_name,
            switch_target,
            player_spawn,
            player_id: None,
            selected_entity: None,
            active_floor: ActiveFloor::Main,
            player_move_speed: 5.0,
            resource_count: 0,
            interactable_cache: Vec::new(),
            interactable_lookup_by_save_id: HashMap::new(),
            completed_target_ids: Vec::new(),
            entity_save_ids: HashMap::new(),
            save_id_to_entity: HashMap::new(),
            next_save_id: 0,
            health_by_entity: HashMap::new(),
            damage_by_entity: HashMap::new(),
            status_sets_by_entity: HashMap::new(),
            ai_agents_by_entity: HashMap::new(),
            active_interactions_by_actor: HashMap::new(),
            completed_attack_pairs_this_tick: HashSet::new(),
            next_interaction_id: 0,
            reselect_player_on_respawn: false,
            selected_completion_enqueued_this_tick: false,
            systems_host: GameplaySystemsHost::default(),
            system_events: GameplayEventBus::default(),
            system_intents: GameplayIntentQueue::default(),
            system_order_text: String::new(),
            combat_chaser_scenario: CombatChaserScenarioSlot::default(),
        }
    }

    fn scene_key(&self) -> SceneKey {
        match self.switch_target {
            SceneKey::A => SceneKey::B,
            SceneKey::B => SceneKey::A,
        }
    }

    fn active_floor_engine(&self) -> FloorId {
        self.active_floor.to_engine_floor()
    }

    fn entity_is_on_active_floor(&self, entity: &engine::Entity) -> bool {
        entity.floor == self.active_floor_engine()
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
        self.damage_by_entity.clear();
        self.status_sets_by_entity.clear();
        self.ai_agents_by_entity.clear();
        self.active_interactions_by_actor.clear();
        self.completed_attack_pairs_this_tick.clear();
        self.next_interaction_id = 0;
    }

    fn sync_runtime_component_stores_with_world(&mut self, world: &SceneWorld) {
        let live_ids: HashSet<EntityId> = world.entities().iter().map(|entity| entity.id).collect();
        self.health_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));
        self.damage_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));
        self.status_sets_by_entity
            .retain(|entity_id, _| live_ids.contains(entity_id));

        for entity in world.entities() {
            if entity.actor {
                let defaults = Self::effective_combat_ai_params(None);
                self.health_by_entity.entry(entity.id).or_insert(Health {
                    current: defaults.health_max,
                    max: defaults.health_max,
                });
                self.damage_by_entity
                    .entry(entity.id)
                    .or_insert(defaults.base_damage);
            } else {
                self.health_by_entity.remove(&entity.id);
                self.damage_by_entity.remove(&entity.id);
            }
            self.status_sets_by_entity.entry(entity.id).or_default();
        }
    }

    fn rebuild_ai_agents_from_world(&mut self, world: &SceneWorld) {
        self.ai_agents_by_entity.clear();
        for entity in world.entities() {
            if !entity.actor || Some(entity.id) == self.player_id {
                continue;
            }
            let defaults = Self::effective_combat_ai_params(None);
            self.ai_agents_by_entity.insert(
                entity.id,
                AiAgent::from_home_position(entity.transform.position, defaults),
            );
        }
    }

    fn archetype_uses_combat_ai(archetype: &EntityArchetype) -> bool {
        archetype.aggro_radius.is_some()
            || archetype.attack_range.is_some()
            || archetype.attack_cooldown_seconds.is_some()
    }

    fn ai_state_counts(&self) -> AiStateCounts {
        let mut counts = AiStateCounts::default();
        for agent in self.ai_agents_by_entity.values() {
            counts.record(agent.state);
        }
        counts
    }

    fn format_dump_state(&self, world: &SceneWorld) -> String {
        let player_text = self
            .player_id
            .and_then(|player_id| {
                world
                    .find_entity(player_id)
                    .map(|entity| (player_id, entity))
            })
            .map(|(player_id, entity)| {
                format!(
                    "{}@({:.2},{:.2})",
                    player_id.0, entity.transform.position.x, entity.transform.position.y
                )
            })
            .unwrap_or_else(|| "none".to_string());
        let camera = world.camera();
        let selected_text = self
            .selected_entity
            .map(|id| id.0.to_string())
            .unwrap_or_else(|| "none".to_string());
        let target_text = self
            .debug_selected_target(world)
            .map(|target| format!("({:.2},{:.2})", target.x, target.y))
            .unwrap_or_else(|| "none".to_string());
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
        let event_counts = self.system_events.last_tick_counts();
        let intent_stats = self.system_intents.last_tick_apply_stats();

        format!(
            "dump.state v1 | player:{} | cam:({:.2},{:.2},{:.2}) | sel:{} | tgt:{} | cnt:ent:{} act:{} int:{} | ev:{} | evk:is:{} ic:{} dm:{} dd:{} sa:{} se:{} | in:{} | ink:sp:{} mt:{} de:{} dmg:{} add:{} rem:{} si:{} ci:{} ca:{} | in_bad:{}",
            player_text,
            camera.position.x,
            camera.position.y,
            camera.zoom,
            selected_text,
            target_text,
            entity_count,
            actor_count,
            interactable_count,
            event_counts.total,
            event_counts.interaction_started,
            event_counts.interaction_completed,
            event_counts.entity_damaged,
            event_counts.entity_died,
            event_counts.status_applied,
            event_counts.status_expired,
            intent_stats.total,
            intent_stats.spawn_by_archetype_id,
            intent_stats.set_move_target,
            intent_stats.despawn_entity,
            intent_stats.apply_damage,
            intent_stats.add_status,
            intent_stats.remove_status,
            intent_stats.start_interaction,
            intent_stats.complete_interaction,
            intent_stats.cancel_interaction,
            intent_stats.invalid_target_count
        )
    }

    fn format_dump_ai(&self, world: &SceneWorld) -> String {
        let counts = self.ai_state_counts();
        let near_text = if let Some(player) = self.player_id.and_then(|id| world.find_entity(id)) {
            let mut nearest = self
                .ai_agents_by_entity
                .keys()
                .filter_map(|entity_id| {
                    world.find_entity(*entity_id).map(|entity| {
                        let dx = entity.transform.position.x - player.transform.position.x;
                        let dy = entity.transform.position.y - player.transform.position.y;
                        let distance = (dx * dx + dy * dy).sqrt();
                        (*entity_id, distance)
                    })
                })
                .collect::<Vec<_>>();
            nearest.sort_by(|(a_id, a_dist), (b_id, b_dist)| {
                a_dist.total_cmp(b_dist).then_with(|| a_id.0.cmp(&b_id.0))
            });
            if nearest.is_empty() {
                "none".to_string()
            } else {
                nearest
                    .into_iter()
                    .take(5)
                    .map(|(entity_id, distance)| format!("{}@{distance:.2}", entity_id.0))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        } else {
            "none".to_string()
        };

        format!(
            "dump.ai v1 | cnt:id:{} wa:{} ch:{} use:{} | near:{}",
            counts.idle, counts.wander, counts.chase, counts.use_interaction, near_text
        )
    }

    fn effective_combat_ai_params(archetype: Option<&EntityArchetype>) -> EffectiveCombatAiParams {
        EffectiveCombatAiParams {
            health_max: archetype
                .and_then(|value| value.health_max)
                .unwrap_or(DEFAULT_MAX_HEALTH),
            base_damage: archetype
                .and_then(|value| value.base_damage)
                .unwrap_or(ATTACK_DAMAGE_PER_HIT),
            aggro_radius: archetype
                .and_then(|value| value.aggro_radius)
                .unwrap_or(AI_AGGRO_RADIUS_UNITS),
            attack_range: archetype
                .and_then(|value| value.attack_range)
                .unwrap_or(AI_ATTACK_RANGE_UNITS),
            attack_cooldown_seconds: archetype
                .and_then(|value| value.attack_cooldown_seconds)
                .unwrap_or(AI_ATTACK_COOLDOWN_SECONDS),
        }
    }

    fn apply_spawn_intent_now(
        &mut self,
        world: &mut SceneWorld,
        def_name: &str,
        position: Vec2,
    ) -> SaveLoadResult<EntityId> {
        let archetype = try_resolve_archetype_by_name(world, def_name)?;
        let stats = self.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::SpawnByArchetypeId {
                archetype_id: archetype.id,
                position,
            }],
            world,
        );
        let Some(spawned_id) = stats.spawned_entity_ids.first().copied() else {
            return Err(format!(
                "failed to spawn '{def_name}' via scenario setup (invalid_target:{})",
                stats.invalid_target_count
            ));
        };
        Ok(spawned_id)
    }

    fn apply_despawn_intent_now(
        &mut self,
        world: &mut SceneWorld,
        entity_id: EntityId,
    ) -> SaveLoadResult<()> {
        if world.find_entity(entity_id).is_none() {
            return Ok(());
        }
        let stats = self.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::DespawnEntity { entity_id }],
            world,
        );
        if stats.invalid_target_count > 0 {
            return Err(format!(
                "failed to despawn entity {} via scenario setup",
                entity_id.0
            ));
        }
        Ok(())
    }

    fn run_scenario_setup_combat_chaser(
        &mut self,
        world: &mut SceneWorld,
    ) -> SaveLoadResult<(EntityId, EntityId, EntityId)> {
        let stale_ids = [
            self.player_id,
            self.combat_chaser_scenario.chaser_id,
            self.combat_chaser_scenario.dummy_id,
        ];
        for stale_id in stale_ids.into_iter().flatten() {
            self.apply_despawn_intent_now(world, stale_id)?;
        }

        self.player_id = None;
        self.selected_entity = None;
        self.combat_chaser_scenario = CombatChaserScenarioSlot::default();

        let player_id =
            self.apply_spawn_intent_now(world, "proto.player", COMBAT_CHASER_PLAYER_POS)?;
        let chaser_id =
            self.apply_spawn_intent_now(world, "proto.npc_chaser", COMBAT_CHASER_CHASER_POS)?;
        let dummy_id =
            self.apply_spawn_intent_now(world, "proto.npc_dummy", COMBAT_CHASER_DUMMY_POS)?;

        self.selected_entity = Some(player_id);
        self.combat_chaser_scenario = CombatChaserScenarioSlot {
            chaser_id: Some(chaser_id),
            dummy_id: Some(dummy_id),
        };

        Ok((player_id, chaser_id, dummy_id))
    }

    fn status_multiplier(status_id: StatusId) -> f32 {
        match status_id {
            STATUS_SLOW => STATUS_SLOW_MULTIPLIER,
            _ => 1.0,
        }
    }

    fn movement_speed_multiplier_for_entity(&self, entity_id: EntityId) -> f32 {
        let Some(status_set) = self.status_sets_by_entity.get(&entity_id) else {
            return 1.0;
        };
        status_set
            .active
            .iter()
            .map(|status| Self::status_multiplier(status.status_id))
            .product()
    }

    fn effective_move_speed_for_entity(&self, entity_id: EntityId, base_speed: f32) -> f32 {
        base_speed * self.movement_speed_multiplier_for_entity(entity_id)
    }

    fn upsert_status_with_refresh(
        status_set: &mut StatusSet,
        status_id: StatusId,
        duration_seconds: f32,
    ) {
        if let Some(existing) = status_set
            .active
            .iter_mut()
            .find(|status| status.status_id == status_id)
        {
            existing.remaining_seconds = duration_seconds;
            return;
        }
        status_set.active.push(ActiveStatus {
            status_id,
            remaining_seconds: duration_seconds,
        });
    }

    fn remove_status_if_present(status_set: &mut StatusSet, status_id: StatusId) -> bool {
        let before_len = status_set.active.len();
        status_set
            .active
            .retain(|status| status.status_id != status_id);
        before_len != status_set.active.len()
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
        self.completed_attack_pairs_this_tick.clear();
        self.selected_completion_enqueued_this_tick = false;
        self.systems_host.run_once_per_tick(
            fixed_dt_seconds,
            WorldView::new(world, self.active_floor),
            input,
            self.player_id,
            self.selected_entity,
            &mut self.ai_agents_by_entity,
            &mut self.status_sets_by_entity,
            &mut self.active_interactions_by_actor,
            &self.damage_by_entity,
            &mut self.completed_attack_pairs_this_tick,
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
        let mut pending = intents;
        let mut cursor = 0usize;

        while cursor < pending.len() {
            let intent = pending[cursor];
            cursor = cursor.saturating_add(1);
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
                    let effective_params = Self::effective_combat_ai_params(Some(&archetype));
                    let archetype_uses_combat_ai = Self::archetype_uses_combat_ai(&archetype);
                    if archetype.def_name == "proto.player"
                        && self
                            .player_id
                            .and_then(|player_id| world.find_entity(player_id))
                            .is_some()
                    {
                        debug!("rejecting proto.player spawn because player already exists");
                        stats.record_invalid_target();
                        continue;
                    }

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
                    if has_actor_tag {
                        if let Some(entity) = world.find_entity_mut(entity_id) {
                            entity.selectable = true;
                        }
                    }
                    stats.spawned_entity_ids.push(entity_id);

                    match self.alloc_next_save_id() {
                        Ok(save_id) => {
                            self.entity_save_ids.insert(entity_id, save_id);
                            self.save_id_to_entity.insert(save_id, entity_id);
                        }
                        Err(_) => stats.record_invalid_target(),
                    }
                    if has_actor_tag {
                        self.health_by_entity.entry(entity_id).or_insert(Health {
                            current: effective_params.health_max,
                            max: effective_params.health_max,
                        });
                        self.damage_by_entity
                            .entry(entity_id)
                            .or_insert(effective_params.base_damage);
                    }
                    self.status_sets_by_entity.entry(entity_id).or_default();

                    if has_interactable_tag {
                        if let Some(entity) = world.find_entity_mut(entity_id) {
                            entity.interactable = Some(Interactable {
                                kind: InteractableKind::ResourcePile,
                                interaction_radius: RESOURCE_PILE_INTERACTION_RADIUS,
                                remaining_uses: RESOURCE_PILE_STARTING_USES,
                            });
                        }
                    }

                    if has_actor_tag
                        && Some(entity_id) != self.player_id
                        && archetype_uses_combat_ai
                    {
                        self.ai_agents_by_entity.insert(
                            entity_id,
                            AiAgent::from_home_position(position, effective_params),
                        );
                    }
                    if archetype.def_name == "proto.player" {
                        let current_player_missing = self
                            .player_id
                            .and_then(|id| world.find_entity(id))
                            .is_none();
                        if self.player_id.is_none() || current_player_missing {
                            self.player_id = Some(entity_id);
                            self.ai_agents_by_entity.remove(&entity_id);
                        }
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
                        if self.player_id == Some(entity_id) {
                            self.reselect_player_on_respawn = true;
                        }
                        self.selected_entity = None;
                    }
                    if self.player_id == Some(entity_id) {
                        self.player_id = None;
                    }
                    self.health_by_entity.remove(&entity_id);
                    self.damage_by_entity.remove(&entity_id);
                    self.status_sets_by_entity.remove(&entity_id);
                    self.ai_agents_by_entity.remove(&entity_id);
                    self.active_interactions_by_actor.remove(&entity_id);
                }
                GameplayIntent::ApplyDamage { entity_id, amount } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    let Some(health) = self.health_by_entity.get_mut(&entity_id) else {
                        debug!(
                            entity_id = entity_id.0,
                            amount, "apply_damage_ignored_missing_health"
                        );
                        stats.record_invalid_target();
                        continue;
                    };
                    let applied = amount.min(health.current);
                    let was_alive = health.current > 0;
                    if applied > 0 {
                        health.current = health.current.saturating_sub(applied);
                        self.system_events.emit(GameplayEvent::EntityDamaged {
                            entity_id,
                            amount: applied,
                        });
                    }
                    if was_alive && health.current == 0 {
                        self.system_events
                            .emit(GameplayEvent::EntityDied { entity_id });
                        if self.player_id == Some(entity_id) {
                            health.current = health.max;
                            debug!(
                                entity_id = entity_id.0,
                                "authoritative_player_death_is_non_despawning"
                            );
                            continue;
                        }
                        pending.push(GameplayIntent::DespawnEntity { entity_id });
                    }
                }
                GameplayIntent::AddStatus {
                    entity_id,
                    status_id,
                    duration_seconds,
                } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    if duration_seconds <= 0.0 {
                        debug!(
                            entity_id = entity_id.0,
                            status_id = status_id.0,
                            duration_seconds,
                            "add_status_ignored_non_positive_duration"
                        );
                        stats.record_invalid_target();
                        continue;
                    }
                    let status_set = self.status_sets_by_entity.entry(entity_id).or_default();
                    Self::upsert_status_with_refresh(status_set, status_id, duration_seconds);
                    self.system_events.emit(GameplayEvent::StatusApplied {
                        entity_id,
                        status_id,
                    });
                }
                GameplayIntent::RemoveStatus {
                    entity_id,
                    status_id,
                } => {
                    if world.find_entity(entity_id).is_none() {
                        stats.record_invalid_target();
                        continue;
                    }
                    if let Some(status_set) = self.status_sets_by_entity.get_mut(&entity_id) {
                        if Self::remove_status_if_present(status_set, status_id) {
                            self.system_events.emit(GameplayEvent::StatusExpired {
                                entity_id,
                                status_id,
                            });
                        }
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
                        self.damage_by_entity.remove(&target_id);
                        self.status_sets_by_entity.remove(&target_id);
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
        self.apply_gameplay_tick_at_safe_point(fixed_dt_seconds, input, world);
        self.system_events.finish_tick_rollover();
    }

    fn ensure_authoritative_player_exists_if_missing(&mut self, world: &mut SceneWorld) {
        if let Some(player_id) = self.player_id {
            if let Some(player) = world.find_entity_mut(player_id) {
                player.selectable = true;
                self.ai_agents_by_entity.remove(&player_id);
                return;
            }
            if self.entity_save_ids.contains_key(&player_id) {
                self.ai_agents_by_entity.remove(&player_id);
                return;
            }
            self.player_id = None;
        }
        if world.def_database().is_none() {
            return;
        }

        let player_archetype = resolve_player_archetype(world);
        let effective_params = Self::effective_combat_ai_params(Some(&player_archetype));
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
        if let Some(player) = world.find_entity_mut(player_id) {
            player.selectable = true;
        }

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

        self.health_by_entity.entry(player_id).or_insert(Health {
            current: effective_params.health_max,
            max: effective_params.health_max,
        });
        self.damage_by_entity
            .entry(player_id)
            .or_insert(effective_params.base_damage);
        self.status_sets_by_entity.entry(player_id).or_default();
        self.ai_agents_by_entity.remove(&player_id);
        self.player_id = Some(player_id);
        if self.reselect_player_on_respawn {
            self.selected_entity = Some(player_id);
            self.reselect_player_on_respawn = false;
        }
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
            world.pick_topmost_interactable_at_cursor(
                cursor_px,
                input.window_size(),
                Some(self.active_floor.to_engine_floor()),
            )
        });
        let active_floor = self.active_floor_engine();
        if self.selected_entity.is_some_and(|selected_id| match world.find_entity(selected_id) {
            Some(entity) => entity.floor != active_floor,
            None => true,
        }) {
            self.selected_entity = None;
        }

        if input.left_click_pressed() {
            self.selected_entity = input.cursor_position_px().and_then(|cursor_px| {
                world.pick_topmost_selectable_at_cursor(
                    cursor_px,
                    input.window_size(),
                    Some(self.active_floor.to_engine_floor()),
                )
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
                    if entity.actor && Some(selected_id) == self.player_id {
                        if GameplaySystemsHost::order_state_indicates_interaction(
                            entity.order_state,
                        ) {
                            marker_position = None;
                        } else if let Some(target_world) = interactable_target {
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
                Some(entity) => !entity.actor || entity.floor != active_floor,
                None => true,
            };
            if stale_or_non_actor {
                world.set_selected_actor_visual(None);
            }
        }
        let selected_actor_visual = self.selected_entity.and_then(|id| {
            world
                .find_entity(id)
                .filter(|entity| entity.actor && entity.floor == active_floor)
                .map(|_| id)
        });
        world.set_selected_actor_visual(selected_actor_visual);
        let targeted_interactable_visual = selected_actor_visual.and_then(|actor_id| {
            let actor = world.find_entity(actor_id)?;
            let target_save_id = match actor.order_state {
                OrderState::Interact { target_save_id }
                | OrderState::Working { target_save_id, .. } => target_save_id,
                _ => return None,
            };
            let target_id = self.resolve_runtime_target_id(target_save_id, world)?;
            let target = world.find_entity(target_id)?;
            if target.floor != active_floor || target.interactable.is_none() {
                return None;
            }
            Some(target_id)
        });
        world.set_targeted_interactable_visual(targeted_interactable_visual);

        if let Some(player_id) = self.player_id {
            if let Some(player) = world.find_entity_mut(player_id) {
                let move_speed =
                    self.effective_move_speed_for_entity(player_id, self.player_move_speed);
                let delta = movement_delta(input, fixed_dt_seconds, move_speed);
                player.transform.position.x += delta.x;
                player.transform.position.y += delta.y;
            }
        }

        self.interactable_cache.clear();
        self.interactable_lookup_by_save_id.clear();
        let mut target_lookup_by_save_id: HashMap<u64, Vec2> = HashMap::new();
        for entity in world.entities() {
            if let Some(target_save_id) = self.entity_save_ids.get(&entity.id).copied() {
                target_lookup_by_save_id.insert(target_save_id, entity.transform.position);
            }
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
                    let move_speed =
                        self.effective_move_speed_for_entity(entity.id, self.player_move_speed);
                    let (next, arrived) = step_toward(
                        entity.transform.position,
                        point,
                        move_speed,
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
                            let move_speed = self
                                .effective_move_speed_for_entity(entity.id, self.player_move_speed);
                            let (next, _) = step_toward(
                                entity.transform.position,
                                target_world,
                                move_speed,
                                fixed_dt_seconds,
                                MOVE_ARRIVAL_THRESHOLD,
                            );
                            entity.transform.position = next;
                        }
                    } else if let Some(target_world) =
                        target_lookup_by_save_id.get(&target_save_id).copied()
                    {
                        let interaction_radius = self
                            .active_interactions_by_actor
                            .get(&entity.id)
                            .map(|interaction| interaction.interaction_range)
                            .unwrap_or(AI_ATTACK_RANGE_UNITS);
                        let dx = target_world.x - entity.transform.position.x;
                        let dy = target_world.y - entity.transform.position.y;
                        if dx * dx + dy * dy <= interaction_radius * interaction_radius {
                            entity.order_state = OrderState::Working {
                                target_save_id,
                                remaining_time: 0.0,
                            };
                        } else {
                            let move_speed = self
                                .effective_move_speed_for_entity(entity.id, self.player_move_speed);
                            let (next, _) = step_toward(
                                entity.transform.position,
                                target_world,
                                move_speed,
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
                        .is_none()
                        && !target_lookup_by_save_id.contains_key(&target_save_id)
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
