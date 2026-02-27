use crate::physics::Ball;
use crate::terrain::Terrain;
use crate::weapons::Weapon;

pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub weapon: Weapon,
    pub fuse: f32,
    pub bounces: i32,
    pub alive: bool,
    pub trail: Vec<(f32, f32)>,
    /// Team that fired this projectile — used to avoid friendly-fire targeting
    pub shooter_team: u32,
}

pub struct ShotgunPellet {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub alive: bool,
    pub damage: i32,
}

pub struct Explosion {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub is_water: bool,
}

pub struct ClusterBomblet {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub fuse: f32,
    pub alive: bool,
    pub radius: f32,
    pub damage: i32,
}

impl ShotgunPellet {
    pub fn tick(&mut self, terrain: &mut Terrain, balls: &mut [Ball], dt: f32) -> bool {
        if !self.alive {
            return false;
        }

        const GRAVITY: f32 = 480.0;
        self.vy += GRAVITY * dt;
        self.vx *= 0.98; // Air resistance
        
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Check bounds - allow going above map (y < 0), die off sides/bottom
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
            // Small terrain damage
            terrain.apply_damage(self.x as i32, self.y as i32, 3);
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
            if dist_sq < 100.0 { // ~10 pixel radius
                w.take_damage(self.damage);
                let dist = dist_sq.sqrt().max(1.0);
                let knock = 80.0;
                w.apply_knockback((dx / dist) * knock, (dy / dist) * knock - 50.0);
                self.alive = false;
                return true;
            }
        }

        false
    }
}

impl ClusterBomblet {
    pub fn tick(&mut self, terrain: &mut Terrain, balls: &mut [Ball], dt: f32) -> Option<Explosion> {
        if !self.alive {
            return None;
        }

        self.vy += 300.0 * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        self.fuse -= dt;
        if self.fuse <= 0.0 {
            return self.explode(terrain, balls);
        }

        // Boundary check
        if self.x < -100.0 || self.x > terrain.width as f32 + 100.0 
            || self.y > terrain.height as f32 + 100.0 || self.y < -300.0 {
            self.alive = false;
            return None;
        }

        // Check collision
        if terrain.is_solid(self.x as i32, self.y as i32) {
            return self.explode(terrain, balls);
        }

        if self.y > crate::terrain::WATER_LEVEL {
            self.alive = false;
            return Some(Explosion {
                x: self.x,
                y: self.y,
                radius: 5.0,
                is_water: true,
            });
        }

        None
    }

    fn explode(&mut self, terrain: &mut Terrain, balls: &mut [Ball]) -> Option<Explosion> {
        self.alive = false;
        
        terrain.apply_damage(self.x as i32, self.y as i32, self.radius as i32);

        let blast_radius = self.radius * 2.0;
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
                let knock = 200.0 * factor;
                let nx = dx / dist * knock;
                let ny = (dy / dist * knock) - 150.0 * factor;
                w.apply_knockback(nx, ny);
            }
        }

        Some(Explosion {
            x: self.x,
            y: self.y,
            radius: self.radius,
            is_water: false,
        })
    }
}

impl Projectile {
    pub fn new(x: f32, y: f32, angle: f32, power: f32, weapon: Weapon, shooter_team: u32) -> Self {
        let speed = power * 12.0;
        let vx = angle.cos() * speed;
        let vy = angle.sin() * speed;
        
        Projectile {
            x,
            y,
            vx,
            vy,
            weapon,
            fuse: match weapon {
                Weapon::Grenade => 3.0,
                Weapon::Sheep => 5.0,
                Weapon::SuperSheep => 10.0,
                _ => -1.0,
            },
            bounces: 0,
            alive: true,
            trail: Vec::new(),
            shooter_team,
        }
    }

