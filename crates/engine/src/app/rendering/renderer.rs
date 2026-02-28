use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::ImageReader;
use pixels::{Error, Pixels, SurfaceTexture};
use tracing::warn;
use winit::window::Window;

use crate::app::{
    tools::{draw_command_palette, draw_console, draw_overlay},
    ActionState, Camera2D, CardinalFacing, CommandPaletteRenderData, ConsoleState, DebugMarkerKind,
    Entity, EntityActionVisual, FloorId, OverlayData, RenderableKind, SceneWorld, SpriteAnchorName,
    SpriteAnchorPx, SpriteAnchors, Tilemap, Vec2,
};
use crate::content::DefDatabase;
use crate::sprite_keys::validate_sprite_key;

use super::transform::camera_pixels_per_world;
use super::{world_to_screen_px, Viewport, PIXELS_PER_WORLD, PLACEHOLDER_HALF_SIZE_PX};
use std::f32::consts::TAU;

const CLEAR_COLOR_ROOFTOP: [u8; 4] = [24, 26, 33, 255];
const CLEAR_COLOR_MAIN: [u8; 4] = [20, 22, 28, 255];
const CLEAR_COLOR_BASEMENT: [u8; 4] = [16, 18, 24, 255];
const PLACEHOLDER_COLOR: [u8; 4] = [220, 220, 240, 255];
const GRID_CELL_WORLD: f32 = 1.0;
const GRID_MAJOR_EVERY: i32 = 5;
const GRID_MINOR_COLOR: [u8; 4] = [35, 39, 46, 255];
const GRID_MAJOR_COLOR: [u8; 4] = [52, 58, 70, 255];
const TILE_FALLBACK_GRASS_COLOR: [u8; 4] = [74, 112, 56, 255];
const TILE_FALLBACK_DIRT_COLOR: [u8; 4] = [112, 83, 58, 255];
const TILE_FALLBACK_UNKNOWN_COLOR: [u8; 4] = [68, 74, 62, 255];
const SELECTED_HIGHLIGHT_COLOR: [u8; 4] = [80, 220, 255, 255];
const SELECTED_XRAY_HIGHLIGHT_COLOR: [u8; 4] = [210, 245, 255, 255];
const HOVER_HIGHLIGHT_COLOR: [u8; 4] = [255, 210, 70, 255];
const TARGET_XRAY_HIGHLIGHT_COLOR: [u8; 4] = [255, 236, 120, 255];
const ORDER_MARKER_COLOR: [u8; 4] = [255, 120, 120, 255];
const SELECTED_HIGHLIGHT_HALF_SIZE_PX: i32 = 10;
const HOVER_HIGHLIGHT_HALF_SIZE_PX: i32 = 8;
const ORDER_MARKER_HALF_SIZE_PX: i32 = 6;
const VIEW_CULL_PADDING_PX: f32 = 16.0;
const ENTITY_CULL_RADIUS_WORLD_TILES: f32 = 0.5;
const MICRO_GRID_RESOLUTION_PX: i32 = 1;
const IDLE_BOB_AMPLITUDE_PX: f32 = 0.35;
const IDLE_BOB_CYCLES_PER_TICK: f32 = 0.0125;
const WALK_BOB_AMPLITUDE_PX: f32 = 1.0;
const WALK_BOB_X_AMPLITUDE_PX: f32 = 0.45;
const WALK_BOB_CYCLES_PER_TICK: f32 = 0.08;
const WALK_SPRING_ALPHA_PER_TICK: f32 = 0.25;
const HIT_KICK_MAX_PX: f32 = 2.5;
const HIT_KICK_ROTATION_SCALE: f32 = 0.02;
const PROCEDURAL_ENTITY_PHASE_SEED: f32 = 0.173;

