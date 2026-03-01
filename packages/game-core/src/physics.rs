use crate::terrain::Terrain;

pub const BALL_RADIUS: f32 = 8.0;
const GRAVITY: f32 = 480.0;
const WALK_SPEED: f32 = 115.0;         // Slightly snappier
const JUMP_VEL: f32 = -320.0;          // More air — bigger, floatier jump
const JUMP_HORIZONTAL_BOOST: f32 = 75.0; // Extra run on jump
const MAX_CLIMB: i32 = 12;             // Can hop up over small terrain bumps
const GROUND_FRICTION: f32 = 0.80;
const AIR_FRICTION: f32 = 0.985;       // Slightly less air drag
const AIR_CONTROL_ACCEL: f32 = 420.0; // Horizontal acceleration applied per-frame while airborne
const MAX_AIR_SPEED: f32 = 105.0;     // Max horizontal speed from air control
const FALL_DAMAGE_THRESHOLD: f32 = 120.0;
const FALL_DAMAGE_FACTOR: f32 = 0.25;
const WALL_IMPACT_THRESHOLD: f32 = 250.0; // min speed to take wall-impact damage
const WALL_IMPACT_FACTOR: f32 = 0.04;    // damage per unit of excess speed
const MOVEMENT_BUDGET: f32 = 170.0;   // Slightly more movement per turn
const COYOTE_TIME: f32 = 0.15;        // Grace window after walking off edge
const JUMP_BUFFER_TIME: f32 = 0.12;   // Jump pressed just before landing

pub const TEAM_COLORS: [(f32, f32, f32); 4] = [
    (0.85, 0.25, 0.25),
    (0.25, 0.50, 0.90),
    (0.25, 0.75, 0.35),
    (0.90, 0.75, 0.20),
];

pub struct Ball {
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
    /// Grace period after walking off an edge — still allows jumping
    pub coyote_timer: f32,
    /// Queued jump — executes on next landing if within window
    pub jump_buffer: f32,
    /// X position when a player-initiated jump started; used to charge actual
    /// horizontal travel distance to the movement budget on landing.
    pub jump_start_x: Option<f32>,
}

impl Ball {
    pub fn new(x: f32, y: f32, team: u32, name: String) -> Self {
        Ball {
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
            coyote_timer: 0.0,
            jump_buffer: 0.0,
            jump_start_x: None,
        }
    }

