use super::input::{ActionStates, InputAction};
use super::rendering::{world_to_screen_px, PLACEHOLDER_HALF_SIZE_PX};
use crate::content::DefDatabase;
use thiserror::Error;

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

#[derive(Debug, Clone, PartialEq)]
pub enum SceneDebugCommand {
    Spawn {
        def_name: String,
        position: Option<(f32, f32)>,
    },
    Despawn {
        entity_id: u64,
    },
    Select {
        entity_id: u64,
    },
    OrderMove {
        x: f32,
        y: f32,
    },
    OrderInteract {
        target_entity_id: u64,
    },
    FloorSet {
        floor: FloorId,
    },
    DumpState,
    DumpAi,
    ScenarioSetup {
        scenario_id: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SceneDebugContext {
    pub cursor_world: Option<Vec2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneDebugCommandResult {
    Unsupported,
    Success(String),
    Error(String),
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
    zoom_delta_steps: i32,
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
        zoom_delta_steps: i32,
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
            zoom_delta_steps,
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

    pub fn with_zoom_delta_steps(mut self, zoom_delta_steps: i32) -> Self {
        self.zoom_delta_steps = zoom_delta_steps;
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

    pub fn zoom_delta_steps(&self) -> i32 {
        self.zoom_delta_steps
    }

    pub fn window_size(&self) -> (u32, u32) {
        (self.window_width, self.window_height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(pub u64);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FloorId {
    Rooftop,
    #[default]
    Main,
    Basement,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const CAMERA_ZOOM_DEFAULT: f32 = 1.0;
pub const CAMERA_ZOOM_MIN: f32 = 0.5;
pub const CAMERA_ZOOM_MAX: f32 = 2.0;
pub const CAMERA_ZOOM_STEP: f32 = 0.1;

#[derive(Debug, Clone, Copy)]
pub struct Camera2D {
    pub position: Vec2,
    pub zoom: f32,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            position: Vec2::default(),
            zoom: CAMERA_ZOOM_DEFAULT,
        }
    }
}

impl Camera2D {
    pub fn effective_zoom(&self) -> f32 {
        clamp_camera_zoom(self.zoom)
    }

    pub fn set_zoom_clamped(&mut self, zoom: f32) {
        self.zoom = clamp_camera_zoom(zoom);
    }

    pub fn apply_zoom_steps(&mut self, steps: i32) {
        if steps == 0 {
            return;
        }
        let target_zoom = self.zoom + steps as f32 * CAMERA_ZOOM_STEP;
        self.set_zoom_clamped(target_zoom);
    }
}

fn clamp_camera_zoom(zoom: f32) -> f32 {
    if !zoom.is_finite() {
        return CAMERA_ZOOM_DEFAULT;
    }
    zoom.clamp(CAMERA_ZOOM_MIN, CAMERA_ZOOM_MAX)
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

/// Tilemap origin convention:
/// - `origin` is the world position of tile (0,0) bottom-left corner.
/// - The center of tile (x,y) is `origin + (x + 0.5, y + 0.5)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tilemap {
    width: u32,
    height: u32,
    origin: Vec2,
    tiles: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TilemapError {
    #[error("tile count mismatch: expected {expected}, got {actual}")]
    TileCountMismatch { expected: usize, actual: usize },
}

impl Tilemap {
    pub fn new(
        width: u32,
        height: u32,
        origin: Vec2,
        tiles: Vec<u16>,
    ) -> Result<Self, TilemapError> {
        let expected = width as usize * height as usize;
        let actual = tiles.len();
        if expected != actual {
            return Err(TilemapError::TileCountMismatch { expected, actual });
        }
        Ok(Self {
            width,
            height,
            origin,
            tiles,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn origin(&self) -> Vec2 {
        self.origin
    }

    pub fn index_of(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }

    pub fn tile_at(&self, x: u32, y: u32) -> Option<u16> {
        self.index_of(x, y)
            .and_then(|index| self.tiles.get(index).copied())
    }

    pub fn tile_center_world(&self, x: u32, y: u32) -> Option<Vec2> {
        self.index_of(x, y)?;
        Some(Vec2 {
            x: self.origin.x + x as f32 + 0.5,
            y: self.origin.y + y as f32 + 0.5,
        })
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
pub enum OrderState {
    Idle,
    MoveTo {
        point: Vec2,
    },
    Interact {
        target_save_id: u64,
    },
    Working {
        target_save_id: u64,
        remaining_time: f32,
    },
}

impl Default for OrderState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DebugJobState {
    None,
    Idle,
    Working { remaining_time: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugInfoSnapshot {
    pub selected_entity: Option<EntityId>,
    pub selected_position_world: Option<Vec2>,
    pub selected_order_world: Option<Vec2>,
    pub selected_job_state: DebugJobState,
    pub entity_count: usize,
    pub actor_count: usize,
    pub interactable_count: usize,
    pub resource_count: u32,
    pub system_order: String,
    pub extra_debug_lines: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMarkerKind {
    Order,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugMarker {
    pub kind: DebugMarkerKind,
    pub position_world: Vec2,
    pub ttl_seconds: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneVisualState {
    pub selected_actor: Option<EntityId>,
    pub hovered_interactable: Option<EntityId>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: EntityId,
    pub transform: Transform,
    pub renderable: RenderableDesc,
    pub floor: FloorId,
    pub selectable: bool,
    pub actor: bool,
    pub order_state: OrderState,
    pub interactable: Option<Interactable>,
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
    active_floor: FloorId,
    tilemap: Option<Tilemap>,
    visual_state: SceneVisualState,
    debug_markers: Vec<DebugMarker>,
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
            floor: self.active_floor,
            selectable,
            actor,
            order_state: OrderState::Idle,
            interactable: None,
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
            self.pending_despawns.sort_by_key(|id| id.0);
            self.pending_despawns.dedup();
            let pending = &self.pending_despawns;
            self.entities.retain(|entity| {
                pending
                    .binary_search_by_key(&entity.id.0, |id| id.0)
                    .is_err()
            });
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
        self.active_floor = FloorId::Main;
        self.visual_state = SceneVisualState::default();
        self.debug_markers.clear();
    }

    pub fn set_tilemap(&mut self, tilemap: Tilemap) {
        self.tilemap = Some(tilemap);
    }

    pub fn clear_tilemap(&mut self) {
        self.tilemap = None;
    }

    pub fn tilemap(&self) -> Option<&Tilemap> {
        self.tilemap.as_ref()
    }

    pub fn set_selected_actor_visual(&mut self, selected: Option<EntityId>) {
        self.visual_state.selected_actor = selected;
    }

    pub fn set_hovered_interactable_visual(&mut self, hovered: Option<EntityId>) {
        self.visual_state.hovered_interactable = hovered;
    }

    pub fn visual_state(&self) -> &SceneVisualState {
        &self.visual_state
    }

    pub fn push_debug_marker(&mut self, marker: DebugMarker) {
        self.debug_markers.push(marker);
    }

    pub fn debug_markers(&self) -> &[DebugMarker] {
        &self.debug_markers
    }

    pub fn clear_debug_markers(&mut self) {
        self.debug_markers.clear();
    }

    pub fn tick_debug_markers(&mut self, fixed_dt_seconds: f32) {
        self.debug_markers.retain_mut(|marker| {
            marker.ttl_seconds -= fixed_dt_seconds;
            marker.ttl_seconds > 0.0
        });
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

    pub fn active_floor(&self) -> FloorId {
        self.active_floor
    }

    pub fn set_active_floor(&mut self, floor: FloorId) {
        self.active_floor = floor;
    }

    pub fn pick_topmost_selectable_at_cursor(
        &self,
        cursor_position_px: Vec2,
        window_size: (u32, u32),
        floor_filter: Option<FloorId>,
    ) -> Option<EntityId> {
        let cursor_x = cursor_position_px.x.round() as i32;
        let cursor_y = cursor_position_px.y.round() as i32;
        let mut best: Option<(u64, EntityId)> = None;

        for entity in &self.entities {
            if !entity.selectable {
                continue;
            }
            if let Some(required_floor) = floor_filter {
                if entity.floor != required_floor {
                    continue;
                }
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
        floor_filter: Option<FloorId>,
    ) -> Option<EntityId> {
        let cursor_x = cursor_position_px.x.round() as i32;
        let cursor_y = cursor_position_px.y.round() as i32;
        let mut best: Option<(u64, EntityId)> = None;

        for entity in &self.entities {
            if entity.interactable.is_none() {
                continue;
            }
            if let Some(required_floor) = floor_filter {
                if entity.floor != required_floor {
                    continue;
                }
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
    fn execute_debug_command(
        &mut self,
        _command: SceneDebugCommand,
        _context: SceneDebugContext,
        _world: &mut SceneWorld,
    ) -> SceneDebugCommandResult {
        SceneDebugCommandResult::Unsupported
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

    pub(crate) fn execute_debug_command_active(
        &mut self,
        command: SceneDebugCommand,
        context: SceneDebugContext,
    ) -> SceneDebugCommandResult {
        let runtime = self.active_runtime_mut();
        runtime
            .scene
            .execute_debug_command(command, context, &mut runtime.world)
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

    fn make_tilemap(width: u32, height: u32, origin: Vec2, fill: u16) -> Tilemap {
        Tilemap::new(
            width,
            height,
            origin,
            vec![fill; width as usize * height as usize],
        )
        .expect("tilemap")
    }

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
                system_order: "test_order".to_string(),
                extra_debug_lines: Some(vec!["extra: ok".to_string()]),
            })
        }
    }

    struct SceneWithDebugHook;

    impl Scene for SceneWithDebugHook {
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

        fn execute_debug_command(
            &mut self,
            command: SceneDebugCommand,
            _context: SceneDebugContext,
            world: &mut SceneWorld,
        ) -> SceneDebugCommandResult {
            match command {
                SceneDebugCommand::Spawn { .. } => {
                    world.spawn(
                        Transform::default(),
                        RenderableDesc {
                            kind: RenderableKind::Placeholder,
                            debug_name: "debug_spawned",
                        },
                    );
                    SceneDebugCommandResult::Success("spawned".to_string())
                }
                SceneDebugCommand::Despawn { entity_id } => {
                    if world.despawn(EntityId(entity_id)) {
                        SceneDebugCommandResult::Success("despawned".to_string())
                    } else {
                        SceneDebugCommandResult::Error("missing entity".to_string())
                    }
                }
                SceneDebugCommand::Select { .. }
                | SceneDebugCommand::OrderMove { .. }
                | SceneDebugCommand::OrderInteract { .. }
                | SceneDebugCommand::FloorSet { .. }
                | SceneDebugCommand::DumpState
                | SceneDebugCommand::DumpAi
                | SceneDebugCommand::ScenarioSetup { .. } => SceneDebugCommandResult::Unsupported,
            }
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
    fn scene_world_duplicate_pending_despawns_are_safe_and_idempotent() {
        let mut world = SceneWorld::default();
        let doomed = world.spawn(
            Transform::default(),
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "doomed",
            },
        );
        let survivor = world.spawn(
            Transform {
                position: Vec2 { x: 3.0, y: 1.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "survivor",
            },
        );
        world.apply_pending();
        assert_eq!(world.entity_count(), 2);

        assert!(world.despawn(doomed));
        assert!(world.despawn(doomed));
        assert!(world.despawn(doomed));
        world.apply_pending();

        assert_eq!(world.entity_count(), 1);
        assert!(world.find_entity(doomed).is_none());
        assert!(world.find_entity(survivor).is_some());
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
        assert!((camera.zoom - CAMERA_ZOOM_DEFAULT).abs() < 0.0001);
    }

    #[test]
    fn camera_apply_zoom_steps_clamps_at_bounds() {
        let mut camera = Camera2D::default();
        camera.apply_zoom_steps(200);
        assert!((camera.zoom - CAMERA_ZOOM_MAX).abs() < 0.0001);

        camera.apply_zoom_steps(-400);
        assert!((camera.zoom - CAMERA_ZOOM_MIN).abs() < 0.0001);
    }

    #[test]
    fn clear_keeps_def_database_resource() {
        let mut world = SceneWorld::default();
        world.set_def_database(DefDatabase::default());
        world.clear();
        assert!(world.def_database().is_some());
    }

    #[test]
    fn tilemap_new_rejects_invalid_tile_count() {
        let err = Tilemap::new(2, 2, Vec2 { x: 0.0, y: 0.0 }, vec![0, 1, 2]).expect_err("err");
        assert_eq!(
            err,
            TilemapError::TileCountMismatch {
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn tilemap_indexing_and_bounds() {
        let tilemap =
            Tilemap::new(2, 2, Vec2 { x: 0.0, y: 0.0 }, vec![10, 11, 12, 13]).expect("tilemap");
        assert_eq!(tilemap.index_of(0, 0), Some(0));
        assert_eq!(tilemap.index_of(1, 1), Some(3));
        assert_eq!(tilemap.tile_at(0, 0), Some(10));
        assert_eq!(tilemap.tile_at(1, 1), Some(13));
        assert_eq!(tilemap.index_of(2, 0), None);
        assert_eq!(tilemap.index_of(0, 2), None);
        assert_eq!(tilemap.tile_at(2, 2), None);
    }

    #[test]
    fn tilemap_origin_center_formula_is_enforced() {
        let tilemap = make_tilemap(4, 4, Vec2 { x: 3.0, y: -2.0 }, 1);
        let center = tilemap.tile_center_world(2, 1).expect("center");
        assert_eq!(center, Vec2 { x: 5.5, y: -0.5 });
    }

    #[test]
    fn scene_world_clear_preserves_tilemap() {
        let mut world = SceneWorld::default();
        world.set_tilemap(make_tilemap(3, 2, Vec2 { x: -1.0, y: 4.0 }, 7));
        world.clear();
        let tilemap = world.tilemap().expect("tilemap");
        assert_eq!(tilemap.width(), 3);
        assert_eq!(tilemap.height(), 2);
        assert_eq!(tilemap.origin(), Vec2 { x: -1.0, y: 4.0 });
        assert_eq!(tilemap.tile_at(1, 1), Some(7));
    }

    #[test]
    fn clear_tilemap_explicitly_removes_tilemap() {
        let mut world = SceneWorld::default();
        world.set_tilemap(make_tilemap(2, 2, Vec2 { x: 0.0, y: 0.0 }, 1));
        assert!(world.tilemap().is_some());
        world.clear_tilemap();
        assert!(world.tilemap().is_none());
    }

    #[test]
    fn debug_markers_ttl_decrements_and_removes_expired_single_pass_behavior() {
        let mut world = SceneWorld::default();
        world.push_debug_marker(DebugMarker {
            kind: DebugMarkerKind::Order,
            position_world: Vec2 { x: 1.0, y: 2.0 },
            ttl_seconds: 1.0,
        });
        world.push_debug_marker(DebugMarker {
            kind: DebugMarkerKind::Order,
            position_world: Vec2 { x: -1.0, y: -2.0 },
            ttl_seconds: 0.25,
        });
        world.push_debug_marker(DebugMarker {
            kind: DebugMarkerKind::Order,
            position_world: Vec2 { x: 3.0, y: 4.0 },
            ttl_seconds: 0.5,
        });

        world.tick_debug_markers(0.5);

        let markers = world.debug_markers();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].position_world, Vec2 { x: 1.0, y: 2.0 });
        assert!((markers[0].ttl_seconds - 0.5).abs() < 0.0001);
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
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720), None);
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
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720), None);
        assert_eq!(before, Some(second_overlap));
        assert_ne!(before, Some(first_overlap));

        assert!(world.despawn(unrelated));
        world.apply_pending();

        let after =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720), None);
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
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 20.0, y: 20.0 }, (1280, 720), None);
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
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720), None);
        assert_eq!(picked, None);
    }

    #[test]
    fn pick_topmost_selectable_includes_sprite_renderables() {
        let mut world = SceneWorld::default();
        let sprite_entity = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Sprite("ui/icons/worker_1".to_string()),
                debug_name: "sprite_selectable",
            },
        );
        world.apply_pending();

        let picked =
            world.pick_topmost_selectable_at_cursor(Vec2 { x: 640.0, y: 360.0 }, (1280, 720), None);
        assert_eq!(picked, Some(sprite_entity));
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
        assert_eq!(actor.order_state, OrderState::Idle);
        assert!(actor.interactable.is_none());
    }

    #[test]
    fn spawn_defaults_to_main_floor() {
        let mut world = SceneWorld::default();
        let entity_id = world.spawn(
            Transform::default(),
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "main_default",
            },
        );
        world.apply_pending();
        assert_eq!(
            world.find_entity(entity_id).expect("entity").floor,
            FloorId::Main
        );
    }

    #[test]
    fn spawn_uses_active_floor_for_new_entities() {
        let mut world = SceneWorld::default();
        world.set_active_floor(FloorId::Basement);
        let entity_id = world.spawn(
            Transform::default(),
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "basement_spawn",
            },
        );
        world.apply_pending();
        assert_eq!(
            world.find_entity(entity_id).expect("entity").floor,
            FloorId::Basement
        );
    }

    #[test]
    fn pick_topmost_selectable_optional_floor_filter_is_deterministic() {
        let mut world = SceneWorld::default();
        world.set_active_floor(FloorId::Main);
        let main = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "main_selectable",
            },
        );
        world.set_active_floor(FloorId::Rooftop);
        let rooftop = world.spawn_selectable(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "rooftop_selectable",
            },
        );
        world.apply_pending();

        let cursor = Vec2 { x: 640.0, y: 360.0 };
        assert_eq!(
            world.pick_topmost_selectable_at_cursor(cursor, (1280, 720), None),
            Some(rooftop)
        );
        assert_eq!(
            world.pick_topmost_selectable_at_cursor(cursor, (1280, 720), Some(FloorId::Main)),
            Some(main)
        );
        assert_eq!(
            world.pick_topmost_selectable_at_cursor(cursor, (1280, 720), Some(FloorId::Basement)),
            None
        );
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

        let picked = world.pick_topmost_interactable_at_cursor(
            Vec2 { x: 640.0, y: 360.0 },
            (1280, 720),
            None,
        );
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

        let picked = world.pick_topmost_interactable_at_cursor(
            Vec2 { x: 640.0, y: 360.0 },
            (1280, 720),
            None,
        );
        assert_eq!(picked, Some(second));
    }

    #[test]
    fn pick_topmost_interactable_includes_sprite_renderables() {
        let mut world = SceneWorld::default();
        let sprite_interactable = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Sprite("objects/resource_pile".to_string()),
                debug_name: "sprite_interactable",
            },
        );
        world.apply_pending();
        world
            .find_entity_mut(sprite_interactable)
            .expect("exists")
            .interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });

        let picked = world.pick_topmost_interactable_at_cursor(
            Vec2 { x: 640.0, y: 360.0 },
            (1280, 720),
            None,
        );
        assert_eq!(picked, Some(sprite_interactable));
    }

    #[test]
    fn pick_topmost_interactable_optional_floor_filter_is_deterministic() {
        let mut world = SceneWorld::default();
        world.set_active_floor(FloorId::Main);
        let main = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "main_interactable",
            },
        );
        world.set_active_floor(FloorId::Basement);
        let basement = world.spawn(
            Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            RenderableDesc {
                kind: RenderableKind::Placeholder,
                debug_name: "basement_interactable",
            },
        );
        world.apply_pending();
        world.find_entity_mut(main).expect("main").interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });
        world
            .find_entity_mut(basement)
            .expect("basement")
            .interactable = Some(Interactable {
            kind: InteractableKind::ResourcePile,
            interaction_radius: 0.75,
            remaining_uses: 3,
        });

        let cursor = Vec2 { x: 640.0, y: 360.0 };
        assert_eq!(
            world.pick_topmost_interactable_at_cursor(cursor, (1280, 720), None),
            Some(basement)
        );
        assert_eq!(
            world.pick_topmost_interactable_at_cursor(cursor, (1280, 720), Some(FloorId::Main)),
            Some(main)
        );
        assert_eq!(
            world.pick_topmost_interactable_at_cursor(cursor, (1280, 720), Some(FloorId::Rooftop)),
            None
        );
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

    #[test]
    fn default_scene_debug_hook_is_unsupported() {
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 0 }),
            Box::new(TestScene { spawn_count: 0 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        let result = machine.execute_debug_command_active(
            SceneDebugCommand::Spawn {
                def_name: "proto.worker".to_string(),
                position: None,
            },
            SceneDebugContext::default(),
        );

        assert_eq!(result, SceneDebugCommandResult::Unsupported);
    }

    #[test]
    fn scene_machine_forwards_debug_command_to_active_scene() {
        let mut machine = SceneMachine::new(
            Box::new(SceneWithDebugHook),
            Box::new(TestScene { spawn_count: 0 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        let result = machine.execute_debug_command_active(
            SceneDebugCommand::Spawn {
                def_name: "proto.worker".to_string(),
                position: Some((1.0, 2.0)),
            },
            SceneDebugContext::default(),
        );
        assert_eq!(
            result,
            SceneDebugCommandResult::Success("spawned".to_string())
        );

        machine.apply_pending_active();
        assert_eq!(machine.active_world().entity_count(), 1);
    }

    #[test]
    fn scene_switch_preserves_each_scene_tilemap_state() {
        let mut machine = SceneMachine::new(
            Box::new(TestScene { spawn_count: 0 }),
            Box::new(TestScene { spawn_count: 0 }),
            SceneKey::A,
        );
        machine.load_active();
        machine.apply_pending_active();

        machine
            .active_world_mut()
            .set_tilemap(make_tilemap(2, 2, Vec2 { x: 0.0, y: 0.0 }, 1));
        assert!(machine.switch_to(SceneKey::B));
        machine.apply_pending_active();
        machine
            .active_world_mut()
            .set_tilemap(make_tilemap(2, 2, Vec2 { x: 10.0, y: 10.0 }, 2));

        assert!(machine.switch_to(SceneKey::A));
        machine.apply_pending_active();
        let a_tilemap = machine.active_world().tilemap().expect("a tilemap");
        assert_eq!(a_tilemap.origin(), Vec2 { x: 0.0, y: 0.0 });
        assert_eq!(a_tilemap.tile_at(0, 0), Some(1));

        assert!(machine.switch_to(SceneKey::B));
        machine.apply_pending_active();
        let b_tilemap = machine.active_world().tilemap().expect("b tilemap");
        assert_eq!(b_tilemap.origin(), Vec2 { x: 10.0, y: 10.0 });
        assert_eq!(b_tilemap.tile_at(0, 0), Some(2));
    }
}
