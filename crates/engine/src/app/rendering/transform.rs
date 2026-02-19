use crate::app::{Camera2D, Vec2};

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

pub fn world_to_screen(
    world: Vec2,
    camera: &Camera2D,
    viewport: Viewport,
    pixels_per_world: f32,
) -> (i32, i32) {
    let x = (world.x - camera.position.x) * pixels_per_world + viewport.width as f32 * 0.5;
    let y = viewport.height as f32 * 0.5 - (world.y - camera.position.y) * pixels_per_world;
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_maps_to_viewport_center() {
        let viewport = Viewport {
            width: 800,
            height: 600,
        };
        let camera = Camera2D::default();
        let (x, y) = world_to_screen(Vec2 { x: 0.0, y: 0.0 }, &camera, viewport, 32.0);
        assert_eq!(x, 400);
        assert_eq!(y, 300);
    }

    #[test]
    fn camera_offset_shifts_screen_position() {
        let viewport = Viewport {
            width: 800,
            height: 600,
        };
        let camera = Camera2D {
            position: Vec2 { x: 10.0, y: -5.0 },
        };
        let (x, y) = world_to_screen(Vec2 { x: 12.0, y: -4.0 }, &camera, viewport, 10.0);
        assert_eq!(x, 420);
        assert_eq!(y, 290);
    }
}
