use pixels::{Error, Pixels, SurfaceTexture};
use winit::window::Window;

use crate::app::{tools::draw_overlay, OverlayData, RenderableKind, SceneWorld, Vec2};

use super::{world_to_screen_px, Viewport, PIXELS_PER_WORLD, PLACEHOLDER_HALF_SIZE_PX};

const CLEAR_COLOR: [u8; 4] = [20, 22, 28, 255];
const PLACEHOLDER_COLOR: [u8; 4] = [220, 220, 240, 255];
const GRID_CELL_WORLD: f32 = 1.0;
const GRID_MAJOR_EVERY: i32 = 5;
const GRID_MINOR_COLOR: [u8; 4] = [35, 39, 46, 255];
const GRID_MAJOR_COLOR: [u8; 4] = [52, 58, 70, 255];

pub struct Renderer<'window> {
    pixels: Pixels<'window>,
    viewport: Viewport,
}

impl<'window> Renderer<'window> {
    pub fn new(window: &'window Window) -> Result<Self, Error> {
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window);
        let pixels = Pixels::new(size.width, size.height, surface)?;
        Ok(Self {
            pixels,
            viewport: Viewport {
                width: size.width,
                height: size.height,
            },
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.pixels.resize_surface(width, height)?;
        self.pixels.resize_buffer(width, height)?;
        self.viewport = Viewport { width, height };
        Ok(())
    }

    pub(crate) fn render_world(
        &mut self,
        world: &SceneWorld,
        overlay_data: Option<&OverlayData>,
    ) -> Result<(), Error> {
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Ok(());
        }

        let frame = self.pixels.frame_mut();
        for chunk in frame.chunks_exact_mut(4) {
            chunk.copy_from_slice(&CLEAR_COLOR);
        }

        draw_world_grid(frame, self.viewport.width, self.viewport.height, world);

        for entity in world.entities() {
            if matches!(entity.renderable.kind, RenderableKind::Placeholder) {
                let (cx, cy) = world_to_screen_px(
                    world.camera(),
                    (self.viewport.width, self.viewport.height),
                    entity.transform.position,
                );
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

        if let Some(data) = overlay_data {
            draw_overlay(frame, self.viewport.width, self.viewport.height, data);
        }

        self.pixels.render()
    }
}

fn draw_world_grid(frame: &mut [u8], width: u32, height: u32, world: &SceneWorld) {
    if width == 0 || height == 0 {
        return;
    }

    let (ix_start, ix_end, iy_start, iy_end) =
        visible_grid_index_bounds(world.camera().position, width, height);

    for ix in ix_start..=ix_end {
        let world_x = ix as f32 * GRID_CELL_WORLD;
        let (screen_x, _) = world_to_screen_px(
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
        let (_, screen_y) = world_to_screen_px(
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

fn visible_grid_index_bounds(camera_pos: Vec2, width: u32, height: u32) -> (i32, i32, i32, i32) {
    let half_w_world = width as f32 / (2.0 * PIXELS_PER_WORLD);
    let half_h_world = height as f32 / (2.0 * PIXELS_PER_WORLD);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Camera2D;

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
        let (ix_start, ix_end, iy_start, iy_end) = visible_grid_index_bounds(camera_pos, 1280, 720);
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
    fn camera_pan_shifts_projected_grid_lines_consistently() {
        let camera_a = Camera2D {
            position: Vec2 { x: 0.0, y: 0.0 },
        };
        let camera_b = Camera2D {
            position: Vec2 { x: 1.0, y: 0.0 },
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
        assert_eq!(xa - xb, PIXELS_PER_WORLD.round() as i32);
    }
}
