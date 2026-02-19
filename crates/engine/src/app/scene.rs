use super::input::{ActionStates, InputAction};

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
}

impl InputSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn new(
        quit_requested: bool,
        switch_scene_pressed: bool,
        actions: ActionStates,
    ) -> Self {
        Self {
            quit_requested,
            switch_scene_pressed,
            actions,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u64);

#[derive(Debug, Clone, Copy, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
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
}

impl SceneWorld {
    pub fn spawn(&mut self, transform: Transform, renderable: RenderableDesc) -> EntityId {
        let id = self.allocator.allocate();
        self.pending_spawns.push(Entity {
            id,
            transform,
            renderable,
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
            self.entities.append(&mut self.pending_spawns);
        }
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.pending_spawns.clear();
        self.pending_despawns.clear();
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn find_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|entity| entity.id == id)
    }

    pub fn find_entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|entity| entity.id == id)
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
}