    pub fn tick(&mut self, terrain: &mut Terrain, balls: &mut [Ball], wind: f32, dt: f32) -> (Option<Explosion>, Vec<ClusterBomblet>) {
        if !self.alive {
            return (None, Vec::new());
        }

        self.trail.push((self.x, self.y));
        if self.trail.len() > 30 {
            self.trail.remove(0);
        }

        const GRAVITY: f32 = 480.0;
        let air_resistance = if self.weapon == Weapon::Bazooka { 0.99 } else { 0.98 };

        // ── Sheep / SuperSheep: walk along the terrain surface ──────────────────
        if self.weapon == Weapon::Sheep || self.weapon == Weapon::SuperSheep {
            let walk_speed = if self.weapon == Weapon::SuperSheep { 140.0 } else { 90.0 };
            let dir = if self.vx >= 0.0 { 1.0f32 } else { -1.0f32 };

            // Gravity so the sheep falls off ledges naturally
            self.vy += GRAVITY * dt;

            // Horizontal walk
            self.x += dir * walk_speed * dt;
            self.y += self.vy * dt;

            // If now inside solid terrain, push up (walk over slopes up to 12px)
            if terrain.is_solid(self.x as i32, self.y as i32) {
                let mut stepped = false;
                for step in 1i32..=12 {
                    if !terrain.is_solid(self.x as i32, (self.y - step as f32) as i32) {
                        self.y -= step as f32;
                        self.vy = 0.0;
                        stepped = true;
                        break;
                    }
                }
                if !stepped {
                    // Completely blocked by a wall – reverse direction
                    self.x -= dir * walk_speed * dt * 2.0;
                    self.vx = -self.vx;
                }
            } else {
                // Snap smoothly onto ground when falling onto it (small gaps)
                for step in 1i32..=4 {
                    if terrain.is_solid(self.x as i32, (self.y + step as f32) as i32) {
                        self.y += step as f32 - 1.0;
                        self.vy = 0.0;
                        break;
                    }
                }
            }

            // Check if sheep touched any player ball → explode immediately
            {
                let hit_radius_sq = 18.0f32 * 18.0f32;
                let sheep_hit = balls.iter().any(|w| {
                    if !w.alive { return false; }
                    let dx = w.x - self.x;
                    let dy = w.y - self.y;
                    dx * dx + dy * dy < hit_radius_sq
                });
                if sheep_hit {
                    self.alive = false;
                    return self.create_explosion(terrain, balls);
                }
            }

            // Fuse countdown → explode
            if self.fuse > 0.0 {
                self.fuse -= dt;
                if self.fuse <= 0.0 {
                    self.alive = false;
                    return self.create_explosion(terrain, balls);
                }
            }

            // Boundary / water checks
            let px = self.x as i32;
            let py = self.y as i32;
            if px < -100 || px >= terrain.width as i32 + 100 || py >= terrain.height as i32 + 100 {
                self.alive = false;
                return (None, Vec::new());
            }
            if py >= crate::terrain::WATER_LEVEL as i32 {
                self.alive = false;
                return (Some(Explosion { x: self.x, y: self.y, radius: 0.0, is_water: true }), Vec::new());
            }

            return (None, Vec::new());
        }
        // ────────────────────────────────────────────────────────────────────────

        // Homing Missile behavior - track nearest ball
        if self.weapon == Weapon::HomingMissile {
            let mut closest_dist = f32::MAX;
            let mut target_x = 0.0;
            let mut target_y = 0.0;
            
            for w in balls.iter() {
                if !w.alive {
                    continue;
                }
                // Only home on enemies, not teammates
                if w.team == self.shooter_team {
                    continue;
                }
                let dx = w.x - self.x;
                let dy = w.y - self.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < closest_dist && dist < 1200.0 {
                    closest_dist = dist;
                    target_x = w.x;
                    target_y = w.y;
                }
            }
            
            if closest_dist < f32::MAX {
                let dx = target_x - self.x;
                let dy = target_y - self.y;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                // Strong homing: steer toward target aggressively
                let turn_force = 800.0 * dt;
                self.vx += (dx / dist) * turn_force;
                self.vy += (dy / dist) * turn_force;
                
                // Higher speed cap so it actually closes in
                let speed = (self.vx * self.vx + self.vy * self.vy).sqrt();
                if speed > 420.0 {
                    self.vx = (self.vx / speed) * 420.0;
                    self.vy = (self.vy / speed) * 420.0;
                }
            }

            // Homing missile self-propels — skip gravity so it isn't dragged down
            self.vx += wind * 5.0 * dt;
            self.vx *= air_resistance;
            self.x += self.vx * dt;
            self.y += self.vy * dt;

            if self.fuse > 0.0 {
                self.fuse -= dt;
                if self.fuse <= 0.0 {
                    self.alive = false;
                    return self.create_explosion(terrain, balls);
                }
            }

            let px = self.x as i32;
            let py = self.y as i32;
            if px < -100 || px >= terrain.width as i32 + 100 || py >= terrain.height as i32 + 100 {
                self.alive = false;
                return (None, Vec::new());
            }
            if py < 0 { return (None, Vec::new()); }

            // Ball hit check
            let hit_radius_sq = 14.0f32 * 14.0f32;
            let hit_idx = balls.iter().enumerate()
                .find(|(_, w)| {
                    if !w.alive { return false; }
                    let dx = w.x - self.x;
                    let dy = w.y - self.y;
                    dx * dx + dy * dy < hit_radius_sq
                })
                .map(|(i, _)| i);
            if hit_idx.is_some() {
                self.alive = false;
                return self.create_explosion(terrain, balls);
            }

            if terrain.is_solid(px, py) {
                self.alive = false;
                return self.create_explosion(terrain, balls);
            }
            return (None, Vec::new());
        }

        self.vx += wind * 15.0 * dt;
        self.vx *= air_resistance;
        self.vy += GRAVITY * dt;
        
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        if self.fuse > 0.0 {
            self.fuse -= dt;
            if self.fuse <= 0.0 {
                self.alive = false;
                return self.create_explosion(terrain, balls);
            }
        }

        let px = self.x as i32;
        let py = self.y as i32;

        // Allow projectiles above map (py < 0), die off sides/far below
        if px < -100 || px >= terrain.width as i32 + 100 || py >= terrain.height as i32 + 100 {
            self.alive = false;
            return (None, Vec::new());
        }

        // Skip terrain/ball checks when above map
        if py < 0 {
            return (None, Vec::new());
        }

        let is_water = py >= crate::terrain::WATER_LEVEL as i32;
        if is_water {
            self.alive = false;
            return (Some(Explosion {
                x: self.x,
                y: self.y,
                radius: 0.0,
                is_water: true,
            }), Vec::new());
        }

        // Check direct ball collision — scan first (immutable), then act (mutable)
        let hit_radius_sq = 14.0f32 * 14.0f32;
        let hit_idx = balls.iter().enumerate()
            .find(|(_, w)| {
                if !w.alive { return false; }
                let dx = w.x - self.x;
                let dy = w.y - self.y;
                dx * dx + dy * dy < hit_radius_sq
            })
            .map(|(i, _)| i);

        if let Some(bi) = hit_idx {
            self.alive = false;
            if self.weapon.explosion_radius() > 0.0 {
                // Explosive on direct hit: full explosion at impact point
                return self.create_explosion(terrain, balls);
            } else {
                // Non-explosive (SniperRifle, etc.): direct damage + directional knockback
                let damage = self.weapon.base_damage();
                let speed = (self.vx * self.vx + self.vy * self.vy).sqrt().max(1.0);
                let knock_scale = 200.0_f32.max(damage as f32 * 0.2);
                balls[bi].take_damage(damage);
                balls[bi].apply_knockback(
                    (self.vx / speed) * knock_scale,
                    (self.vy / speed) * knock_scale - 80.0,
                );
                return (None, Vec::new());
            }
        }

        if terrain.is_solid(px, py) {
            // Handle bouncing for specific weapons
            let max_bounces = self.weapon.max_bounces();
            if max_bounces > 0 && self.bounces < max_bounces {
                self.bounces += 1;
                
                // Different bounce physics for different weapons
                let bounce_damping = match self.weapon {
                    Weapon::BananaBomb => 0.7,  // High bounce
                    Weapon::Grenade => 0.6,
                    Weapon::ClusterBomb => 0.5,
                    _ => 0.6,
                };
                
                self.vy *= -bounce_damping;
                self.vx *= 0.8;
                self.y = py as f32 - 2.0;
                return (None, Vec::new());
            }

            // ConcreteShell: carve a walkable tunnel along trajectory, no damage
            if self.weapon == Weapon::ConcreteShell {
                let speed = (self.vx * self.vx + self.vy * self.vy).sqrt().max(1.0);
                let dir_x = self.vx / speed;
                let dir_y = self.vy / speed;
                // Perpendicular direction for tunnel width
                let perp_x = -dir_y;
                let perp_y = dir_x;
                let tunnel_half_w: i32 = 11; // ~22px wide — enough to walk through
                let tunnel_back: i32 = 20;   // carve behind impact point too
                let tunnel_fwd: i32 = 120;   // carve forward into terrain
                for along in -tunnel_back..=tunnel_fwd {
                    for perp in -tunnel_half_w..=tunnel_half_w {
                        let cx = (self.x + dir_x * along as f32 + perp_x * perp as f32) as i32;
                        let cy = (self.y + dir_y * along as f32 + perp_y * perp as f32) as i32;
                        terrain.set(cx, cy, 0); // AIR
                    }
                }
                self.alive = false;
                return (None, Vec::new());
            }

            self.alive = false;
            return self.create_explosion(terrain, balls);
        }

        (None, Vec::new())
    }

