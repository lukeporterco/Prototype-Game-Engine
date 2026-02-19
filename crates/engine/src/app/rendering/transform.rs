use crate::app::{Camera2D, Vec2};

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

pub const PIXELS_PER_WORLD: f32 = 32.0;

pub fn world_to_screen_px(
    camera: &Camera2D,
    window_size: (u32, u32),
    world_pos: Vec2,
) -> (i32, i32) {
    world_to_screen(
        world_pos,
        camera,
        Viewport {
            width: window_size.0,
            height: window_size.1,
        },
        PIXELS_PER_WORLD,
    )
}

pub fn screen_to_world_px(camera: &Camera2D, window_size: (u32, u32), screen_px: Vec2) -> Vec2 {
    let half_width = window_size.0 as f32 * 0.5;
    let half_height = window_size.1 as f32 * 0.5;
    Vec2 {
        x: camera.position.x + (screen_px.x - half_width) / PIXELS_PER_WORLD,
        y: camera.position.y - (screen_px.y - half_height) / PIXELS_PER_WORLD,
    }
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

    #[test]
    fn world_to_screen_px_uses_default_pixels_per_world() {
        let camera = Camera2D::default();
        let (x, y) = world_to_screen_px(&camera, (800, 600), Vec2 { x: 1.0, y: -1.0 });
        assert_eq!(x, 432);
        assert_eq!(y, 332);
    }

    #[test]
    fn screen_center_maps_to_camera_position() {
        let camera = Camera2D {
            position: Vec2 { x: 4.5, y: -2.0 },
        };
        let world = screen_to_world_px(&camera, (800, 600), Vec2 { x: 400.0, y: 300.0 });
        assert!((world.x - 4.5).abs() < 0.0001);
        assert!((world.y + 2.0).abs() < 0.0001);
    }

    #[test]
    fn screen_to_world_px_inverts_known_point() {
        let camera = Camera2D {
            position: Vec2 { x: 10.0, y: -5.0 },
        };
        let world = screen_to_world_px(&camera, (800, 600), Vec2 { x: 420.0, y: 290.0 });
        assert!((world.x - 10.625).abs() < 0.0001);
        assert!((world.y + 4.6875).abs() < 0.0001);
    }

    #[test]
    fn world_to_screen_then_back_is_consistent() {
        let camera = Camera2D {
            position: Vec2 { x: 3.0, y: 1.0 },
        };
        let input = Vec2 { x: -1.75, y: 6.25 };
        let (sx, sy) = world_to_screen_px(&camera, (1280, 720), input);
        let output = screen_to_world_px(
            &camera,
            (1280, 720),
            Vec2 {
                x: sx as f32,
                y: sy as f32,
            },
        );
        assert!((output.x - input.x).abs() <= (0.5 / PIXELS_PER_WORLD));
        assert!((output.y - input.y).abs() <= (0.5 / PIXELS_PER_WORLD));
    }
}
