mod camera;
mod hud;
mod network;
mod physics;
mod projectile;
mod special_weapons;
mod state;
mod terrain;
mod weapons;

use camera::GameCamera;
use macroquad::prelude::*;
use physics::{Worm, WORM_RADIUS};
use projectile::{Projectile, ClusterBomblet, ShotgunPellet};
use special_weapons::{AirstrikeDroplet, UziBullet, PlacedExplosive, AirstrikeType};
use state::Phase;
use terrain::Terrain;
use weapons::Weapon;

const TURN_TIME: f32 = 45.0;
const TURN_END_DELAY: f32 = 1.5;
const SETTLE_TIMEOUT: f32 = 5.0;
const CHARGE_SPEED: f32 = 55.0;

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn console_log(ptr: *const u8);
}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    color: Color,
    size: f32,
}

struct Game {
    terrain: Terrain,
    terrain_image: Image,
    terrain_texture: Texture2D,
    terrain_dirty: bool,

    worms: Vec<Worm>,
    current_worm: usize,

    phase: Phase,
    turn_timer: f32,
    settle_timer: f32,
    turn_end_timer: f32,
    retreat_timer: f32,

    selected_weapon: Weapon,
    aim_angle: f32,
    charge_power: f32,
    charging: bool,
    has_fired: bool,

    proj: Option<Projectile>,
    cluster_bomblets: Vec<ClusterBomblet>,
    shotgun_pellets: Vec<ShotgunPellet>,
    airstrike_droplets: Vec<AirstrikeDroplet>,
    uzi_bullets: Vec<UziBullet>,
    placed_explosives: Vec<PlacedExplosive>,
    teleport_mode: bool,
    baseball_bat_mode: bool,

    cam: GameCamera,
    panning: bool,
    last_mouse: (f32, f32),

    wind: f32,
    rng_state: u32,

    particles: Vec<Particle>,
    winning_team: Option<u32>,
    
    weapon_menu_open: bool,
    weapon_menu_scroll: f32,

    net: network::NetworkState,
    /// Current turn index from server (which player's turn it is: 0, 1, etc.)
    current_turn_index: usize,
    /// Number of teams/players in the game
    num_teams: usize,
    /// When we receive turn_advanced during ProjectileFlying/Settling, apply when settling ends
    pending_turn_sync: Option<usize>,
    /// Deferred restart seed
    restart_seed: Option<u32>,
    /// Throttle for network movement messages (seconds since last send)
    last_movement_send: f32,
    /// Throttle for network aim messages (seconds since last send)
    last_aim_send: f32,
    /// Track last logged turn state to reduce console spam
    last_logged_turn_state: (usize, Option<usize>),
    /// Track which worm index was last used per team for round-robin rotation
    last_worm_per_team: Vec<Option<usize>>,
}

impl Game {
    fn new(seed: u32) -> Self {
        // Default to 2 teams for offline play
        Self::new_with_teams(seed, 2)
    }

    fn new_with_teams(seed: u32, num_teams: usize) -> Self {
        let t = terrain::generate(seed);
        let img = t.bake_image();
        let tex = Texture2D::from_image(&img);
        tex.set_filter(FilterMode::Nearest);

        let team_names = [
            ["Spike", "Tank", "Blaze"],
            ["Frost", "Storm", "Shadow"],
            ["Viper", "Ghost", "Flash"],
            ["Rex", "Duke", "Scout"],
        ];
        let worms_per_team: usize = 3;
        let total = num_teams * worms_per_team;
        let mut worms = Vec::new();

        // Spawn worms within the playable land area only
        let mut positions: Vec<f32> = Vec::new();
        for i in 0..total {
            let x = terrain::LAND_START_X + (i + 1) as f32 * terrain::PLAYABLE_LAND_WIDTH / (total + 1) as f32;
            positions.push(x);
        }
        let mut interleaved: Vec<(usize, usize)> = Vec::new();
        for wi in 0..worms_per_team {
            for ti in 0..num_teams {
                interleaved.push((ti, wi));
            }
        }

        for (slot, &(ti, wi)) in interleaved.iter().enumerate() {
            let x = positions[slot];
            
            // Find safe spawn position (avoid lava)
            let mut spawn_y = None;
            let mut search_x = x as i32;
            
            // Try original position first
            if let Some(surface_y) = t.find_surface_y(search_x) {
                // Check if there's lava at or near where the worm would spawn
                let worm_y = surface_y - (WORM_RADIUS as i32) - 2;
                let mut is_safe = true;
                
                // Check area around spawn position for lava
                for dy in -2..3 {
                    for dx in -2..3 {
                        let check_x = search_x + dx;
                        let check_y = worm_y + dy;
                        if t.get(check_x, check_y) == terrain::LAVA {
                            is_safe = false;
                            break;
                        }
                    }
                    if !is_safe { break; }
                }
                
                if is_safe {
                    spawn_y = Some(surface_y as f32 - WORM_RADIUS - 2.0);
                }
            }
            
            // If original position has lava, search nearby for safe spot
            if spawn_y.is_none() {
                for offset in 1..50 {
                    for dir in [-1, 1] {
                        let test_x = (x as i32 + offset * dir).max(terrain::LAND_START_X as i32).min(terrain::LAND_END_X as i32);
                        if let Some(surface_y) = t.find_surface_y(test_x) {
                            let worm_y = surface_y - (WORM_RADIUS as i32) - 2;
                            let mut is_safe = true;
                            
                            // Check area around spawn position for lava
                            for dy in -2..3 {
                                for dx in -2..3 {
                                    let check_x = test_x + dx;
                                    let check_y = worm_y + dy;
                                    if t.get(check_x, check_y) == terrain::LAVA {
                                        is_safe = false;
                                        break;
                                    }
                                }
                                if !is_safe { break; }
                            }
                            
                            if is_safe {
                                spawn_y = Some(surface_y as f32 - WORM_RADIUS - 2.0);
                                search_x = test_x;
                                break;
                            }
                        }
                    }
                    if spawn_y.is_some() {
                        break;
                    }
                }
            }
            
            let y = spawn_y.unwrap_or(400.0);
            let spawn_x = search_x as f32;
            let name = team_names[ti % team_names.len()][wi % 3].to_string();
            worms.push(Worm::new(spawn_x, y, ti as u32, name));
        }

        // Center camera on playable land area
        let cam_x = terrain::LAND_START_X + terrain::PLAYABLE_LAND_WIDTH / 2.0;
        let cam_y = t.height as f32 * 0.45;
        let mut rng = seed;
        rng = lcg(rng);
        let wind = ((rng >> 16) as f32 / 65536.0 - 0.5) * 6.0;

        Game {
            terrain: t,
            terrain_image: img,
            terrain_texture: tex,
            terrain_dirty: false,
            worms,
            current_worm: 0,
            phase: Phase::Aiming,
            turn_timer: TURN_TIME,
            settle_timer: 0.0,
            turn_end_timer: 0.0,
            selected_weapon: Weapon::Bazooka,
            aim_angle: -0.5,
            charge_power: 0.0,
            charging: false,
            has_fired: false,
            proj: None,
            cluster_bomblets: Vec::new(),
            shotgun_pellets: Vec::new(),
            airstrike_droplets: Vec::new(),
            uzi_bullets: Vec::new(),
            placed_explosives: Vec::new(),
            teleport_mode: false,
            baseball_bat_mode: false,
            cam: GameCamera::new(cam_x, cam_y),
            panning: false,
            last_mouse: (0.0, 0.0),
            wind,
            rng_state: rng,
            particles: Vec::new(),
            winning_team: None,
            weapon_menu_open: false,
            weapon_menu_scroll: 0.0,
            net: network::NetworkState::new(),
            current_turn_index: 0,
            num_teams,
            pending_turn_sync: None,
            restart_seed: None,
            last_movement_send: 0.0,
            last_aim_send: 0.0,
            last_logged_turn_state: (0, None),
            retreat_timer: 0.0,
            last_worm_per_team: {
                // Pre-record that worm 0 (team 0's first worm) is the initial
                // current_worm, so the next sync_to_player_turn(0) knows to
                // advance to the second worm instead of re-picking the first.
                let mut v = vec![None; num_teams];
                if num_teams > 0 {
                    v[0] = Some(0); // current_worm starts at 0 which belongs to team 0
                }
                v
            },
        }
    }