#[derive(Debug, Clone, Copy)]
struct WorldBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileRectInclusive {
    x_min: u32,
    x_max: u32,
    y_min: u32,
    y_max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenRectPx {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

struct LoadedSprite {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedCarrySprite {
    sprite_key: String,
    pixel_scale: u8,
    anchors: SpriteAnchors,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProceduralVisualOffset {
    offset_px: Vec2,
    #[allow(dead_code)]
    rotation_radians: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct WalkSpringState {
    amplitude: f32,
    last_tick: u64,
}

pub struct Renderer {
    window: Arc<Window>,
    pixels: Pixels<'static>,
    viewport: Viewport,
    asset_root: PathBuf,
    sprite_cache: HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: HashSet<String>,
    visible_entity_draw_indices: Vec<usize>,
    carry_sprite_cache: HashMap<String, Option<CachedCarrySprite>>,
    last_def_db_identity: Option<usize>,
    walk_spring_by_entity: HashMap<crate::app::EntityId, WalkSpringState>,
}

impl Renderer {
    pub fn new(window: Arc<Window>, asset_root: PathBuf) -> Result<Self, Error> {
        let size = window.inner_size();
        let pixels = Self::build_pixels(Arc::clone(&window), size.width, size.height)?;
        Ok(Self {
            window,
            pixels,
            viewport: Viewport {
                width: size.width,
                height: size.height,
            },
            asset_root,
            sprite_cache: HashMap::new(),
            warned_missing_sprite_keys: HashSet::new(),
            visible_entity_draw_indices: Vec::new(),
            carry_sprite_cache: HashMap::new(),
            last_def_db_identity: None,
            walk_spring_by_entity: HashMap::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.pixels = Self::build_pixels(Arc::clone(&self.window), width, height)?;
        self.viewport = Viewport { width, height };
        Ok(())
    }

    fn build_pixels(
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<Pixels<'static>, Error> {
        let surface = SurfaceTexture::new(width, height, window);
        Pixels::new(width, height, surface)
    }

    pub(crate) fn render_world(
        &mut self,
        world: &SceneWorld,
        sim_tick_counter: u64,
        overlay_data: Option<&OverlayData>,
        console_state: Option<&ConsoleState>,
        command_palette: Option<&CommandPaletteRenderData>,
    ) -> Result<(), Error> {
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Ok(());
        }

        let asset_root = self.asset_root.as_path();
        let sprite_cache = &mut self.sprite_cache;
        let warned_missing_sprite_keys = &mut self.warned_missing_sprite_keys;
        let visible_entity_draw_indices = &mut self.visible_entity_draw_indices;
        let carry_sprite_cache = &mut self.carry_sprite_cache;
        let walk_spring_by_entity = &mut self.walk_spring_by_entity;
        let frame = self.pixels.frame_mut();
        let def_db = world.def_database();
        let def_db_identity = def_db.map(|db| db as *const DefDatabase as usize);
        if self.last_def_db_identity != def_db_identity {
            carry_sprite_cache.clear();
            walk_spring_by_entity.clear();
            self.last_def_db_identity = def_db_identity;
        }
        let active_floor = world.active_floor();
        let clear_color = clear_color_for_floor(active_floor);
        for chunk in frame.chunks_exact_mut(4) {
            chunk.copy_from_slice(&clear_color);
        }
        let view_bounds = view_bounds_world(
            world.camera(),
            (self.viewport.width, self.viewport.height),
            VIEW_CULL_PADDING_PX,
        );
        collect_sorted_visible_entity_draw_indices(
            world,
            active_floor,
            &view_bounds,
            visible_entity_draw_indices,
        );

        draw_tilemap(
            frame,
            self.viewport.width,
            self.viewport.height,
            world,
            &view_bounds,
            sprite_cache,
            warned_missing_sprite_keys,
            asset_root,
        );
        draw_world_grid(frame, self.viewport.width, self.viewport.height, world);
        let default_action_visual = EntityActionVisual::default();

        for entity_index in visible_entity_draw_indices.iter().copied() {
            let entity = &world.entities()[entity_index];
            let action_visual = world
                .entity_action_visual_ref(entity.id)
                .unwrap_or(&default_action_visual);
            let procedural_offset = compute_procedural_offset(
                walk_spring_by_entity,
                entity.id,
                action_visual,
                sim_tick_counter,
            );
            draw_renderable_at_world_position(
                frame,
                self.viewport.width,
                self.viewport.height,
                world.camera(),
                entity.transform.position,
                procedural_offset.offset_px,
                action_visual,
                &entity.renderable.kind,
                sprite_cache,
                warned_missing_sprite_keys,
                asset_root,
                true,
            );

            let Some(anchor_name) = held_attachment_anchor_name(action_visual.action_state) else {
                continue;
            };
            let Some(held_visual_def_name) = action_visual.held_visual.as_deref() else {
                continue;
            };
            let Some(carry_sprite) =
                resolve_cached_carry_sprite(carry_sprite_cache, def_db, held_visual_def_name)
            else {
                continue;
            };
            let (carry_x, carry_y) = entity_carry_anchor_screen_position_px(
                world.camera(),
                (self.viewport.width, self.viewport.height),
                entity,
                action_visual.action_params.facing,
                procedural_offset.offset_px,
                anchor_name,
            );
            draw_cached_carry_sprite_at_screen_position(
                frame,
                self.viewport.width,
                self.viewport.height,
                world.camera(),
                carry_x,
                carry_y,
                carry_sprite,
                sprite_cache,
                warned_missing_sprite_keys,
                asset_root,
            );
        }

        draw_affordances(
            frame,
            self.viewport.width,
            self.viewport.height,
            world,
            &view_bounds,
        );

        if let Some(data) = overlay_data {
            draw_overlay(frame, self.viewport.width, self.viewport.height, data);
        }
        if let Some(palette) = command_palette {
            draw_command_palette(frame, self.viewport.width, self.viewport.height, palette);
        }
        if let Some(console) = console_state {
            draw_console(frame, self.viewport.width, self.viewport.height, console);
        }

        self.pixels.render()
    }
}

fn draw_renderable_at_world_position(
    frame: &mut [u8],
    width: u32,
    height: u32,
    camera: &Camera2D,
    world_position: Vec2,
    visual_offset_px: Vec2,
    action_visual: &EntityActionVisual,
    renderable: &RenderableKind,
    sprite_cache: &mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
    draw_placeholder_on_missing_sprite: bool,
) {
    let (cx, cy) = world_to_snapped_screen_px_with_offset(
        camera,
        (width, height),
        world_position,
        visual_offset_px,
    );
    match renderable {
        RenderableKind::Placeholder => {
            draw_square(
                frame,
                width,
                height,
                cx,
                cy,
                PLACEHOLDER_HALF_SIZE_PX,
                PLACEHOLDER_COLOR,
            );
        }
        RenderableKind::Sprite {
            key, pixel_scale, ..
        } => {
            if let Some(sprite) = resolve_sprite_for_action_visual(
                sprite_cache,
                warned_missing_sprite_keys,
                asset_root,
                key,
                action_visual,
            ) {
                draw_sprite_centered_scaled(
                    frame,
                    width,
                    height,
                    cx,
                    cy,
                    sprite,
                    *pixel_scale as f32 * camera.effective_zoom(),
                );
            } else if draw_placeholder_on_missing_sprite {
                draw_square(
                    frame,
                    width,
                    height,
                    cx,
                    cy,
                    PLACEHOLDER_HALF_SIZE_PX,
                    PLACEHOLDER_COLOR,
                );
            }
        }
    }
}

fn held_attachment_anchor_name(action_state: ActionState) -> Option<SpriteAnchorName> {
    match action_state {
        ActionState::Carry => Some(SpriteAnchorName::Carry),
        ActionState::UseTool => Some(SpriteAnchorName::Tool),
        _ => None,
    }
}

fn resolve_sprite_for_action_visual<'a>(
    sprite_cache: &'a mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
    base_key: &str,
    action_visual: &EntityActionVisual,
) -> Option<&'a LoadedSprite> {
    let Some((state_and_facing_key, state_key)) =
        visual_test_variant_candidate_keys(base_key, action_visual)
    else {
        return resolve_cached_sprite(
            sprite_cache,
            warned_missing_sprite_keys,
            asset_root,
            base_key,
        );
    };

    if let Some(sprite_ptr) = resolve_cached_sprite_with_missing_policy(
        sprite_cache,
        warned_missing_sprite_keys,
        asset_root,
        &state_and_facing_key,
        false,
    )
    .map(|sprite| sprite as *const LoadedSprite)
    {
        // SAFETY: pointer originates from `sprite_cache` and remains valid for the cache borrow.
        return unsafe { sprite_ptr.as_ref() };
    }

    if let Some(sprite_ptr) = resolve_cached_sprite_with_missing_policy(
        sprite_cache,
        warned_missing_sprite_keys,
        asset_root,
        &state_key,
        false,
    )
    .map(|sprite| sprite as *const LoadedSprite)
    {
        // SAFETY: pointer originates from `sprite_cache` and remains valid for the cache borrow.
        return unsafe { sprite_ptr.as_ref() };
    }

    resolve_cached_sprite(
        sprite_cache,
        warned_missing_sprite_keys,
        asset_root,
        base_key,
    )
}

fn visual_test_variant_candidate_keys(
    base_key: &str,
    action_visual: &EntityActionVisual,
) -> Option<(String, String)> {
    if !base_key.starts_with("visual_test/") {
        return None;
    }
    let state_token = action_state_variant_token(action_visual.action_state)?;
    let facing_token = facing_variant_token(action_visual.action_params.facing)?;
    Some((
        format!("{base_key}__{state_token}_{facing_token}"),
        format!("{base_key}__{state_token}"),
    ))
}

fn action_state_variant_token(action_state: ActionState) -> Option<&'static str> {
    match action_state {
        ActionState::Idle => Some("idle"),
        ActionState::Walk => Some("walk"),
        ActionState::Interact => Some("interact"),
        ActionState::Carry => Some("carry"),
        ActionState::UseTool => Some("usetool"),
        ActionState::Hit => Some("hit"),
        ActionState::Downed => Some("downed"),
    }
}

fn facing_variant_token(facing: Option<CardinalFacing>) -> Option<&'static str> {
    match facing {
        Some(CardinalFacing::North) => Some("north"),
        Some(CardinalFacing::South) => Some("south"),
        Some(CardinalFacing::East) => Some("east"),
        Some(CardinalFacing::West) => Some("west"),
        None => Some("south"),
    }
}

fn resolve_cached_carry_sprite<'a>(
    cache: &'a mut HashMap<String, Option<CachedCarrySprite>>,
    def_db: Option<&DefDatabase>,
    held_visual_def_name: &str,
) -> Option<&'a CachedCarrySprite> {
    if !cache.contains_key(held_visual_def_name) {
        let resolved = def_db
            .and_then(|db| db.entity_def_id_by_name(held_visual_def_name))
            .and_then(|def_id| def_db.and_then(|db| db.entity_def(def_id)))
            .and_then(|archetype| match &archetype.renderable {
                RenderableKind::Sprite {
                    key,
                    pixel_scale,
                    anchors,
                } => Some(CachedCarrySprite {
                    sprite_key: key.clone(),
                    pixel_scale: *pixel_scale,
                    anchors: *anchors,
                }),
                RenderableKind::Placeholder => None,
            });
        cache.insert(held_visual_def_name.to_string(), resolved);
    }
    cache.get(held_visual_def_name).and_then(Option::as_ref)
}

fn draw_cached_carry_sprite_at_screen_position(
    frame: &mut [u8],
    width: u32,
    height: u32,
    camera: &Camera2D,
    cx: i32,
    cy: i32,
    cached_carry_sprite: &CachedCarrySprite,
    sprite_cache: &mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
) {
    let Some(sprite) = resolve_cached_sprite(
        sprite_cache,
        warned_missing_sprite_keys,
        asset_root,
        &cached_carry_sprite.sprite_key,
    ) else {
        return;
    };
    draw_sprite_centered_scaled(
        frame,
        width,
        height,
        cx,
        cy,
        sprite,
        cached_carry_sprite.pixel_scale as f32 * camera.effective_zoom(),
    );
}

fn entity_carry_anchor_screen_position_px(
    camera: &Camera2D,
    window_size: (u32, u32),
    entity: &Entity,
    facing: Option<CardinalFacing>,
    visual_offset_px: Vec2,
    anchor_name: SpriteAnchorName,
) -> (i32, i32) {
    let RenderableKind::Sprite {
        pixel_scale,
        anchors,
        ..
    } = &entity.renderable.kind
    else {
        return world_to_snapped_screen_px_with_offset(
            camera,
            window_size,
            entity.transform.position,
            visual_offset_px,
        );
    };
    let anchor = sprite_anchor_or_origin(*anchors, anchor_name);
    let transformed_anchor = transform_anchor_for_facing(anchor, facing);
    let anchor_scale = *pixel_scale as f32 * camera.effective_zoom();
    let (offset_x, offset_y) = anchor_screen_delta_px(transformed_anchor, anchor_scale);
    let (base_x, base_y) = world_to_snapped_screen_px_with_offset(
        camera,
        window_size,
        entity.transform.position,
        visual_offset_px,
    );
    (
        snap_screen_coordinate_px(base_x + offset_x, MICRO_GRID_RESOLUTION_PX),
        snap_screen_coordinate_px(base_y + offset_y, MICRO_GRID_RESOLUTION_PX),
    )
}

fn world_to_snapped_screen_px_with_offset(
    camera: &Camera2D,
    window_size: (u32, u32),
    world_pos: Vec2,
    visual_offset_px: Vec2,
) -> (i32, i32) {
    let (base_x, base_y) = world_to_screen_px(camera, window_size, world_pos);
    let x = base_x + visual_offset_px.x.round() as i32;
    let y = base_y + visual_offset_px.y.round() as i32;
    (
        snap_screen_coordinate_px(x, MICRO_GRID_RESOLUTION_PX),
        snap_screen_coordinate_px(y, MICRO_GRID_RESOLUTION_PX),
    )
}

