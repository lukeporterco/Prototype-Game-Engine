impl Scene for GameplayScene {
    fn load(&mut self, world: &mut SceneWorld) {
        self.entity_save_ids.clear();
        self.save_id_to_entity.clear();
        self.next_save_id = 0;
        self.reset_runtime_component_stores();
        let player_archetype = resolve_player_archetype(world);
        world.set_tilemap(build_ground_tilemap(self.scene_key()));
        self.player_move_speed = player_archetype.move_speed;
        self.player_id = None;
        self.selected_entity = None;
        self.active_floor = ActiveFloor::Main;
        self.last_player_facing = CardinalFacing::South;
        world.set_active_floor(self.active_floor_engine());
        self.resource_count = 0;
        self.interactable_cache.clear();
        self.interactable_lookup_by_save_id.clear();
        self.completed_target_ids.clear();
        self.combat_chaser_scenario = CombatChaserScenarioSlot::default();
        self.visual_sandbox_demo_active = false;
        self.system_order_text = GAMEPLAY_SYSTEM_ORDER_TEXT.to_string();
        world.apply_pending();
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

        world.set_active_floor(self.active_floor_engine());
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
                if archetype.def_name == "proto.player"
                    && self
                        .player_id
                        .and_then(|player_id| world.find_entity(player_id))
                        .is_some()
                {
                    return SceneDebugCommandResult::Error(
                        "only one proto.player allowed at a time".to_string(),
                    );
                }
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
            SceneDebugCommand::Select { entity_id } => {
                let runtime_id = EntityId(entity_id);
                let Some(entity) = world.find_entity(runtime_id) else {
                    return SceneDebugCommandResult::Error(format!("entity {entity_id} not found"));
                };
                if !self.entity_is_on_active_floor(entity) {
                    return SceneDebugCommandResult::Error(format!(
                        "entity {entity_id} is not on active floor"
                    ));
                }
                if !entity.selectable {
                    return SceneDebugCommandResult::Error(format!(
                        "entity {entity_id} is not selectable"
                    ));
                }
                self.selected_entity = Some(runtime_id);
                SceneDebugCommandResult::Success(format!("selected entity {entity_id}"))
            }
            SceneDebugCommand::OrderMove { x, y } => {
                let Some(actor_id) = self.selected_entity else {
                    return SceneDebugCommandResult::Error("no selected entity".to_string());
                };
                if Some(actor_id) != self.player_id {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not player pawn",
                        actor_id.0
                    ));
                }
                let Some(actor) = world.find_entity(actor_id) else {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} not found",
                        actor_id.0
                    ));
                };
                if !self.entity_is_on_active_floor(actor) {
                    self.selected_entity = None;
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not on active floor",
                        actor_id.0
                    ));
                }
                if !actor.actor {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not an actor",
                        actor_id.0
                    ));
                }
                let point = Vec2 { x, y };
                self.system_intents
                    .enqueue(GameplayIntent::SetMoveTarget { actor_id, point });
                world.push_debug_marker(DebugMarker {
                    kind: DebugMarkerKind::Order,
                    position_world: point,
                    ttl_seconds: ORDER_MARKER_TTL_SECONDS,
                });
                SceneDebugCommandResult::Success(format!(
                    "queued move for entity {} to ({:.2}, {:.2})",
                    actor_id.0, x, y
                ))
            }
            SceneDebugCommand::OrderInteract { target_entity_id } => {
                let Some(actor_id) = self.selected_entity else {
                    return SceneDebugCommandResult::Error("no selected entity".to_string());
                };
                if Some(actor_id) != self.player_id {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not player pawn",
                        actor_id.0
                    ));
                }
                let Some(actor) = world.find_entity(actor_id) else {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} not found",
                        actor_id.0
                    ));
                };
                if !self.entity_is_on_active_floor(actor) {
                    self.selected_entity = None;
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not on active floor",
                        actor_id.0
                    ));
                }
                if !actor.actor {
                    return SceneDebugCommandResult::Error(format!(
                        "selected entity {} is not an actor",
                        actor_id.0
                    ));
                }
                let target_id = EntityId(target_entity_id);
                if target_id == actor_id {
                    return SceneDebugCommandResult::Error("cannot interact with self".to_string());
                }
                let Some(target) = world.find_entity(target_id) else {
                    return SceneDebugCommandResult::Error(format!(
                        "target entity {target_entity_id} not found"
                    ));
                };
                if !self.entity_is_on_active_floor(target) {
                    return SceneDebugCommandResult::Error(format!(
                        "target entity {target_entity_id} is not on active floor"
                    ));
                }
                if target.interactable.is_none() {
                    return SceneDebugCommandResult::Error(format!(
                        "target entity {target_entity_id} is not interactable"
                    ));
                }
                if self.active_interactions_by_actor.contains_key(&actor_id) {
                    self.system_intents
                        .enqueue(GameplayIntent::CancelInteraction { actor_id });
                }
                let interaction_id =
                    GameplaySystemsHost::alloc_interaction_id(&mut self.next_interaction_id);
                let Some(interaction_range) =
                    GameplaySystemsHost::interaction_range_for_use_target(target)
                else {
                    return SceneDebugCommandResult::Error(format!(
                        "target entity {target_entity_id} is not interactable"
                    ));
                };
                let duration_seconds =
                    GameplaySystemsHost::interaction_duration_seconds_for_use_target(target);
                self.active_interactions_by_actor.insert(
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
                self.system_events.emit(GameplayEvent::InteractionStarted {
                    actor_id,
                    target_id,
                });
                self.system_intents
                    .enqueue(GameplayIntent::StartInteraction {
                        actor_id,
                        target_id,
                    });
                SceneDebugCommandResult::Success(format!(
                    "queued interact actor {} target {}",
                    actor_id.0, target_entity_id
                ))
            }
            SceneDebugCommand::FloorSet { floor } => {
                self.active_floor = ActiveFloor::from_engine_floor(floor);
                world.set_active_floor(self.active_floor_engine());
                world.set_hovered_interactable_visual(None);
                world.set_selected_actor_visual(None);
                world.set_targeted_interactable_visual(None);
                world.clear_debug_markers();

                if self.selected_entity.is_some_and(|selected_id| {
                    match world.find_entity(selected_id) {
                        Some(entity) => !self.entity_is_on_active_floor(entity),
                        None => true,
                    }
                }) {
                    self.selected_entity = None;
                }

                SceneDebugCommandResult::Success(format!(
                    "floor.set v1 active:{}",
                    self.active_floor.as_token()
                ))
            }
            SceneDebugCommand::DumpState => {
                SceneDebugCommandResult::Success(self.format_dump_state(world))
            }
            SceneDebugCommand::DumpAi => {
                SceneDebugCommandResult::Success(self.format_dump_ai(world))
            }
            SceneDebugCommand::ScenarioSetup { scenario_id } => {
                match scenario_id.as_str() {
                    "combat_chaser" => match self.run_scenario_setup_combat_chaser(world) {
                        Ok((player_id, chaser_id, dummy_id)) => {
                            SceneDebugCommandResult::Success(format!(
                                "scenario.setup combat_chaser player:{} chaser:{} dummy:{}",
                                player_id.0, chaser_id.0, dummy_id.0
                            ))
                        }
                        Err(error) => SceneDebugCommandResult::Error(error),
                    },
                    "visual_sandbox" => match self.run_scenario_setup_visual_sandbox(world) {
                        Ok((player_id, prop_id, wall_id, floor_id)) => {
                            SceneDebugCommandResult::Success(format!(
                                "scenario.setup visual_sandbox player:{} prop:{} wall:{} floor:{}",
                                player_id.0, prop_id.0, wall_id.0, floor_id.0
                            ))
                        }
                        Err(error) => SceneDebugCommandResult::Error(error),
                    },
                    _ => SceneDebugCommandResult::Error(format!("unknown scenario '{scenario_id}'")),
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
        self.active_floor = ActiveFloor::Main;
        self.last_player_facing = CardinalFacing::South;
        world.set_active_floor(self.active_floor_engine());
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
        self.combat_chaser_scenario = CombatChaserScenarioSlot::default();
        self.visual_sandbox_demo_active = false;
        self.selected_completion_enqueued_this_tick = false;
        self.reselect_player_on_respawn = false;
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
        if !entity.actor || !self.entity_is_on_active_floor(entity) {
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
