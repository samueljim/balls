use macroquad::prelude::*;

/// World units visible along the shorter screen axis at zoom 1.0.
/// This anchors the view to screen shape rather than terrain width, so the
/// camera stays tight and action-focused on any orientation or window size.
/// Smaller value = more zoomed in. At default zoom 1.2 → ~290 units on the
/// short axis, giving a close-up view of the current player / projectile.
const BASE_SHORT_AXIS: f32 = 350.0;

/// Maximum inertia speed in world-units/second.
const MAX_VEL: f32 = 2500.0;

pub struct GameCamera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub target_x: f32,
    pub target_y: f32,
    /// Momentum velocity in world-units/second. Applied every tick by apply_momentum().
    pub vel_x: f32,
    pub vel_y: f32,
}

impl GameCamera {
    pub fn new(x: f32, y: f32) -> Self {
        GameCamera {
            x,
            y,
            zoom: 1.2,
            target_x: x,
            target_y: y,
            vel_x: 0.0,
            vel_y: 0.0,
        }
    }

    /// Smoothly follow a world-space target. Also bleeds off any residual momentum
    /// so it doesn't fight the auto-follow interpolation.
    pub fn follow(&mut self, tx: f32, ty: f32, speed: f32, dt: f32) {
        self.target_x = tx;
        self.target_y = ty;
        let rate = (speed * dt).min(1.0);
        self.x += (self.target_x - self.x) * rate;
        self.y += (self.target_y - self.y) * rate;
        // Drain momentum toward zero at the same rate so inertia doesn't fight the follow
        self.vel_x *= 1.0 - rate;
        self.vel_y *= 1.0 - rate;
    }

    /// Pan by a screen-pixel delta, imparting inertial velocity proportional to swipe speed.
    /// Immediate position offset is also applied so the view feels 1:1 with the gesture.
    pub fn pan_push(&mut self, dx_screen: f32, dy_screen: f32, dt: f32) {
        let vw = self.visible_width();
        let vh = self.visible_height();
        let world_dx = -dx_screen / screen_width() * vw;
        let world_dy = -dy_screen / screen_height() * vh;
        // Apply position immediately (1:1 with finger/mouse)
        self.x += world_dx;
        self.y += world_dy;
        self.target_x = self.x;
        self.target_y = self.y;
        // Impart velocity for the inertia coast after release
        let dt_safe = dt.max(0.001);
        let new_vx = (world_dx / dt_safe).clamp(-MAX_VEL, MAX_VEL);
        let new_vy = (world_dy / dt_safe).clamp(-MAX_VEL, MAX_VEL);
        // Blend toward new velocity (smooths out jitter from tiny deltas)
        self.vel_x = self.vel_x * 0.4 + new_vx * 0.6;
        self.vel_y = self.vel_y * 0.4 + new_vy * 0.6;
    }

    /// Apply inertial coast. Call every frame unconditionally (even when following).
    /// When the camera is in auto-follow mode, follow() drains the velocity so this
    /// becomes a no-op quickly.
    pub fn apply_momentum(&mut self, dt: f32) {
        if self.vel_x.abs() < 1.0 && self.vel_y.abs() < 1.0 {
            self.vel_x = 0.0;
            self.vel_y = 0.0;
            return;
        }
        self.x += self.vel_x * dt;
        self.y += self.vel_y * dt;
        self.target_x = self.x;
        self.target_y = self.y;
        // Frame-rate-independent friction (~75% speed remaining after 1 second)
        let friction = 0.82_f32.powf(dt * 60.0);
        self.vel_x *= friction;
        self.vel_y *= friction;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.4, 3.0);
    }

    /// World units visible horizontally.
    /// On landscape screens the width axis is the long one; on portrait/narrow
    /// windows (e.g. devtools open) the width axis is the short one — both are
    /// handled correctly because we anchor to the shorter screen dimension.
    pub fn visible_width(&self) -> f32 {
        let short_axis = BASE_SHORT_AXIS / self.zoom;
        if screen_width() <= screen_height() {
            // Portrait / square: width is the short axis
            short_axis
        } else {
            // Landscape: height is the short axis, scale width proportionally
            short_axis * screen_width() / screen_height()
        }
    }

    /// World units visible vertically. Always `visible_width * (h/w)`.
    pub fn visible_height(&self) -> f32 {
        let short_axis = BASE_SHORT_AXIS / self.zoom;
        if screen_height() <= screen_width() {
            // Landscape / square: height is the short axis
            short_axis
        } else {
            // Portrait: width is the short axis, scale height proportionally
            short_axis * screen_height() / screen_width()
        }
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
        // Legacy alias — callers should prefer pan_push() for momentum support.
        let vw = self.visible_width();
        let vh = self.visible_height();
        self.x -= dx_screen / screen_width() * vw;
        self.y -= dy_screen / screen_height() * vh;
        self.target_x = self.x;
        self.target_y = self.y;
    }
}