    /// Returns true if it's currently our turn (or if offline/native)
    fn is_my_turn(&self) -> bool {
        // In WASM builds, NEVER allow control until server tells us who we are
        #[cfg(target_arch = "wasm32")]
        {
            if !self.net.connected {
                return false; // Block all input until server identity arrives
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !self.net.connected {
                return true; // Native/offline mode - allow all control
            }
        }
        
        let my_player_index = match self.net.my_player_index {
            Some(i) => i,
            None => {
                return false; // Multiplayer but no player index - block all control
            }
        };
        
        // Check if it's our turn
        self.current_turn_index == my_player_index
    }
    
    /// Find the first alive worm for a given team/player
    fn find_worm_for_player(&self, player_index: usize) -> Option<usize> {
        let team = player_index as u32;
        self.worms
            .iter()
            .enumerate()
            .find(|(_, w)| w.alive && w.team == team)
            .map(|(idx, _)| idx)
    }

    fn handle_input(&mut self) {
        if let Some(seed) = self.restart_seed.take() {
            // Restart with same team count
            *self = Game::new_with_teams(seed, self.num_teams);
            return;
        }
        
        // Log turn state changes (not every frame)
        if self.net.connected {
            let current_state = (self.current_turn_index, self.net.my_player_index);
            if current_state != self.last_logged_turn_state {
                #[cfg(target_arch = "wasm32")]
                {
                    let is_my_turn = self.is_my_turn();
                    let msg = format!("[TURN] current_turn_index={}, my_player_index={:?}, is_my_turn={}\0", 
                        self.current_turn_index, self.net.my_player_index, is_my_turn);
                    unsafe { console_log(msg.as_ptr()); }
                }
                self.last_logged_turn_state = current_state;
            }
        }
        
        let (mx, my) = mouse_position();

        if is_mouse_button_pressed(MouseButton::Right) || is_mouse_button_pressed(MouseButton::Middle) {
            self.panning = true;
            self.last_mouse = (mx, my);
        }
        if is_mouse_button_released(MouseButton::Right) || is_mouse_button_released(MouseButton::Middle) {
            self.panning = false;
        }
        if self.panning {
            let dx = mx - self.last_mouse.0;
            let dy = my - self.last_mouse.1;
            self.cam.pan(dx, dy);
            self.last_mouse = (mx, my);
        }

        // Mouse wheel: scroll weapon menu when open, otherwise camera zoom
        let wheel = mouse_wheel().1;
        if wheel.abs() > 0.1 {
            if self.weapon_menu_open {
                let layout = hud::WeaponMenuLayout::new();
                self.weapon_menu_scroll = (self.weapon_menu_scroll - wheel * 30.0)
                    .clamp(0.0, layout.max_scroll());
            } else {
                let factor = if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 };
                self.cam.zoom_by(factor);
            }
        }

        if self.phase == Phase::GameOver {
            if is_key_pressed(KeyCode::R) {
                let seed = lcg(self.rng_state);
                if self.net.connected {
                    let msg = format!("{{\"type\":\"restart\",\"seed\":{}}}", seed);
                    self.net.send_message(&msg);
                } else {
                    self.restart_seed = Some(seed);
                }
            }
            return;
        }

        // During Retreat or ProjectileFlying phase: allow movement for local player's worm
        if self.phase == Phase::Retreat || self.phase == Phase::ProjectileFlying {
            // During Retreat the active player moves the worm that just fired,
            // which is already stored in current_worm.
            // During ProjectileFlying the non-active player can also move; use
            // find_worm_for_player so they control one of their own worms.
            let worm_idx_opt = if self.phase == Phase::Retreat {
                // Always the worm that fired (current_worm)
                Some(self.current_worm)
            } else if self.net.connected {
                self.net.my_player_index.and_then(|pi| self.find_worm_for_player(pi))
            } else {
                Some(self.current_worm)
            };

            if let Some(wi) = worm_idx_opt {
                if wi < self.worms.len() && self.worms[wi].alive {
                    let worm = &mut self.worms[wi];
                    let can_move = worm.can_move();

                    let current_time = get_time() as f32;
                    let should_send_movement = self.net.connected && (current_time - self.last_movement_send > 0.1);

                    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
                        physics::walk(worm, &self.terrain, -1.0);
                        if should_send_movement {
                            let msg = r#"{"type":"input","input":"{\"Walk\":{\"dir\":-1.0}}"}"#;
                            self.net.send_message(msg);
                            self.last_movement_send = current_time;
                        }
                    }
                    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
                        physics::walk(worm, &self.terrain, 1.0);
                        if should_send_movement {
                            let msg = r#"{"type":"input","input":"{\"Walk\":{\"dir\":1.0}}"}"#;
                            self.net.send_message(msg);
                            self.last_movement_send = current_time;
                        }
                    }

                    if can_move {
                        if is_key_pressed(KeyCode::W) || is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Space) {
                            physics::jump(worm);
                            worm.movement_used += 20.0;
                            if self.net.connected {
                                let msg = r#"{"type":"input","input":"{\"Jump\":{}}"}"#;
                                self.net.send_message(msg);
                            }
                        }
                        if is_key_pressed(KeyCode::S) || is_key_pressed(KeyCode::Down) {
                            physics::backflip(worm);
                            worm.movement_used += 30.0;
                            if self.net.connected {
                                let msg = r#"{"type":"input","input":"{\"Backflip\":{}}"}"#;
                                self.net.send_message(msg);
                            }
                        }
                    }
                }
            }
            return; // No weapon/aim/firing during retreat or projectile flying
        }

        // CRITICAL: Block all GAME input (not camera) if not our turn in multiplayer
        if !self.is_my_turn() {
            return;
        }

        

        if !self.phase.allows_input() {
            if self.charging {
                self.charging = false;
            }
            return;
        }

        // Toggle weapon menu with Tab or Q (only on your turn)
        if self.is_my_turn() && (is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::Q)) {
            self.weapon_menu_open = !self.weapon_menu_open;
            if !self.weapon_menu_open {
                self.weapon_menu_scroll = 0.0;
            }
        }
        
        // Toggle weapon menu with mouse click on button (only on your turn)
        if self.is_my_turn() && !self.weapon_menu_open && is_mouse_button_pressed(MouseButton::Left) {
            let button = hud::get_weapon_button_bounds();
            let (mx, my) = mouse_position();
            if mx >= button.0 && mx <= button.0 + button.2 
                && my >= button.1 && my <= button.1 + button.3 {
                self.weapon_menu_open = true;
            }
        }
        
        // Close menu with ESC
        if self.weapon_menu_open && is_key_pressed(KeyCode::Escape) {
            self.weapon_menu_open = false;
            self.weapon_menu_scroll = 0.0;
        }
        
        // Handle weapon menu clicks (only on your turn)
        if self.is_my_turn() && self.weapon_menu_open && is_mouse_button_pressed(MouseButton::Left) {
            let layout = hud::WeaponMenuLayout::new();
            
            // Organize weapons by category (same as hud.rs)
            use weapons::WeaponCategory;
            let all_weapons = Weapon::all();
            let mut by_category: std::collections::HashMap<WeaponCategory, Vec<&Weapon>> = std::collections::HashMap::new();
            for w in all_weapons {
                by_category.entry(w.category()).or_insert_with(Vec::new).push(w);
            }
            
            let categories = [
                WeaponCategory::Explosives,
                WeaponCategory::Ballistics,
                WeaponCategory::Special,
                WeaponCategory::Utilities,
            ];
            
            let mut current_y = layout.content_y - self.weapon_menu_scroll;
            
            // Only accept clicks within the content viewport
            let content_top = layout.content_y;
            let content_bottom = layout.content_y + layout.content_h;
            
            for cat in &categories {
                if let Some(weapons) = by_category.get(cat) {
                    // Skip category header
                    current_y += layout.cat_header_h + layout.item_padding;
                    
                    // Check weapon item clicks
                    for w in weapons {
                        let item_y = current_y;
                        let item_x = layout.menu_x + layout.padding;
                        let item_w = layout.menu_w - layout.padding * 2.0;
                        
                        // Only register clicks within the visible content area
                        if item_y + layout.item_h > content_top && item_y < content_bottom {
                            if mx >= item_x && mx <= item_x + item_w && my >= item_y && my <= item_y + layout.item_h {
                                self.selected_weapon = **w;
                                self.weapon_menu_open = false;
                                self.weapon_menu_scroll = 0.0;
                                return;
                            }
                        }
                        
                        current_y += layout.item_h + layout.item_padding;
                    }
                    
                    current_y += 4.0; // Space between categories
                }
            }
            
            // Close menu if clicking outside
            if mx < layout.menu_x || mx > layout.menu_x + layout.menu_w || my < layout.menu_y || my > layout.menu_y + layout.menu_h {
                self.weapon_menu_open = false;
                self.weapon_menu_scroll = 0.0;
            }
            return;
        }

        // Only update aim angle on your turn
        if self.is_my_turn() {
            if let Some(worm) = self.worms.get(self.current_worm) {
                if worm.alive {
                    let (wx, wy) = (worm.x, worm.y);
                    let (world_mx, world_my) = self.cam.screen_to_world(mx, my);
                    let dx = world_mx - wx;
                    let dy = world_my - wy;
                    let new_angle = dy.atan2(dx);
                    
                    // Only update if angle changed significantly
                    if (new_angle - self.aim_angle).abs() > 0.01 {
                        self.aim_angle = new_angle;
                        
                        // Broadcast aim angle periodically
                        let current_time = get_time() as f32;
                        if self.net.connected && (current_time - self.last_aim_send > 0.05) {
                            let msg = format!("{{\"type\":\"aim\",\"aim\":{}}}", self.aim_angle);
                            self.net.send_message(&msg);
                            self.last_aim_send = current_time;
                        }
                    }
                }
            }
        }

        // Only allow movement if it's the player's turn and phase allows it
        if self.is_my_turn() && self.phase.allows_movement() && self.current_worm < self.worms.len() && self.worms[self.current_worm].alive && !self.weapon_menu_open {
            let worm = &mut self.worms[self.current_worm];
            let can_move = worm.can_move();
            
            let current_time = get_time() as f32;
            let should_send_movement = self.net.connected && (current_time - self.last_movement_send > 0.1);
            
            if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
                physics::walk(worm, &self.terrain, -1.0);
                if should_send_movement {
                    let msg = r#"{"type":"input","input":"{\"Walk\":{\"dir\":-1.0}}"}"#;
                    self.net.send_message(msg);
                    self.last_movement_send = current_time;
                }
            }
            if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
                physics::walk(worm, &self.terrain, 1.0);
                if should_send_movement {
                    let msg = r#"{"type":"input","input":"{\"Walk\":{\"dir\":1.0}}"}"#;
                    self.net.send_message(msg);
                    self.last_movement_send = current_time;
                }
            }
            
            // Only allow jumping if there's movement budget
            if can_move {
                if is_key_pressed(KeyCode::W) || is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Space) {
                    physics::jump(worm);
                    worm.movement_used += 20.0; // Jumping costs movement
                    if self.net.connected {
                        let msg = r#"{"type":"input","input":"{\"Jump\":{}}"}"#;
                        self.net.send_message(msg);
                    }
                }
                if is_key_pressed(KeyCode::S) || is_key_pressed(KeyCode::Down) {
                    physics::backflip(worm);
                    worm.movement_used += 30.0; // Backflip costs more
                    if self.net.connected {
                        let msg = r#"{"type":"input","input":"{\"Backflip\":{}}"}"#;
                        self.net.send_message(msg);
                    }
                }
            }
        }

        if is_mouse_button_pressed(MouseButton::Left) && !self.has_fired && self.is_my_turn() && self.phase.allows_input() && !self.weapon_menu_open {
            // Handle Teleport mode
            if self.teleport_mode {
                let (mx, my) = mouse_position();
                let world_pos = self.cam.to_macroquad().screen_to_world(vec2(mx, my));
                
                let idx = self.current_worm;
                if idx < self.worms.len() && self.worms[idx].alive {
                    // Check if the destination is valid (not inside solid terrain)
                    let target_x = world_pos.x.clamp(0.0, self.terrain.width as f32);
                    let target_y = world_pos.y.clamp(0.0, self.terrain.height as f32);
                    
                    // Simple teleport - place worm at clicked location
                    self.worms[idx].x = target_x;
                    self.worms[idx].y = target_y;
                    self.worms[idx].vx = 0.0;
                    self.worms[idx].vy = 0.0;
                    
                    self.teleport_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                }
            }
            // Handle Baseball Bat mode
            else if self.baseball_bat_mode {
                let idx = self.current_worm;
                if idx < self.worms.len() && self.worms[idx].alive {
                    // Store necessary values to avoid borrowing issues
                    let worm_x = self.worms[idx].x;
                    let worm_y = self.worms[idx].y;
                    let bat_range = 60.0;
                    let angle = self.aim_angle;
                    let knockback_power = 15.0;
                    
                    // Find worms in range and knock them back
                    for i in 0..self.worms.len() {
                        if i == idx { continue; }
                        if !self.worms[i].alive { continue; }
                        
                        let dx = self.worms[i].x - worm_x;
                        let dy = self.worms[i].y - worm_y;
                        let dist = (dx*dx + dy*dy).sqrt();
                        
                        if dist < bat_range {
                            // Apply knockback based on aim direction
                            self.worms[i].vx += angle.cos() * knockback_power;
                            self.worms[i].vy += angle.sin() * knockback_power;
                            
                            // Deal minimal damage
                            self.worms[i].health = self.worms[i].health.saturating_sub(10);
                            if self.worms[i].health == 0 {
                                self.worms[i].alive = false;
                            }
                        }
                    }
                    
                    self.baseball_bat_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                }
            }
            // Normal weapon charging
            else {
                self.charging = true;
                self.charge_power = 0.0;
                self.phase = Phase::Charging;
            }
        }
        if self.charging {
            self.charge_power = (self.charge_power + CHARGE_SPEED * get_frame_time()).min(100.0);
            if is_mouse_button_released(MouseButton::Left) || self.charge_power >= 100.0 {
                self.fire();
            }
        }

        // Only allow ending turn if it's your turn
        if self.is_my_turn() && is_key_pressed(KeyCode::E) {
            self.end_turn();
        }
    }

    fn fire(&mut self) {
        self.charging = false;
        if self.has_fired {
            return;
        }
        let idx = self.current_worm;
        if idx >= self.worms.len() || !self.worms[idx].alive {
            return;
        }

        let power = self.charge_power.clamp(0.0, 100.0);
        let angle = self.aim_angle;
        let weapon = self.selected_weapon;

        self.do_fire(idx, angle, power, weapon);
        
        // Don't set has_fired for Baseball Bat and Teleport - they need a second click
        if weapon != Weapon::BaseballBat && weapon != Weapon::Teleport {
            self.has_fired = true;
        }
        self.charge_power = 0.0;
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("[FIRE] Fired {:?}, has_fired={}\0", weapon, self.has_fired);
            unsafe { console_log(msg.as_ptr()); }
        }

        if self.net.connected {
            let angle_deg = angle.to_degrees();
            let weapon_name = weapon.name();
            let input_json = format!(
                r#"{{"Fire":{{"weapon":"{}","angle_deg":{},"power_percent":{}}}}}"#,
                weapon_name, angle_deg, power
            );
            let mut escaped = String::new();
            for c in input_json.chars() {
                match c {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    _ => escaped.push(c),
                }
            }
            let msg = format!(r#"{{"type":"input","input":"{}"}}"#, escaped);
            self.net.send_message(&msg);
        }
    }

    fn do_fire(&mut self, idx: usize, angle: f32, power: f32, weapon: Weapon) {
        if idx >= self.worms.len() || !self.worms[idx].alive {
            return;
        }
        let worm = &self.worms[idx];
        let offset = WORM_RADIUS + 4.0;
        let sx = worm.x + angle.cos() * offset;
        let sy = worm.y + angle.sin() * offset;
        
        match weapon {
            // Shotgun - spread of pellets
            Weapon::Shotgun => {
                self.shotgun_pellets.clear();
                let pellet_count = 6;
                let spread = 0.25;
                let base_speed = power * 12.0;
                
                for i in 0..pellet_count {
                    let offset_angle = (i as f32 - (pellet_count as f32 / 2.0)) * (spread / pellet_count as f32);
                    let pellet_angle = angle + offset_angle;
                    let speed_variance = 0.9 + (i as f32 * 0.05) % 0.2;
                    let speed = base_speed * speed_variance;
                    
                    self.shotgun_pellets.push(ShotgunPellet {
                        x: sx,
                        y: sy,
                        vx: pellet_angle.cos() * speed,
                        vy: pellet_angle.sin() * speed,
                        alive: true,
                        damage: 10,
                    });
                }
                self.phase = Phase::ProjectileFlying;
            },
            
            // Uzi - rapid fire 10 bullets with spread
            Weapon::Uzi => {
                self.uzi_bullets.clear();
                let bullet_count = 10;
                let spread = 0.15;
                let base_speed = power * 15.0;
                
                for i in 0..bullet_count {
                    let offset_angle = (rand::gen_range(0.0, 1.0) - 0.5) * spread;
                    let bullet_angle = angle + offset_angle;
                    let speed = base_speed * (0.95 + rand::gen_range(0.0, 0.1));
                    
                    self.uzi_bullets.push(UziBullet {
                        x: sx,
                        y: sy,
                        vx: bullet_angle.cos() * speed,
                        vy: bullet_angle.sin() * speed,
                        alive: true,
                    });
                }
                self.phase = Phase::ProjectileFlying;
            },
            
            // Airstrike - 5 droplets from above
            Weapon::Airstrike => {
                self.airstrike_droplets.clear();
                let worm_x = worm.x;
                let spacing = 80.0;
                
                for i in 0..5 {
                    let x = worm_x + (i as f32 - 2.0) * spacing;
                    self.airstrike_droplets.push(AirstrikeDroplet {
                        x,
                        y: -50.0,
                        vy: 0.0,
                        alive: true,
                        weapon_type: AirstrikeType::Explosive,
                    });
                }
                self.phase = Phase::ProjectileFlying;
            },
            
            // Napalm Strike - 7 droplets in a line
            Weapon::NapalmStrike => {
                self.airstrike_droplets.clear();
                let worm_x = worm.x;
                let spacing = 60.0;
                
                for i in 0..7 {
                    let x = worm_x + (i as f32 - 3.0) * spacing;
                    self.airstrike_droplets.push(AirstrikeDroplet {
                        x,
                        y: -50.0,
                        vy: 0.0,
                        alive: true,
                        weapon_type: AirstrikeType::Napalm,
                    });
                }
                self.phase = Phase::ProjectileFlying;
            },
            
            // Dynamite - place at worm position
            Weapon::Dynamite => {
                self.placed_explosives.push(PlacedExplosive {
                    x: worm.x,
                    y: worm.y + WORM_RADIUS - 2.0,
                    fuse: 5.0,
                    alive: true,
                    radius: 45.0,
                    damage: 50,
                });
                self.phase = Phase::ProjectileFlying;
            },
            
            // Baseball Bat - enter melee mode
            Weapon::BaseballBat => {
                self.baseball_bat_mode = true;
                // Stay in aiming phase, will handle click for bat swing
            },

            // Build Wall - place wood in a small rectangle at the impact point
            Weapon::BuildWall => {
                // Build dimensions scale slightly with power
                let build_w = (40.0 + power * 0.6).round() as i32; // width in pixels
                let build_h = (14.0 + (power * 0.08)).round() as i32; // height in pixels
                let cx = sx as i32;
                let cy = sy as i32;
                // Place wall centered horizontally, and with base at cy (so it rises upward)
                let half_w = build_w / 2;
                for dx in -half_w..=half_w {
                    for dy in 0..build_h {
                        let x = cx + dx;
                        let y = cy - dy;
                        self.terrain.set(x, y, terrain::WOOD);
                    }
                }
                self.terrain_dirty = true;
                // Apply a very small settle so physics can run
                self.phase = Phase::Settling;
                self.settle_timer = 0.0;
                self.has_fired = true;
            },

            // Teleport - enter teleport mode
            Weapon::Teleport => {
                self.teleport_mode = true;
                // Stay in aiming phase, will handle click for teleport
            },
            
            // All other weapons use regular projectile
            _ => {
                let proj = Projectile::new(sx, sy, angle, power, weapon);
                self.proj = Some(proj);
                self.phase = Phase::ProjectileFlying;
            }
        }
    }

    fn end_turn(&mut self) {
        if self.phase == Phase::GameOver {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("[TURN] end_turn called, is_my_turn={}, connected={}\0", self.is_my_turn(), self.net.connected);
            unsafe { console_log(msg.as_ptr()); }
        }
        if self.net.connected && self.is_my_turn() {
            // Send worm state snapshot so opponent syncs positions/health
            self.send_worm_state();
            self.net.send_message(r#"{"type":"end_turn"}"#);
        }
        self.phase = Phase::TurnEnd;
        self.turn_end_timer = TURN_END_DELAY;
        self.charging = false;
        self.charge_power = 0.0;
    }

    /// Send a snapshot of all worm positions/health to sync with other players
    fn send_worm_state(&self) {
        let mut worm_data = String::from("[");
        for (i, w) in self.worms.iter().enumerate() {
            if i > 0 { worm_data.push(','); }
            worm_data.push_str(&format!(
                "{{\"x\":{},\"y\":{},\"vx\":{},\"vy\":{},\"hp\":{},\"alive\":{}}}",
                w.x, w.y, w.vx, w.vy, w.health, w.alive
            ));
        }
        worm_data.push(']');
        let msg = format!("{{\"type\":\"worm_state\",\"worms\":{}}}", worm_data);
        self.net.send_message(&msg);
    }

    /// Send the full terrain damage log to the server for persistence across reconnects
    fn send_terrain_damages(&self) {
        let log = &self.terrain.damage_log;
        if log.is_empty() {
            return;
        }
        let mut arr = String::from("[");
        for (i, &(cx, cy, r)) in log.iter().enumerate() {
            if i > 0 { arr.push(','); }
            arr.push_str(&format!("[{},{},{}]", cx, cy, r));
        }
        arr.push(']');
        let msg = format!("{{\"type\":\"terrain_damages\",\"log\":{}}}", arr);
        self.net.send_message(&msg);
    }

    /// Apply terrain damage log received from server on reconnect
    fn apply_terrain_sync(&mut self, msg: &str) {
        // Format: {"type":"terrain_sync","log":[[cx,cy,r],[cx,cy,r],...]}
        let key = "\"log\":[";
        let start = match msg.find(key) {
            Some(i) => i + key.len(),
            None => return,
        };
        // Find the matching closing bracket
        let mut depth = 1i32;
        let mut end = start;
        for (i, ch) in msg[start..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let content = &msg[start..end];
        if content.is_empty() {
            return;
        }

        // Parse array of [cx,cy,r] triples
        let mut damages: Vec<(i32, i32, i32)> = Vec::new();
        let mut pos = 0;
        while pos < content.len() {
            // Find next sub-array
            let sub_start = match content[pos..].find('[') {
                Some(i) => pos + i + 1,
                None => break,
            };
            let sub_end = match content[sub_start..].find(']') {
                Some(i) => sub_start + i,
                None => break,
            };
            let triple = &content[sub_start..sub_end];
            let nums: Vec<&str> = triple.split(',').collect();
            if nums.len() == 3 {
                if let (Ok(cx), Ok(cy), Ok(r)) = (
                    nums[0].trim().parse::<i32>(),
                    nums[1].trim().parse::<i32>(),
                    nums[2].trim().parse::<i32>(),
                ) {
                    damages.push((cx, cy, r));
                }
            }
            pos = sub_end + 1;
        }

        if !damages.is_empty() {
            #[cfg(target_arch = "wasm32")]
            {
                let debug_msg = format!("[SYNC] Replaying {} terrain damage events\0", damages.len());
                unsafe { console_log(debug_msg.as_ptr()); }
            }
            self.terrain.replay_damage(&damages);
            self.terrain_dirty = true;
        }
    }

    /// Apply worm state snapshot from the active player to sync positions/health
    fn apply_worm_state(&mut self, msg: &str) {
        // Parse the worms array from the message
        // Format: {"type":"worm_state","worms":[{"x":..,"y":..,"vx":..,"vy":..,"hp":..,"alive":..}, ...]}
        let worms_key = "\"worms\":[";
        let start = match msg.find(worms_key) {
            Some(i) => i + worms_key.len(),
            None => return,
        };
        // Find the closing bracket
        let end = match msg[start..].rfind(']') {
            Some(i) => start + i,
            None => return,
        };
        let array_content = &msg[start..end];
        
        // Split by "},{" to get individual worm objects
        let mut worm_idx = 0;
        let mut pos = 0;
        while pos < array_content.len() && worm_idx < self.worms.len() {
            // Find the next object boundaries
            let obj_start = match array_content[pos..].find('{') {
                Some(i) => pos + i,
                None => break,
            };
            let obj_end = match array_content[obj_start..].find('}') {
                Some(i) => obj_start + i + 1,
                None => break,
            };
            let obj = &array_content[obj_start..obj_end];
            
            // Parse fields
            if let Some(x) = parse_json_number(obj, "x") {
                self.worms[worm_idx].x = x as f32;
            }
            if let Some(y) = parse_json_number(obj, "y") {
                self.worms[worm_idx].y = y as f32;
            }
            if let Some(vx) = parse_json_number(obj, "vx") {
                self.worms[worm_idx].vx = vx as f32;
            }
            if let Some(vy) = parse_json_number(obj, "vy") {
                self.worms[worm_idx].vy = vy as f32;
            }
            if let Some(hp) = parse_json_number(obj, "hp") {
                self.worms[worm_idx].health = hp as i32;
            }
            // Parse alive (boolean)
            if obj.contains("\"alive\":true") {
                self.worms[worm_idx].alive = true;
            } else if obj.contains("\"alive\":false") {
                self.worms[worm_idx].alive = false;
            }
            
            worm_idx += 1;
            pos = obj_end;
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            let debug_msg = format!("[SYNC] Applied worm_state for {} worms\0", worm_idx);
            unsafe { console_log(debug_msg.as_ptr()); }
        }
    }

    /// Local turn advancement (offline or fallback)
    fn advance_turn(&mut self) {
        if self.check_game_over() {
            return;
        }

        let n = self.worms.len();
        if n == 0 {
            return;
        }
        let start = self.current_worm;
        let mut next = (start + 1) % n;
        loop {
            if self.worms[next].alive {
                break;
            }
            next = (next + 1) % n;
            if next == start {
                break;
            }
        }
        self.current_worm = next;
        self.reset_turn_state();
    }

    /// Sync to a player's turn from the worker. Finds the next alive worm belonging to
    /// the given team (player_index) using round-robin so all worms get a turn.
    fn sync_to_player_turn(&mut self, player_index: usize) {
        if self.check_game_over() {
            return;
        }
        let team = player_index as u32;
        let n = self.worms.len();
        if n == 0 {
            return;
        }

        // Ensure last_worm_per_team has enough entries
        while self.last_worm_per_team.len() <= player_index {
            self.last_worm_per_team.push(None);
        }

        // Collect indices of all alive worms on this team
        let team_worms: Vec<usize> = (0..n)
            .filter(|&i| self.worms[i].alive && self.worms[i].team == team)
            .collect();

        if team_worms.is_empty() {
            // Fallback: just find any alive worm
            for i in 0..n {
                if self.worms[i].alive {
                    self.current_worm = i;
                    self.reset_turn_state();
                    return;
                }
            }
            return;
        }

        // Pick the next worm in rotation after the last one used
        let last = self.last_worm_per_team[player_index];
        let chosen = match last {
            Some(prev) => {
                // Find the team worm that comes after prev in the global worm list
                let mut pick = team_worms[0]; // default to first
                for &wi in &team_worms {
                    if wi > prev {
                        pick = wi;
                        break;
                    }
                }
                // If none found after prev, wrap around to first
                if pick <= prev {
                    pick = team_worms[0];
                }
                pick
            }
            None => team_worms[0],
        };

        self.last_worm_per_team[player_index] = Some(chosen);
        self.current_worm = chosen;
        #[cfg(target_arch = "wasm32")]
        {
            let worm_name = if chosen < self.worms.len() { self.worms[chosen].name.as_str() } else { "?" };
            let debug_msg = format!("[TURN] sync_to_player_turn({}): chose worm {} '{}', team_worms={:?}, last={:?}\0",
                player_index, chosen, worm_name, team_worms, last);
            unsafe { console_log(debug_msg.as_ptr()); }
        }
        self.reset_turn_state();
    }

    fn reset_turn_state(&mut self) {
        self.phase = Phase::Aiming;
        self.turn_timer = TURN_TIME;
        self.has_fired = false;
        self.retreat_timer = 0.0;
        self.charging = false;
        self.charge_power = 0.0;
        
        // Reset movement budget for the current worm
        if self.current_worm < self.worms.len() {
            self.worms[self.current_worm].reset_movement_budget();
            self.aim_angle = if self.worms[self.current_worm].facing > 0.0 {
                -0.3
            } else {
                std::f32::consts::PI + 0.3
            };
        }
        
        self.rng_state = lcg(self.rng_state);
        self.wind = ((self.rng_state >> 16) as f32 / 65536.0 - 0.5) * 6.0;
    }

    fn check_game_over(&mut self) -> bool {
        let mut alive_teams: Vec<u32> = Vec::new();
        for w in &self.worms {
            if w.alive && !alive_teams.contains(&w.team) {
                alive_teams.push(w.team);
            }
        }
        if alive_teams.len() <= 1 {
            self.phase = Phase::GameOver;
            self.winning_team = alive_teams.first().copied();
            return true;
        }
        false
    }

    fn apply_network_messages(&mut self) {
        for msg in self.net.poll_messages() {
            if msg.contains("\"type\":\"init\"") || msg.contains("\"type\": \"init\"") {
                #[cfg(target_arch = "wasm32")]
                {
                    let debug_msg = format!("[apply_network] Received init message\0");
                    unsafe { console_log(debug_msg.as_ptr()); }
                }
                
                self.net.connected = true;
                if let Some(idx) = parse_json_number(&msg, "myPlayerIndex") {
                    self.net.my_player_index = Some(idx as usize);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let debug_msg = format!("[apply_network] Set my_player_index={}\0", idx as usize);
                        unsafe { console_log(debug_msg.as_ptr()); }
                    }
                }
                if let Some(names_str) = parse_json_string(&msg, "playerNames") {
                    self.net.player_names = names_str.split(',').map(|s| s.to_string()).collect();
                }
                if let Some(bots_str) = parse_json_string(&msg, "playerBots") {
                    self.net.player_is_bot = bots_str.split(',').map(|s| s == "1").collect();
                }
                
                // Count number of players to determine team count
                let num_players = self.net.player_names.len().max(self.net.player_is_bot.len());
                
                // Use rngSeed from server to regenerate terrain with same seed for all players
                if let Some(seed) = parse_json_number(&msg, "rngSeed") {
                    let seed_u32 = seed as u32;
                    #[cfg(target_arch = "wasm32")]
                    {
                        let debug_msg = format!("[apply_network] Got seed={}, num_players={}, current rng_state={}\0", seed_u32, num_players, self.rng_state);
                        unsafe { console_log(debug_msg.as_ptr()); }
                    }
                    if seed_u32 != self.rng_state || num_players != self.num_teams {
                        // Regenerate terrain with proper seed and team count
                        *self = Game::new_with_teams(seed_u32, num_players);
                        // Restore network state that was just set
                        self.net.connected = true;
                        self.net.my_player_index = parse_json_number(&msg, "myPlayerIndex").map(|i| i as usize);
                        #[cfg(target_arch = "wasm32")]
                        {
                            let debug_msg = format!("[apply_network] After regenerate with {} teams, restored my_player_index={:?}\0", num_players, self.net.my_player_index);
                            unsafe { console_log(debug_msg.as_ptr()); }
                        }
                        if let Some(names_str) = parse_json_string(&msg, "playerNames") {
                            self.net.player_names = names_str.split(',').map(|s| s.to_string()).collect();
                        }
                        if let Some(bots_str) = parse_json_string(&msg, "playerBots") {
                            self.net.player_is_bot = bots_str.split(',').map(|s| s == "1").collect();
                        }
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"turn_advanced\"") || msg.contains("\"type\": \"turn_advanced\"") {
                if let Some(player_index) = parse_turn_index_from_message(&msg) {
                    // Always update our stored turn index
                    self.current_turn_index = player_index;
                    match self.phase {
                        Phase::Aiming | Phase::Charging | Phase::TurnEnd | Phase::Retreat => {
                            self.sync_to_player_turn(player_index);
                        }
                        _ => {
                            self.pending_turn_sync = Some(player_index);
                        }
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"restart\"") || msg.contains("\"type\": \"restart\"") {
                // Server requested a restart for all clients
                if let Some(seed) = parse_json_number(&msg, "seed") {
                    self.restart_seed = Some(seed as u32);
                }
                continue;
            }
            if msg.contains("\"type\":\"state\"") || msg.contains("\"type\": \"state\"") {
                // Handle state message to sync with server's current turn
                if let Some(current_turn_index) = parse_state_turn_index(&msg) {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let debug_msg = format!("[apply_network] State message: current_turn_index={}, ours={}\0", current_turn_index, self.current_turn_index);
                        unsafe { console_log(debug_msg.as_ptr()); }
                    }
                    // Only process if the turn index actually changed — echoed state
                    // messages (e.g. after firing) carry the same index and must NOT
                    // reset the phase/pending_turn_sync.
                    if current_turn_index != self.current_turn_index {
                        self.current_turn_index = current_turn_index;
                        match self.phase {
                            Phase::Aiming | Phase::Charging | Phase::TurnEnd => {
                                self.sync_to_player_turn(current_turn_index);
                            }
                            _ => {
                                self.pending_turn_sync = Some(current_turn_index);
                            }
                        }
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"input\"") || msg.contains("\"type\": \"input\"") {
                // If we have a pending turn sync, apply it first so we fire on the right worm
                if let Some(player_idx) = self.pending_turn_sync.take() {
                    self.sync_to_player_turn(player_idx);
                }

                if let Some((player_index, input_str)) = parse_input_message(&msg) {
                    // Skip our own input echo
                    if self.net.my_player_index == Some(player_index) {
                        continue;
                    }
                    
                    // Use current_worm for the active turn player (already set by
                    // sync_to_player_turn with correct rotation). Only fall back to
                    // find_worm_for_player for non-turn messages.
                    let worm_idx_opt = if player_index == self.current_turn_index {
                        Some(self.current_worm)
                    } else {
                        self.find_worm_for_player(player_index)
                    };
                    if let Some(worm_idx) = worm_idx_opt {
                        // Parse and apply different input types
                        if let Some((angle_rad, power, weapon)) = parse_fire_input(&input_str) {
                            self.do_fire(worm_idx, angle_rad, power, weapon);
                            self.has_fired = true;
                        } else if let Some(dir) = parse_walk_input(&input_str) {
                            if worm_idx < self.worms.len() {
                                physics::walk(&mut self.worms[worm_idx], &self.terrain, dir);
                            }
                        } else if input_str.contains("\"Jump\"") || input_str.contains("Jump") {
                            if worm_idx < self.worms.len() {
                                physics::jump(&mut self.worms[worm_idx]);
                                self.worms[worm_idx].movement_used += 20.0;
                            }
                        } else if input_str.contains("\"Backflip\"") || input_str.contains("Backflip") {
                            if worm_idx < self.worms.len() {
                                physics::backflip(&mut self.worms[worm_idx]);
                                self.worms[worm_idx].movement_used += 30.0;
                            }
                        }
                    }
                }  
                continue;
            }
            if msg.contains("\"type\":\"worm_state\"") || msg.contains("\"type\": \"worm_state\"") {
                // Apply worm positions/health from the active player
                self.apply_worm_state(&msg);
                continue;
            }
            if msg.contains("\"type\":\"terrain_sync\"") || msg.contains("\"type\": \"terrain_sync\"") {
                // Replay terrain damage events received from server on reconnect
                self.apply_terrain_sync(&msg);
                continue;
            }
            if msg.contains("\"type\":\"aim\"") || msg.contains("\"type\": \"aim\"") {
                // Handle aim angle updates from other players
                if let Some((player_index, aim_angle)) = parse_aim_message(&msg) {
                    // Skip our own aim echo
                    if self.net.my_player_index == Some(player_index) {
                        continue;
                    }
                    
                    // Find the worm for this player and update their local aim
                    if let Some(worm_idx) = self.find_worm_for_player(player_index) {
                        if worm_idx == self.current_worm {
                            self.aim_angle = aim_angle;
                        }
                    }
                }
                continue;
            }
        }
    }

    fn update(&mut self, dt: f32) {
        let dt = dt.min(1.0 / 30.0);

        self.apply_network_messages();

        for p in &mut self.particles {
            p.vy += 200.0 * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);

        match self.phase {
            Phase::Aiming | Phase::Charging => {
                self.turn_timer -= dt;
                if self.turn_timer <= 0.0 && self.is_my_turn() {
                    self.end_turn();
                }
                for w in &mut self.worms {
                    w.tick(&self.terrain, dt);
                }
                // If the current worm died (walked into water/lava), end turn immediately
                if self.current_worm < self.worms.len() && !self.worms[self.current_worm].alive {
                    self.end_turn();
                }
                if self.current_worm < self.worms.len() {
                    let w = &self.worms[self.current_worm];
                    if w.alive {
                        self.cam.follow(w.x, w.y - 30.0, 4.0, dt);
                    }
                }
            }
            Phase::ProjectileFlying => {
                for w in &mut self.worms {
                    w.tick(&self.terrain, dt);
                }
                let mut explosion_opt = None;
                let mut proj_died = false;
                
                // Handle regular projectile
                if let Some(ref mut proj) = self.proj {
                    let (explosion, bomblets) = proj.tick(&mut self.terrain, &mut self.worms, self.wind, dt);
                    self.cam.follow(proj.x, proj.y, 8.0, dt);
                    explosion_opt = explosion;
                    proj_died = !proj.alive;
                    
                    // Spawn cluster bomblets if any
                    if !bomblets.is_empty() {
                        self.cluster_bomblets.extend(bomblets);
                    }
                } 
                
                // Handle shotgun pellets
                if !self.shotgun_pellets.is_empty() {
                    let mut any_active = false;
                    for pellet in &mut self.shotgun_pellets {
                        if pellet.alive {
                            let hit = pellet.tick(&mut self.terrain, &mut self.worms, dt);
                            if hit {
                                self.terrain_dirty = true;
                            }
                            if pellet.alive {
                                any_active = true;
                                self.cam.follow(pellet.x, pellet.y, 6.0, dt);
                            }
                        }
                    }
                    if !any_active {
                        self.shotgun_pellets.clear();
                    }
                }
                
                // Handle Uzi bullets
                if !self.uzi_bullets.is_empty() {
                    let mut any_active = false;
                    for bullet in &mut self.uzi_bullets {
                        if bullet.alive {
                            let hit = bullet.tick(&mut self.terrain, &mut self.worms, dt);
                            if hit {
                                self.terrain_dirty = true;
                            }
                            if bullet.alive {
                                any_active = true;
                                self.cam.follow(bullet.x, bullet.y, 5.0, dt);
                            }
                        }
                    }
                    if !any_active {
                        self.uzi_bullets.clear();
                    }
                }
                
                // Handle airstrike droplets
                if !self.airstrike_droplets.is_empty() {
                    let mut any_active = false;
                    let mut explosions = Vec::new();
                    for droplet in &mut self.airstrike_droplets {
                        if droplet.alive {
                            if let Some(exp) = droplet.tick(&mut self.terrain, &mut self.worms, dt) {
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                            if droplet.alive {
                                any_active = true;
                                self.cam.follow(droplet.x, droplet.y, 7.0, dt);
                            }
                        }
                    }
                    for exp in explosions {
                        self.spawn_explosion_particles(&exp);
                    }
                    if !any_active {
                        self.airstrike_droplets.clear();
                    }
                }
                
                // Handle placed explosives
                if !self.placed_explosives.is_empty() {
                    let mut explosions = Vec::new();
                    for explosive in &mut self.placed_explosives {
                        if explosive.tick(dt) {
                            let exp = explosive.explode(&mut self.terrain, &mut self.worms);
                            explosions.push(exp);
                            self.terrain_dirty = true;
                        }
                    }
                    self.placed_explosives.retain(|e| e.alive);
                    
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                }
                
                // Handle cluster bomblets
                if !self.cluster_bomblets.is_empty() {
                    let mut explosions = Vec::new();
                    let mut any_active = false;
                    for bomblet in &mut self.cluster_bomblets {
                        if bomblet.alive {
                            if let Some(exp) = bomblet.tick(&mut self.terrain, &mut self.worms, dt) {
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                            if bomblet.alive {
                                any_active = true;
                                self.cam.follow(bomblet.x, bomblet.y, 6.0, dt);
                            }
                        }
                    }
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                    if !any_active {
                        self.cluster_bomblets.clear();
                    }
                }
                
                // Check if all projectiles/effects are done
                let all_done = self.proj.is_none() 
                    && self.shotgun_pellets.is_empty() 
                    && self.uzi_bullets.is_empty()
                    && self.airstrike_droplets.is_empty()
                    && self.placed_explosives.is_empty()
                    && self.cluster_bomblets.is_empty();
                
                if proj_died || explosion_opt.is_some() {
                    self.proj = None;
                }
                
                if all_done {
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    // All players send terrain damage log so the worker always
                    // has the latest cumulative state for reconnect sync.
                    if self.net.connected {
                        self.send_terrain_damages();
                        // Only the active player sends authoritative worm positions.
                        if self.is_my_turn() {
                            self.send_worm_state();
                        }
                    }
                }
                
                if let Some(ref exp) = explosion_opt {
                    self.spawn_explosion_particles(exp);
                    if !exp.is_water {
                        self.terrain_dirty = true;
                    }
                }
            }
            Phase::Settling => {
                self.settle_timer += dt;
                for w in &mut self.worms {
                    w.tick(&self.terrain, dt);
                }
                let all_settled = self.worms.iter().all(|w| w.is_settled());
                if all_settled || self.settle_timer > SETTLE_TIMEOUT {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let msg = format!("[PHASE] Settling done. pending_sync={:?}, connected={}, is_my_turn={}, has_fired={}\0",
                            self.pending_turn_sync, self.net.connected, self.is_my_turn(), self.has_fired);
                        unsafe { console_log(msg.as_ptr()); }
                    }
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        self.sync_to_player_turn(player_idx);
                    } else if self.has_fired && self.is_my_turn() {
                        // Send fresh worm state after settling
                        if self.net.connected {
                            self.send_worm_state();
                        }
                        // Active player: enter retreat phase - 5 seconds to move
                        self.phase = Phase::Retreat;
                        self.retreat_timer = 5.0;
                        // Reset movement budget for retreat
                        if self.current_worm < self.worms.len() {
                            self.worms[self.current_worm].reset_movement_budget();
                        }
                    } else if !self.net.connected {
                        self.end_turn();
                    } else if self.is_my_turn() {
                        self.end_turn();
                    } else {
                        // Not our turn in multiplayer: enter TurnEnd and wait for worker
                        self.phase = Phase::TurnEnd;
                        self.turn_end_timer = TURN_END_DELAY;
                    }
                }
                if self.current_worm < self.worms.len() {
                    let w = &self.worms[self.current_worm];
                    self.cam.follow(w.x, w.y - 30.0, 3.0, dt);
                }
            }
            Phase::Retreat => {
                self.retreat_timer -= dt;
                // Tick worm physics (gravity, etc) during retreat
                for w in &mut self.worms {
                    w.tick(&self.terrain, dt);
                }
                // If current worm died during retreat (fell in water/lava), end turn now
                if self.current_worm < self.worms.len() && !self.worms[self.current_worm].alive {
                    self.retreat_timer = 0.0; // Force turn end
                }
                // Follow current worm with camera
                if self.current_worm < self.worms.len() {
                    let w = &self.worms[self.current_worm];
                    if w.alive {
                        self.cam.follow(w.x, w.y - 30.0, 4.0, dt);
                    }
                }
                // When retreat time expires, end the turn
                if self.retreat_timer <= 0.0 {
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        self.sync_to_player_turn(player_idx);
                    } else {
                        self.end_turn();
                    }
                }
            }
            Phase::TurnEnd => {
                self.turn_end_timer -= dt;
                for w in &mut self.worms {
                    w.tick(&self.terrain, dt);
                }
                if self.turn_end_timer <= 0.0 {
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        self.sync_to_player_turn(player_idx);
                    } else if !self.net.connected {
                        self.advance_turn();
                    }
                    // When connected without pending sync, keep waiting for turn_advanced
                }
            }
            Phase::GameOver => {}
        }

        self.cam
            .clamp_to_world(terrain::PLAYABLE_LAND_WIDTH, self.terrain.height as f32);

        if self.terrain_dirty {
            self.terrain_image = self.terrain.bake_image();
            // Recreate texture entirely instead of updating in-place to avoid WebGL state issues
            // The old texture will be automatically dropped and cleaned up by Rust
            self.terrain_texture = Texture2D::from_image(&self.terrain_image);
            self.terrain_texture.set_filter(FilterMode::Nearest);
            self.terrain_dirty = false;
        }
    }

    fn spawn_explosion_particles(&mut self, exp: &projectile::Explosion) {
        // Scale particle count, speed, size and lifetime based on explosion radius
        let scale = (exp.radius / 25.0).max(1.0); // 25px = baseline
        let count = if exp.is_water {
            20
        } else {
            (35.0 * scale).min(300.0) as usize // Cap at 300 particles
        };
        let speed_mult = scale.min(5.0); // Cap speed scaling
        let size_mult = scale.min(4.0);  // Cap size scaling
        let life_mult = scale.min(3.0);  // Cap life scaling
        for i in 0..count {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU
                + (self.rng_state as f32 * 0.01).sin() * 0.5;
            self.rng_state = lcg(self.rng_state);
            let speed = (60.0 + (self.rng_state >> 16) as f32 / 65536.0 * 180.0) * speed_mult;
            self.rng_state = lcg(self.rng_state);
            let color = if exp.is_water {
                Color::new(0.3, 0.5, 1.0, 0.9)
            } else {
                let v = (self.rng_state >> 16) as f32 / 65536.0;
                if v < 0.3 {
                    Color::new(1.0, 0.4, 0.1, 1.0)
                } else if v < 0.6 {
                    Color::new(1.0, 0.7, 0.2, 1.0)
                } else {
                    Color::new(0.5, 0.3, 0.15, 0.9)
                }
            };
            self.rng_state = lcg(self.rng_state);
            // Spawn particles across the blast area, not just from center
            let spawn_offset = if scale > 2.0 {
                let r = (self.rng_state >> 16) as f32 / 65536.0 * exp.radius * 0.4;
                self.rng_state = lcg(self.rng_state);
                let a = (self.rng_state >> 16) as f32 / 65536.0 * std::f32::consts::TAU;
                self.rng_state = lcg(self.rng_state);
                (a.cos() * r, a.sin() * r)
            } else {
                (0.0, 0.0)
            };
            self.particles.push(Particle {
                x: exp.x + spawn_offset.0,
                y: exp.y + spawn_offset.1,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed - 80.0 * speed_mult,
                life: (0.5 + (self.rng_state >> 16) as f32 / 65536.0 * 0.8) * life_mult,
                color,
                size: (2.0 + (self.rng_state >> 24) as f32 / 256.0 * 3.0) * size_mult,
            });
            self.rng_state = lcg(self.rng_state);
        }
    }

    /// Get a display label for whose turn it is
    fn turn_owner_label(&self) -> String {
        if !self.net.connected {
            return String::new();
        }
        let team = self.worms.get(self.current_worm).map(|w| w.team as usize).unwrap_or(0);
        
        // Safely get player name, handling out-of-bounds and empty cases
        let name = if team < self.net.player_names.len() {
            let n = &self.net.player_names[team];
            if n.is_empty() {
                format!("Player {}", team + 1)
            } else {
                n.clone()
            }
        } else {
            format!("Player {}", team + 1)
        };
        
        let is_bot = self.net.player_is_bot.get(team).copied().unwrap_or(false);
        if is_bot {
            format!("{} (Bot)", name)
        } else {
            name
        }
    }

    fn draw(&self) {
        clear_background(Color::new(0.40, 0.65, 0.88, 1.0));

        let mq_cam = self.cam.to_macroquad();
        set_camera(&mq_cam);

        self.draw_sky();

        draw_texture_ex(
            &self.terrain_texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(
                    self.terrain.width as f32,
                    self.terrain.height as f32,
                )),
                ..Default::default()
            },
        );

        self.draw_water();

        hud::draw_worm_world(&self.worms, self.current_worm);

        if let Some(ref proj) = self.proj {
            for (i, &(tx, ty)) in proj.trail.iter().enumerate() {
                let alpha = i as f32 / proj.trail.len().max(1) as f32 * 0.6;
                draw_circle(tx, ty, 2.0, Color::new(1.0, 0.6, 0.2, alpha));
            }
            draw_circle(proj.x, proj.y, 4.0, Color::new(1.0, 0.3, 0.1, 1.0));
            draw_circle(proj.x, proj.y, 2.5, Color::new(1.0, 0.8, 0.3, 1.0));
        }
        
        // Draw cluster bomblets
        for bomblet in &self.cluster_bomblets {
            if bomblet.alive {
                draw_circle(bomblet.x, bomblet.y, 3.0, Color::new(0.9, 0.5, 0.1, 1.0));
                draw_circle(bomblet.x, bomblet.y, 1.8, Color::new(1.0, 0.9, 0.4, 1.0));
            }
        }
        
        // Draw shotgun pellets
        for pellet in &self.shotgun_pellets {
            if pellet.alive {
                draw_circle(pellet.x, pellet.y, 2.5, Color::new(0.7, 0.6, 0.5, 0.9));
                draw_circle(pellet.x, pellet.y, 1.2, Color::new(0.9, 0.8, 0.6, 1.0));
            }
        }
        
        // Draw Uzi bullets
        for bullet in &self.uzi_bullets {
            if bullet.alive {
                draw_circle(bullet.x, bullet.y, 2.0, Color::new(0.8, 0.8, 0.2, 1.0));
                draw_circle(bullet.x, bullet.y, 1.0, Color::new(1.0, 1.0, 0.5, 1.0));
            }
        }
        
        // Draw airstrike droplets
        for droplet in &self.airstrike_droplets {
            if droplet.alive {
                use special_weapons::AirstrikeType;
                let (outer_color, inner_color) = match droplet.weapon_type {
                    AirstrikeType::Explosive => (
                        Color::new(0.9, 0.3, 0.1, 0.9),
                        Color::new(1.0, 0.6, 0.2, 1.0)
                    ),
                    AirstrikeType::Napalm => (
                        Color::new(1.0, 0.5, 0.0, 0.9),
                        Color::new(1.0, 0.8, 0.0, 1.0)
                    ),
                };
                draw_circle(droplet.x, droplet.y, 4.0, outer_color);
                draw_circle(droplet.x, droplet.y, 2.0, inner_color);
            }
        }
        
        // Draw placed explosives
        for explosive in &self.placed_explosives {
            if explosive.alive {
                // Pulsing effect based on remaining fuse time
                let pulse = (explosive.fuse * 3.0).sin() * 0.3 + 0.7;
                let size = 8.0 * pulse;
                
                // Draw dynamite stick
                draw_rectangle(
                    explosive.x - 3.0,
                    explosive.y - 6.0,
                    6.0,
                    12.0,
                    Color::new(0.9, 0.2, 0.1, 1.0)
                );
                
                // Draw fuse indicator (gets brighter as it gets closer to exploding)
                let fuse_brightness = 1.0 - (explosive.fuse / 5.0).min(1.0); // Assume max fuse is 5.0s
                draw_circle(
                    explosive.x,
                    explosive.y - 8.0,
                    size * 0.5,
                    Color::new(1.0, fuse_brightness * 0.8, 0.0, 0.8)
                );
            }
        }

        if (self.phase == Phase::Aiming || self.phase == Phase::Charging) && !self.has_fired && self.is_my_turn() {
            self.draw_aim();
        }

        for p in &self.particles {
            let alpha = (p.life / 1.0).min(1.0);
            let c = Color::new(p.color.r, p.color.g, p.color.b, p.color.a * alpha);
            draw_circle(p.x, p.y, p.size, c);
        }

        set_default_camera();

        let is_my_turn = self.is_my_turn();
        let turn_owner = self.turn_owner_label();
        hud::draw_hud(
            &self.worms,
            self.current_worm,
            self.phase,
            self.selected_weapon,
            self.charge_power,
            if self.phase == Phase::Retreat { self.retreat_timer } else { self.turn_timer },
            self.wind,
            self.winning_team,
            is_my_turn,
            &turn_owner,
            self.weapon_menu_open,
            self.weapon_menu_scroll,
        );
    }

    fn draw_sky(&self) {
        let vw = self.cam.visible_width();
        let vh = self.cam.visible_height();
        let left = self.cam.x - vw / 2.0;
        let top = self.cam.y - vh / 2.0;
        let steps = 8;
        let step_h = vh / steps as f32;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let r = 0.35 + t * 0.15;
            let g = 0.58 + t * 0.12;
            let b = 0.88 - t * 0.10;
            draw_rectangle(left, top + i as f32 * step_h, vw, step_h + 1.0, Color::new(r, g, b, 1.0));
        }
    }

    fn draw_water(&self) {
        let water_y = terrain::WATER_LEVEL;
        let t = get_time() as f32;
        let vw = self.cam.visible_width();
        let left = self.cam.x - vw / 2.0;

        // Draw full-width water at the bottom
        draw_rectangle(
            left - 10.0,
            water_y,
            vw + 20.0,
            self.terrain.height as f32 - water_y + 100.0,
            Color::new(0.08, 0.25, 0.55, 0.85),
        );

        // Draw waves across the top of the water
        let wave_h = 3.0;
        let wave_len = 40.0;
        let steps = (vw / 4.0) as i32 + 2;
        for i in 0..steps {
            let wx = left + i as f32 * 4.0;
            let wy = water_y + (wx / wave_len + t * 2.0).sin() * wave_h;
            draw_rectangle(wx, wy - 2.0, 5.0, 4.0, Color::new(0.2, 0.45, 0.8, 0.6));
        }
    }

    fn draw_aim(&self) {
        let idx = self.current_worm;
        if idx >= self.worms.len() || !self.worms[idx].alive {
            return;
        }
        let worm = &self.worms[idx];
        let angle = self.aim_angle;
        let line_len = 50.0 + self.charge_power * 0.5;
        let ex = worm.x + angle.cos() * line_len;
        let ey = worm.y + angle.sin() * line_len;

        draw_line(worm.x, worm.y, ex, ey, 2.0, Color::new(1.0, 1.0, 0.4, 0.8));

        draw_circle(ex, ey, 4.0, Color::new(1.0, 0.2, 0.2, 0.8));
        draw_circle_lines(ex, ey, 6.0, 1.5, WHITE);

        let power_for_preview = if self.charging {
            self.charge_power
        } else {
            50.0
        };
        let traj = projectile::simulate_trajectory(
            worm.x + angle.cos() * (WORM_RADIUS + 4.0),
            worm.y + angle.sin() * (WORM_RADIUS + 4.0),
            angle,
            power_for_preview,
            self.selected_weapon,
            self.wind,
            &self.terrain,
        );
        for (i, &(tx, ty)) in traj.iter().enumerate() {
            if i % 2 == 0 {
                let alpha = 1.0 - i as f32 / traj.len().max(1) as f32;
                draw_circle(tx, ty, 1.5, Color::new(1.0, 1.0, 0.6, alpha * 0.6));
            }
        }
    }
}

