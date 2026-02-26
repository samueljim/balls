// Simple 2D physics: gravity, velocity, collision vs terrain.
// Uses fixed-point style for determinism (i32 hundredths).

use serde::{Deserialize, Serialize};

const GRAVITY: i32 = 2; // per tick, in hundredths

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: i32, // in hundredths
    pub y: i32,
}

impl Vec2 {
    pub fn from_f32(x: f32, y: f32) -> Self {
        Vec2 {
            x: (x * 100.0).round() as i32,
            y: (y * 100.0).round() as i32,
        }
    }
    pub fn to_f32(self) -> (f32, f32) {
        (self.x as f32 / 100.0, self.y as f32 / 100.0)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Worm {
    pub position: Vec2,
    pub velocity: Vec2,
    pub health: i32,
    pub facing: i32, // -1 left, 1 right
}

impl Worm {
    pub fn new(px: i32, py: i32) -> Self {
        Worm {
            position: Vec2 { x: px, y: py },
            velocity: Vec2 { x: 0, y: 0 },
            health: 100,
            facing: 1,
        }
    }

    /// Step one tick: apply gravity, move, collide with terrain.
    pub fn tick(&mut self, terrain: &crate::terrain::Terrain) {
        self.velocity.y += GRAVITY;
        self.position.x += self.velocity.x / 100;
        self.position.y += self.velocity.y / 100;
        self.velocity.x = self.velocity.x * 95 / 100; // friction
        self.velocity.y = self.velocity.y * 95 / 100;

        // Collision: snap to ground if standing on solid
        let (px, py) = (self.position.x / 100, self.position.y / 100);
        let r = 8i32; // worm radius approx
        if terrain.is_solid(px, py + r) || terrain.is_solid(px + r, py + r) || terrain.is_solid(px - r, py + r) {
            self.position.y = (py - r) * 100;
            self.velocity.y = 0;
            if self.velocity.x.abs() < 10 {
                self.velocity.x = 0;
            }
        }
        // Left/right bounds
        if terrain.is_solid(px - r, py) {
            self.position.x = (r + 1) * 100;
            self.velocity.x = 0;
        }
        if terrain.is_solid(px + r, py) {
            self.position.x = (terrain.width as i32 - r - 1) * 100;
            self.velocity.x = 0;
        }
    }

    pub fn apply_knockback(&mut self, dx: i32, dy: i32) {
        self.velocity.x += dx;
        self.velocity.y += dy;
    }

    pub fn take_damage(&mut self, amount: i32) {
        self.health = (self.health - amount).max(0);
    }
}
