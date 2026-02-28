use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::ImageReader;
use pixels::{Error, Pixels, SurfaceTexture};
use tracing::warn;
use winit::window::Window;

use crate::app::{
    tools::{draw_command_palette, draw_console, draw_overlay},
    Camera2D, CommandPaletteRenderData, ConsoleState, DebugMarkerKind, Entity, FloorId,
    OverlayData, RenderableKind, SceneWorld, Tilemap, Vec2,
};
use crate::sprite_keys::validate_sprite_key;

use super::transform::camera_pixels_per_world;
use super::{world_to_screen_px, Viewport, PIXELS_PER_WORLD, PLACEHOLDER_HALF_SIZE_PX};

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

pub struct Renderer {
    window: Arc<Window>,
    pixels: Pixels<'static>,
    viewport: Viewport,
    asset_root: PathBuf,
    sprite_cache: HashMap<String, Option<LoadedSprite>>,
    warned_missing_sprite_keys: HashSet<String>,
    visible_entity_draw_indices: Vec<usize>,
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
        let frame = self.pixels.frame_mut();
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

        for entity_index in visible_entity_draw_indices.iter().copied() {
            let entity = &world.entities()[entity_index];
            let (cx, cy) = snapped_world_to_screen_px(
                world.camera(),
                (self.viewport.width, self.viewport.height),
                entity.transform.position,
            );
            match &entity.renderable.kind {
                RenderableKind::Placeholder => {
                    draw_square(
                        frame,
                        self.viewport.width,
                        self.viewport.height,
                        cx,
                        cy,
                        PLACEHOLDER_HALF_SIZE_PX,
                        PLACEHOLDER_COLOR,
                    );
                }
                RenderableKind::Sprite { key, pixel_scale } => {
                    if let Some(sprite) = resolve_cached_sprite(
                        sprite_cache,
                        warned_missing_sprite_keys,
                        asset_root,
                        key,
                    ) {
                        draw_sprite_centered_scaled(
                            frame,
                            self.viewport.width,
                            self.viewport.height,
                            cx,
                            cy,
                            sprite,
                            *pixel_scale as f32 * world.camera().effective_zoom(),
                        );
                    } else {
                        draw_square(
                            frame,
                            self.viewport.width,
                            self.viewport.height,
                            cx,
                            cy,
                            PLACEHOLDER_HALF_SIZE_PX,
                            PLACEHOLDER_COLOR,
                        );
                    }
                }
            }
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
                warn_sprite_load_once(
                    warned_missing_sprite_keys,
                    key,
                    Some(path.as_path()),
                    reason.as_str(),
                );
                None
            }
        },
        Err(reason) => {
            warn_sprite_load_once(warned_missing_sprite_keys, key, None, reason.as_str());
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
    use crate::app::{Camera2D, DebugMarker, DebugMarkerKind, EntityId, FloorId, Tilemap};
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
