use pixels::{Error, Pixels, SurfaceTexture};
use winit::window::Window;

use crate::app::{tools::draw_overlay, OverlayData, RenderableKind, SceneWorld};

use super::{world_to_screen_px, Viewport, PLACEHOLDER_HALF_SIZE_PX};

const CLEAR_COLOR: [u8; 4] = [20, 22, 28, 255];
const PLACEHOLDER_COLOR: [u8; 4] = [220, 220, 240, 255];

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
            let idx = ((y as u32 * width + x as u32) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&color);
        }
    }
}
