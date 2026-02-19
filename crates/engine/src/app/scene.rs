use super::input::{ActionStates, InputAction};
use super::rendering::{world_to_screen_px, PLACEHOLDER_HALF_SIZE_PX};
use crate::content::DefDatabase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneKey {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCommand {
    None,
    SwitchTo(SceneKey),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputSnapshot {
    quit_requested: bool,
    switch_scene_pressed: bool,
    actions: ActionStates,
    cursor_position_px: Option<Vec2>,
    left_click_pressed: bool,
    right_click_pressed: bool,
    window_width: u32,
    window_height: u32,
}

impl InputSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn new(
        quit_requested: bool,
        switch_scene_pressed: bool,
        actions: ActionStates,
        cursor_position_px: Option<Vec2>,
        left_click_pressed: bool,
        right_click_pressed: bool,
        window_width: u32,
        window_height: u32,
    ) -> Self {
        Self {
            quit_requested,
            switch_scene_pressed,
            actions,
            cursor_position_px,
            left_click_pressed,
            right_click_pressed,
            window_width,
            window_height,
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn switch_scene_pressed(&self) -> bool {
        self.switch_scene_pressed
    }

    pub fn is_down(&self, action: InputAction) -> bool {
        self.actions.is_down(action)
    }

    pub fn with_action_down(mut self, action: InputAction, is_down: bool) -> Self {
        self.actions.set(action, is_down);
        self
    }

    pub fn with_cursor_position_px(mut self, cursor_position_px: Option<Vec2>) -> Self {
        self.cursor_position_px = cursor_position_px;
        self
    }

    pub fn with_left_click_pressed(mut self, left_click_pressed: bool) -> Self {
        self.left_click_pressed = left_click_pressed;
        self
    }

    pub fn with_right_click_pressed(mut self, right_click_pressed: bool) -> Self {
        self.right_click_pressed = right_click_pressed;
        self
    }

    pub fn with_window_size(mut self, window_size: (u32, u32)) -> Self {
        self.window_width = window_size.0;
        self.window_height = window_size.1;
        self
    }

    pub fn cursor_position_px(&self) -> Option<Vec2> {
        self.cursor_position_px
    }

    pub fn left_click_pressed(&self) -> bool {
        self.left_click_pressed
    }

    pub fn right_click_pressed(&self) -> bool {
        self.right_click_pressed
    }

    pub fn window_size(&self) -> (u32, u32) {
        (self.window_width, self.window_height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u64);

#[derive(Debug, Clone, Copy, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Camera2D {
    pub position: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub position: Vec2,
    pub rotation_radians: Option<f32>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec2::default(),
            rotation_radians: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RenderableKind {
    Placeholder,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderableDesc {
    pub kind: RenderableKind,
    pub debug_name: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Entity {
    pub id: EntityId,
    pub transform: Transform,
    pub renderable: RenderableDesc,
    pub selectable: bool,
    pub actor: bool,
    pub move_target_world: Option<Vec2>,
    applied_spawn_order: u64,
}

#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    pub fn allocate(&mut self) -> EntityId {
        let id = EntityId(self.next);
        self.next = self.next.saturating_add(1);
        id
    }
}

#[derive(Debug, Default)]
pub struct SceneWorld {
    allocator: EntityIdAllocator,
    entities: Vec<Entity>,
    pending_spawns: Vec<Entity>,
    pending_despawns: Vec<EntityId>,
    next_applied_spawn_order: u64,
    camera: Camera2D,
    def_database: Option<DefDatabase>,
}

impl SceneWorld {
    pub fn spawn(&mut self, transform: Transform, renderable: RenderableDesc) -> EntityId {
        self.spawn_internal(transform, renderable, false, false)
    }

    pub fn spawn_selectable(
        &mut self,
        transform: Transform,
        renderable: RenderableDesc,
    ) -> EntityId {
        self.spawn_internal(transform, renderable, true, false)
    }

    pub fn spawn_actor(&mut self, transform: Transform, renderable: RenderableDesc) -> EntityId {
        self.spawn_internal(transform, renderable, false, true)
    }

    fn spawn_internal(
        &mut self,
        transform: Transform,
        renderable: RenderableDesc,
        selectable: bool,
        actor: bool,
    ) -> EntityId {
        let id = self.allocator.allocate();
        self.pending_spawns.push(Entity {
            id,
            transform,
            renderable,
            selectable,
            actor,
            move_target_world: None,
            applied_spawn_order: 0,
        });
        id
    }

    pub fn despawn(&mut self, id: EntityId) -> bool {
        let exists_now = self.entities.iter().any(|entity| entity.id == id);
        let pending_spawn = self.pending_spawns.iter().any(|entity| entity.id == id);
        if !exists_now && !pending_spawn {
            return false;
        }
        self.pending_despawns.push(id);
        true
    }

    pub fn apply_pending(&mut self) {
        if !self.pending_despawns.is_empty() {
            let pending = &self.pending_despawns;
            self.entities.retain(|entity| !pending.contains(&entity.id));
            self.pending_despawns.clear();
        }

        if !self.pending_spawns.is_empty() {
            for mut entity in self.pending_spawns.drain(..) {
                entity.applied_spawn_order = self.next_applied_spawn_order;
                self.next_applied_spawn_order = self.next_applied_spawn_order.saturating_add(1);
                self.entities.push(entity);
            }
        }
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.pending_spawns.clear();
        self.pending_despawns.clear();
        self.next_applied_spawn_order = 0;
        self.camera = Camera2D::default();
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn entities_mut(&mut self) -> &mut [Entity] {
        &mut self.entities
    }

    pub fn find_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|entity| entity.id == id)
    }

    pub fn find_entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|entity| entity.id == id)
    }

    pub fn camera(&self) -> &Camera2D {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    pub fn pick_topmost_selectable_at_cursor(
        &self,
        cursor_position_px: Vec2,
        window_size: (u32, u32),
    ) -> Option<EntityId> {
        let cursor_x = cursor_position_px.x.round() as i32;
        let cursor_y = cursor_position_px.y.round() as i32;
        let mut best: Option<(u64, EntityId)> = None;

        for entity in &self.entities {
            if !entity.selectable {
                continue;
            }
            if !matches!(entity.renderable.kind, RenderableKind::Placeholder) {
                continue;
            }

            let (cx, cy) =
                world_to_screen_px(self.camera(), window_size, entity.transform.position);
            let in_bounds = cursor_x >= cx - PLACEHOLDER_HALF_SIZE_PX
                && cursor_x <= cx + PLACEHOLDER_HALF_SIZE_PX
                && cursor_y >= cy - PLACEHOLDER_HALF_SIZE_PX
                && cursor_y <= cy + PLACEHOLDER_HALF_SIZE_PX;
            if !in_bounds {
                continue;
            }

            match best {
                Some((order, _)) if order >= entity.applied_spawn_order => {}
                _ => best = Some((entity.applied_spawn_order, entity.id)),
            }
        }

        best.map(|(_, id)| id)
    }

    pub fn set_def_database(&mut self, def_database: DefDatabase) {
        self.def_database = Some(def_database);
    }

    pub fn def_database(&self) -> Option<&DefDatabase> {
        self.def_database.as_ref()
    }
}

pub trait Scene {
    fn load(&mut self, world: &mut SceneWorld);
    fn update(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &mut SceneWorld,
    ) -> SceneCommand;
    fn render(&mut self, world: &SceneWorld);
    fn unload(&mut self, world: &mut SceneWorld);
    fn debug_title(&self, _world: &SceneWorld) -> Option<String> {
        None
    }
    fn debug_selected_entity(&self) -> Option<EntityId> {
        None
    }
    fn debug_selected_target(&self, _world: &SceneWorld) -> Option<Vec2> {
        None
    }
}

pub(crate) struct SceneMachine {
    scene_a: Box<dyn Scene>,
    scene_b: Box<dyn Scene>,
    active_scene: SceneKey,
}

impl SceneMachine {
    pub(crate) fn new(
        scene_a: Box<dyn Scene>,
        scene_b: Box<dyn Scene>,
        active_scene: SceneKey,
    ) -> Self {
        Self {
            scene_a,
            scene_b,
            active_scene,
        }
    }

    pub(crate) fn active_scene(&self) -> SceneKey {
        self.active_scene
    }

    pub(crate) fn load_active(&mut self, world: &mut SceneWorld) {
        self.active_scene_mut().load(world);
    }

    pub(crate) fn update_active(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
        world: &mut SceneWorld,
    ) -> SceneCommand {
        self.active_scene_mut()
            .update(fixed_dt_seconds, input, world)
    }

    pub(crate) fn render_active(&mut self, world: &SceneWorld) {
        self.active_scene_mut().render(world);
    }

    pub(crate) fn unload_active(&mut self, world: &mut SceneWorld) {
        self.active_scene_mut().unload(world);
    }

    pub(crate) fn debug_title_active(&self, world: &SceneWorld) -> Option<String> {
        self.active_scene_ref().debug_title(world)
    }

    pub(crate) fn debug_selected_entity_active(&self) -> Option<EntityId> {
        self.active_scene_ref().debug_selected_entity()
    }

    pub(crate) fn debug_selected_target_active(&self, world: &SceneWorld) -> Option<Vec2> {
        self.active_scene_ref().debug_selected_target(world)
    }

    pub(crate) fn switch_to(&mut self, next_scene: SceneKey, world: &mut SceneWorld) -> bool {
        if self.active_scene == next_scene {
            return false;
        }

        self.active_scene_mut().unload(world);
        world.clear();
        self.active_scene = next_scene;
        self.active_scene_mut().load(world);
        true
    }

    fn active_scene_mut(&mut self) -> &mut dyn Scene {
        match self.active_scene {
            SceneKey::A => self.scene_a.as_mut(),
            SceneKey::B => self.scene_b.as_mut(),
        }
    }

    fn active_scene_ref(&self) -> &dyn Scene {
        match self.active_scene {
            SceneKey::A => self.scene_a.as_ref(),
            SceneKey::B => self.scene_b.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    struct TestScene {
        spawn_count: usize,
    }

    impl Scene for TestScene {
        fn load(&mut self, world: &mut SceneWorld) {
            for _ in 0..self.spawn_count {
                world.spawn(
                    Transform::default(),
                    RenderableDesc {
                        kind: RenderableKind::Placeholder,
                        debug_name: "test",
                    },
                );
            }
            world.apply_pending();
        }

        fn update(
            &mut self,
            _fixed_dt_seconds: f32,
            _input: &InputSnapshot,
            _world: &mut SceneWorld,
        ) -> SceneCommand {
            SceneCommand::None
        }

        fn render(&mut self, _world: &SceneWorld) {}

        fn unload(&mut self, _world: &mut SceneWorld) {}
    }

    #[test]
    fn allocator_never_reuses_ids() {
        let mut allocator = EntityIdAllocator::default();
        let first = allocator.allocate();
        let second = allocator.allocate();
        let third = allocator.allocate();

        assert_eq!(first.0, 0);
        assert_eq!(second.0, 1);
        assert_eq!(third.0, 2);
    }

    #[test]
    fn scene_world_spawn_and_despawn_updates_count() {
        let mut world = SceneWorld::default();
        let id = world.spawn(
            Transform::default(),
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "spawned",
            },
        );
        world.apply_pending();
        assert_eq!(world.entity_count(), 1);

        world.despawn(id);
        world.apply_pending();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn repeated_switches_reset_counts_and_keep_global_ids_unique() {
        let mut world = SceneWorld::default();
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 2 }),
            Box::new(TestScene { spawn_count: 3 }),
            SceneKey::A,
        );
        machine.load_active(&mut world);
        assert_eq!(world.entity_count(), 2);
        let mut seen_ids: HashSet<u64> =
            world.entities().iter().map(|entity| entity.id.0).collect();

        for i in 0..50 {
            let target = if i % 2 == 0 { SceneKey::B } else { SceneKey::A };
            machine.switch_to(target, &mut world);
            let expected = if target == SceneKey::A { 2 } else { 3 };
            assert_eq!(world.entity_count(), expected);

            for entity in world.entities() {
                assert!(
                    seen_ids.insert(entity.id.0),
                    "entity id {} was reused after scene switch",
                    entity.id.0
                );
            }
        }
    }

    #[test]
    fn camera_accessors_round_trip_position() {
        let mut world = SceneWorld::default();
        world.camera_mut().position = Vec2 { x: 3.0, y: -7.0 };
        let camera = world.camera();
        assert_eq!(camera.position.x, 3.0);
        assert_eq!(camera.position.y, -7.0);
    }

    #[test]
    fn clear_keeps_def_database_resource() {
        let mut world = SceneWorld::default();
        world.set_def_database(DefDatabase::default());
        world.clear();
        assert!(world.def_database().is_some());
    }

    #[test]
    fn pick_topmost_selectable_picks_last_applied_spawn_on_overlap() {
        let mut world = SceneWorld::default();
        let first = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "first",
            },
        );
        let second = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "second",
            },
        );
        world.apply_pending();

        let picked =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(picked, Some(second));
        assert_ne!(picked, Some(first));
    }

    #[test]
    fn pick_topmost_selectable_stable_after_unrelated_despawn() {
        let mut world = SceneWorld::default();
        let unrelated = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 10.0, y: 10.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "unrelated",
            },
        );
        let first_overlap = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "first_overlap",
            },
        );
        let second_overlap = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "second_overlap",
            },
        );
        world.apply_pending();

        let before =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(before, Some(second_overlap));
        assert_ne!(before, Some(first_overlap));

        assert!(world.despawn(unrelated));
        world.apply_pending();

        let after =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(after, Some(second_overlap));
        assert_ne!(after, Some(first_overlap));
    }

    #[test]
    fn pick_returns_none_for_empty_space() {
        let mut world = SceneWorld::default();
        world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "entity",
            },
        );
        world.apply_pending();

        let picked =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 20.0, y: 20.0 }, (1280, 720));
        assert_eq!(picked, None);
    }

    #[test]
    fn pick_ignores_non_selectable_entities() {
        let mut world = SceneWorld::default();
        world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "entity",
            },
        );
        world.apply_pending();

        let picked =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(picked, None);
    }

    #[test]
    fn spawn_actor_marks_entity_as_actor_and_without_target() {
        let mut world = SceneWorld::default();
        let actor_id = world.spawn_actor(
            Transform::default(),
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "actor",
            },
        );
        world.apply_pending();

        let actor = world.find_entity(actor_id).expect("actor exists");
        assert!(actor.actor);
        assert!(!actor.selectable);
        assert!(actor.move_target_world.is_none());
    }
}
