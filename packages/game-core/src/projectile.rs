// Projectile flight and explosion resolution.

use crate::physics::{Vec2, Worm};
use crate::terrain::Terrain;
use crate::weapons::Weapon;

const TICK_SCALE: i32 = 100;

#[derive(Clone, Debug)]
pub struct Projectile {
    pub pos: Vec2,
    pub vel: Vec2,
    pub weapon: Weapon,
    pub ticks_remaining: i32, // for grenade fuse etc
}

pub fn fire_bazooka(
    start_x: i32,
    start_y: i32,
    angle_deg: i32,
    power_percent: i32,
) -> Projectile {
    let angle_rad = (angle_deg as f32) * std::f32::consts::PI / 180.0;
    let speed = (power_percent as i32 * 15).min(1200);
    let vx = (angle_rad.cos() * speed as f32).round() as i32 * TICK_SCALE / 100;
    let vy = (-angle_rad.sin() * speed as f32).round() as i32 * TICK_SCALE / 100;
    Projectile {
        pos: Vec2 {
            x: start_x * TICK_SCALE,
            y: start_y * TICK_SCALE,
        },
        vel: Vec2 { x: vx, y: vy },
        weapon: Weapon::Bazooka,
        ticks_remaining: -1,
    }
}

/// Returns true when projectile has stopped (hit or out of bounds).
pub fn tick_projectile(
    p: &mut Projectile,
    terrain: &mut Terrain,
    worms: &mut [Worm],
) -> bool {
    const G: i32 = 3;
    p.vel.y += G;
    p.pos.x += p.vel.x / TICK_SCALE;
    p.pos.y += p.vel.y / TICK_SCALE;
    let px = p.pos.x / TICK_SCALE;
    let py = p.pos.y / TICK_SCALE;

    if px < 0 || py < 0 || px >= terrain.width as i32 || py >= terrain.height as i32 {
        return true;
    }
    if terrain.is_solid(px, py) {
        // Explode
        let r = 25i32;
        terrain.apply_damage(px, py, r);
        for w in worms.iter_mut() {
            let (wx, wy) = (w.position.x / TICK_SCALE, w.position.y / TICK_SCALE);
            let dx = wx - px;
            let dy = wy - py;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < r * r * 4 {
                let dist = (dist_sq as f32).sqrt().max(1.0) as i32;
                let damage = (50 * r / dist).max(5);
                w.take_damage(damage);
                w.apply_knockback(dx * 80 / dist, dy * 80 / dist - 200);
            }
        }
        return true;
    }
    false
}