fn lcg(s: u32) -> u32 {
    s.wrapping_mul(1103515245).wrapping_add(12345)
}

fn parse_state_turn_index(msg: &str) -> Option<usize> {
    // Parse currentTurnIndex from state message: {"type":"state","state":{"currentTurnIndex":0,...}}
    let state_pos = msg.find("\"state\":")?;
    let after_state = msg.get(state_pos + 8..)?; // Skip past "state":
    let turn_pos = after_state.find("\"currentTurnIndex\":")?;
    let after_turn = after_state.get(turn_pos + 19..)?; // Skip past "currentTurnIndex":
    let num_slice = after_turn.trim_start();
    let num_end = num_slice
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(num_slice.len());
    num_slice[..num_end].trim().parse().ok()
}

fn parse_turn_index_from_message(msg: &str) -> Option<usize> {
    let turn_pos = msg.find("\"turnIndex\":")?;
    let after_turn = msg.get(turn_pos..)?;
    let num_start = after_turn.find(':')? + 1;
    let num_slice = after_turn[num_start..].trim_start();
    let num_end = num_slice
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(num_slice.len());
    num_slice[..num_end].trim().parse().ok()
}

fn parse_input_message(msg: &str) -> Option<(usize, String)> {
    let turn_pos = msg.find("\"turnIndex\":")?;
    let after_turn = msg.get(turn_pos..)?;
    let num_start = after_turn.find(':')? + 1;
    let num_slice = after_turn[num_start..].trim_start();
    let num_end = num_slice.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(num_slice.len());
    let turn_index: usize = num_slice[..num_end].trim().parse().ok()?;
    let input_key = "\"input\":\"";
    let input_start = msg.find(input_key)? + input_key.len();
    let mut input_end = input_start;
    let bytes = msg.as_bytes();
    while input_end < bytes.len() {
        if bytes[input_end] == b'\\' && input_end + 1 < bytes.len() {
            input_end += 2;
            continue;
        }
        if bytes[input_end] == b'"' {
            break;
        }
        input_end += 1;
    }
    let raw = msg.get(input_start..input_end)?;
    // Unescape JSON string escapes so inner parsers see clean JSON
    let unescaped = raw.replace("\\\"", "\"").replace("\\\\", "\\");
    Some((turn_index, unescaped))
}