fn compute_procedural_offset(
    walk_spring_by_entity: &mut HashMap<crate::app::EntityId, WalkSpringState>,
    entity_id: crate::app::EntityId,
    action_visual: &EntityActionVisual,
    sim_tick_counter: u64,
) -> ProceduralVisualOffset {
    let phase_seed = entity_id.0 as f32 * PROCEDURAL_ENTITY_PHASE_SEED;
    let phase = action_visual.action_params.phase + phase_seed;
    match action_visual.action_state {
        ActionState::Idle => {
            let theta = TAU * (sim_tick_counter as f32 * IDLE_BOB_CYCLES_PER_TICK + phase);
            let y = theta.sin() * IDLE_BOB_AMPLITUDE_PX;
            ProceduralVisualOffset {
                offset_px: Vec2 { x: 0.0, y },
                rotation_radians: y * 0.01,
            }
        }
        ActionState::Walk => {
            let smoothed_speed01 = update_walk_spring_amplitude(
                walk_spring_by_entity,
                entity_id,
                action_visual.action_params.speed01.clamp(0.0, 1.0),
                sim_tick_counter,
            );
            let theta = TAU * (sim_tick_counter as f32 * WALK_BOB_CYCLES_PER_TICK + phase);
            let y = theta.sin() * WALK_BOB_AMPLITUDE_PX * smoothed_speed01;
            let x = (theta * 2.0).cos() * WALK_BOB_X_AMPLITUDE_PX * smoothed_speed01;
            ProceduralVisualOffset {
                offset_px: Vec2 { x, y },
                rotation_radians: x * 0.01,
            }
        }
        ActionState::Hit => {
            let phase01 = action_visual.action_params.phase.clamp(0.0, 1.0);
            let envelope = if phase01 < 0.2 {
                phase01 / 0.2
            } else if phase01 < 0.6 {
                1.0 - ((phase01 - 0.2) / 0.4)
            } else {
                0.0
            };
            let magnitude01 = action_visual
                .action_params
                .intensity
                .max(action_visual.action_params.speed01)
                .clamp(0.0, 1.0);
            let x = envelope * magnitude01 * HIT_KICK_MAX_PX;
            ProceduralVisualOffset {
                offset_px: Vec2 { x, y: 0.0 },
                rotation_radians: x * HIT_KICK_ROTATION_SCALE,
            }
        }
        _ => ProceduralVisualOffset::default(),
    }
}

fn update_walk_spring_amplitude(
    walk_spring_by_entity: &mut HashMap<crate::app::EntityId, WalkSpringState>,
    entity_id: crate::app::EntityId,
    target: f32,
    sim_tick_counter: u64,
) -> f32 {
    let entry = walk_spring_by_entity
        .entry(entity_id)
        .or_insert(WalkSpringState {
            amplitude: target,
            last_tick: sim_tick_counter,
        });
    if sim_tick_counter > entry.last_tick {
        let ticks_elapsed = sim_tick_counter.saturating_sub(entry.last_tick);
        let decay = (1.0 - WALK_SPRING_ALPHA_PER_TICK).powf(ticks_elapsed as f32);
        entry.amplitude = target + (entry.amplitude - target) * decay;
        entry.last_tick = sim_tick_counter;
    }
    entry.amplitude.clamp(0.0, 1.0)
}

fn sprite_anchor_or_origin(anchors: SpriteAnchors, name: SpriteAnchorName) -> SpriteAnchorPx {
    anchors.get(name).unwrap_or_default()
}

fn transform_anchor_for_facing(
    anchor: SpriteAnchorPx,
    facing: Option<CardinalFacing>,
) -> SpriteAnchorPx {
    if matches!(facing, Some(CardinalFacing::West)) {
        SpriteAnchorPx {
            x_px: anchor.x_px.saturating_neg(),
            y_px: anchor.y_px,
        }
    } else {
        anchor
    }
}

fn anchor_screen_delta_px(anchor: SpriteAnchorPx, scale: f32) -> (i32, i32) {
    (
        (anchor.x_px as f32 * scale).round() as i32,
        (anchor.y_px as f32 * scale).round() as i32,
    )
}

fn clear_color_for_floor(floor: FloorId) -> [u8; 4] {
    match floor {
        FloorId::Rooftop => CLEAR_COLOR_ROOFTOP,
        FloorId::Main => CLEAR_COLOR_MAIN,
        FloorId::Basement => CLEAR_COLOR_BASEMENT,
    }
}

fn entity_visible_on_active_floor(
    entity: &Entity,
    active_floor: FloorId,
    view_bounds: &WorldBounds,
) -> bool {
    entity.floor == active_floor
        && bounds_intersects_point_radius(
            view_bounds,
            entity.transform.position,
            ENTITY_CULL_RADIUS_WORLD_TILES,
        )
}

fn collect_sorted_visible_entity_draw_indices(
    world: &SceneWorld,
    active_floor: FloorId,
    view_bounds: &WorldBounds,
    out: &mut Vec<usize>,
) {
    out.clear();
    for (index, entity) in world.entities().iter().enumerate() {
        if entity_visible_on_active_floor(entity, active_floor, view_bounds) {
            out.push(index);
        }
    }
    out.sort_by(|left, right| {
        let left_entity = &world.entities()[*left];
        let right_entity = &world.entities()[*right];
        left_entity
            .renderer_overlap_order_key()
            .cmp(&right_entity.renderer_overlap_order_key())
            .then_with(|| left_entity.id.0.cmp(&right_entity.id.0))
    });
}

fn snap_screen_coordinate_px(coord_px: i32, micro_grid_px: i32) -> i32 {
    let step = micro_grid_px.max(1);
    let remainder = coord_px.rem_euclid(step);
    if remainder == 0 {
        return coord_px;
    }

    let lower = coord_px - remainder;
    let upper = lower + step;
    let distance_to_lower = remainder;
    let distance_to_upper = step - remainder;

    if distance_to_lower < distance_to_upper {
        lower
    } else if distance_to_upper < distance_to_lower {
        upper
    } else if coord_px >= 0 {
        upper
    } else {
        lower
    }
}

fn snapped_world_to_screen_px(
    camera: &Camera2D,
    window_size: (u32, u32),
    world_pos: Vec2,
) -> (i32, i32) {
    let (x, y) = world_to_screen_px(camera, window_size, world_pos);
    (
        snap_screen_coordinate_px(x, MICRO_GRID_RESOLUTION_PX),
        snap_screen_coordinate_px(y, MICRO_GRID_RESOLUTION_PX),
    )
}

fn draw_affordances(
    frame: &mut [u8],
    width: u32,
    height: u32,
    world: &SceneWorld,
    view_bounds: &WorldBounds,
) {
    let visuals = world.visual_state();
    let active_floor = world.active_floor();

    if let Some(selected_id) = visuals.selected_actor {
        if let Some(entity) = world.find_entity(selected_id) {
            if entity.actor && entity.floor == active_floor {
                if bounds_intersects_point_radius(
                    view_bounds,
                    entity.transform.position,
                    ENTITY_CULL_RADIUS_WORLD_TILES,
                ) {
                    let (cx, cy) = snapped_world_to_screen_px(
                        world.camera(),
                        (width, height),
                        entity.transform.position,
                    );
                    draw_square_outline(
                        frame,
                        width,
                        height,
                        cx,
                        cy,
                        SELECTED_HIGHLIGHT_HALF_SIZE_PX,
                        SELECTED_HIGHLIGHT_COLOR,
                    );

                    let subject_rect =
                        entity_screen_rect_placeholder(world.camera(), (width, height), entity);
                    if is_entity_occluded_by_front_overlap(
                        world,
                        (width, height),
                        active_floor,
                        entity.id,
                        subject_rect,
                        entity.renderer_overlap_order_key(),
                    ) {
                        draw_square_outline(
                            frame,
                            width,
                            height,
                            cx,
                            cy,
                            SELECTED_HIGHLIGHT_HALF_SIZE_PX,
                            SELECTED_XRAY_HIGHLIGHT_COLOR,
                        );
                    }
                }
            }
        }
    }

    if let Some(hovered_id) = visuals.hovered_interactable {
        if let Some(entity) = world.find_entity(hovered_id) {
            if entity.interactable.is_some() && entity.floor == active_floor {
                if bounds_intersects_point_radius(
                    view_bounds,
                    entity.transform.position,
                    ENTITY_CULL_RADIUS_WORLD_TILES,
                ) {
                    let (cx, cy) = snapped_world_to_screen_px(
                        world.camera(),
                        (width, height),
                        entity.transform.position,
                    );
                    draw_square_outline(
                        frame,
                        width,
                        height,
                        cx,
                        cy,
                        HOVER_HIGHLIGHT_HALF_SIZE_PX,
                        HOVER_HIGHLIGHT_COLOR,
                    );
                }
            }
        }
    }

    if let Some(targeted_id) = visuals.targeted_interactable {
        if let Some(entity) = world.find_entity(targeted_id) {
            if entity.interactable.is_some() && entity.floor == active_floor {
                if bounds_intersects_point_radius(
                    view_bounds,
                    entity.transform.position,
                    ENTITY_CULL_RADIUS_WORLD_TILES,
                ) {
                    let (cx, cy) = snapped_world_to_screen_px(
                        world.camera(),
                        (width, height),
                        entity.transform.position,
                    );
                    let subject_rect =
                        entity_screen_rect_placeholder(world.camera(), (width, height), entity);
                    if is_entity_occluded_by_front_overlap(
                        world,
                        (width, height),
                        active_floor,
                        entity.id,
                        subject_rect,
                        entity.renderer_overlap_order_key(),
                    ) {
                        draw_square_outline(
                            frame,
                            width,
                            height,
                            cx,
                            cy,
                            HOVER_HIGHLIGHT_HALF_SIZE_PX,
                            TARGET_XRAY_HIGHLIGHT_COLOR,
                        );
                    }
                }
            }
        }
    }

    for marker in world.debug_markers() {
        if matches!(marker.kind, DebugMarkerKind::Order) {
            if !bounds_intersects_point_radius(
                view_bounds,
                marker.position_world,
                ENTITY_CULL_RADIUS_WORLD_TILES,
            ) {
                continue;
            }
            let (cx, cy) =
                snapped_world_to_screen_px(world.camera(), (width, height), marker.position_world);
            draw_cross(
                frame,
                width,
                height,
                cx,
                cy,
                ORDER_MARKER_HALF_SIZE_PX,
                ORDER_MARKER_COLOR,
            );
        }
    }
}

