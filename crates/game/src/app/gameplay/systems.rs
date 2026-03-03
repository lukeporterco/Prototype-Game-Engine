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

struct GameplaySystemContext<'a> {
    fixed_dt_seconds: f32,
    world_view: WorldView<'a>,
    input: &'a InputSnapshot,
    player_id: Option<EntityId>,
    selected_entity: Option<EntityId>,
    visual_sandbox_demo_active: bool,
    pawn_role_by_entity: &'a HashMap<EntityId, PawnControlRole>,
    entity_archetype_id_by_entity: &'a HashMap<EntityId, EntityDefId>,
    carry_visual_by_actor: &'a HashMap<EntityId, String>,
    ai_agents_by_entity: &'a mut HashMap<EntityId, AiAgent>,
    status_sets_by_entity: &'a mut HashMap<EntityId, StatusSet>,
    active_interactions_by_actor: &'a mut HashMap<EntityId, ActiveInteraction>,
    damage_by_entity: &'a HashMap<EntityId, u32>,
    completed_attack_pairs_this_tick: &'a mut HashSet<(EntityId, EntityId)>,
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
        visual_sandbox_demo_active: bool,
        pawn_role_by_entity: &HashMap<EntityId, PawnControlRole>,
        entity_archetype_id_by_entity: &HashMap<EntityId, EntityDefId>,
        carry_visual_by_actor: &HashMap<EntityId, String>,
        ai_agents_by_entity: &mut HashMap<EntityId, AiAgent>,
        status_sets_by_entity: &mut HashMap<EntityId, StatusSet>,
        active_interactions_by_actor: &mut HashMap<EntityId, ActiveInteraction>,
        damage_by_entity: &HashMap<EntityId, u32>,
        completed_attack_pairs_this_tick: &mut HashSet<(EntityId, EntityId)>,
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
                visual_sandbox_demo_active,
                pawn_role_by_entity,
                entity_archetype_id_by_entity,
                carry_visual_by_actor,
                ai_agents_by_entity,
                status_sets_by_entity,
                active_interactions_by_actor,
                damage_by_entity,
                completed_attack_pairs_this_tick,
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
        let interactable_target = context
            .world_view
            .pick_topmost_interactable_at_cursor(cursor_px, window_size);
        let Some(actor) = context.world_view.find_entity(actor_id) else {
            return;
        };
        if !actor.actor {
            return;
        }
        let is_authoritative_player = Some(actor_id) == context.player_id;
        let is_settler = context
            .pawn_role_by_entity
            .get(&actor_id)
            .copied()
            .is_some_and(|role| role == PawnControlRole::Settler);
        if !is_authoritative_player
            && !context
                .pawn_role_by_entity
                .get(&actor_id)
                .copied()
                .is_some_and(PawnControlRole::is_orderable)
        {
            return;
        }
        if is_settler {
            return;
        }
        if let Some(target_id) = interactable_target {
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
            return;
        }

        let Some(target_id) = context
            .world_view
            .pick_topmost_selectable_at_cursor(cursor_px, window_size)
        else {
            return;
        };
        if target_id == actor_id {
            return;
        }
        let Some(target) = context.world_view.find_entity(target_id) else {
            return;
        };
        if !target.actor {
            return;
        }

        if context.active_interactions_by_actor.contains_key(&actor_id) {
            context
                .intents
                .enqueue(GameplayIntent::CancelInteraction { actor_id });
        }

        let interaction_id = Self::alloc_interaction_id(context.next_interaction_id);
        context.active_interactions_by_actor.insert(
            actor_id,
            ActiveInteraction {
                actor_id,
                target_id,
                interaction_id,
                kind: ActiveInteractionKind::Attack,
                interaction_range: AI_ATTACK_RANGE_UNITS,
                duration_seconds: AI_ATTACK_INTERACTION_DURATION_SECONDS,
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
            let movement_blocked =
                has_runtime_interaction || has_world_interaction || matches!(actor.order_state, OrderState::MoveTo { .. });

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
                if matches!(interaction.kind, ActiveInteractionKind::Attack) {
                    context
                        .completed_attack_pairs_this_tick
                        .insert((interaction.actor_id, interaction.target_id));
                }
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
                if matches!(interaction.kind, ActiveInteractionKind::Attack) {
                    context
                        .completed_attack_pairs_this_tick
                        .insert((interaction.actor_id, interaction.target_id));
                }
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

    fn run_status_effects_system(&self, context: &mut GameplaySystemContext<'_>) {
        let mut entity_ids = context
            .status_sets_by_entity
            .keys()
            .copied()
            .collect::<Vec<_>>();
        entity_ids.sort_by_key(|id| id.0);

        let mut expired = Vec::new();
        for entity_id in entity_ids {
            let Some(status_set) = context.status_sets_by_entity.get_mut(&entity_id) else {
                continue;
            };
            for status in &mut status_set.active {
                status.remaining_seconds -= context.fixed_dt_seconds;
                if status.remaining_seconds <= 0.0 {
                    expired.push((entity_id, status.status_id));
                }
            }
        }

        for (entity_id, status_id) in expired {
            context.intents.enqueue(GameplayIntent::RemoveStatus {
                entity_id,
                status_id,
            });
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
            GameplaySystemId::CombatResolution => {
                let mut completed_interactions = context
                    .events
                    .iter_emitted_so_far()
                    .filter_map(|event| match event {
                        GameplayEvent::InteractionCompleted { actor_id, target_id } => {
                            Some((*actor_id, *target_id))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                completed_interactions.sort_by_key(|(actor_id, target_id)| (actor_id.0, target_id.0));

                let mut predicted_remaining_uses_by_target = HashMap::<EntityId, u32>::new();
                for (actor_id, target_id) in completed_interactions {
                    if context
                        .completed_attack_pairs_this_tick
                        .contains(&(actor_id, target_id))
                    {
                        let damage = context
                            .damage_by_entity
                            .get(&actor_id)
                            .copied()
                            .unwrap_or(ATTACK_DAMAGE_PER_HIT);
                        context.intents.enqueue(GameplayIntent::ApplyDamage {
                            entity_id: target_id,
                            amount: damage,
                        });
                        context.intents.enqueue(GameplayIntent::AddStatus {
                            entity_id: target_id,
                            status_id: STATUS_SLOW,
                            duration_seconds: STATUS_SLOW_DURATION_SECONDS,
                        });
                    }

                    let Some(target_kind) = Self::interaction_outcome_target_kind(
                        context.world_view,
                        context.entity_archetype_id_by_entity,
                        target_id,
                    ) else {
                        continue;
                    };

                    match target_kind {
                        InteractionOutcomeTargetKind::ResourcePile => {
                            let predicted_remaining = predicted_remaining_uses_by_target
                                .entry(target_id)
                                .or_insert_with(|| {
                                    context
                                        .world_view
                                        .find_entity(target_id)
                                        .and_then(|target| target.interactable)
                                        .map(|interactable| interactable.remaining_uses)
                                        .unwrap_or(0)
                                });
                            if *predicted_remaining == 0 {
                                continue;
                            }
                            *predicted_remaining = predicted_remaining.saturating_sub(1);
                            context.intents.enqueue(GameplayIntent::SetCarryVisual {
                                actor_id,
                                carry_visual_def: VISUAL_SANDBOX_CARRY_VISUAL_DEF.to_string(),
                            });
                            context
                                .intents
                                .enqueue(GameplayIntent::DecrementInteractableUses {
                                    target_id,
                                    amount: 1,
                                });
                            if *predicted_remaining == 0 {
                                context
                                    .intents
                                    .enqueue(GameplayIntent::DespawnEntity { entity_id: target_id });
                            }
                        }
                        InteractionOutcomeTargetKind::StockpileSmall => {
                            if context.carry_visual_by_actor.contains_key(&actor_id) {
                                context
                                    .intents
                                    .enqueue(GameplayIntent::ClearCarryVisual { actor_id });
                                context
                                    .intents
                                    .enqueue(GameplayIntent::IncrementResourceCount { amount: 1 });
                            }
                        }
                        InteractionOutcomeTargetKind::DoorDummy => {
                            if context.visual_sandbox_demo_active {
                                context
                                    .intents
                                    .enqueue(GameplayIntent::StartHitVisualTimer {
                                        actor_id,
                                        ticks: VISUAL_SANDBOX_HIT_DURATION_TICKS,
                                    });
                            }
                        }
                        InteractionOutcomeTargetKind::OtherInteractable => {}
                    }
                }
            }
            GameplaySystemId::StatusEffects => {
                self.run_status_effects_system(context);
            }
            GameplaySystemId::Cleanup => {
                let _ = context.input.quit_requested();
                let _ = context.events.iter_emitted_so_far().count();
            }
        }

        if cfg!(debug_assertions) {
            let event = match system_id {
                GameplaySystemId::InputIntent => GameplayEvent::InteractionStarted {
                    actor_id: EntityId(0),
                    target_id: EntityId(0),
                },
                GameplaySystemId::Interaction => return,
                GameplaySystemId::AI => return,
                GameplaySystemId::CombatResolution => return,
                GameplaySystemId::StatusEffects => return,
                GameplaySystemId::Cleanup => return,
            };
            context.events.emit(event);
        }
    }

    fn entity_has_archetype_tag(
        world_view: WorldView<'_>,
        entity_archetype_id_by_entity: &HashMap<EntityId, EntityDefId>,
        entity_id: EntityId,
        tag: &str,
    ) -> bool {
        let Some(archetype_id) = entity_archetype_id_by_entity.get(&entity_id).copied() else {
            return false;
        };
        let Some(def_db) = world_view.world.def_database() else {
            return false;
        };
        let Some(archetype) = def_db.entity_def(archetype_id) else {
            return false;
        };
        archetype.tags.iter().any(|candidate| candidate == tag)
    }

    fn interaction_outcome_target_kind(
        world_view: WorldView<'_>,
        entity_archetype_id_by_entity: &HashMap<EntityId, EntityDefId>,
        target_id: EntityId,
    ) -> Option<InteractionOutcomeTargetKind> {
        let target = world_view.find_entity(target_id)?;
        if target.interactable.is_none() {
            return None;
        }
        if Self::entity_has_archetype_tag(
            world_view,
            entity_archetype_id_by_entity,
            target_id,
            "stockpile_small",
        ) {
            return Some(InteractionOutcomeTargetKind::StockpileSmall);
        }
        if Self::entity_has_archetype_tag(
            world_view,
            entity_archetype_id_by_entity,
            target_id,
            "door_dummy",
        ) {
            return Some(InteractionOutcomeTargetKind::DoorDummy);
        }
        if Self::entity_has_archetype_tag(
            world_view,
            entity_archetype_id_by_entity,
            target_id,
            "resource_pile",
        ) {
            return Some(InteractionOutcomeTargetKind::ResourcePile);
        }
        Some(InteractionOutcomeTargetKind::OtherInteractable)
    }
}
