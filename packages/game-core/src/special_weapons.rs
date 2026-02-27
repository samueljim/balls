use crate::physics::Ball;
use crate::terrain::Terrain;
use crate::projectile::Explosion;
use macroquad::prelude::*;

pub struct AirstrikeDroplet {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub alive: bool,
    pub weapon_type: AirstrikeType,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AirstrikeType {
    Explosive,
    Napalm,
}

impl AirstrikeDroplet {
    pub fn tick(&mut self, terrain: &mut Terrain, balls: &mut [Ball], dt: f32) -> Option<Explosion> {
        if !self.alive {
            return None;
        }

        self.vy += 600.0 * dt; // Faster fall than projectiles
        self.y += self.vy * dt;

        // Check bounds
        if self.y < 0.0 || self.y > terrain.height as f32 + 100.0 {
            self.alive = false;
            return None;
        }

        // Check collision with terrain or water
        let is_water = self.y >= crate::terrain::WATER_LEVEL;
        if is_water || terrain.is_solid(self.x as i32, self.y as i32) {
            return self.explode(terrain, balls);
        }

        None
    }

    fn explode(&mut self, terrain: &mut Terrain, balls: &mut [Ball]) -> Option<Explosion> {
        self.alive = false;

        let (radius, damage) = match self.weapon_type {
            AirstrikeType::Explosive => (25.0, 30),
            AirstrikeType::Napalm => (20.0, 25),
        };

        terrain.apply_damage(self.x as i32, self.y as i32, radius as i32);

        // Apply damage to balls
        let blast_radius = radius * 1.8;
        let r2 = blast_radius * blast_radius;
        for w in balls.iter_mut() {
            if !w.alive {
                continue;
            }
            let dx = w.x - self.x;
            let dy = w.y - self.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < r2 {
                let dist = dist_sq.sqrt().max(1.0);
                let factor = 1.0 - (dist / blast_radius).min(1.0);
                let dmg = (damage as f32 * factor) as i32;
                if dmg > 0 {
                    w.take_damage(dmg.max(1));
                }
                let knock = 180.0 * factor;
                let nx = dx / dist * knock;
                let ny = (dy / dist * knock) - 120.0 * factor;
                w.apply_knockback(nx, ny);
            }
        }

        Some(Explosion {
            x: self.x,
            y: self.y,
            radius,
            is_water: false,
        })
    }
}

pub struct UziBullet {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub alive: bool,
}

impl UziBullet {
    pub fn tick(&mut self, terrain: &mut Terrain, balls: &mut [Ball], dt: f32) -> bool {
        if !self.alive {
            return false;
        }

        const GRAVITY: f32 = 300.0; // Less gravity than shotgun pellets
        self.vy += GRAVITY * dt;
        self.vx *= 0.995; // Very little air resistance
        
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Check bounds - allow going above map, die off sides/bottom
        if self.x < -50.0 || self.x >= terrain.width as f32 + 50.0 || self.y >= terrain.height as f32 + 50.0 {
            self.alive = false;
            return false;
        }

        // Skip terrain/ball checks when above map
        if self.y < 0.0 {
            return false;
        }

        // Check water
        if self.y >= crate::terrain::WATER_LEVEL {
            self.alive = false;
            return false;
        }

        // Check terrain collision
        if terrain.is_solid(self.x as i32, self.y as i32) {
            self.alive = false;
            terrain.apply_damage(self.x as i32, self.y as i32, 2);
            return true;
        }

        // Check ball collision
        for w in balls.iter_mut() {
            if !w.alive {
                continue;
            }
            let dx = w.x - self.x;
            let dy = w.y - self.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < 80.0 { // ~9 pixel radius
                w.take_damage(5);
                let dist = dist_sq.sqrt().max(1.0);
                let knock = 40.0;
                w.apply_knockback((dx / dist) * knock, (dy / dist) * knock - 30.0);
                self.alive = false;
                return true;
            }
        }

        false
    }
}

pub struct PlacedExplosive {
    pub x: f32,
    pub y: f32,
    pub fuse: f32,
    pub alive: bool,
    pub radius: f32,
    pub damage: i32,
}

impl PlacedExplosive {
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.alive {
            return false;
        }

        self.fuse -= dt;
        if self.fuse <= 0.0 {
            self.alive = false;
            return true; // Explode!
        }

        false
    }

    pub fn explode(&self, terrain: &mut Terrain, balls: &mut [Ball]) -> Explosion {
        terrain.apply_damage(self.x as i32, self.y as i32, self.radius as i32);

        // Apply damage to balls
        let blast_radius = self.radius * 1.8;
        let r2 = blast_radius * blast_radius;
        for w in balls.iter_mut() {
            if !w.alive {
                continue;
            }
            let dx = w.x - self.x;
            let dy = w.y - self.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < r2 {
                let dist = dist_sq.sqrt().max(1.0);
                let factor = 1.0 - (dist / blast_radius).min(1.0);
                let dmg = (self.damage as f32 * factor) as i32;
                if dmg > 0 {
                    w.take_damage(dmg.max(1));
                }
                let knock = 280.0 * factor;
                let nx = dx / dist * knock;
                let ny = (dy / dist * knock) - 150.0 * factor;
                w.apply_knockback(nx, ny);
            }
        }

        Explosion {
            x: self.x,
            y: self.y,
            radius: self.radius,
            is_water: false,
        }
    }
}