fn screen_rect_from_center(cx: i32, cy: i32, half_size: i32) -> ScreenRectPx {
    ScreenRectPx {
        left: cx - half_size,
        right: cx + half_size,
        top: cy - half_size,
        bottom: cy + half_size,
    }
}

fn entity_screen_rect_placeholder(
    camera: &Camera2D,
    window_size: (u32, u32),
    entity: &Entity,
) -> ScreenRectPx {
    let (cx, cy) = snapped_world_to_screen_px(camera, window_size, entity.transform.position);
    screen_rect_from_center(cx, cy, PLACEHOLDER_HALF_SIZE_PX)
}

fn screen_rects_overlap(a: ScreenRectPx, b: ScreenRectPx) -> bool {
    !(a.right < b.left || b.right < a.left || a.bottom < b.top || b.bottom < a.top)
}

fn is_occluded_by_front_overlap(
    subject_rect: ScreenRectPx,
    subject_order_key: u64,
    occluder_rect: ScreenRectPx,
    occluder_order_key: u64,
) -> bool {
    occluder_order_key > subject_order_key && screen_rects_overlap(subject_rect, occluder_rect)
}

fn is_entity_occluded_by_front_overlap(
    world: &SceneWorld,
    window_size: (u32, u32),
    active_floor: FloorId,
    subject_id: crate::app::EntityId,
    subject_rect: ScreenRectPx,
    subject_order_key: u64,
) -> bool {
    world.entities().iter().any(|candidate| {
        if candidate.id == subject_id || candidate.floor != active_floor {
            return false;
        }
        let occluder_rect = entity_screen_rect_placeholder(world.camera(), window_size, candidate);
        is_occluded_by_front_overlap(
            subject_rect,
            subject_order_key,
            occluder_rect,
            candidate.renderer_overlap_order_key(),
        )
    })
}

fn draw_tilemap(
    frame: &mut [u8],
    width: u32,
    height: u32,
    world: &SceneWorld,
    view_bounds: &WorldBounds,
    sprite_cache: &mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
) {
    let Some(tilemap) = world.tilemap() else {
        return;
    };
    let Some(visible_rect) = visible_tile_rect(tilemap, view_bounds) else {
        return;
    };
    let pixels_per_world = camera_pixels_per_world(world.camera());

    for y in visible_rect.y_min..=visible_rect.y_max {
        for x in visible_rect.x_min..=visible_rect.x_max {
            let Some(tile_id) = tilemap.tile_at(x, y) else {
                continue;
            };
            let Some(center_world) = tilemap.tile_center_world(x, y) else {
                continue;
            };
            let (cx, cy) =
                snapped_world_to_screen_px(world.camera(), (width, height), center_world);
            if let Some(key) = tile_sprite_key(tile_id) {
                if let Some(sprite) =
                    resolve_cached_sprite(sprite_cache, warned_missing_sprite_keys, asset_root, key)
                {
                    draw_sprite_centered_scaled(
                        frame,
                        width,
                        height,
                        cx,
                        cy,
                        sprite,
                        world.camera().effective_zoom(),
                    );
                    continue;
                }
            }
            draw_tile_fallback(frame, width, height, cx, cy, tile_id, pixels_per_world);
        }
    }
}

fn view_bounds_world(camera: &Camera2D, window_size: (u32, u32), padding_px: f32) -> WorldBounds {
    let pixels_per_world = camera_pixels_per_world(camera);
    let safe_pixels_per_world = if pixels_per_world.is_finite() && pixels_per_world > f32::EPSILON {
        pixels_per_world
    } else {
        PIXELS_PER_WORLD
    };
    let half_w_world = window_size.0 as f32 / (2.0 * safe_pixels_per_world);
    let half_h_world = window_size.1 as f32 / (2.0 * safe_pixels_per_world);
    let padding_world = (padding_px.max(0.0) / safe_pixels_per_world).max(0.0);

    WorldBounds {
        min_x: camera.position.x - half_w_world - padding_world,
        max_x: camera.position.x + half_w_world + padding_world,
        min_y: camera.position.y - half_h_world - padding_world,
        max_y: camera.position.y + half_h_world + padding_world,
    }
}

fn bounds_intersects_point_radius(bounds: &WorldBounds, center: Vec2, radius_world: f32) -> bool {
    let radius = radius_world.max(0.0);
    let min_x = center.x - radius;
    let max_x = center.x + radius;
    let min_y = center.y - radius;
    let max_y = center.y + radius;

    !(max_x < bounds.min_x || min_x > bounds.max_x || max_y < bounds.min_y || min_y > bounds.max_y)
}

fn visible_tile_rect(tilemap: &Tilemap, bounds: &WorldBounds) -> Option<TileRectInclusive> {
    if tilemap.width() == 0 || tilemap.height() == 0 {
        return None;
    }

    let origin = tilemap.origin();
    let raw_x_min = (bounds.min_x - origin.x).floor() as i32;
    let raw_x_max = (bounds.max_x - origin.x).ceil() as i32 - 1;
    let raw_y_min = (bounds.min_y - origin.y).floor() as i32;
    let raw_y_max = (bounds.max_y - origin.y).ceil() as i32 - 1;

    let x_limit = tilemap.width() as i32 - 1;
    let y_limit = tilemap.height() as i32 - 1;

    let x_min = raw_x_min.max(0);
    let x_max = raw_x_max.min(x_limit);
    let y_min = raw_y_min.max(0);
    let y_max = raw_y_max.min(y_limit);

    if x_min > x_max || y_min > y_max {
        return None;
    }

    Some(TileRectInclusive {
        x_min: x_min as u32,
        x_max: x_max as u32,
        y_min: y_min as u32,
        y_max: y_max as u32,
    })
}

fn tile_sprite_key(tile_id: u16) -> Option<&'static str> {
    match tile_id {
        0 => Some("tile/grass"),
        1 => Some("tile/dirt"),
        _ => None,
    }
}

fn draw_tile_fallback(
    frame: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    tile_id: u16,
    pixels_per_world: f32,
) {
    let color = match tile_id {
        0 => TILE_FALLBACK_GRASS_COLOR,
        1 => TILE_FALLBACK_DIRT_COLOR,
        _ => TILE_FALLBACK_UNKNOWN_COLOR,
    };
    let half_size = (pixels_per_world / 2.0).round() as i32;
    draw_square(
        frame,
        width,
        height,
        center_x,
        center_y,
        half_size.max(1),
        color,
    );
}

fn resolve_cached_sprite<'a>(
    cache: &'a mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
    key: &str,
) -> Option<&'a LoadedSprite> {
    resolve_cached_sprite_with_missing_policy(
        cache,
        warned_missing_sprite_keys,
        asset_root,
        key,
        true,
    )
}

fn resolve_cached_sprite_with_missing_policy<'a>(
    cache: &'a mut HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: &mut HashSet<String>,
    asset_root: &Path,
    key: &str,
    warn_on_missing: bool,
) -> Option<&'a LoadedSprite> {
    if let Some(cached) = cache.get(key) {
        let sprite_ptr = cached.as_ref().map(|sprite| sprite as *const LoadedSprite);
        // SAFETY: `sprite_ptr` is derived from an immutable reference into `cache`.
        // We return immediately on this hit-path, so `cache` is not mutated before use.
        return sprite_ptr.map(|ptr| unsafe { &*ptr });
    }
    let sprite = match resolve_sprite_image_path(asset_root, key) {
        Ok(path) => match load_sprite_rgba(&path) {
            Ok(sprite) => Some(sprite),
            Err(reason) => {
                if warn_on_missing {
                    warn_sprite_load_once(
                        warned_missing_sprite_keys,
                        key,
                        Some(path.as_path()),
                        reason.as_str(),
                    );
                }
                None
            }
        },
        Err(reason) => {
            if warn_on_missing {
                warn_sprite_load_once(warned_missing_sprite_keys, key, None, reason.as_str());
            }
            None
        }
    };
    cache.insert(key.to_string(), sprite);
    cache.get(key).and_then(Option::as_ref)
}

