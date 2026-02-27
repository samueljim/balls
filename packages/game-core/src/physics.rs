use crate::terrain::Terrain;

pub const WORM_RADIUS: f32 = 8.0;
const GRAVITY: f32 = 480.0;
const WALK_SPEED: f32 = 100.0;
const JUMP_VEL: f32 = -270.0;
const MAX_CLIMB: i32 = 7;
const GROUND_FRICTION: f32 = 0.80;
const AIR_FRICTION: f32 = 0.98;
const FALL_DAMAGE_THRESHOLD: f32 = 120.0;
const FALL_DAMAGE_FACTOR: f32 = 0.25;
const MOVEMENT_BUDGET: f32 = 150.0; // Maximum horizontal distance a worm can move per turn

pub const TEAM_COLORS: [(f32, f32, f32); 4] = [
    (0.85, 0.25, 0.25),
    (0.25, 0.50, 0.90),
    (0.25, 0.75, 0.35),
    (0.90, 0.75, 0.20),
];

pub struct Worm {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub health: i32,
    pub max_health: i32,
    pub facing: f32,
    pub team: u32,
    pub name: String,
    pub on_ground: bool,
    pub alive: bool,
    pub fall_start_y: f32,
    pub last_damage: i32,
    pub damage_timer: f32,
    pub movement_budget: f32,
    pub movement_used: f32,
}

impl Worm {
    pub fn new(x: f32, y: f32, team: u32, name: String) -> Self {
        Worm {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            health: 100,
            max_health: 100,
            facing: if team % 2 == 0 { 1.0 } else { -1.0 },
            team,
            name,
            on_ground: false,
            alive: true,
            fall_start_y: y,
            last_damage: 0,
            damage_timer: 0.0,
            movement_budget: MOVEMENT_BUDGET,
            movement_used: 0.0,
        }
    }

    pub fn reset_movement_budget(&mut self) {
        self.movement_used = 0.0;
        self.movement_budget = MOVEMENT_BUDGET;
    }

    pub fn can_move(&self) -> bool {
        self.movement_used < self.movement_budget
    }

    pub fn movement_remaining(&self) -> f32 {
        (self.movement_budget - self.movement_used).max(0.0)
    }

    pub fn tick(&mut self, terrain: &Terrain, dt: f32) {
        if !self.alive {
            return;
        }

        let was_on_ground = self.on_ground;

        self.vy += GRAVITY * dt;
        if self.vy > 600.0 {
            self.vy = 600.0;
        }

        self.x += self.vx * dt;
        self.y += self.vy * dt;

        let friction = if self.on_ground {
            GROUND_FRICTION
        } else {
            AIR_FRICTION
        };
        self.vx *= friction;
        if self.vx.abs() < 0.5 {
            self.vx = 0.0;
        }

        self.on_ground = false;
        let r = WORM_RADIUS;
        for &offset in &[-r * 0.4, 0.0, r * 0.4] {
            let cx = (self.x + offset) as i32;
            let foot_y = (self.y + r) as i32;
            if terrain.is_solid(cx, foot_y) {
                let mut sy = foot_y - 1;
                while sy > (self.y as i32 - r as i32) && terrain.is_solid(cx, sy) {
                    sy -= 1;
                }
                let new_y = (sy + 1) as f32 - r;
                if new_y < self.y + 2.0 {
                    self.y = new_y;
                    self.on_ground = true;

                    if !was_on_ground && self.vy > 0.0 {
                        let fall_dist = self.y - self.fall_start_y;
                        if fall_dist > FALL_DAMAGE_THRESHOLD {
                            let dmg = ((fall_dist - FALL_DAMAGE_THRESHOLD) * FALL_DAMAGE_FACTOR) as i32;
                            if dmg > 0 {
                                self.take_damage(dmg);
                            }
                        }
                    }
                    self.vy = 0.0;
                    break;
                }
            }
        }

        if !self.on_ground && was_on_ground && self.vy >= 0.0 {
            self.fall_start_y = self.y;
        }

        let head_y = (self.y - r * 0.5) as i32;
        let body_y = self.y as i32;
        if terrain.is_solid((self.x - r) as i32, body_y)
            || terrain.is_solid((self.x - r) as i32, head_y)
        {
            self.x = (self.x - r).ceil() + r + 1.0;
            if self.vx < 0.0 {
                self.vx = 0.0;
            }
        }
        if terrain.is_solid((self.x + r) as i32, body_y)
            || terrain.is_solid((self.x + r) as i32, head_y)
        {
            self.x = (self.x + r).floor() - r - 1.0;
            if self.vx > 0.0 {
                self.vx = 0.0;
            }
        }

        self.x = self.x.clamp(r, terrain.width as f32 - r);

        // Check for lava — instant death!
        let mut touching_lava = false;
        for &offset_x in &[-r * 0.5, 0.0, r * 0.5] {
            for &offset_y in &[-r * 0.5, 0.0, r * 0.5] {
                let check_x = (self.x + offset_x) as i32;
                let check_y = (self.y + offset_y) as i32;
                if terrain.get(check_x, check_y) == crate::terrain::LAVA {
                    touching_lava = true;
                    break;
                }
            }
            if touching_lava {
                break;
            }
        }
        if touching_lava {
            self.alive = false;
            self.health = 0;
        }

        // Drowning — instant death when touching water!
        if self.y + r > crate::terrain::WATER_LEVEL {
            self.alive = false;
            self.health = 0;
        }

        if self.health <= 0 {
            self.alive = false;
            self.health = 0;
        }

        if self.damage_timer > 0.0 {
            self.damage_timer -= dt;
        }
    }