fn parse_fire_input(input: &str) -> Option<(f32, f32, Weapon)> {
    if !input.contains("Fire") {
        return None;
    }
    let angle_deg = parse_json_number(input, "angle_deg")? as f32;
    let power_percent = parse_json_number(input, "power_percent")? as f32;
    let weapon_name = parse_json_string(input, "weapon")?;
    let weapon = Weapon::from_name(weapon_name)?;
    Some((angle_deg.to_radians(), power_percent, weapon))
}

fn parse_walk_input(input: &str) -> Option<f32> {
    if !input.contains("Walk") {
        return None;
    }
    parse_json_number(input, "dir").map(|v| v as f32)
}

fn parse_aim_message(msg: &str) -> Option<(usize, f32)> {
    // Parse turnIndex
    let turn_pos = msg.find("\"turnIndex\":")?;
    let after_turn = msg.get(turn_pos..)?;
    let num_start = after_turn.find(':')? + 1;
    let num_slice = after_turn[num_start..].trim_start();
    let num_end = num_slice.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(num_slice.len());
    let turn_index: usize = num_slice[..num_end].trim().parse().ok()?;
    
    // Parse aim angle
    let aim_angle = parse_json_number(msg, "aim")? as f32;
    
    Some((turn_index, aim_angle))
}