fn resolve_sprite_image_path(asset_root: &Path, key: &str) -> Result<PathBuf, String> {
    validate_sprite_key(key).map_err(|error| format!("invalid_key:{error}"))?;
    Ok(asset_root
        .join("base")
        .join("sprites")
        .join(format!("{key}.png")))
}

fn load_sprite_rgba(path: &Path) -> Result<LoadedSprite, String> {
    let reader = ImageReader::open(path).map_err(|error| format!("file_open_failed:{error}"))?;
    let decoded = reader
        .decode()
        .map_err(|error| format!("decode_failed:{error}"))?;
    let image = decoded.to_rgba8();
    Ok(LoadedSprite {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn warn_sprite_load_once(
    warned_keys: &mut HashSet<String>,
    key: &str,
    resolved_path: Option<&Path>,
    reason: &str,
) {
    if !warned_keys.insert(key.to_string()) {
        return;
    }
    let path_display = resolved_path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    warn!(
        sprite_key = key,
        path = %path_display,
        reason = reason,
        "renderer_sprite_load_failed_using_placeholder"
    );
}

fn draw_world_grid(frame: &mut [u8], width: u32, height: u32, world: &SceneWorld) {
    if width == 0 || height == 0 {
        return;
    }

    let pixels_per_world = camera_pixels_per_world(world.camera());
    let (ix_start, ix_end, iy_start, iy_end) =
        visible_grid_index_bounds(world.camera().position, width, height, pixels_per_world);

    for ix in ix_start..=ix_end {
        let world_x = ix as f32 * GRID_CELL_WORLD;
        let (screen_x, _) = snapped_world_to_screen_px(
            world.camera(),
            (width, height),
            Vec2 {
                x: world_x,
                y: world.camera().position.y,
            },
        );
        let color = if is_major_index(ix) {
            GRID_MAJOR_COLOR
        } else {
            GRID_MINOR_COLOR
        };
        draw_vertical_line_clipped(frame, width, height, screen_x, color);
    }

    for iy in iy_start..=iy_end {
        let world_y = iy as f32 * GRID_CELL_WORLD;
        let (_, screen_y) = snapped_world_to_screen_px(
            world.camera(),
            (width, height),
            Vec2 {
                x: world.camera().position.x,
                y: world_y,
            },
        );
        let color = if is_major_index(iy) {
            GRID_MAJOR_COLOR
        } else {
            GRID_MINOR_COLOR
        };
        draw_horizontal_line_clipped(frame, width, height, screen_y, color);
    }
}

fn visible_grid_index_bounds(
    camera_pos: Vec2,
    width: u32,
    height: u32,
    pixels_per_world: f32,
) -> (i32, i32, i32, i32) {
    let half_w_world = width as f32 / (2.0 * pixels_per_world);
    let half_h_world = height as f32 / (2.0 * pixels_per_world);
    let min_x = camera_pos.x - half_w_world;
    let max_x = camera_pos.x + half_w_world;
    let min_y = camera_pos.y - half_h_world;
    let max_y = camera_pos.y + half_h_world;

    let ix_start = (min_x / GRID_CELL_WORLD).floor() as i32 - 1;
    let ix_end = (max_x / GRID_CELL_WORLD).ceil() as i32 + 1;
    let iy_start = (min_y / GRID_CELL_WORLD).floor() as i32 - 1;
    let iy_end = (max_y / GRID_CELL_WORLD).ceil() as i32 + 1;
    (ix_start, ix_end, iy_start, iy_end)
}

fn is_major_index(idx: i32) -> bool {
    idx.rem_euclid(GRID_MAJOR_EVERY) == 0
}

fn draw_vertical_line_clipped(frame: &mut [u8], width: u32, height: u32, x: i32, color: [u8; 4]) {
    if width == 0 || height == 0 || x < 0 || x >= width as i32 {
        return;
    }
    for y in 0..height as i32 {
        write_pixel_rgba_clipped(frame, width as usize, x, y, color);
    }
}

fn draw_horizontal_line_clipped(frame: &mut [u8], width: u32, height: u32, y: i32, color: [u8; 4]) {
    if width == 0 || height == 0 || y < 0 || y >= height as i32 {
        return;
    }
    for x in 0..width as i32 {
        write_pixel_rgba_clipped(frame, width as usize, x, y, color);
    }
}

fn write_pixel_rgba_clipped(frame: &mut [u8], width: usize, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    let Some(pixel_offset) = y.checked_mul(width).and_then(|row| row.checked_add(x)) else {
        return;
    };
    let Some(byte_offset) = pixel_offset.checked_mul(4) else {
        return;
    };
    let Some(end) = byte_offset.checked_add(4) else {
        return;
    };
    if end > frame.len() {
        return;
    }
    frame[byte_offset..end].copy_from_slice(&color);
}

fn draw_square(
    frame: &mut [u8],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    half_size: i32,
    color: [u8; 4],
) {
    for y in (cy - half_size)..=(cy + half_size) {
        for x in (cx - half_size)..=(cx + half_size) {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            write_pixel_rgba_clipped(frame, width as usize, x, y, color);
        }
    }
}

fn draw_square_outline(
    frame: &mut [u8],
    width: u32,
    _height: u32,
    cx: i32,
    cy: i32,
    half_size: i32,
    color: [u8; 4],
) {
    let left = cx - half_size;
    let right = cx + half_size;
    let top = cy - half_size;
    let bottom = cy + half_size;

    for x in left..=right {
        write_pixel_rgba_clipped(frame, width as usize, x, top, color);
        write_pixel_rgba_clipped(frame, width as usize, x, bottom, color);
    }
    for y in top..=bottom {
        write_pixel_rgba_clipped(frame, width as usize, left, y, color);
        write_pixel_rgba_clipped(frame, width as usize, right, y, color);
    }
}

fn draw_cross(
    frame: &mut [u8],
    width: u32,
    _height: u32,
    cx: i32,
    cy: i32,
    half_size: i32,
    color: [u8; 4],
) {
    for x in (cx - half_size)..=(cx + half_size) {
        write_pixel_rgba_clipped(frame, width as usize, x, cy, color);
    }
    for y in (cy - half_size)..=(cy + half_size) {
        write_pixel_rgba_clipped(frame, width as usize, cx, y, color);
    }
}

fn normalized_sprite_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn scaled_sprite_dimensions(sprite: &LoadedSprite, scale: f32) -> (u32, u32) {
    let scale = normalized_sprite_scale(scale);
    let width = (sprite.width as f32 * scale).round().max(1.0) as u32;
    let height = (sprite.height as f32 * scale).round().max(1.0) as u32;
    (width, height)
}

fn draw_sprite_centered_scaled(
    frame: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    sprite: &LoadedSprite,
    scale: f32,
) {
    if sprite.width == 0 || sprite.height == 0 || width == 0 || height == 0 {
        return;
    }
    let expected_rgba_len = sprite.width as usize * sprite.height as usize * 4;
    if sprite.rgba.len() < expected_rgba_len {
        return;
    }

    let scale = normalized_sprite_scale(scale);
    let inv_scale = scale.recip();
    let (scaled_w, scaled_h) = scaled_sprite_dimensions(sprite, scale);
    let left = center_x - (scaled_w as i32 / 2);
    let top = center_y - (scaled_h as i32 / 2);
    let right = left + scaled_w as i32;
    let bottom = top + scaled_h as i32;

    let draw_left = left.max(0);
    let draw_top = top.max(0);
    let draw_right = right.min(width as i32);
    let draw_bottom = bottom.min(height as i32);
    if draw_left >= draw_right || draw_top >= draw_bottom {
        return;
    }

    let frame_width = width as usize;
    let sprite_width = sprite.width as usize;

    for out_y in draw_top..draw_bottom {
        let dy = out_y - top;
        let src_y = ((dy as f32) * inv_scale).floor() as u32;
        let src_y = src_y.min(sprite.height - 1) as usize;
        let src_row_offset = src_y * sprite_width * 4;
        let dst_row_offset = out_y as usize * frame_width * 4;

        for out_x in draw_left..draw_right {
            let dx = out_x - left;
            let src_x = ((dx as f32) * inv_scale).floor() as u32;
            let src_x = src_x.min(sprite.width - 1) as usize;
            let src_offset = src_row_offset + src_x * 4;
            let alpha = sprite.rgba[src_offset + 3];
            if alpha == 0 {
                continue;
            }
            let dst_offset = dst_row_offset + out_x as usize * 4;
            frame[dst_offset] = sprite.rgba[src_offset];
            frame[dst_offset + 1] = sprite.rgba[src_offset + 1];
            frame[dst_offset + 2] = sprite.rgba[src_offset + 2];
            frame[dst_offset + 3] = alpha;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        ActionParams, Camera2D, CardinalFacing, DebugMarker, DebugMarkerKind, EntityId, FloorId,
        SpriteAnchorName, SpriteAnchorPx, SpriteAnchors, Tilemap,
    };
    use crate::content::{DefDatabase, EntityArchetype, EntityDefId};
    use tempfile::TempDir;

    fn collect_visible_entity_ids_for_active_floor(
        world: &SceneWorld,
        view_bounds: &WorldBounds,
    ) -> Vec<EntityId> {
        let active_floor = world.active_floor();
        let mut indices = Vec::new();
        collect_sorted_visible_entity_draw_indices(world, active_floor, view_bounds, &mut indices);
        indices
            .into_iter()
            .map(|index| world.entities()[index].id)
            .collect()
    }

    #[test]
    fn renderer_type_is_non_generic() {
        let _renderer: Option<Renderer> = None;
    }

    #[test]
    fn clear_color_varies_by_active_floor() {
        assert_eq!(clear_color_for_floor(FloorId::Main), CLEAR_COLOR_MAIN);
        assert_eq!(clear_color_for_floor(FloorId::Rooftop), CLEAR_COLOR_ROOFTOP);
        assert_eq!(
            clear_color_for_floor(FloorId::Basement),
            CLEAR_COLOR_BASEMENT
        );
        assert_ne!(CLEAR_COLOR_MAIN, CLEAR_COLOR_ROOFTOP);
        assert_ne!(CLEAR_COLOR_MAIN, CLEAR_COLOR_BASEMENT);
    }

    #[test]
    fn render_list_selection_filters_to_active_floor_deterministically() {
        let mut world = SceneWorld::default();

        world.set_active_floor(FloorId::Main);
        let main_visible = world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "main_visible",
            },
        );

        world.set_active_floor(FloorId::Basement);
        let basement_visible = world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "basement_visible",
            },
        );

        world.set_active_floor(FloorId::Main);
        world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 200.0, y: 200.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "main_out_of_view",
            },
        );
        world.apply_pending();

        let bounds = WorldBounds {
            min_x: -1.0,
            max_x: 1.0,
            min_y: -1.0,
            max_y: 1.0,
        };

        let first_main = collect_visible_entity_ids_for_active_floor(&world, &bounds);
        let second_main = collect_visible_entity_ids_for_active_floor(&world, &bounds);
        assert_eq!(first_main, vec![main_visible]);
        assert_eq!(second_main, first_main);

        world.set_active_floor(FloorId::Basement);
        assert_eq!(
            collect_visible_entity_ids_for_active_floor(&world, &bounds),
            vec![basement_visible]
        );
    }

    #[test]
    fn sorted_draw_list_helper_uses_overlap_order_not_storage_order() {
        let mut world = SceneWorld::default();
        world.set_active_floor(FloorId::Main);
        let first = world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "first",
            },
        );
        let second = world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "second",
            },
        );
        let third = world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "third",
            },
        );
        world.apply_pending();
        world.entities_mut().reverse();

        let bounds = WorldBounds {
            min_x: -1.0,
            max_x: 1.0,
            min_y: -1.0,
            max_y: 1.0,
        };
        let ids = collect_visible_entity_ids_for_active_floor(&world, &bounds);
        assert_eq!(ids, vec![first, second, third]);
    }

    #[test]
    fn sorted_draw_list_helper_is_repeatable() {
        let mut world = SceneWorld::default();
        world.set_active_floor(FloorId::Main);
        world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "a",
            },
        );
        world.spawn(
            crate::app::Transform {
                position: Vec2 { x: 0.0, y: 0.0 },
                rotation_radians: None,
            },
            crate::app::RenderableDesc {
                kind: crate::app::RenderableKind::Placeholder,
                debug_name: "b",
            },
        );
        world.apply_pending();

        let bounds = WorldBounds {
            min_x: -1.0,
            max_x: 1.0,
            min_y: -1.0,
            max_y: 1.0,
        };
        let first = collect_visible_entity_ids_for_active_floor(&world, &bounds);
        let second = collect_visible_entity_ids_for_active_floor(&world, &bounds);
        assert_eq!(first, second);
    }

    #[test]
    fn snap_screen_coordinate_handles_positive_values() {
        assert_eq!(snap_screen_coordinate_px(5, 4), 4);
        assert_eq!(snap_screen_coordinate_px(7, 4), 8);
    }

    #[test]
    fn snap_screen_coordinate_handles_negative_values() {
        assert_eq!(snap_screen_coordinate_px(-5, 4), -4);
        assert_eq!(snap_screen_coordinate_px(-7, 4), -8);
    }

    #[test]
    fn snap_screen_coordinate_half_step_ties_break_away_from_zero() {
        assert_eq!(snap_screen_coordinate_px(2, 4), 4);
        assert_eq!(snap_screen_coordinate_px(-2, 4), -4);
    }

    #[test]
    fn snap_screen_coordinate_is_deterministic_across_calls() {
        let expected = snap_screen_coordinate_px(-37, 6);
        for _ in 0..128 {
            assert_eq!(snap_screen_coordinate_px(-37, 6), expected);
        }
    }

    #[test]
    fn visual_test_variant_candidates_include_state_and_facing_then_state() {
        let visual = EntityActionVisual {
            action_state: ActionState::Walk,
            action_params: ActionParams {
                facing: Some(CardinalFacing::West),
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let (state_facing, state_only) =
            visual_test_variant_candidate_keys("visual_test/pawn_blue", &visual)
                .expect("variant keys");
        assert_eq!(state_facing, "visual_test/pawn_blue__walk_west");
        assert_eq!(state_only, "visual_test/pawn_blue__walk");
    }

    #[test]
    fn non_visual_test_keys_do_not_use_variant_candidates() {
        let visual = EntityActionVisual {
            action_state: ActionState::Interact,
            action_params: ActionParams {
                facing: Some(CardinalFacing::North),
                ..ActionParams::default()
            },
            held_visual: None,
        };
        assert!(visual_test_variant_candidate_keys("npc/chaser_red", &visual).is_none());
    }

    #[test]
    fn held_attachment_anchor_routes_carry_and_use_tool() {
        assert_eq!(
            held_attachment_anchor_name(ActionState::Carry),
            Some(SpriteAnchorName::Carry)
        );
        assert_eq!(
            held_attachment_anchor_name(ActionState::UseTool),
            Some(SpriteAnchorName::Tool)
        );
        assert_eq!(held_attachment_anchor_name(ActionState::Idle), None);
    }

    fn approx_eq_f32(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.0001
    }

    fn approx_eq_vec2(a: Vec2, b: Vec2) -> bool {
        approx_eq_f32(a.x, b.x) && approx_eq_f32(a.y, b.y)
    }

    #[test]
    fn procedural_offset_is_deterministic_for_same_tick_and_input() {
        let mut spring = HashMap::new();
        let visual = EntityActionVisual {
            action_state: ActionState::Walk,
            action_params: ActionParams {
                phase: 0.2,
                speed01: 0.6,
                intensity: 0.6,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let first = compute_procedural_offset(&mut spring, EntityId(11), &visual, 100);
        let second = compute_procedural_offset(&mut spring, EntityId(11), &visual, 100);
        assert!(approx_eq_vec2(first.offset_px, second.offset_px));
        assert!(approx_eq_f32(
            first.rotation_radians,
            second.rotation_radians
        ));
    }

    #[test]
    fn idle_and_walk_offsets_differ_and_walk_scales_with_speed01() {
        let mut spring = HashMap::new();
        let idle = EntityActionVisual {
            action_state: ActionState::Idle,
            action_params: ActionParams::default(),
            held_visual: None,
        };
        let walk_slow = EntityActionVisual {
            action_state: ActionState::Walk,
            action_params: ActionParams {
                speed01: 0.2,
                phase: 0.15,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let walk_fast = EntityActionVisual {
            action_state: ActionState::Walk,
            action_params: ActionParams {
                speed01: 1.0,
                phase: 0.15,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let idle_offset = compute_procedural_offset(&mut spring, EntityId(1), &idle, 23);
        let walk_slow_offset = compute_procedural_offset(&mut spring, EntityId(2), &walk_slow, 23);
        let walk_fast_offset = compute_procedural_offset(&mut spring, EntityId(3), &walk_fast, 23);

        assert!(!approx_eq_vec2(
            idle_offset.offset_px,
            walk_slow_offset.offset_px
        ));
        let slow_mag_sq = walk_slow_offset.offset_px.x * walk_slow_offset.offset_px.x
            + walk_slow_offset.offset_px.y * walk_slow_offset.offset_px.y;
        let fast_mag_sq = walk_fast_offset.offset_px.x * walk_fast_offset.offset_px.x
            + walk_fast_offset.offset_px.y * walk_fast_offset.offset_px.y;
        assert!(fast_mag_sq > slow_mag_sq);
    }

    #[test]
    fn hit_impulse_curve_is_bounded_and_decays() {
        let mut spring = HashMap::new();
        let early = EntityActionVisual {
            action_state: ActionState::Hit,
            action_params: ActionParams {
                phase: 0.2,
                intensity: 1.0,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let late = EntityActionVisual {
            action_state: ActionState::Hit,
            action_params: ActionParams {
                phase: 0.8,
                intensity: 1.0,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let early_offset = compute_procedural_offset(&mut spring, EntityId(9), &early, 80);
        let late_offset = compute_procedural_offset(&mut spring, EntityId(9), &late, 81);

        assert!(early_offset.offset_px.x >= 0.0);
        assert!(early_offset.offset_px.x <= HIT_KICK_MAX_PX + 0.001);
        assert!(late_offset.offset_px.x <= early_offset.offset_px.x);
    }

    #[test]
    fn final_position_with_procedural_offset_still_respects_microgrid_snap() {
        let camera = Camera2D::default();
        let world_pos = Vec2 { x: 0.33, y: -0.42 };
        let visual_offset = Vec2 { x: 0.7, y: -1.2 };
        let (x, y) =
            world_to_snapped_screen_px_with_offset(&camera, (1280, 720), world_pos, visual_offset);

        let (base_x, base_y) = world_to_screen_px(&camera, (1280, 720), world_pos);
        let expected_x = snap_screen_coordinate_px(
            base_x + visual_offset.x.round() as i32,
            MICRO_GRID_RESOLUTION_PX,
        );
        let expected_y = snap_screen_coordinate_px(
            base_y + visual_offset.y.round() as i32,
            MICRO_GRID_RESOLUTION_PX,
        );
        assert_eq!(x, expected_x);
        assert_eq!(y, expected_y);
    }

    #[test]
    fn offset_changes_only_when_tick_counter_changes() {
        let mut spring = HashMap::new();
        let visual = EntityActionVisual {
            action_state: ActionState::Walk,
            action_params: ActionParams {
                phase: 0.4,
                speed01: 0.8,
                ..ActionParams::default()
            },
            held_visual: None,
        };
        let first = compute_procedural_offset(&mut spring, EntityId(15), &visual, 200);
        let second = compute_procedural_offset(&mut spring, EntityId(15), &visual, 200);
        let third = compute_procedural_offset(&mut spring, EntityId(15), &visual, 201);

        assert!(approx_eq_vec2(first.offset_px, second.offset_px));
        assert!(approx_eq_f32(
            first.rotation_radians,
            second.rotation_radians
        ));
        assert!(!approx_eq_vec2(second.offset_px, third.offset_px));
    }

    #[test]
    fn scaled_sprite_dimensions_multiplies_native_size() {
        let sprite = LoadedSprite {
            width: 4,
            height: 6,
            rgba: vec![255; 4 * 6 * 4],
        };
        assert_eq!(scaled_sprite_dimensions(&sprite, 1.0), (4, 6));
        assert_eq!(scaled_sprite_dimensions(&sprite, 2.0), (8, 12));
    }

    #[test]
    fn sprite_anchor_lookup_falls_back_to_origin_when_missing() {
        let anchors = SpriteAnchors::default();
        assert_eq!(
            sprite_anchor_or_origin(anchors, SpriteAnchorName::Carry),
            SpriteAnchorPx::default()
        );
    }

    #[test]
    fn west_facing_anchor_transform_mirrors_x_only() {
        let anchor = SpriteAnchorPx { x_px: 7, y_px: -3 };
        let transformed = transform_anchor_for_facing(anchor, Some(CardinalFacing::West));
        assert_eq!(transformed.x_px, -7);
        assert_eq!(transformed.y_px, -3);
    }

    #[test]
    fn north_and_south_anchor_transform_do_not_modify_anchor() {
        let anchor = SpriteAnchorPx { x_px: 4, y_px: 2 };
        assert_eq!(
            transform_anchor_for_facing(anchor, Some(CardinalFacing::North)),
            anchor
        );
        assert_eq!(
            transform_anchor_for_facing(anchor, Some(CardinalFacing::South)),
            anchor
        );
    }

    #[test]
    fn carry_sprite_resolution_uses_cache_for_repeated_defname() {
        let mut cache = HashMap::<String, Option<CachedCarrySprite>>::new();
        let def_db = DefDatabase::from_entity_defs(vec![
            EntityArchetype {
                id: EntityDefId(0),
                def_name: "proto.visual_carry_item".to_string(),
                label: "Carry".to_string(),
                renderable: RenderableKind::Sprite {
                    key: "visual_test/carry_item".to_string(),
                    pixel_scale: 3,
                    anchors: SpriteAnchors::default(),
                },
                move_speed: 5.0,
                health_max: None,
                base_damage: None,
                aggro_radius: None,
                attack_range: None,
                attack_cooldown_seconds: None,
                tags: Vec::new(),
            },
            EntityArchetype {
                id: EntityDefId(0),
                def_name: "proto.not_sprite".to_string(),
                label: "NotSprite".to_string(),
                renderable: RenderableKind::Placeholder,
                move_speed: 5.0,
                health_max: None,
                base_damage: None,
                aggro_radius: None,
                attack_range: None,
                attack_cooldown_seconds: None,
                tags: Vec::new(),
            },
        ]);

        let first =
            resolve_cached_carry_sprite(&mut cache, Some(&def_db), "proto.visual_carry_item")
                .cloned();
        assert!(first.is_some());
        assert_eq!(cache.len(), 1);

        let second =
            resolve_cached_carry_sprite(&mut cache, Some(&def_db), "proto.visual_carry_item")
                .cloned();
        assert_eq!(second, first);
        assert_eq!(cache.len(), 1);

        let miss_first = resolve_cached_carry_sprite(&mut cache, Some(&def_db), "proto.not_sprite");
        assert!(miss_first.is_none());
        assert_eq!(cache.len(), 2);

        let miss_second =
            resolve_cached_carry_sprite(&mut cache, Some(&def_db), "proto.not_sprite");
        assert!(miss_second.is_none());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn occlusion_predicate_respects_order_and_overlap() {
        let subject = ScreenRectPx {
            left: 10,
            right: 20,
            top: 10,
            bottom: 20,
        };
        let overlapping = ScreenRectPx {
            left: 15,
            right: 25,
            top: 15,
            bottom: 25,
        };
        let separate = ScreenRectPx {
            left: 100,
            right: 110,
            top: 100,
            bottom: 110,
        };

        assert!(is_occluded_by_front_overlap(subject, 1, overlapping, 2));
        assert!(!is_occluded_by_front_overlap(subject, 2, overlapping, 1));
        assert!(!is_occluded_by_front_overlap(subject, 1, separate, 2));
        assert!(!is_occluded_by_front_overlap(subject, 2, overlapping, 2));
    }

    #[test]
    fn occlusion_predicate_is_deterministic_across_calls() {
        let subject = ScreenRectPx {
            left: -20,
            right: -10,
            top: -20,
            bottom: -10,
        };
        let occluder = ScreenRectPx {
            left: -18,
            right: -8,
            top: -18,
            bottom: -8,
        };
        let expected = is_occluded_by_front_overlap(subject, 5, occluder, 9);
        for _ in 0..128 {
            assert_eq!(
                is_occluded_by_front_overlap(subject, 5, occluder, 9),
                expected
            );
        }
    }

    #[test]
    fn major_index_classification_handles_negative_indices() {
        for idx in -10..=10 {
            let expected = idx % 5 == 0;
            assert_eq!(is_major_index(idx), expected, "idx={idx}");
        }
    }

    #[test]
    fn grid_bounds_iteration_is_finite_and_viewport_scoped() {
        let camera_pos = Vec2 { x: 3.25, y: -1.75 };
        let (ix_start, ix_end, iy_start, iy_end) =
            visible_grid_index_bounds(camera_pos, 1280, 720, crate::app::PIXELS_PER_WORLD);
        assert!(ix_start < ix_end);
        assert!(iy_start < iy_end);
        let x_count = ix_end - ix_start + 1;
        let y_count = iy_end - iy_start + 1;
        assert!(x_count > 0 && x_count < 1280);
        assert!(y_count > 0 && y_count < 720);
    }

    #[test]
    fn grid_draw_is_safe_for_tiny_or_zero_viewports() {
        let mut zero = vec![];
        draw_vertical_line_clipped(&mut zero, 0, 0, 0, GRID_MINOR_COLOR);
        draw_horizontal_line_clipped(&mut zero, 0, 0, 0, GRID_MINOR_COLOR);
        write_pixel_rgba_clipped(&mut zero, 0, 0, 0, GRID_MINOR_COLOR);

        let mut tiny = vec![0u8; 4];
        draw_vertical_line_clipped(&mut tiny, 1, 1, -1, GRID_MINOR_COLOR);
        draw_vertical_line_clipped(&mut tiny, 1, 1, 0, GRID_MINOR_COLOR);
        draw_horizontal_line_clipped(&mut tiny, 1, 1, 0, GRID_MINOR_COLOR);
        write_pixel_rgba_clipped(&mut tiny, 1, 99, 99, GRID_MINOR_COLOR);
        assert_eq!(tiny.len(), 4);
    }

    #[test]
    fn view_bounds_world_centered_camera_expands_with_padding() {
        let camera = Camera2D::default();
        let bounds = view_bounds_world(&camera, (64, 64), 16.0);
        let expected_half_world = 64.0 / (2.0 * crate::app::PIXELS_PER_WORLD);
        let expected_padding_world = 16.0 / crate::app::PIXELS_PER_WORLD;
        let expected_extent = expected_half_world + expected_padding_world;

        assert!((bounds.min_x + expected_extent).abs() < 0.0001);
        assert!((bounds.max_x - expected_extent).abs() < 0.0001);
        assert!((bounds.min_y + expected_extent).abs() < 0.0001);
        assert!((bounds.max_y - expected_extent).abs() < 0.0001);
    }

    #[test]
    fn view_bounds_world_handles_negative_camera_and_zoom() {
        let camera = Camera2D {
            position: Vec2 { x: -10.0, y: -5.0 },
            zoom: 2.0,
        };
        let bounds = view_bounds_world(&camera, (128, 64), VIEW_CULL_PADDING_PX);
        assert!(bounds.min_x < -10.0);
        assert!(bounds.max_x > -10.0);
        assert!(bounds.min_y < -5.0);
        assert!(bounds.max_y > -5.0);
    }

    #[test]
    fn view_bounds_world_tiny_viewport_is_finite_and_safe() {
        let camera = Camera2D::default();
        let bounds = view_bounds_world(&camera, (1, 1), VIEW_CULL_PADDING_PX);
        assert!(bounds.min_x.is_finite());
        assert!(bounds.max_x.is_finite());
        assert!(bounds.min_y.is_finite());
        assert!(bounds.max_y.is_finite());
        assert!(bounds.min_x <= bounds.max_x);
        assert!(bounds.min_y <= bounds.max_y);
    }

    #[test]
    fn point_radius_visibility_handles_negative_coords() {
        let bounds = WorldBounds {
            min_x: -2.0,
            max_x: 2.0,
            min_y: -2.0,
            max_y: 2.0,
        };

        assert!(bounds_intersects_point_radius(
            &bounds,
            Vec2 { x: -2.1, y: 0.0 },
            0.2
        ));
        assert!(!bounds_intersects_point_radius(
            &bounds,
            Vec2 { x: -2.3, y: 0.0 },
            0.2
        ));
    }

    #[test]
    fn visible_tile_rect_matches_floor_ceil_minus_one_formula() {
        let tilemap =
            Tilemap::new(10, 10, Vec2 { x: 3.0, y: -2.0 }, vec![0; 100]).expect("tilemap");
        let bounds = WorldBounds {
            min_x: 4.2,
            max_x: 6.1,
            min_y: -0.8,
            max_y: 1.01,
        };
        let rect = visible_tile_rect(&tilemap, &bounds).expect("rect");
        assert_eq!(
            rect,
            TileRectInclusive {
                x_min: 1,
                x_max: 3,
                y_min: 1,
                y_max: 3,
            }
        );
    }

    #[test]
    fn visible_tile_rect_handles_negative_origin_and_coords() {
        let tilemap = Tilemap::new(8, 6, Vec2 { x: -5.0, y: -4.0 }, vec![0; 48]).expect("tilemap");
        let bounds = WorldBounds {
            min_x: -4.8,
            max_x: -2.1,
            min_y: -3.6,
            max_y: -1.2,
        };
        let rect = visible_tile_rect(&tilemap, &bounds).expect("rect");
        assert_eq!(
            rect,
            TileRectInclusive {
                x_min: 0,
                x_max: 2,
                y_min: 0,
                y_max: 2,
            }
        );
    }

    #[test]
    fn visible_tile_rect_clamps_inclusive_to_map_bounds() {
        let tilemap = Tilemap::new(4, 3, Vec2 { x: 0.0, y: 0.0 }, vec![0; 12]).expect("tilemap");
        let bounds = WorldBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
        };
        let rect = visible_tile_rect(&tilemap, &bounds).expect("rect");
        assert_eq!(
            rect,
            TileRectInclusive {
                x_min: 0,
                x_max: 3,
                y_min: 0,
                y_max: 2,
            }
        );
    }

    #[test]
    fn visible_tile_rect_returns_none_when_fully_outside() {
        let tilemap = Tilemap::new(4, 4, Vec2 { x: 0.0, y: 0.0 }, vec![0; 16]).expect("tilemap");
        let bounds = WorldBounds {
            min_x: 10.0,
            max_x: 12.0,
            min_y: 10.0,
            max_y: 12.0,
        };
        assert!(visible_tile_rect(&tilemap, &bounds).is_none());
    }

    #[test]
    fn visible_tile_rect_tiny_viewport_case_is_safe() {
        let tilemap = Tilemap::new(2, 2, Vec2 { x: -1.0, y: -1.0 }, vec![0; 4]).expect("tilemap");
        let camera = Camera2D {
            position: Vec2 { x: -0.5, y: -0.5 },
            ..Camera2D::default()
        };
        let bounds = view_bounds_world(&camera, (1, 1), VIEW_CULL_PADDING_PX);
        let rect = visible_tile_rect(&tilemap, &bounds).expect("rect");
        assert!(rect.x_min <= rect.x_max);
        assert!(rect.y_min <= rect.y_max);
    }

    #[test]
    fn camera_pan_shifts_projected_grid_lines_consistently() {
        let camera_a = Camera2D {
            position: Vec2 { x: 0.0, y: 0.0 },
            ..Camera2D::default()
        };
        let camera_b = Camera2D {
            position: Vec2 { x: 1.0, y: 0.0 },
            ..Camera2D::default()
        };
        let world_line_x = 0.0;
        let (xa, _) = world_to_screen_px(
            &camera_a,
            (1280, 720),
            Vec2 {
                x: world_line_x,
                y: 0.0,
            },
        );
        let (xb, _) = world_to_screen_px(
            &camera_b,
            (1280, 720),
            Vec2 {
                x: world_line_x,
                y: 0.0,
            },
        );
        assert_eq!(xa - xb, crate::app::PIXELS_PER_WORLD.round() as i32);
    }

    #[test]
    fn sprite_path_resolution_and_missing_asset_fallback_behavior() {
        let temp = TempDir::new().expect("temp");
        let asset_root = temp.path();

        assert!(resolve_sprite_image_path(asset_root, r"bad\key").is_err());

        let valid_path = resolve_sprite_image_path(asset_root, "player").expect("path");
        assert_eq!(
            valid_path,
            asset_root.join("base").join("sprites").join("player.png")
        );
        assert!(load_sprite_rgba(&valid_path).is_err());
    }

    #[test]
    fn tile_id_mapping_known_and_unknown() {
        assert_eq!(tile_sprite_key(0), Some("tile/grass"));
        assert_eq!(tile_sprite_key(1), Some("tile/dirt"));
        assert_eq!(tile_sprite_key(999), None);
    }

    #[test]
    fn tile_center_projection_pans_with_camera() {
        let tilemap = Tilemap::new(4, 4, Vec2 { x: 0.0, y: 0.0 }, vec![0; 16]).expect("tilemap");
        let center = tilemap.tile_center_world(1, 2).expect("center");
        let camera_a = Camera2D {
            position: Vec2 { x: 0.0, y: 0.0 },
            ..Camera2D::default()
        };
        let camera_b = Camera2D {
            position: Vec2 { x: 1.0, y: 1.0 },
            ..Camera2D::default()
        };

        let (xa, ya) = world_to_screen_px(&camera_a, (1280, 720), center);
        let (xb, yb) = world_to_screen_px(&camera_b, (1280, 720), center);
        assert_eq!(xa - xb, crate::app::PIXELS_PER_WORLD.round() as i32);
        assert_eq!(yb - ya, crate::app::PIXELS_PER_WORLD.round() as i32);
    }

    #[test]
    fn tilemap_draw_handles_missing_sprites_with_fallback() {
        let temp = TempDir::new().expect("temp");
        let asset_root = temp.path();

        let grass_path = resolve_sprite_image_path(asset_root, tile_sprite_key(0).expect("key"))
            .expect("sprite path");
        assert!(load_sprite_rgba(&grass_path).is_err());
    }

    #[test]
    fn affordances_skip_stale_visual_ids_without_panic() {
        let mut world = SceneWorld::default();
        world.set_selected_actor_visual(Some(EntityId(999)));
        world.set_hovered_interactable_visual(Some(EntityId(1000)));
        world.set_targeted_interactable_visual(Some(EntityId(1001)));

        let mut frame = vec![0u8; 64 * 64 * 4];
        let bounds = view_bounds_world(world.camera(), (64, 64), VIEW_CULL_PADDING_PX);
        draw_affordances(&mut frame, 64, 64, &world, &bounds);
        assert!(frame.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn affordances_draw_order_marker_primitive() {
        let mut world = SceneWorld::default();
        world.push_debug_marker(DebugMarker {
            kind: DebugMarkerKind::Order,
            position_world: Vec2 { x: 0.0, y: 0.0 },
            ttl_seconds: 0.75,
        });

        let mut frame = vec![0u8; 64 * 64 * 4];
        let bounds = view_bounds_world(world.camera(), (64, 64), VIEW_CULL_PADDING_PX);
        draw_affordances(&mut frame, 64, 64, &world, &bounds);
        assert!(frame.iter().any(|byte| *byte != 0));
    }
}
