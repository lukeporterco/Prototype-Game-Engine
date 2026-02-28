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

    fn actor_entity_count(world: &SceneWorld) -> usize {
        world
            .entities()
            .iter()
            .filter(|entity| entity.actor)
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

    fn spawn_def_via_console(
        scene: &mut GameplayScene,
        world: &mut SceneWorld,
        def_name: &str,
        position: Vec2,
    ) -> EntityId {
        let result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: def_name.to_string(),
                position: Some((position.x, position.y)),
            },
            SceneDebugContext::default(),
            world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Success(_)));
        scene.update(0.1, &InputSnapshot::empty(), world);
        world.apply_pending();
        let spawned_ids = scene
            .system_intents
            .last_tick_apply_stats()
            .spawned_entity_ids
            .clone();
        assert_eq!(spawned_ids.len(), 1, "expected exactly one spawned entity");
        spawned_ids[0]
    }

    fn spawn_authoritative_player_via_console(
        scene: &mut GameplayScene,
        world: &mut SceneWorld,
        position: Vec2,
    ) -> EntityId {
        let spawned_id = spawn_def_via_console(scene, world, "proto.player", position);
        let player_id = scene.player_id.expect("player id assigned");
        assert_eq!(player_id, spawned_id);
        player_id
    }

    fn parse_scenario_setup_ids(message: &str) -> (u64, u64, u64) {
        let mut player = None::<u64>;
        let mut chaser = None::<u64>;
        let mut dummy = None::<u64>;
        for token in message.split_whitespace() {
            if let Some(value) = token.strip_prefix("player:") {
                player = value.parse::<u64>().ok();
            } else if let Some(value) = token.strip_prefix("chaser:") {
                chaser = value.parse::<u64>().ok();
            } else if let Some(value) = token.strip_prefix("dummy:") {
                dummy = value.parse::<u64>().ok();
            }
        }
        (
            player.expect("player id"),
            chaser.expect("chaser id"),
            dummy.expect("dummy id"),
        )
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
            status_id: StatusId("status.test"),
        });
        bus.emit(GameplayEvent::StatusExpired {
            entity_id: EntityId(1),
            status_id: StatusId("status.test"),
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
            status_id: StatusId("status.other"),
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
        let entity_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "status_target",
            },
        );
        world.apply_pending();
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);
        let status_id = StatusId("status.test");
        let intents = vec![
            GameplayIntent::AddStatus {
                entity_id,
                status_id,
                duration_seconds: 1.0,
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
        let status_set = scene
            .status_sets_by_entity
            .get(&entity_id)
            .expect("status store entry");
        assert!(!status_set
            .active
            .iter()
            .any(|status| status.status_id == status_id));
    }

    #[test]
    fn status_add_tick_expire_at_expected_time() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor_id = world.spawn_actor(
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);

        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::AddStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
                duration_seconds: 0.2,
            }],
            &mut world,
        );

        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let first_tick_intents = scene.system_intents.drain_current_tick();
        assert!(!first_tick_intents.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::RemoveStatus { entity_id, status_id }
                    if *entity_id == actor_id && *status_id == STATUS_SLOW
            )
        }));

        scene.run_gameplay_systems_once(0.11, &InputSnapshot::empty(), &world);
        let second_tick_intents = scene.system_intents.drain_current_tick();
        assert!(second_tick_intents.iter().any(|intent| {
            matches!(
                intent,
                GameplayIntent::RemoveStatus { entity_id, status_id }
                    if *entity_id == actor_id && *status_id == STATUS_SLOW
            )
        }));
        scene.apply_gameplay_intents_at_safe_point(second_tick_intents, &mut world);

        assert!(!scene
            .status_sets_by_entity
            .get(&actor_id)
            .expect("status set")
            .active
            .iter()
            .any(|status| status.status_id == STATUS_SLOW));
    }

    #[test]
    fn status_reapply_refreshes_duration_emits_applied() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor_id = world.spawn_actor(
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);

        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::AddStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
                duration_seconds: 0.3,
            }],
            &mut world,
        );
        scene.system_events.clear_current_tick();
        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let tick_intents = scene.system_intents.drain_current_tick();
        scene.apply_gameplay_intents_at_safe_point(tick_intents, &mut world);

        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::AddStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
                duration_seconds: 0.8,
            }],
            &mut world,
        );

        let remaining = scene
            .status_sets_by_entity
            .get(&actor_id)
            .expect("status set")
            .active
            .iter()
            .find(|status| status.status_id == STATUS_SLOW)
            .expect("slow status")
            .remaining_seconds;
        assert!((remaining - 0.8).abs() < 0.001);
        let applied_count = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| {
                matches!(
                    event,
                    GameplayEvent::StatusApplied { entity_id, status_id }
                        if *entity_id == actor_id && *status_id == STATUS_SLOW
                )
            })
            .count();
        assert_eq!(applied_count, 1);
    }

    #[test]
    fn status_remove_early_emits_expired_once_when_present() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor_id = world.spawn_actor(
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);

        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::AddStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
                duration_seconds: 1.0,
            }],
            &mut world,
        );
        scene.system_events.clear_current_tick();
        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::RemoveStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
            }],
            &mut world,
        );

        assert!(!scene
            .status_sets_by_entity
            .get(&actor_id)
            .expect("status set")
            .active
            .iter()
            .any(|status| status.status_id == STATUS_SLOW));
        let expired_count = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| {
                matches!(
                    event,
                    GameplayEvent::StatusExpired { entity_id, status_id }
                        if *entity_id == actor_id && *status_id == STATUS_SLOW
                )
            })
            .count();
        assert_eq!(expired_count, 1);
    }

    #[test]
    fn status_remove_missing_does_not_emit_expired() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let actor_id = world.spawn_actor(
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
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);

        scene.system_events.clear_current_tick();
        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::RemoveStatus {
                entity_id: actor_id,
                status_id: STATUS_SLOW,
            }],
            &mut world,
        );

        let expired_count = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| {
                matches!(
                    event,
                    GameplayEvent::StatusExpired { entity_id, status_id }
                        if *entity_id == actor_id && *status_id == STATUS_SLOW
                )
            })
            .count();
        assert_eq!(expired_count, 0);
    }

    #[test]
    fn slow_reduces_movement_speed_then_restores_after_expiry() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let player_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "player",
            },
        );
        world.apply_pending();
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);
        scene.player_id = Some(player_id);

        scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::AddStatus {
                entity_id: player_id,
                status_id: STATUS_SLOW,
                duration_seconds: 0.4,
            }],
            &mut world,
        );
        let move_input = snapshot_from_actions(&[InputAction::MoveRight]);

        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        scene.update(0.2, &move_input, &mut world);
        let slowed_distance = world
            .find_entity(player_id)
            .expect("player")
            .transform
            .position
            .x;
        assert!(slowed_distance > 0.0);

        scene.update(0.3, &InputSnapshot::empty(), &mut world);
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        scene.update(0.2, &move_input, &mut world);
        let restored_distance = world
            .find_entity(player_id)
            .expect("player")
            .transform
            .position
            .x;

        assert!(restored_distance > slowed_distance + 0.01);
    }

    #[test]
    fn status_multiplier_combines_as_product() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let entity_id = EntityId(77);
        scene.status_sets_by_entity.insert(
            entity_id,
            StatusSet {
                active: vec![
                    ActiveStatus {
                        status_id: STATUS_SLOW,
                        remaining_seconds: 1.0,
                    },
                    ActiveStatus {
                        status_id: StatusId("status.unknown"),
                        remaining_seconds: 1.0,
                    },
                    ActiveStatus {
                        status_id: STATUS_SLOW,
                        remaining_seconds: 1.0,
                    },
                ],
            },
        );

        let multiplier = scene.movement_speed_multiplier_for_entity(entity_id);
        assert!((multiplier - (STATUS_SLOW_MULTIPLIER * STATUS_SLOW_MULTIPLIER)).abs() < 0.0001);
    }

    #[test]
    fn intent_apply_spawn_and_despawn_hooks_run_in_order() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        let victim_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "victim",
            },
        );
        world.apply_pending();
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);
        assert!(scene.damage_by_entity.contains_key(&victim_id));
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
        assert!(!scene.damage_by_entity.contains_key(&victim_id));
        assert!(scene.damage_by_entity.contains_key(&spawned_id));
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
    fn apply_damage_reduces_health_and_zero_triggers_died_and_same_tick_despawn() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        let victim_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "victim",
            },
        );
        world.apply_pending();
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);
        scene.health_by_entity.insert(
            victim_id,
            Health {
                current: 10,
                max: DEFAULT_MAX_HEALTH,
            },
        );

        let stats = scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::ApplyDamage {
                entity_id: victim_id,
                amount: 10,
            }],
            &mut world,
        );
        world.apply_pending();

        assert_eq!(stats.apply_damage, 1);
        assert_eq!(stats.despawn_entity, 1);
        assert!(world.find_entity(victim_id).is_none());
        assert!(scene.health_by_entity.get(&victim_id).is_none());
        let damage_events = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| matches!(event, GameplayEvent::EntityDamaged { .. }))
            .count();
        let died_events = scene
            .system_events
            .iter_emitted_so_far()
            .filter(|event| matches!(event, GameplayEvent::EntityDied { .. }))
            .count();
        assert_eq!(damage_events, 1);
        assert_eq!(died_events, 1);
    }

    #[test]
    fn apply_damage_to_entity_without_health_is_ignored() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        let target_id = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 2);
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");
        scene.sync_runtime_component_stores_with_world(&world);
        assert!(scene.health_by_entity.get(&target_id).is_none());

        let stats = scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::ApplyDamage {
                entity_id: target_id,
                amount: 5,
            }],
            &mut world,
        );

        assert_eq!(stats.total, 1);
        assert_eq!(stats.apply_damage, 1);
        assert_eq!(stats.invalid_target_count, 1);
        assert!(scene
            .system_events
            .iter_emitted_so_far()
            .all(|event| !matches!(
                event,
                GameplayEvent::EntityDamaged { .. } | GameplayEvent::EntityDied { .. }
            )));
    }

    #[test]
    fn combat_resolution_derives_damage_only_from_attack_completions() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let attacker_attack = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "attacker_attack",
            },
        );
        let target_attack = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "target_attack",
            },
        );
        let attacker_use = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "attacker_use",
            },
        );
        let target_use = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        world.apply_pending();

        scene.active_interactions_by_actor.insert(
            attacker_attack,
            ActiveInteraction {
                actor_id: attacker_attack,
                target_id: target_attack,
                interaction_id: InteractionId(1),
                kind: ActiveInteractionKind::Attack,
                interaction_range: AI_ATTACK_RANGE_UNITS,
                duration_seconds: 0.0,
                remaining_seconds: None,
            },
        );
        scene.damage_by_entity.insert(attacker_attack, 77);
        scene.active_interactions_by_actor.insert(
            attacker_use,
            ActiveInteraction {
                actor_id: attacker_use,
                target_id: target_use,
                interaction_id: InteractionId(2),
                kind: ActiveInteractionKind::Use,
                interaction_range: RESOURCE_PILE_INTERACTION_RADIUS,
                duration_seconds: 0.0,
                remaining_seconds: None,
            },
        );

        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let intents = scene.system_intents.drain_current_tick();
        let mut apply_damage = intents
            .iter()
            .filter_map(|intent| match intent {
                GameplayIntent::ApplyDamage { entity_id, amount } => Some((*entity_id, *amount)),
                _ => None,
            })
            .collect::<Vec<_>>();
        apply_damage.sort_by_key(|(id, _)| id.0);
        assert_eq!(apply_damage, vec![(target_attack, 77)]);
    }

    #[test]
    fn combat_resolution_attack_completion_applies_slow_and_damage() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let attacker_attack = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "attacker_attack",
            },
        );
        let target_attack = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "target_attack",
            },
        );
        let attacker_use = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "attacker_use",
            },
        );
        let target_use = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        world.apply_pending();

        scene.active_interactions_by_actor.insert(
            attacker_attack,
            ActiveInteraction {
                actor_id: attacker_attack,
                target_id: target_attack,
                interaction_id: InteractionId(10),
                kind: ActiveInteractionKind::Attack,
                interaction_range: AI_ATTACK_RANGE_UNITS,
                duration_seconds: 0.0,
                remaining_seconds: None,
            },
        );
        scene.active_interactions_by_actor.insert(
            attacker_use,
            ActiveInteraction {
                actor_id: attacker_use,
                target_id: target_use,
                interaction_id: InteractionId(11),
                kind: ActiveInteractionKind::Use,
                interaction_range: RESOURCE_PILE_INTERACTION_RADIUS,
                duration_seconds: 0.0,
                remaining_seconds: None,
            },
        );

        scene.run_gameplay_systems_once(0.1, &InputSnapshot::empty(), &world);
        let intents = scene.system_intents.drain_current_tick();
        let add_status_intents = intents
            .iter()
            .filter_map(|intent| match intent {
                GameplayIntent::AddStatus {
                    entity_id,
                    status_id,
                    duration_seconds,
                } => Some((*entity_id, *status_id, *duration_seconds)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let apply_damage_targets = intents
            .iter()
            .filter_map(|intent| match intent {
                GameplayIntent::ApplyDamage { entity_id, .. } => Some(*entity_id),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(apply_damage_targets, vec![target_attack]);
        assert_eq!(
            add_status_intents,
            vec![(target_attack, STATUS_SLOW, STATUS_SLOW_DURATION_SECONDS)]
        );
    }

    #[test]
    fn player_attack_interaction_applies_damage_to_npc() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let npc_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 1.0, y: 0.0 },
        );
        scene.ai_agents_by_entity.clear();
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 0.5, y: 0.0 };
        scene.selected_entity = Some(player_id);

        let before = scene
            .health_by_entity
            .get(&npc_id)
            .expect("npc health")
            .current;
        let (sx, sy) = engine::world_to_screen_px(
            world.camera(),
            (1280, 720),
            world.find_entity(npc_id).expect("npc").transform.position,
        );
        let click = right_click_snapshot(
            Vec2 {
                x: sx as f32,
                y: sy as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();
        for _ in 0..10 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            if scene
                .health_by_entity
                .get(&npc_id)
                .map(|health| health.current < before)
                .unwrap_or(true)
            {
                break;
            }
        }

        let after = scene
            .health_by_entity
            .get(&npc_id)
            .map(|health| health.current);
        assert!(after.is_none() || after.expect("health") < before);
    }

    #[test]
    fn player_attack_from_out_of_range_moves_into_range_and_applies_damage() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let npc_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 3.0, y: 0.0 },
        );
        scene.ai_agents_by_entity.clear();
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 3.0, y: 0.0 };
        scene.selected_entity = Some(player_id);

        let before = scene
            .health_by_entity
            .get(&npc_id)
            .expect("npc health")
            .current;
        let (sx, sy) = engine::world_to_screen_px(
            world.camera(),
            (1280, 720),
            world.find_entity(npc_id).expect("npc").transform.position,
        );
        let click = right_click_snapshot(
            Vec2 {
                x: sx as f32,
                y: sy as f32,
            },
            (1280, 720),
        );
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();

        for _ in 0..60 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            if scene
                .health_by_entity
                .get(&npc_id)
                .map(|health| health.current < before)
                .unwrap_or(true)
            {
                break;
            }
        }

        let after = scene
            .health_by_entity
            .get(&npc_id)
            .map(|health| health.current);
        assert!(after.is_none() || after.expect("health") < before);
    }

    #[test]
    fn existing_authoritative_player_selectable_is_not_forced_without_auto_spawn() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        world.find_entity_mut(player_id).expect("player").selectable = false;

        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        assert!(!world.find_entity(player_id).expect("player").selectable);
    }

    #[test]
    fn selected_player_death_is_non_despawning_and_selection_stable() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        scene.ai_agents_by_entity.clear();
        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        scene.selected_entity = Some(player_id);
        scene.health_by_entity.insert(
            player_id,
            Health {
                current: 1,
                max: DEFAULT_MAX_HEALTH,
            },
        );

        let _stats = scene.apply_gameplay_intents_at_safe_point(
            vec![GameplayIntent::ApplyDamage {
                entity_id: player_id,
                amount: 1,
            }],
            &mut world,
        );
        world.apply_pending();

        assert_eq!(scene.player_id, Some(player_id));
        assert_eq!(scene.selected_entity, Some(player_id));
        assert!(world.find_entity(player_id).is_some());
        assert_eq!(
            scene
                .health_by_entity
                .get(&player_id)
                .expect("player health")
                .current,
            DEFAULT_MAX_HEALTH
        );
    }

    #[test]
    fn npc_attack_applies_damage_to_player() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let npc_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 0.5, y: 0.0 },
        );
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            npc_id,
            AiAgent::from_home_position(
                world.find_entity(npc_id).expect("npc").transform.position,
                GameplayScene::effective_combat_ai_params(None),
            ),
        );
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        world
            .find_entity_mut(npc_id)
            .expect("npc")
            .transform
            .position = Vec2 { x: 0.5, y: 0.0 };

        let before = scene
            .health_by_entity
            .get(&player_id)
            .expect("player health")
            .current;
        for _ in 0..20 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            if scene
                .health_by_entity
                .get(&player_id)
                .map(|health| health.current < before)
                .unwrap_or(true)
            {
                break;
            }
        }

        let after = scene
            .health_by_entity
            .get(&player_id)
            .expect("player health")
            .current;
        assert!(after < before);
    }

    #[test]
    fn proto_npc_chaser_attack_applies_slow_then_slow_expires() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let chaser_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 0.5, y: 0.0 },
        );
        world
            .find_entity_mut(player_id)
            .expect("player")
            .transform
            .position = Vec2 { x: 0.0, y: 0.0 };
        world
            .find_entity_mut(chaser_id)
            .expect("chaser")
            .transform
            .position = Vec2 { x: 0.5, y: 0.0 };

        let player_health_before = scene
            .health_by_entity
            .get(&player_id)
            .expect("player health")
            .current;
        let mut slow_applied = false;
        for _ in 0..80 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let has_slow = scene
                .status_sets_by_entity
                .get(&player_id)
                .map(|set| {
                    set.active
                        .iter()
                        .any(|status| status.status_id == STATUS_SLOW)
                })
                .unwrap_or(false);
            if has_slow {
                slow_applied = true;
                break;
            }
        }
        assert!(
            slow_applied,
            "expected proto.npc_chaser to apply status.slow"
        );
        let player_health_after_hit = scene
            .health_by_entity
            .get(&player_id)
            .expect("player health")
            .current;
        assert!(player_health_after_hit < player_health_before);

        world
            .find_entity_mut(chaser_id)
            .expect("chaser")
            .transform
            .position = Vec2 { x: 100.0, y: 0.0 };
        let mut slow_expired = false;
        for _ in 0..80 {
            scene.update(0.1, &InputSnapshot::empty(), &mut world);
            world.apply_pending();
            let has_slow = scene
                .status_sets_by_entity
                .get(&player_id)
                .map(|set| {
                    set.active
                        .iter()
                        .any(|status| status.status_id == STATUS_SLOW)
                })
                .unwrap_or(false);
            if !has_slow {
                slow_expired = true;
                break;
            }
        }
        assert!(slow_expired, "expected status.slow to expire");
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
            .map(|entity| entity.id)
            .next()
            .expect("spawned victim");
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
    fn scenario_setup_combat_chaser_creates_expected_layout_and_selection() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let result = scene.execute_debug_command(
            SceneDebugCommand::ScenarioSetup {
                scenario_id: "combat_chaser".to_string(),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        let message = match result {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success result, got {other:?}"),
        };
        assert!(message.starts_with("scenario.setup combat_chaser "));
        assert!(message.contains("player:"));
        assert!(message.contains("chaser:"));
        assert!(message.contains("dummy:"));

        let (player_raw, chaser_raw, dummy_raw) = parse_scenario_setup_ids(&message);
        let player_id = EntityId(player_raw);
        let chaser_id = EntityId(chaser_raw);
        let dummy_id = EntityId(dummy_raw);

        assert!(world.find_entity(player_id).is_some());
        assert!(world.find_entity(chaser_id).is_some());
        assert!(world.find_entity(dummy_id).is_some());
        assert_eq!(scene.player_id, Some(player_id));
        assert_eq!(scene.selected_entity, Some(player_id));
        assert_eq!(world.entity_count(), 3);
        assert_eq!(actor_entity_count(&world), 3);

        let player = world.find_entity(player_id).expect("player");
        let chaser = world.find_entity(chaser_id).expect("chaser");
        let dummy = world.find_entity(dummy_id).expect("dummy");
        assert_eq!(player.transform.position, COMBAT_CHASER_PLAYER_POS);
        assert_eq!(chaser.transform.position, COMBAT_CHASER_CHASER_POS);
        assert_eq!(dummy.transform.position, COMBAT_CHASER_DUMMY_POS);

        assert!(scene.health_by_entity.contains_key(&player_id));
        assert!(scene.health_by_entity.contains_key(&chaser_id));
        assert!(scene.health_by_entity.contains_key(&dummy_id));
        assert!(scene.damage_by_entity.contains_key(&player_id));
        assert!(scene.damage_by_entity.contains_key(&chaser_id));
        assert!(scene.damage_by_entity.contains_key(&dummy_id));
        assert!(!scene.ai_agents_by_entity.contains_key(&player_id));
        assert!(scene.ai_agents_by_entity.contains_key(&chaser_id));
        assert!(!scene.ai_agents_by_entity.contains_key(&dummy_id));
    }

    #[test]
    fn scenario_setup_combat_chaser_is_idempotent() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let first = scene.execute_debug_command(
            SceneDebugCommand::ScenarioSetup {
                scenario_id: "combat_chaser".to_string(),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        let first_message = match first {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success result, got {other:?}"),
        };
        let (first_player_raw, first_chaser_raw, first_dummy_raw) =
            parse_scenario_setup_ids(&first_message);

        let second = scene.execute_debug_command(
            SceneDebugCommand::ScenarioSetup {
                scenario_id: "combat_chaser".to_string(),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        let second_message = match second {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success result, got {other:?}"),
        };
        let (second_player_raw, second_chaser_raw, second_dummy_raw) =
            parse_scenario_setup_ids(&second_message);
        let second_player_id = EntityId(second_player_raw);
        let second_chaser_id = EntityId(second_chaser_raw);
        let second_dummy_id = EntityId(second_dummy_raw);

        assert_eq!(world.entity_count(), 3);
        assert_eq!(actor_entity_count(&world), 3);
        assert_eq!(scene.player_id, Some(second_player_id));
        assert_eq!(scene.selected_entity, Some(second_player_id));
        assert!(world.find_entity(second_player_id).is_some());
        assert!(world.find_entity(second_chaser_id).is_some());
        assert!(world.find_entity(second_dummy_id).is_some());
        assert_eq!(
            world
                .find_entity(second_player_id)
                .expect("player")
                .transform
                .position,
            COMBAT_CHASER_PLAYER_POS
        );
        assert_eq!(
            world
                .find_entity(second_chaser_id)
                .expect("chaser")
                .transform
                .position,
            COMBAT_CHASER_CHASER_POS
        );
        assert_eq!(
            world
                .find_entity(second_dummy_id)
                .expect("dummy")
                .transform
                .position,
            COMBAT_CHASER_DUMMY_POS
        );

        if first_player_raw != second_player_raw {
            assert!(world.find_entity(EntityId(first_player_raw)).is_none());
        }
        if first_chaser_raw != second_chaser_raw {
            assert!(world.find_entity(EntityId(first_chaser_raw)).is_none());
        }
        if first_dummy_raw != second_dummy_raw {
            assert!(world.find_entity(EntityId(first_dummy_raw)).is_none());
        }
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
    fn debug_select_success_sets_selected_entity() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let selectable = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "selectable",
            },
        );
        world.apply_pending();

        let result = scene.execute_debug_command(
            SceneDebugCommand::Select {
                entity_id: selectable.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Success(_)));
        assert_eq!(scene.selected_entity, Some(selectable));
    }

    #[test]
    fn debug_select_missing_or_non_selectable_returns_error() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let non_selectable = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "non_selectable",
            },
        );
        world.apply_pending();

        let missing = scene.execute_debug_command(
            SceneDebugCommand::Select { entity_id: 999_999 },
            SceneDebugContext::default(),
            &mut world,
        );
        let not_selectable = scene.execute_debug_command(
            SceneDebugCommand::Select {
                entity_id: non_selectable.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(missing, SceneDebugCommandResult::Error(_)));
        assert!(matches!(not_selectable, SceneDebugCommandResult::Error(_)));
    }

    #[test]
    fn debug_order_move_queues_intent_and_applies_after_update() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

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
        world.find_entity_mut(actor).expect("actor").selectable = true;
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);

        let result = scene.execute_debug_command(
            SceneDebugCommand::OrderMove { x: 3.0, y: -2.5 },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Success(_)));

        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        let actor = world.find_entity(actor).expect("actor");
        assert_eq!(
            actor.order_state,
            OrderState::MoveTo {
                point: Vec2 { x: 3.0, y: -2.5 }
            }
        );
    }

    #[test]
    fn debug_order_move_errors_without_valid_selected_actor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let no_selection = scene.execute_debug_command(
            SceneDebugCommand::OrderMove { x: 1.0, y: 1.0 },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(no_selection, SceneDebugCommandResult::Error(_)));

        let non_actor = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "non_actor",
            },
        );
        world.apply_pending();
        scene.selected_entity = Some(non_actor);

        let non_actor_result = scene.execute_debug_command(
            SceneDebugCommand::OrderMove { x: 1.0, y: 1.0 },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(
            non_actor_result,
            SceneDebugCommandResult::Error(_)
        ));
    }

    #[test]
    fn debug_order_interact_queues_and_applies_for_selected_actor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: -1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        world.find_entity_mut(actor).expect("actor").selectable = true;
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);
        scene.sync_save_id_map_with_world(&world).expect("sync");

        let pile = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 2);
        scene.sync_save_id_map_with_world(&world).expect("sync");
        let target_save_id = scene.save_id_for_entity(pile).expect("save id");

        let result = scene.execute_debug_command(
            SceneDebugCommand::OrderInteract {
                target_entity_id: pile.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Success(_)));

        scene.update(1.0 / 60.0, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        let actor = world.find_entity(actor).expect("actor");
        assert_eq!(actor.order_state, OrderState::Interact { target_save_id });
    }

    #[test]
    fn debug_order_interact_errors_for_invalid_target() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let actor = world.spawn_actor(
            Transform {
                position: Vec2 { x: -1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();
        world.find_entity_mut(actor).expect("actor").selectable = true;
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);

        let missing = scene.execute_debug_command(
            SceneDebugCommand::OrderInteract {
                target_entity_id: 999_999,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(missing, SceneDebugCommandResult::Error(_)));

        let non_interactable = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "non_interactable",
            },
        );
        world.apply_pending();
        let non_interactable_result = scene.execute_debug_command(
            SceneDebugCommand::OrderInteract {
                target_entity_id: non_interactable.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(
            non_interactable_result,
            SceneDebugCommandResult::Error(_)
        ));
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
                    entity.id,
                    entity.transform.position,
                    entity.actor,
                    entity.interactable.is_some(),
                )
            })
            .collect::<Vec<_>>();
        let chaser = spawned
            .iter()
            .find(|(_, pos, _, _)| (pos.x - 10.0).abs() < 0.001)
            .expect("chaser spawn");
        assert!(chaser.2);
        assert!(!chaser.3);
        let dummy = spawned
            .iter()
            .find(|(_, pos, _, _)| (pos.x - 11.0).abs() < 0.001)
            .expect("dummy spawn");
        assert!(dummy.2);
        assert!(!dummy.3);
        let stockpile = spawned
            .iter()
            .find(|(_, pos, _, _)| (pos.x - 12.0).abs() < 0.001)
            .expect("stockpile spawn");
        assert!(!stockpile.2);
        assert!(stockpile.3);
        let door = spawned
            .iter()
            .find(|(_, pos, _, _)| (pos.x - 13.0).abs() < 0.001)
            .expect("door spawn");
        assert!(!door.2);
        assert!(door.3);

        let chaser_id = chaser.0;
        let dummy_id = dummy.0;
        let chaser_health = scene
            .health_by_entity
            .get(&chaser_id)
            .expect("chaser health");
        assert_eq!(chaser_health.max, 200);
        assert_eq!(chaser_health.current, 200);
        assert_eq!(scene.damage_by_entity.get(&chaser_id).copied(), Some(40));
        let chaser_ai = scene
            .ai_agents_by_entity
            .get(&chaser_id)
            .expect("chaser ai");
        assert!((chaser_ai.aggro_radius - 10.0).abs() < 0.001);
        assert!((chaser_ai.attack_range - 1.2).abs() < 0.001);
        assert!((chaser_ai.cooldown_seconds - 0.6).abs() < 0.001);

        let dummy_health = scene.health_by_entity.get(&dummy_id).expect("dummy health");
        assert_eq!(dummy_health.max, DEFAULT_MAX_HEALTH);
        assert_eq!(dummy_health.current, DEFAULT_MAX_HEALTH);
        assert_eq!(
            scene.damage_by_entity.get(&dummy_id).copied(),
            Some(ATTACK_DAMAGE_PER_HIT)
        );
        assert!(
            !scene.ai_agents_by_entity.contains_key(&dummy_id),
            "npc dummy should not auto-register combat AI"
        );
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
    fn debug_order_move_errors_for_selected_non_player_actor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "player",
            },
        );
        let npc = world.spawn_actor(
            Transform {
                position: Vec2 { x: 2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "npc",
            },
        );
        world.apply_pending();
        world.find_entity_mut(player).expect("player").selectable = true;
        world.find_entity_mut(npc).expect("npc").selectable = true;
        scene.player_id = Some(player);
        scene.selected_entity = Some(npc);

        let result = scene.execute_debug_command(
            SceneDebugCommand::OrderMove { x: 1.0, y: 1.0 },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(result, SceneDebugCommandResult::Error(_)));
    }

    #[test]
    fn floor_set_changes_active_floor_and_selection_filter() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();

        world.set_active_floor(engine::FloorId::Main);
        let main_id = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "main_selectable",
            },
        );
        world.set_active_floor(engine::FloorId::Basement);
        let basement_id = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "basement_selectable",
            },
        );
        world.apply_pending();

        let click = click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        assert_eq!(scene.debug_selected_entity(), Some(main_id));

        let floor_result = scene.execute_debug_command(
            SceneDebugCommand::FloorSet {
                floor: engine::FloorId::Basement,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert_eq!(
            floor_result,
            SceneDebugCommandResult::Success("floor.set v1 active:basement".to_string())
        );

        scene.update(1.0 / 60.0, &click, &mut world);
        assert_eq!(scene.debug_selected_entity(), Some(basement_id));
    }

    #[test]
    fn debug_order_interact_rejects_target_on_inactive_floor() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();

        world.set_active_floor(engine::FloorId::Main);
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
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);

        world.set_active_floor(engine::FloorId::Basement);
        let basement_target = spawn_interactable_pile(&mut world, Vec2 { x: 0.0, y: 0.0 }, 1);
        world.set_active_floor(engine::FloorId::Main);

        let result = scene.execute_debug_command(
            SceneDebugCommand::OrderInteract {
                target_entity_id: basement_target.0,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(
            result,
            SceneDebugCommandResult::Error(message)
                if message.contains("not on active floor")
        ));
    }

    #[test]
    fn spawn_uses_active_floor_after_floor_set() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let floor_result = scene.execute_debug_command(
            SceneDebugCommand::FloorSet {
                floor: engine::FloorId::Basement,
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert_eq!(
            floor_result,
            SceneDebugCommandResult::Success("floor.set v1 active:basement".to_string())
        );

        let spawned = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_dummy",
            Vec2 { x: 1.0, y: 0.0 },
        );
        assert_eq!(
            world.find_entity(spawned).expect("spawned").floor,
            engine::FloorId::Basement
        );
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
        scene.player_id = Some(actor);
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
        scene.player_id = Some(actor);
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
        scene.player_id = Some(actor);
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
        scene.player_id = Some(actor);
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
        scene.player_id = Some(actor);
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
        let actor_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        scene.selected_entity = Some(actor_id);
        scene.ai_agents_by_entity.clear();

        let stockpile = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.stockpile_small",
            Vec2 { x: 3.0, y: 0.0 },
        );
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
        scene.player_id = Some(actor);
        scene.selected_entity = Some(actor);
        scene
            .sync_save_id_map_with_world(&world)
            .expect("save-id sync");

        let click = right_click_snapshot(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        scene.update(1.0 / 60.0, &click, &mut world);
        world.apply_pending();
        let started_counts = scene.system_events.last_tick_counts();
        assert!(started_counts.interaction_started >= 1);
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
            .filter(|event| {
                matches!(
                    event,
                    GameplayEvent::InteractionStarted { actor_id, target_id }
                        if *actor_id == actor && *target_id == target
                )
            })
            .count();
        assert_eq!(completion_events, 1);
        assert_eq!(start_events, 0);

        let intents = scene.system_intents.drain_current_tick();
        let complete_count = intents
            .iter()
            .filter(|intent| matches!(intent, GameplayIntent::CompleteInteraction { .. }))
            .count();
        assert_eq!(complete_count, 1);
    }

    #[test]
    fn ai_state_transitions_idle_wander_chase_useinteraction_with_cooldown() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let npc_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 2.0, y: 0.0 },
        );
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            npc_id,
            AiAgent::from_home_position(
                world.find_entity(npc_id).expect("npc").transform.position,
                GameplayScene::effective_combat_ai_params(None),
            ),
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

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let player_pos = world
            .find_entity(player_id)
            .expect("player")
            .transform
            .position;
        let spawn_result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.npc_chaser".to_string(),
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
                GameplayScene::effective_combat_ai_params(None),
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

        let player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let npc_id = spawn_def_via_console(
            &mut scene,
            &mut world,
            "proto.npc_chaser",
            Vec2 { x: 2.0, y: 0.0 },
        );
        scene.ai_agents_by_entity.clear();
        scene.ai_agents_by_entity.insert(
            npc_id,
            AiAgent::from_home_position(
                world.find_entity(npc_id).expect("npc").transform.position,
                GameplayScene::effective_combat_ai_params(None),
            ),
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

        let original_player_id =
            spawn_authoritative_player_via_console(&mut scene, &mut world, Vec2 { x: 0.0, y: 0.0 });
        let spawn_result = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((10.0, -4.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_result, SceneDebugCommandResult::Error(_)));

        assert_eq!(scene.player_id, Some(original_player_id));
    }

    #[test]
    fn only_one_proto_player_spawn_allowed_at_a_time() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let spawn_first = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((0.0, 0.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_first, SceneDebugCommandResult::Success(_)));
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        let first_player_id = scene.player_id.expect("first player id");
        let entity_count_after_first = world.entity_count();

        let spawn_second = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((2.0, 0.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(spawn_second, SceneDebugCommandResult::Error(_)));
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        assert_eq!(scene.player_id, Some(first_player_id));
        assert_eq!(world.entity_count(), entity_count_after_first);
        assert_eq!(
            scene
                .system_intents
                .last_tick_apply_stats()
                .invalid_target_count,
            0
        );
    }

    #[test]
    fn debug_spawn_proto_player_returns_error_when_player_exists() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let first = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((0.0, 0.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        assert!(matches!(first, SceneDebugCommandResult::Success(_)));
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();

        let second = scene.execute_debug_command(
            SceneDebugCommand::Spawn {
                def_name: "proto.player".to_string(),
                position: Some((1.0, 0.0)),
            },
            SceneDebugContext::default(),
            &mut world,
        );
        match second {
            SceneDebugCommandResult::Error(message) => {
                assert_eq!(message, "only one proto.player allowed at a time");
            }
            SceneDebugCommandResult::Success(message) => {
                panic!("expected error, got success: {message}");
            }
            SceneDebugCommandResult::Unsupported => {
                panic!("expected error, got unsupported");
            }
        }
    }

    #[test]
    fn no_auto_spawn_restores_player_when_missing() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        seed_def_database(&mut world);
        scene.load(&mut world);
        world.apply_pending();

        let baseline_entity_count = world.entity_count();
        scene.update(0.1, &InputSnapshot::empty(), &mut world);
        world.apply_pending();
        assert_eq!(scene.player_id, None);
        assert_eq!(world.entity_count(), baseline_entity_count);
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
    fn dump_state_format_includes_required_fields_and_v1_header() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let player_id = world.spawn_actor(
            Transform {
                position: Vec2 {
                    x: 1.2345,
                    y: -2.3456,
                },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "player",
            },
        );
        world.apply_pending();
        world.camera_mut().position = Vec2 { x: 7.89, y: -1.23 };
        world.camera_mut().zoom = 1.234;
        scene.player_id = Some(player_id);
        scene.selected_entity = Some(player_id);

        let result = scene.execute_debug_command(
            SceneDebugCommand::DumpState,
            SceneDebugContext::default(),
            &mut world,
        );
        let line = match result {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success, got {other:?}"),
        };

        assert!(line.starts_with("dump.state v1 | "));
        assert!(line.contains("player:0@(1.23,-2.35)"));
        assert!(line.contains("cam:(7.89,-1.23,1.23)"));
        assert!(line.contains("sel:0"));
        assert!(line.contains("tgt:none"));
        assert!(line.contains("cnt:ent:1 act:1 int:0"));
        assert!(line.contains("evk:is:"));
        assert!(line.contains("ink:sp:"));
        assert!(line.contains("in_bad:"));

        let expected_order = [
            "player:", "cam:", "sel:", "tgt:", "cnt:", "ev:", "evk:", "in:", "ink:", "in_bad:",
        ];
        let mut cursor = 0usize;
        for key in expected_order {
            let offset = line[cursor..]
                .find(key)
                .unwrap_or_else(|| panic!("missing key in dump.state: {key}"));
            cursor += offset;
        }

        let none_result = scene.execute_debug_command(
            SceneDebugCommand::DumpState,
            SceneDebugContext::default(),
            &mut SceneWorld::default(),
        );
        let none_line = match none_result {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success for empty scene, got {other:?}"),
        };
        assert!(none_line.starts_with("dump.state v1 | "));
        assert!(none_line.contains("player:none"));
        assert!(none_line.contains("cam:(0.00,0.00,1.00)"));
        assert!(none_line.contains("sel:0"));
        assert!(none_line.contains("tgt:none"));
        assert!(none_line.contains("cnt:ent:0 act:0 int:0"));
        assert!(none_line.contains("ev:"));
        assert!(none_line.contains("in:"));
        assert!(none_line.contains("in_bad:"));
    }

    #[test]
    fn dump_ai_format_includes_required_fields_and_v1_header() {
        let mut scene = GameplayScene::new("A", SceneKey::B, Vec2 { x: 0.0, y: 0.0 });
        let mut world = SceneWorld::default();
        let player_id = world.spawn_actor(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "player",
            },
        );
        world.spawn_actor(
            Transform {
                position: Vec2 { x: 2.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "npc1",
            },
        );
        world.spawn_actor(
            Transform {
                position: Vec2 { x: 1.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: engine::RenderableKind::Placeholder,
                debug_name: "npc2",
            },
        );
        world.apply_pending();
        scene.player_id = Some(player_id);
        scene.rebuild_ai_agents_from_world(&world);

        let result = scene.execute_debug_command(
            SceneDebugCommand::DumpAi,
            SceneDebugContext::default(),
            &mut world,
        );
        let line = match result {
            SceneDebugCommandResult::Success(message) => message,
            other => panic!("expected success, got {other:?}"),
        };

        assert!(line.starts_with("dump.ai v1 | "));
        assert!(line.contains("cnt:id:"));
        assert!(line.contains(" wa:"));
        assert!(line.contains(" ch:"));
        assert!(line.contains(" use:"));
        assert!(line.contains(" | near:"));
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
    ) -> (usize, Option<Vec2>, Option<EntityId>, Option<EntityId>, u32) {
        (
            world.entity_count(),
            world
                .entities()
                .first()
                .map(|entity| entity.transform.position),
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
        let before_resource_count = scene.resource_count;
        let before_entity_count = world.entity_count();
        let before_first_pos = world
            .entities()
            .first()
            .map(|entity| entity.transform.position);

        let mut bad_version = sample_save_game(SavedSceneKey::A);
        bad_version.save_version = SAVE_VERSION + 1;
        assert!(GameplayScene::validate_save_game(&bad_version, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(
            world
                .entities()
                .first()
                .map(|entity| entity.transform.position),
            before_first_pos
        );
        assert_eq!(scene.resource_count, before_resource_count);

        let bad_scene = sample_save_game(SavedSceneKey::B);
        assert!(GameplayScene::validate_save_game(&bad_scene, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(
            world
                .entities()
                .first()
                .map(|entity| entity.transform.position),
            before_first_pos
        );
        assert_eq!(scene.resource_count, before_resource_count);

        let mut bad_reference = sample_save_game(SavedSceneKey::A);
        bad_reference.selected_entity_save_id = Some(9999);
        assert!(GameplayScene::validate_save_game(&bad_reference, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(
            world
                .entities()
                .first()
                .map(|entity| entity.transform.position),
            before_first_pos
        );
        assert_eq!(scene.resource_count, before_resource_count);

        let mut bad_next_save_id = sample_save_game(SavedSceneKey::A);
        bad_next_save_id.next_save_id = 20;
        assert!(GameplayScene::validate_save_game(&bad_next_save_id, SavedSceneKey::A).is_err());
        assert_eq!(world.entity_count(), before_entity_count);
        assert_eq!(
            world
                .entities()
                .first()
                .map(|entity| entity.transform.position),
            before_first_pos
        );
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
