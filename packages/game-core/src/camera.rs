use macroquad::prelude::*;

pub struct GameCamera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub target_x: f32,
    pub target_y: f32,
}

impl GameCamera {
    pub fn new(x: f32, y: f32) -> Self {
        GameCamera {
            x,
            y,
            zoom: 1.2,
            target_x: x,
            target_y: y,
        }
    }

    pub fn follow(&mut self, tx: f32, ty: f32, speed: f32, dt: f32) {
        self.target_x = tx;
        self.target_y = ty;
        let rate = (speed * dt).min(1.0);
        self.x += (self.target_x - self.x) * rate;
        self.y += (self.target_y - self.y) * rate;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.4, 3.0);
    }

    pub fn visible_width(&self) -> f32 {
        crate::terrain::WIDTH as f32 / self.zoom
    }

    pub fn visible_height(&self) -> f32 {
        self.visible_width() * screen_height() / screen_width()
    }

    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        let vw = self.visible_width();
        let vh = self.visible_height();
        let left = self.x - vw / 2.0;
        let top = self.y - vh / 2.0;
        (
            left + sx / screen_width() * vw,
            top + sy / screen_height() * vh,
        )
    }

    pub fn clamp_to_world(&mut self, world_w: f32, world_h: f32) {
        let vw = self.visible_width();
        let vh = self.visible_height();
        let half_vw = vw / 2.0;
        let half_vh = vh / 2.0;
        // Clamp camera to stay within the playable land area
        let min_x = crate::terrain::LAND_START_X + half_vw;
        let max_x = crate::terrain::LAND_END_X - half_vw;
        self.x = self.x.clamp(min_x, max_x.max(min_x));
        self.y = self.y.clamp(half_vh, (world_h - half_vh).max(half_vh));
    }

    pub fn to_macroquad(&self) -> Camera2D {
        let vw = self.visible_width();
        let vh = self.visible_height();
        #[cfg(not(target_arch = "wasm32"))]
        let rect = Rect::new(self.x - vw / 2.0, self.y - vh / 2.0, vw, vh);
        // WebGL: flip Y so Y-down world (terrain at bottom) displays right-side up
        #[cfg(target_arch = "wasm32")]
        let rect = Rect::new(self.x - vw / 2.0, self.y + vh / 2.0, vw, -vh);
        Camera2D::from_display_rect(rect)
    }

    pub fn pan(&mut self, dx_screen: f32, dy_screen: f32) {
        let vw = self.visible_width();
        let vh = self.visible_height();
        self.x -= dx_screen / screen_width() * vw;
        self.y -= dy_screen / screen_height() * vh;
        self.target_x = self.x;
        self.target_y = self.y;
    }
}