fn parse_json_number(s: &str, key: &str) -> Option<f64> {
    let key_plain = format!("\"{}\":", key);
    let key_escaped = format!("\\\"{}\\\":", key);
    let (start, _) = s.find(&key_plain).map(|i| (i + key_plain.len(), ())).or_else(|| s.find(&key_escaped).map(|i| (i + key_escaped.len(), ())))?;
    let end = s[start..].find(|c: char| c != '-' && c != '.' && !c.is_ascii_digit()).map(|i| start + i).unwrap_or(s.len());
    s[start..end].trim().parse().ok()
}

fn parse_json_string<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    for prefix in &[format!("\"{}\":\"", key), format!("\\\"{}\\\":\\\"", key)] {
        if let Some(i) = s.find(prefix) {
            let start = i + prefix.len();
            let mut end = start;
            let bytes = s.as_bytes();
            while end < bytes.len() {
                if bytes[end] == b'\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                if bytes[end] == b'"' {
                    break;
                }
                end += 1;
            }
            return s.get(start..end);
        }
    }
    None
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Worms".to_string(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Start with default seed - will be replaced when server sends authoritative seed
    let seed = 12345_u32;
    let mut game = Game::new(seed);

    loop {
        let dt = get_frame_time();
        game.handle_input();
        game.update(dt);
        game.draw();
        next_frame().await;
    }
}