    pub fn reset_movement_budget(&mut self) {
        self.movement_used = 0.0;
        self.movement_budget = MOVEMENT_BUDGET;
        self.jump_start_x = None;
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
        let r = BALL_RADIUS;
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
                        // Charge the actual horizontal distance traveled during a
                        // player-initiated jump to the movement budget.
                        if let Some(jx) = self.jump_start_x.take() {
                            let dist = (self.x - jx).abs();
                            self.movement_used = (self.movement_used + dist).min(self.movement_budget);
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

        // Coyote time: grant a grace window to jump after walking off an edge
        if self.on_ground {
            self.coyote_timer = 0.0;
        } else if was_on_ground && self.vy >= 0.0 {
            // Just walked off edge (vy positive = not a jump); start coyote window
            self.coyote_timer = COYOTE_TIME;
        } else if self.coyote_timer > 0.0 {
            self.coyote_timer -= dt;
        }

        // Jump buffer: if jump was pressed in air, fire when we land
        if self.jump_buffer > 0.0 {
            self.jump_buffer -= dt;
            if self.on_ground && self.jump_buffer > 0.0 {
                self.vy = JUMP_VEL;
                self.vx += self.facing * JUMP_HORIZONTAL_BOOST;
                self.on_ground = false;
                self.fall_start_y = self.y;
                self.jump_buffer = 0.0;
                self.coyote_timer = 0.0;
                // Record jump origin for movement-budget tracking on landing
                self.jump_start_x = Some(self.x);
            }
        }

        let head_y = (self.y - r * 0.5) as i32;
        let body_y = self.y as i32;
        if terrain.is_solid((self.x - r) as i32, body_y)
            || terrain.is_solid((self.x - r) as i32, head_y)
        {
            self.x = (self.x - r).ceil() + r + 1.0;
            if self.vx < 0.0 {
                let impact = self.vx.abs();
                if impact > WALL_IMPACT_THRESHOLD {
                    let dmg = ((impact - WALL_IMPACT_THRESHOLD) * WALL_IMPACT_FACTOR) as i32;
                    if dmg > 0 { self.take_damage(dmg); }
                }
                self.vx = 0.0;
            }
        }
        if terrain.is_solid((self.x + r) as i32, body_y)
            || terrain.is_solid((self.x + r) as i32, head_y)
        {
            self.x = (self.x + r).floor() - r - 1.0;
            if self.vx > 0.0 {
                let impact = self.vx.abs();
                if impact > WALL_IMPACT_THRESHOLD {
                    let dmg = ((impact - WALL_IMPACT_THRESHOLD) * WALL_IMPACT_FACTOR) as i32;
                    if dmg > 0 { self.take_damage(dmg); }
                }
                self.vx = 0.0;
            }
        }

        self.x = self.x.clamp(r, terrain.width as f32 - r);

        // Ceiling collision — stop upward movement when the top of the ball hits terrain
        let ball_top_y = (self.y - r) as i32;
        for &offset in &[-r * 0.4, 0.0, r * 0.4] {
            if terrain.is_solid((self.x + offset) as i32, ball_top_y) {
                self.y = ball_top_y as f32 + r + 1.0;
                if self.vy < 0.0 {
                    self.vy = 0.0;
                }
                // Any queued jump that was blocked by a ceiling should not
                // charge distance from the aborted jump.
                self.jump_start_x = None;
                break;
            }
        }

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

pub fn walk(ball: &mut Ball, terrain: &Terrain, dir: f32) {
    if !ball.alive {
        return;
    }

    // Check if movement budget is exhausted
    if !ball.can_move() {
        return;
    }

    ball.facing = dir;

    // ── Air control ───────────────────────────────────────────────────────
    // While airborne, nudge horizontal velocity instead of snapping position.
    // This lets the player steer jumps to reach higher spots.
    // Budget is NOT drained here — horizontal travel is charged on landing via
    // jump_start_x so that only actual distance counts against the budget.
    if !ball.on_ground {
        let push = dir * AIR_CONTROL_ACCEL * (1.0 / 60.0);
        // Only push if we haven't hit the air-speed cap in that direction
        if (dir > 0.0 && ball.vx < MAX_AIR_SPEED) || (dir < 0.0 && ball.vx > -MAX_AIR_SPEED) {
            ball.vx = (ball.vx + push).clamp(-MAX_AIR_SPEED, MAX_AIR_SPEED);
        }
        return;
    }

    // ── Ground walk ───────────────────────────────────────────────────────
    let step = dir * WALK_SPEED * (1.0 / 60.0);
    let movement_distance = step.abs();

    // Clamp step to remaining budget (may be less than a full step).
    let actual_step = if ball.movement_used + movement_distance > ball.movement_budget {
        let remaining = ball.movement_budget - ball.movement_used;
        if remaining < 0.5 {
            return;
        }
        dir.signum() * remaining
    } else {
        step
    };

    let old_x = ball.x;
    let new_x = ball.x + actual_step;
    let r = BALL_RADIUS;
    let nx = new_x as i32;
    let foot_y = (ball.y + r) as i32;

    // Check for walls in the direction of movement.
    // Match the EXACT same heights tick() uses for wall resolution so that
    // walk() and tick() NEVER disagree — disagreement causes the ball to
    // oscillate (walk moves, tick pushes back) draining budget with no motion.
    //   tick() checks: body_y = ball.y as i32,  head_y = (ball.y - r*0.5) as i32
    // We additionally check foot-level (just above the ground cell) for small bumps.
    let forward_x = (new_x + dir * r) as i32;
    let head_y    = (ball.y - r * 0.5) as i32;           // matches tick()
    let body_y    = ball.y as i32;                        // matches tick()
    let mid_y     = (ball.y + r * 0.4) as i32;
    let blocked_head   = terrain.is_solid(forward_x, head_y);
    let blocked_body   = terrain.is_solid(forward_x, body_y);
    let blocked_mid    = terrain.is_solid(forward_x, mid_y);
    let blocked_foot   = terrain.is_solid(forward_x, foot_y - 1);

    if blocked_head || blocked_body || blocked_mid || blocked_foot {
        // Try to step up over small terrain bumps.
        // Require clearance at both ball centre AND ball top so tick()'s
        // head-wall check won't immediately push the ball back after the step.
        let ball_top_offset = r as i32;
        for climb in 1..=MAX_CLIMB {
            let test_y     = ball.y as i32 - climb;
            let test_top   = test_y - ball_top_offset;
            let test_head  = test_y - (r * 0.5) as i32;   // head_y at climbed position
            if !terrain.is_solid(forward_x, test_y)
                && !terrain.is_solid(forward_x, test_top)
                && !terrain.is_solid(forward_x, test_head)
            {
                ball.x = new_x;
                ball.y = test_y as f32;
                ball.movement_used += (ball.x - old_x).abs();
                return;
            }
        }
        // Truly blocked — don't move and don't charge budget.
        return;
    }

    for drop in 0..=10 {
        if terrain.is_solid(nx, foot_y + drop) {
            ball.x = new_x;
            // Place foot ON the solid cell (consistent with tick()'s snap logic)
            ball.y = (foot_y + drop) as f32 - r;
            // Track the actual distance moved
            ball.movement_used += (ball.x - old_x).abs();
            return;
        }
    }

    ball.x = new_x;
    // Track the actual distance moved
    ball.movement_used += (ball.x - old_x).abs();
}

pub fn jump(ball: &mut Ball) {
    if !ball.alive {
        return;
    }
    if ball.on_ground || ball.coyote_timer > 0.0 {
        // Normal jump or coyote-time jump
        // Record origin so the horizontal distance is charged to movement
        // budget when the ball lands (see tick() landing handler).
        ball.jump_start_x = Some(ball.x);
        ball.vy = JUMP_VEL;
        ball.vx += ball.facing * JUMP_HORIZONTAL_BOOST;
        ball.on_ground = false;
        ball.coyote_timer = 0.0;
        ball.jump_buffer = 0.0;
        ball.fall_start_y = ball.y;
    } else {
        // In the air — buffer the jump for when we land
        ball.jump_buffer = JUMP_BUFFER_TIME;
    }
}

pub fn backflip(ball: &mut Ball) {
    if !ball.alive {
        return;
    }
    if ball.on_ground || ball.coyote_timer > 0.0 {
        // Record origin so horizontal distance is charged on landing
        ball.jump_start_x = Some(ball.x);
        ball.vy = JUMP_VEL - 70.0;          // Extra height for backflip
        ball.vx += -ball.facing * 130.0;
        ball.on_ground = false;
        ball.coyote_timer = 0.0;
        ball.jump_buffer = 0.0;
        ball.fall_start_y = ball.y;
    }
}
