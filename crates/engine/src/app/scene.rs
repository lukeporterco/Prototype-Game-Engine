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
    HardResetTo(SceneKey),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputSnapshot {
    quit_requested: bool,
    switch_scene_pressed: bool,
    actions: ActionStates,
    cursor_position_px: Option<Vec2>,
    left_click_pressed: bool,
    right_click_pressed: bool,
    save_pressed: bool,
    load_pressed: bool,
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
        save_pressed: bool,
        load_pressed: bool,
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
            save_pressed,
            load_pressed,
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

    pub fn with_save_pressed(mut self, save_pressed: bool) -> Self {
        self.save_pressed = save_pressed;
        self
    }

    pub fn with_load_pressed(mut self, load_pressed: bool) -> Self {
        self.load_pressed = load_pressed;
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

    pub fn save_pressed(&self) -> bool {
        self.save_pressed
    }

    pub fn load_pressed(&self) -> bool {
        self.load_pressed
    }

    pub fn window_size(&self) -> (u32, u32) {
        (self.window_width, self.window_height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u64);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderableKind {
    Placeholder,
    Sprite(String),
}

#[derive(Debug, Clone)]
pub struct RenderableDesc {
    pub kind: RenderableKind,
    pub debug_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractableKind {
    ResourcePile,
}

#[derive(Debug, Clone, Copy)]
pub struct Interactable {
    pub kind: InteractableKind,
    pub interaction_radius: f32,
    pub remaining_uses: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Idle,
    Working {
        target: EntityId,
        remaining_time: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DebugJobState {
    None,
    Idle,
    Working { remaining_time: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugInfoSnapshot {
    pub selected_entity: Option<EntityId>,
    pub selected_position_world: Option<Vec2>,
    pub selected_order_world: Option<Vec2>,
    pub selected_job_state: DebugJobState,
    pub entity_count: usize,
    pub actor_count: usize,
    pub interactable_count: usize,
    pub resource_count: u32,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: EntityId,
    pub transform: Transform,
    pub renderable: RenderableDesc,
    pub selectable: bool,
    pub actor: bool,
    pub move_target_world: Option<Vec2>,
    pub interactable: Option<Interactable>,
    pub job_state: JobState,
    pub interaction_target: Option<EntityId>,
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
            interactable: None,
            job_state: JobState::Idle,
            interaction_target: None,
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
            if !matches!(&entity.renderable.kind, RenderableKind::Placeholder) {
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

    pub fn pick_topmost_interactable_at_cursor(
        &self,
        cursor_position_px: Vec2,
        window_size: (u32, u32),
    ) -> Option<EntityId> {
        let cursor_x = cursor_position_px.x.round() as i32;
        let cursor_y = cursor_position_px.y.round() as i32;
        let mut best: Option<(u64, EntityId)> = None;

        for entity in &self.entities {
            if entity.interactable.is_none() {
                continue;
            }
            if !matches!(&entity.renderable.kind, RenderableKind::Placeholder) {
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
    fn debug_resource_count(&self) -> Option<u32> {
        None
    }
    fn debug_info_snapshot(&self, _world: &SceneWorld) -> Option<DebugInfoSnapshot> {
        None
    }
}

struct SceneRuntime {
    scene: Box<dyn Scene>,
    world: SceneWorld,
    is_loaded: bool,
}

pub(crate) struct SceneMachine {
    scene_a: SceneRuntime,
    scene_b: SceneRuntime,
    active_scene: SceneKey,
}

impl SceneMachine {
    pub(crate) fn new(
        scene_a: Box<dyn Scene>,
        scene_b: Box<dyn Scene>,
        active_scene: SceneKey,
    ) -> Self {
        Self {
            scene_a: SceneRuntime {
                scene: scene_a,
                world: SceneWorld::default(),
                is_loaded: false,
            },
            scene_b: SceneRuntime {
                scene: scene_b,
                world: SceneWorld::default(),
                is_loaded: false,
            },
            active_scene,
        }
    }

    pub(crate) fn active_scene(&self) -> SceneKey {
        self.active_scene
    }

    pub(crate) fn set_def_database_for_all(&mut self, def_database: DefDatabase) {
        self.scene_a.world.set_def_database(def_database.clone());
        self.scene_b.world.set_def_database(def_database);
    }

    pub(crate) fn load_active(&mut self) {
        if self.active_runtime_ref().is_loaded {
            return;
        }
        let runtime = self.active_runtime_mut();
        let (scene, world) = (&mut runtime.scene, &mut runtime.world);
        scene.load(world);
        runtime.is_loaded = true;
    }

    pub(crate) fn update_active(
        &mut self,
        fixed_dt_seconds: f32,
        input: &InputSnapshot,
    ) -> SceneCommand {
        let runtime = self.active_runtime_mut();
        let (scene, world) = (&mut runtime.scene, &mut runtime.world);
        scene.update(fixed_dt_seconds, input, world)
    }

    pub(crate) fn apply_pending_active(&mut self) {
        self.active_runtime_mut().world.apply_pending();
    }

    pub(crate) fn render_active(&mut self) {
        let runtime = self.active_runtime_mut();
        runtime.scene.render(&runtime.world);
    }

    pub(crate) fn active_world(&self) -> &SceneWorld {
        &self.active_runtime_ref().world
    }

    #[cfg(test)]
    pub(crate) fn active_world_mut(&mut self) -> &mut SceneWorld {
        &mut self.active_runtime_mut().world
    }

    pub(crate) fn debug_title_active(&self) -> Option<String> {
        let runtime = self.active_runtime_ref();
        runtime.scene.debug_title(&runtime.world)
    }

    pub(crate) fn debug_selected_entity_active(&self) -> Option<EntityId> {
        self.active_runtime_ref().scene.debug_selected_entity()
    }

    pub(crate) fn debug_selected_target_active(&self) -> Option<Vec2> {
        let runtime = self.active_runtime_ref();
        runtime.scene.debug_selected_target(&runtime.world)
    }

    pub(crate) fn debug_resource_count_active(&self) -> Option<u32> {
        self.active_runtime_ref().scene.debug_resource_count()
    }

    pub(crate) fn debug_info_snapshot_active(&self) -> Option<DebugInfoSnapshot> {
        let runtime = self.active_runtime_ref();
        runtime.scene.debug_info_snapshot(&runtime.world)
    }

    pub(crate) fn switch_to(&mut self, next_scene: SceneKey) -> bool {
        if self.active_scene == next_scene {
            return false;
        }

        self.load_scene_if_needed(next_scene);
        self.active_scene = next_scene;
        true
    }

    pub(crate) fn hard_reset_to(&mut self, next_scene: SceneKey) -> bool {
        let runtime = self.runtime_mut(next_scene);
        if runtime.is_loaded {
            let (scene, world) = (&mut runtime.scene, &mut runtime.world);
            scene.unload(world);
        }
        runtime.world.clear();
        {
            let (scene, world) = (&mut runtime.scene, &mut runtime.world);
            scene.load(world);
        }
        runtime.is_loaded = true;
        let changed = self.active_scene != next_scene;
        self.active_scene = next_scene;
        changed
    }

    pub(crate) fn shutdown_all(&mut self) {
        for runtime in [&mut self.scene_a, &mut self.scene_b] {
            if runtime.is_loaded {
                let (scene, world) = (&mut runtime.scene, &mut runtime.world);
                scene.unload(world);
                runtime.world.clear();
                runtime.is_loaded = false;
            }
        }
    }

    fn load_scene_if_needed(&mut self, key: SceneKey) {
        if self.runtime_ref(key).is_loaded {
            return;
        }
        let runtime = self.runtime_mut(key);
        {
            let (scene, world) = (&mut runtime.scene, &mut runtime.world);
            scene.load(world);
        }
        runtime.is_loaded = true;
    }

    fn active_runtime_mut(&mut self) -> &mut SceneRuntime {
        self.runtime_mut(self.active_scene)
    }

    fn active_runtime_ref(&self) -> &SceneRuntime {
        self.runtime_ref(self.active_scene)
    }

    fn runtime_mut(&mut self, key: SceneKey) -> &mut SceneRuntime {
        match key {
            SceneKey::A => &mut self.scene_a,
            SceneKey::B => &mut self.scene_b,
        }
    }

    fn runtime_ref(&self, key: SceneKey) -> &SceneRuntime {
        match key {
            SceneKey::A => &self.scene_a,
            SceneKey::B => &self.scene_b,
        }
    }
}

#[cfg(test)]
mod tests {
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

    struct DebugScene;

    impl Scene for DebugScene {
        fn load(&mut self, _world: &mut SceneWorld) {}

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

        fn debug_info_snapshot(&self, world: &SceneWorld) -> Option<DebugInfoSnapshot> {
            Some(DebugInfoSnapshot {
                selected_entity: Some(EntityId(7)),
                selected_position_world: Some(Vec2 { x: 1.0, y: 2.0 }),
                selected_order_world: None,
                selected_job_state: DebugJobState::Idle,
                entity_count: world.entity_count(),
                actor_count: 0,
                interactable_count: 0,
                resource_count: 0,
            })
        }
    }

    struct SteppingScene {
        spawn_count: usize,
        step_x: f32,
    }

    impl Scene for SteppingScene {
        fn load(&mut self, world: &mut SceneWorld) {
            for _ in 0..self.spawn_count {
                world.spawn(
                    Transform::default(),
                    RenderableDesc {
                        kind: RenderableKind::Placeholder,
                        debug_name: "step",
                    },
                );
            }
            world.apply_pending();
        }

        fn update(
            &mut self,
            _fixed_dt_seconds: f32,
            _input: &InputSnapshot,
            world: &mut SceneWorld,
        ) -> SceneCommand {
            if let Some(entity) = world.entities_mut().first_mut() {
                entity.transform.position.x += self.step_x;
            }
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
    fn switch_away_and_back_preserves_entity_ids_and_transforms() {
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 2 }),
            Box::new(TestScene { spawn_count: 1 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        {
            let world = machine.active_world_mut();
            world.entities_mut()[0].transform.position = Vec2 { x: 2.5, y: -1.0 };
        }
        let before: Vec<(u64, Vec2)> = machine
            .active_world()
            .entities()
            .iter()
            .map(|entity| (entity.id.0, entity.transform.position))
            .collect();

        assert!(machine.switch_to(SceneKey::B));
        machine.apply_pending_active();
        assert!(machine.switch_to(SceneKey::A));
        machine.apply_pending_active();

        let after: Vec<(u64, Vec2)> = machine
            .active_world()
            .entities()
            .iter()
            .map(|entity| (entity.id.0, entity.transform.position))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn inactive_scene_world_does_not_advance() {
        let mut machine = SceneMachine::new(
            Box::new(SteppingScene {
                spawn_count: 1,
                step_x: 1.0,
            }),
            Box::new(SteppingScene {
                spawn_count: 1,
                step_x: 3.0,
            }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        let _ = machine.update_active(1.0 / 60.0, &InputSnapshot::empty());
        machine.apply_pending_active();
        let before_switch = machine.active_world().entities()[0].transform.position.x;

        assert!(machine.switch_to(SceneKey::B));
        machine.apply_pending_active();
        for _ in 0..10 {
            let _ = machine.update_active(1.0 / 60.0, &InputSnapshot::empty());
            machine.apply_pending_active();
        }

        assert!(machine.switch_to(SceneKey::A));
        let after_return = machine.active_world().entities()[0].transform.position.x;
        assert_eq!(before_switch, after_return);
    }

    #[test]
    fn hard_reset_recreates_target_scene_state() {
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 1 }),
            Box::new(TestScene { spawn_count: 1 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        machine.active_world_mut().entities_mut()[0]
            .transform
            .position = Vec2 { x: 9.0, y: 3.0 };
        assert_eq!(
            machine.active_world().entities()[0].transform.position.x,
            9.0
        );

        let _ = machine.hard_reset_to(SceneKey::A);
        machine.apply_pending_active();

        assert_eq!(machine.active_world().entity_count(), 1);
        assert_eq!(
            machine.active_world().entities()[0].transform.position,
            Vec2 { x: 0.0, y: 0.0 }
        );
    }

    #[test]
    fn repeated_switching_after_despawn_is_stable() {
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 2 }),
            Box::new(TestScene { spawn_count: 1 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        let doomed = machine.active_world().entities()[0].id;
        assert!(machine.active_world_mut().despawn(doomed));
        machine.apply_pending_active();
        assert_eq!(machine.active_world().entity_count(), 1);

        for _ in 0..25 {
            assert!(machine.switch_to(SceneKey::B));
            machine.apply_pending_active();
            assert!(machine.switch_to(SceneKey::A));
            machine.apply_pending_active();
            assert_eq!(machine.active_world().entity_count(), 1);
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
        assert!(actor.interactable.is_none());
        assert_eq!(actor.job_state, JobState::Idle);
        assert!(actor.interaction_target.is_none());
    }

    #[test]
    fn pick_topmost_interactable_returns_hit() {
        let mut world = SceneWorld::default();
        let interactable_id = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "pile",
            },
        );
        world.apply_pending();
        world
            .find_entity_mut(interactable_id)
            .expect("exists")
            .interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });

        let picked =
            world.pick_topmost_interactable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(picked, Some(interactable_id));
    }

    #[test]
    fn pick_topmost_interactable_uses_last_applied_spawn_order() {
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
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "second",
            },
        );
        world.apply_pending();
        world.find_entity_mut(first).expect("first").interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });
        world.find_entity_mut(second).expect("second").interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });

        let picked =
            world.pick_topmost_interactable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720));
        assert_eq!(picked, Some(second));
    }

    #[test]
    fn scene_machine_debug_info_passthrough_returns_active_scene_snapshot() {
        let mut machine = SceneMachine::new(
            Box::new(DebugScene),
            Box::new(TestScene { spawn_count: 0 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        let snapshot = machine.debug_info_snapshot_active().expect("snapshot");
        assert_eq!(snapshot.selected_entity, Some(EntityId(7)));
        assert_eq!(
            snapshot.selected_position_world,
            Some(Vec2 { x: 1.0, y: 2.0 })
        );
        assert_eq!(snapshot.selected_job_state, DebugJobState::Idle);
    }
}
