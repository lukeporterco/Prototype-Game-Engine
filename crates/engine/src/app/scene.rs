pub trait Scene {
    fn update(&mut self, fixed_dt_seconds: f32);
    fn render(&mut self);
}