    fn create_explosion(&self, terrain: &mut Terrain, balls: &mut [Ball]) -> (Option<Explosion>, Vec<ClusterBomblet>) {
        let explosion_radius = self.weapon.explosion_radius() as i32;

        let px = self.x as i32;
        let py = self.y as i32;

        terrain.apply_damage(px, py, explosion_radius);

        let max_damage = self.weapon.base_damage();

        for w in balls.iter_mut() {
            if !w.alive {
                continue;
            }
            let dx = w.x - self.x;
            let dy = w.y - self.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let r = explosion_radius as f32 * 1.5;
            
            if dist < r {
                let damage_factor = (1.0 - dist / r).max(0.0);
                let damage = (max_damage as f32 * damage_factor) as i32;
                if damage > 0 {
                    w.take_damage(damage);
                    let knockback_force = 250.0 * damage_factor;
                    let knockback_x = (dx / dist.max(1.0)) * knockback_force;
                    let knockback_y = (dy / dist.max(1.0)) * knockback_force - 100.0;
                    w.apply_knockback(knockback_x, knockback_y);
                }
            }
        }

        // Generate cluster bomblets for cluster weapons
        let cluster_count = self.weapon.cluster_count();
        let mut bomblets = Vec::new();
        if cluster_count > 0 {
            use std::f32::consts::PI;
            for i in 0..cluster_count {
                let angle = (i as f32 / cluster_count as f32) * 2.0 * PI;
                let speed = 100.0 + (i as f32 * 20.0) % 80.0;
                bomblets.push(ClusterBomblet {
                    x: self.x,
                    y: self.y,
                    vx: angle.cos() * speed,
                    vy: angle.sin() * speed - 50.0,
                    fuse: 1.0 + (i as f32 * 0.1),
                    alive: true,
                    radius: match self.weapon {
                        Weapon::ClusterBomb => 15.0,
                        Weapon::BananaBomb => 18.0,
                        Weapon::BananaBonanza => 20.0,
                        Weapon::Mortar => 12.0,
                        _ => 10.0,
                    },
                    damage: match self.weapon {
                        Weapon::ClusterBomb => 20,
                        Weapon::BananaBomb => 25,
                        Weapon::BananaBonanza => 18,
                        Weapon::Mortar => 15,
                        _ => 10,
                    },
                });
            }
        }

        (Some(Explosion {
            x: self.x,
            y: self.y,
            radius: explosion_radius as f32,
            is_water: false,
        }), bomblets)
    }
}

pub fn simulate_trajectory(
    start_x: f32,
    start_y: f32,
    angle: f32,
    power: f32,
    weapon: Weapon,
    wind: f32,
    terrain: &Terrain,
) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    let speed = power * 12.0;
    let mut x = start_x;
    let mut y = start_y;
    let mut vx = angle.cos() * speed;
    let mut vy = angle.sin() * speed;
    
    const GRAVITY: f32 = 480.0;
    const DT: f32 = 1.0 / 60.0;
    let air_resistance = if weapon == Weapon::Bazooka { 0.99 } else { 0.98 };
    let max_steps = 180;

    for _ in 0..max_steps {
        vx += wind * 15.0 * DT;
        vx *= air_resistance;
        vy += GRAVITY * DT;
        x += vx * DT;
        y += vy * DT;

        points.push((x, y));

        let px = x as i32;
        let py = y as i32;

        if px < -100 || px >= terrain.width as i32 + 100 || py >= terrain.height as i32 + 100 {
            break;
        }

        // Only check terrain/water when on-screen vertically
        if py >= 0 && (py >= crate::terrain::WATER_LEVEL as i32 || terrain.is_solid(px, py)) {
            break;
        }
    }

    points
}