    pub fn take_damage(&mut self, amount: i32) {
        self.health = (self.health - amount).max(0);
        self.last_damage = amount;
        self.damage_timer = 2.0;
        if self.health <= 0 {
            self.alive = false;
        }
    }

    pub fn apply_knockback(&mut self, dx: f32, dy: f32) {
        self.vx += dx;
        self.vy += dy;
        self.on_ground = false;
    }

    pub fn is_settled(&self) -> bool {
        !self.alive || (self.on_ground && self.vx.abs() < 2.0 && self.vy.abs() < 2.0)
    }
}

pub fn walk(worm: &mut Worm, terrain: &Terrain, dir: f32) {
    if !worm.on_ground || !worm.alive {
        return;
    }
    
    // Check if movement budget is exhausted
    if !worm.can_move() {
        return;
    }
    
    worm.facing = dir;
    let step = dir * WALK_SPEED * (1.0 / 60.0);
    let movement_distance = step.abs();
    
    // Check if this step would exceed budget
    if worm.movement_used + movement_distance > worm.movement_budget {
        // Only move the remaining budget amount
        let remaining = worm.movement_budget - worm.movement_used;
        let limited_step = dir.signum() * remaining;
        if remaining < 0.5 {
            return; // Too small to move
        }
        worm.movement_used = worm.movement_budget;
        let new_x = worm.x + limited_step;
        worm.x = new_x;
        return;
    }
    
    let old_x = worm.x;
    let new_x = worm.x + step;
    let r = WORM_RADIUS;
    let nx = new_x as i32;
    let foot_y = (worm.y + r) as i32;

    if terrain.is_solid((new_x + dir * r) as i32, worm.y as i32) {
        for climb in 1..=MAX_CLIMB {
            let test_y = worm.y as i32 - climb;
            if !terrain.is_solid((new_x + dir * r) as i32, test_y)
                && !terrain.is_solid(nx, test_y - r as i32)
            {
                worm.x = new_x;
                worm.y = test_y as f32;
                // Track the actual distance moved
                worm.movement_used += (worm.x - old_x).abs();
                return;
            }
        }
        return;
    }

    for drop in 0..=10 {
        if terrain.is_solid(nx, foot_y + drop) {
            worm.x = new_x;
            worm.y = (foot_y + drop - 1) as f32 - r;
            // Track the actual distance moved
            worm.movement_used += (worm.x - old_x).abs();
            return;
        }
    }

    worm.x = new_x;
    // Track the actual distance moved
    worm.movement_used += (worm.x - old_x).abs();
}

pub fn jump(worm: &mut Worm) {
    if !worm.on_ground || !worm.alive {
        return;
    }
    worm.vy = JUMP_VEL;
    worm.vx += worm.facing * 60.0;
    worm.on_ground = false;
    worm.fall_start_y = worm.y;
}

pub fn backflip(worm: &mut Worm) {
    if !worm.on_ground || !worm.alive {
        return;
    }
    worm.vy = JUMP_VEL - 60.0;
    worm.vx += -worm.facing * 120.0;
    worm.on_ground = false;
    worm.fall_start_y = worm.y;
}
