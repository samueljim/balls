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
use egui_macroquad::egui;
use macroquad::prelude::*;
use physics::{Ball, BALL_RADIUS};
use projectile::{Projectile, ClusterBomblet, ShotgunPellet};
use special_weapons::{AirstrikeDroplet, FirePool, UziBullet, PlacedExplosive, AirstrikeType};
use state::Phase;
use terrain::Terrain;
use weapons::{Weapon, WeaponCategory};

const TURN_TIME: f32 = 45.0;
const TURN_END_DELAY: f32 = 3.0;
const SETTLE_TIMEOUT: f32 = 3.0;
const CHARGE_SPEED: f32 = 55.0;
/// Default camera zoom level. Values > 1 mean “more zoomed in” relative to BASE_SHORT_AXIS.
const DEFAULT_ZOOM: f32 = 2.0;
/// Seconds past zero before the TurnEnd watchdog sends pings to the server.
const TURN_END_PING_START: f32 = -1.0;
/// Seconds past zero before the TurnEnd watchdog force-advances locally (server silent).
const TURN_END_FORCE_ADVANCE: f32 = -4.0;
/// Maximum timer drift (s) tolerated before the heartbeat correction is applied.
/// Kept large to avoid jittering the visual countdown display.
const TURN_TIMER_RESYNC_THRESHOLD: f32 = 3.0;

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

    balls: Vec<Ball>,
    current_ball: usize,

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
    /// True when the current charge was started by the F-key fire button
    /// rather than a left-click. Determines which input release triggers the shot.
    firing_by_key: bool,
    /// When true, mouse movement no longer updates the aim angle.
    /// Toggled by a clean left-click (no drag) so the game is playable with mouse-only.
    aim_locked: bool,

    proj: Option<Projectile>,
    cluster_bomblets: Vec<ClusterBomblet>,
    shotgun_pellets: Vec<ShotgunPellet>,
    airstrike_droplets: Vec<AirstrikeDroplet>,
    fire_pools: Vec<FirePool>,
    uzi_bullets: Vec<UziBullet>,
    /// Bullets remaining in the current burst (0 = not firing)
    uzi_burst_remaining: u32,
    /// Countdown in seconds until the next burst bullet is spawned
    uzi_burst_timer: f32,
    uzi_burst_angle: f32,
    uzi_burst_speed: f32,
    uzi_burst_ox: f32,
    uzi_burst_oy: f32,
    uzi_burst_shooter: usize,
    placed_explosives: Vec<PlacedExplosive>,
    teleport_mode: bool,
    baseball_bat_mode: bool,
    build_wall_mode: bool,
    /// First click anchor for Build Wall (world coords). None = waiting for pos, Some = waiting for rotation.
    build_wall_anchor: Option<(f32, f32)>,
    /// Airstrike or NapalmStrike waiting for a click-target. Stores which weapon.
    airstrike_mode: Option<Weapon>,
    /// Locked X coordinate for airstrike/napalm target; set by tapping the map, consumed by FIRE.
    /// None means the player hasn't picked a target yet — FIRE will not launch until this is set.
    airstrike_locked_x: Option<f32>,
    /// Locked world position for Teleport; set by tapping the map, consumed by FIRE.
    /// None means the player hasn't picked a destination yet — FIRE will not teleport until this is set.
    teleport_locked_pos: Option<(f32, f32)>,
    /// Cumulative log of wall placements for reconnect sync: (ax, ay, angle_mrad)
    wall_log: Vec<(i32, i32, i32)>,
    /// Cumulative log of drill tunnels for reconnect sync: (bx, by, angle_mrad)
    drill_log: Vec<(i32, i32, i32)>,
    /// Countdown before bot fires (resets each turn)
    bot_think_timer: f32,

    cam: GameCamera,
    last_mouse: (f32, f32),
    /// Origin of a left-button press; cleared once drag threshold exceeded or released.
    left_drag_start: Option<(f32, f32)>,
    /// True once left-drag has moved > 8px and is panning the camera.
    left_drag_panning: bool,
    /// Seconds remaining where the camera won't auto-follow (user manually panned).
    /// Set to 6 s on every pan gesture; resets to 0 when the player fires.
    cam_free_timer: f32,
    /// After cam_free_timer expires, glide back to the action over this many seconds.
    /// Speed ramps up from ~5 % to 100 % as this timer counts down to 0.
    cam_return_timer: f32,
    /// Zoom level the camera smoothly returns to when cam_return_timer is active.
    /// Reset to DEFAULT_ZOOM on every turn start.
    cam_target_zoom: f32,

    wind: f32,
    rng_state: u32,

    particles: Vec<Particle>,
    winning_team: Option<u32>,
    
    weapon_menu_open: bool,
    /// Currently visible category tab in the weapon menu.
    weapon_menu_category: WeaponCategory,

    net: network::NetworkState,
    /// Current turn index from server (which player's turn it is: 0, 1, etc.)
    current_turn_index: usize,
    /// Number of teams/players in the game
    num_teams: usize,
    /// When we receive turn_advanced during ProjectileFlying/Settling, apply when settling ends
    pending_turn_sync: Option<usize>,
    /// True when pending_turn_sync was set while it was our turn (e.g. server watchdog fired
    /// during our retreat). When the deferred sync is applied, we first send terrain_damages
    /// so the server has our final terrain state before the turn officially advances.
    pending_sync_send_terrain: bool,
    /// Deferred restart seed
    restart_seed: Option<u32>,
    /// Set after re-connecting so the next `state` message always forces a turn sync
    /// regardless of whether current_turn_index appears unchanged from the freshly
    /// re-initialised value of 0.
    just_reconnected: bool,
    /// True once we've received init at least once; used to detect reconnect vs first connect.
    had_network_connection: bool,
    /// Throttle for network aim messages (seconds since last send)
    last_aim_send: f32,
    /// Throttle for position-streaming messages (seconds since last send)
    last_pos_send: f32,
    /// Last value transmitted as pos_update (bi, x, y, vx, vy); None = never sent.
    last_pos_sent: Option<(usize, f32, f32, f32, f32)>,
    /// Per-ball lerp targets received from pos_update messages: (x, y, vx, vy)
    /// Used to smoothly interpolate remote balls toward their authoritative positions.
    ball_lerp_targets: Vec<Option<(f32, f32, f32, f32)>>,
    /// Track last logged turn state to reduce console spam
    last_logged_turn_state: (usize, Option<usize>),
    /// Track which ball index was last used per team for round-robin rotation
    last_ball_per_team: Vec<Option<usize>>,
    /// Watchdog: seconds spent in ProjectileFlying/Retreat; force-ends turn if too long
    stuck_phase_timer: f32,
    /// Watchdog: seconds we have been waiting for a non-active player's turn to progress
    /// past the turn deadline. Used to send server ping nudges so the server force-advances.
    non_active_stuck_timer: f32,
    /// Per-ball cooldown (seconds) for game-event toasts — prevents spam from fires/DoT
    ball_event_cooldown: Vec<f32>,
    /// Last label sent to the JS FIRE button — used to detect changes and avoid re-sending
    last_fire_label: &'static str,
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
        let balls_per_team: usize = 3;
        let total = num_teams * balls_per_team;
        let mut balls = Vec::new();

        // Spawn balls within the playable land area only
        let mut positions: Vec<f32> = Vec::new();
        for i in 0..total {
            let x = terrain::LAND_START_X + (i + 1) as f32 * terrain::PLAYABLE_LAND_WIDTH / (total + 1) as f32;
            positions.push(x);
        }
        let mut interleaved: Vec<(usize, usize)> = Vec::new();
        for wi in 0..balls_per_team {
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
                // Check if there's lava at or near where the ball would spawn
                let ball_y = surface_y - (BALL_RADIUS as i32) - 2;
                let mut is_safe = true;
                
                // Check area around spawn position for lava
                for dy in -2..3 {
                    for dx in -2..3 {
                        let check_x = search_x + dx;
                        let check_y = ball_y + dy;
                        if t.get(check_x, check_y) == terrain::LAVA {
                            is_safe = false;
                            break;
                        }
                    }
                    if !is_safe { break; }
                }
                
                if is_safe {
                    spawn_y = Some(surface_y as f32 - BALL_RADIUS - 2.0);
                }
            }
            
            // If original position has lava, search nearby for safe spot
            if spawn_y.is_none() {
                for offset in 1..50 {
                    for dir in [-1, 1] {
                        let test_x = (x as i32 + offset * dir).max(terrain::LAND_START_X as i32).min(terrain::LAND_END_X as i32);
                        if let Some(surface_y) = t.find_surface_y(test_x) {
                            let ball_y = surface_y - (BALL_RADIUS as i32) - 2;
                            let mut is_safe = true;
                            
                            // Check area around spawn position for lava
                            for dy in -2..3 {
                                for dx in -2..3 {
                                    let check_x = test_x + dx;
                                    let check_y = ball_y + dy;
                                    if t.get(check_x, check_y) == terrain::LAVA {
                                        is_safe = false;
                                        break;
                                    }
                                }
                                if !is_safe { break; }
                            }
                            
                            if is_safe {
                                spawn_y = Some(surface_y as f32 - BALL_RADIUS - 2.0);
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
            balls.push(Ball::new(spawn_x, y, ti as u32, name));
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
            balls,
            current_ball: 0,
            phase: Phase::Aiming,
            turn_timer: TURN_TIME,
            settle_timer: 0.0,
            turn_end_timer: 0.0,
            selected_weapon: Weapon::Bazooka,
            aim_angle: -0.5,
            charge_power: 0.0,
            charging: false,
            has_fired: false,
            firing_by_key: false,
            aim_locked: false,
            proj: None,
            cluster_bomblets: Vec::new(),
            shotgun_pellets: Vec::new(),
            airstrike_droplets: Vec::new(),
            fire_pools: Vec::new(),
            uzi_bullets: Vec::new(),
            uzi_burst_remaining: 0,
            uzi_burst_timer: 0.0,
            uzi_burst_angle: 0.0,
            uzi_burst_speed: 0.0,
            uzi_burst_ox: 0.0,
            uzi_burst_oy: 0.0,
            uzi_burst_shooter: 0,
            placed_explosives: Vec::new(),
            teleport_mode: false,
            baseball_bat_mode: false,
            build_wall_mode: false,
            build_wall_anchor: None,
            airstrike_mode: None,
            airstrike_locked_x: None,
            teleport_locked_pos: None,
            wall_log: Vec::new(),
            drill_log: Vec::new(),
            bot_think_timer: 3.0,
            cam: GameCamera::new(cam_x, cam_y),
            last_mouse: (0.0, 0.0),
            left_drag_start: None,
            left_drag_panning: false,
            cam_free_timer: 0.0,
            cam_return_timer: 0.0,
            cam_target_zoom: DEFAULT_ZOOM,
            wind,
            rng_state: rng,
            particles: Vec::new(),
            winning_team: None,
            weapon_menu_open: false,
            weapon_menu_category: WeaponCategory::Explosives,
            net: network::NetworkState::new(),
            current_turn_index: 0,
            num_teams,
            pending_turn_sync: None,
            pending_sync_send_terrain: false,
            restart_seed: None,
            just_reconnected: false,
            had_network_connection: false,
            last_aim_send: 0.0,
            last_pos_send: 0.0,
            last_pos_sent: None,
            ball_lerp_targets: {
                let total = num_teams * 3; // balls_per_team = 3
                vec![None; total]
            },
            last_logged_turn_state: (0, None),
            retreat_timer: 0.0,
            stuck_phase_timer: 0.0,
            non_active_stuck_timer: 0.0,
            ball_event_cooldown: vec![0.0; num_teams * 3],
            last_fire_label: "",
            last_ball_per_team: {
                // Pre-record that ball 0 (team 0's first ball) is the initial
                // current_ball, so the next sync_to_player_turn(0) knows to
                // advance to the second ball instead of re-picking the first.
                let mut v = vec![None; num_teams];
                if num_teams > 0 {
                    v[0] = Some(0); // current_ball starts at 0 which belongs to team 0
                }
                v
            },
        }
    }

    /// Auto-follow helper that respects cam_free_timer and applies smooth glide-back easing.
    /// Call this in place of cam.follow() at every follow site.
    fn auto_follow(&mut self, tx: f32, ty: f32, speed: f32, dt: f32) {
        if self.cam_free_timer > 0.0 {
            return; // user is looking around, don't fight them
        }
        let ease = if self.cam_return_timer > 0.0 {
            // Ramp from ~5 % at the start of the glide to 100 % as timer reaches 0
            (1.0 - self.cam_return_timer / 2.0).max(0.05)
        } else {
            1.0
        };
        self.cam.follow(tx, ty, speed * ease, dt);
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

    /// Returns true when a deferred turn sync (pending_turn_sync) should also trigger a
    /// terrain_damages send. This is the case when the server advanced the turn while it
    /// was our turn (e.g. watchdog firing during our Retreat) — we need to flush our latest
    /// terrain state so all clients end up with the correct map.
    ///
    /// Must be called BEFORE `current_turn_index` is updated (i.e. before the pending sync
    /// is applied) so that `is_my_turn()` still reflects the turn that is ending.
    fn should_send_terrain_on_deferred_sync(&self) -> bool {
        // If we're currently in Retreat, this is definitely our turn ending (non-active
        // players enter TurnEnd, never Retreat, so Retreat ↔ active player).
        if matches!(self.phase, Phase::Retreat) {
            return true;
        }
        // For other phases (Settling, ProjectileFlying): check if it's our turn via
        // the current turn index — this will be correct because current_turn_index has
        // not been updated yet (the caller should invoke this before updating it).
        self.is_my_turn()
    }
    
    /// Find the first alive ball for a given team/player
    fn find_ball_for_player(&self, player_index: usize) -> Option<usize> {
        let team = player_index as u32;
        self.balls
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

        // Left-click drag-to-pan: only start when weapon menu is closed, so egui
        // can handle touch/pointer events inside the menu without interference.
        if is_mouse_button_pressed(MouseButton::Left) && !self.weapon_menu_open {
            self.left_drag_start = Some((mx, my));
            self.left_drag_panning = false;
            self.last_mouse = (mx, my);
        }
        if is_mouse_button_released(MouseButton::Left) {
            // A tap (release with no preceding drag) does one of:
            //   • locks the airstrike/napalm target X at the tapped world position
            //   • locks the teleport destination at the tapped world position
            //   • toggles aim lock for all other weapons
            let was_tap = self.left_drag_start.is_some(); // Some = drag never started
            self.left_drag_panning = false;
            self.left_drag_start = None;
            if was_tap
                && !self.charging
                && self.is_my_turn()
                && self.phase.allows_input()
                && !self.weapon_menu_open
                && !self.build_wall_mode
                && !self.baseball_bat_mode
            {
                if self.airstrike_mode.is_some() {
                    // Lock the airstrike at the tapped world position.
                    // Tapping again overwrites the target so the player can reposition.
                    let (wx, _wy) = self.cam.screen_to_world(mx, my);
                    self.airstrike_locked_x = Some(wx.clamp(0.0, self.terrain.width as f32));
                } else if self.teleport_mode {
                    // Lock the teleport destination at the tapped world position.
                    // Tapping again overwrites the destination so the player can reposition.
                    let (wx, wy) = self.cam.screen_to_world(mx, my);
                    let tx = wx.clamp(0.0, self.terrain.width as f32);
                    let ty = wy.clamp(0.0, self.terrain.height as f32);
                    self.teleport_locked_pos = Some((tx, ty));
                } else {
                    self.aim_locked = !self.aim_locked;
                }
            }
        }
        if !self.left_drag_panning {
            if let Some((sx, sy)) = self.left_drag_start {
                let dist = ((mx - sx) * (mx - sx) + (my - sy) * (my - sy)).sqrt();
                if dist > 8.0 {
                    self.left_drag_panning = true;
                    self.left_drag_start = None;
                    // Cancel any charge that started on this same click
                    self.charging = false;
                    self.charge_power = 0.0;
                }
            }
        }

        if self.left_drag_panning {
            let dx = mx - self.last_mouse.0;
            let dy = my - self.last_mouse.1;
            if dx.abs() > 0.5 || dy.abs() > 0.5 {
                let dt_input = get_frame_time().max(0.001);
                self.cam.pan_push(dx, dy, dt_input);
                self.cam_free_timer = 6.0; // 6 s free-look window
                self.cam_return_timer = 0.0; // cancel any in-progress glide-back
            }
            self.last_mouse = (mx, my);
        }

        // Tick cam timers every frame
        let ft = get_frame_time();
        if self.cam_free_timer > 0.0 {
            let prev = self.cam_free_timer;
            self.cam_free_timer = (self.cam_free_timer - ft).max(0.0);
            if self.cam_free_timer == 0.0 && prev > 0.0 {
                // Free-look just expired — begin the 2-second glide back
                self.cam_return_timer = 2.0;
            }
        } else if self.cam_return_timer > 0.0 {
            self.cam_return_timer = (self.cam_return_timer - ft).max(0.0);
        }

        // Mouse wheel: camera zoom (weapon menu scroll is handled by egui's ScrollArea)
        let wheel = mouse_wheel().1;
        if wheel.abs() > 0.1 && !self.weapon_menu_open {
            // Smooth trackpad pinch / fine scroll wheel use a proportional factor;
            // discrete mouse clicks (large delta) snap by a fixed step.
            let factor = if wheel.abs() > 5.0 {
                // Discrete mouse wheel notch
                if wheel > 0.0 { 1.15 } else { 1.0 / 1.15 }
            } else {
                // Continuous trackpad — proportional zoom so it feels silky
                1.0 + wheel * 0.015
            };
            // Zoom toward the cursor position so the point under the mouse stays put
            self.cam.zoom_toward_screen_point(mx, my, factor);
            // Pin cam_target_zoom to the new level so the auto-return lerp doesn't fight
            self.cam_target_zoom = self.cam.zoom;
        }

        // Keyboard zoom: + / = to zoom in, - to zoom out (toward screen centre)
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.cam.zoom_by(1.25);
            self.cam_target_zoom = self.cam.zoom;
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.cam.zoom_by(1.0 / 1.25);
            self.cam_target_zoom = self.cam.zoom;
        }

        if self.phase == Phase::GameOver {
            // Game is over — no input is accepted.
            return;
        }

        // During Retreat, ProjectileFlying, or Settling: allow movement for local player's ball.
        // Settling is included so players can reposition while explosions are still resolving,
        // giving them a head-start before the formal 5-second Retreat window opens.
        if self.phase == Phase::Retreat
            || self.phase == Phase::ProjectileFlying
            || self.phase == Phase::Settling
        {
            // During Retreat/Settling the active player moves the ball that just fired,
            // which is already stored in current_ball.
            // Phase::Retreat is only entered when has_fired && is_my_turn() (in the Settling
            // handler), so we don't need to re-check is_my_turn() here. Checking phase alone
            // is reliable and avoids the bug where a server watchdog advances current_turn_index
            // mid-retreat, making is_my_turn() return false and incorrectly blocking movement.
            // During ProjectileFlying and Settling each player controls their own ball so they
            // can dodge; during Retreat only the active player moves (current_ball).
            let ball_idx_opt = if self.phase == Phase::Retreat {
                // Retreat always belongs to the player who just fired (current_ball)
                if self.current_ball < self.balls.len() { Some(self.current_ball) } else { None }
            } else if self.net.connected {
                // ProjectileFlying / Settling in multiplayer: each player moves their own ball
                self.net.my_player_index.and_then(|pi| self.find_ball_for_player(pi))
            } else {
                Some(self.current_ball)
            };

            if let Some(wi) = ball_idx_opt {
                if wi < self.balls.len() && self.balls[wi].alive {
                    let ball = &mut self.balls[wi];
                    let can_move = ball.can_move();

                    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
                        physics::walk(ball, &self.terrain, -1.0);
                    }
                    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
                        physics::walk(ball, &self.terrain, 1.0);
                    }

                    if can_move {
                        if is_key_pressed(KeyCode::W) || is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Space) {
                            physics::jump(ball);
                            if self.net.connected {
                                let msg = r#"{"type":"input","input":"{\"Jump\":{}}"}"#;
                                self.net.send_message(msg);
                            }
                        }
                        if is_key_pressed(KeyCode::S) || is_key_pressed(KeyCode::Down) {
                            physics::backflip(ball);
                            if self.net.connected {
                                let msg = r#"{"type":"input","input":"{\"Backflip\":{}}"}"#;
                                self.net.send_message(msg);
                            }
                        }
                    }
                }
            }
            return; // No weapon/aim/firing during retreat, settling, or projectile flying
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

        // Toggle weapon menu with Tab or Q (only on your turn).
        // If currently charging, cancel the charge first so the player can switch weapon.
        if self.is_my_turn() && (is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::Q)) {
            if self.charging {
                self.charging = false;
                self.charge_power = 0.0;
                self.phase = Phase::Aiming;
            }
            self.weapon_menu_open = !self.weapon_menu_open;
            // Drain the char queue so the key that opened the menu (Q) isn't
            // accidentally processed by egui on the same frame.
            while get_char_pressed().is_some() {}
        }

        // ESC or right-click while charging cancels the charge (return to aiming).
        // Also cancel any click-targeting modes.
        if self.is_my_turn() && (is_key_pressed(KeyCode::Escape) || is_mouse_button_pressed(MouseButton::Right)) {
            if self.charging {
                self.charging = false;
                self.charge_power = 0.0;
                self.phase = Phase::Aiming;
                self.firing_by_key = false;
            }
            self.baseball_bat_mode = false;
            self.airstrike_locked_x = None;
            self.teleport_locked_pos = None;
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

        // Drain char queue every frame to prevent stale keypresses from leaking into
        // other systems (e.g. WASD keys typed while menu was closed).
        while get_char_pressed().is_some() {}

        // Weapon menu input is now handled entirely by egui inside game.draw().
        // When the menu is open, skip all the normal pointer / aim processing
        // so egui's events are not double-consumed.
        if self.weapon_menu_open {
            return;
        }

        // Update aim angle unless locked or charging.
        // Click to lock/unlock; locked aim lets you move camera freely before firing.
        if self.is_my_turn() && !self.charging && !self.aim_locked {
            if let Some(ball) = self.balls.get(self.current_ball) {
                if ball.alive {
                    let (wx, wy) = (ball.x, ball.y);
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
        if self.is_my_turn() && self.phase.allows_movement() && self.current_ball < self.balls.len() && self.balls[self.current_ball].alive && !self.weapon_menu_open {
            let ball = &mut self.balls[self.current_ball];
            let can_move = ball.can_move();
            
            if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
                physics::walk(ball, &self.terrain, -1.0);
            }
            if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
                physics::walk(ball, &self.terrain, 1.0);
            }
            
            // Jumping allowed while budget remains; cost is the horizontal distance
            // traveled, charged on landing via jump_start_x (not a fixed fee).
            if can_move {
                if is_key_pressed(KeyCode::W) || is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Space) {
                    physics::jump(ball);
                    if self.net.connected {
                        let msg = format!(
                            r#"{{"type":"input","input":"{{\"Jump\":{{}}}}","bi":{}}}"#,
                            self.current_ball,
                        );
                        self.net.send_message(&msg);
                    }
                }
                if is_key_pressed(KeyCode::S) || is_key_pressed(KeyCode::Down) {
                    physics::backflip(ball);
                    if self.net.connected {
                        let msg = format!(
                            r#"{{"type":"input","input":"{{\"Backflip\":{{}}}}","bi":{}}}"#,
                            self.current_ball,
                        );
                        self.net.send_message(&msg);
                    }
                }
            }
        }

        if is_mouse_button_pressed(MouseButton::Left) && !self.has_fired && self.is_my_turn() && self.phase.allows_input() && !self.weapon_menu_open && !self.left_drag_panning {
            // Handle Build Wall mode: two clicks — first sets position, second sets rotation
            if self.build_wall_mode {
                let (mx, my) = mouse_position();
                let world_pos = self.cam.to_macroquad().screen_to_world(vec2(mx, my));

                if self.build_wall_anchor.is_none() {
                    // First click: lock in the wall's centre position
                    self.build_wall_anchor = Some((world_pos.x, world_pos.y));
                } else {
                    // Second click: derive angle from anchor → mouse, then stamp terrain
                    let (ax, ay) = self.build_wall_anchor.unwrap();
                    let dx = world_pos.x - ax;
                    let dy = world_pos.y - ay;
                    let angle = if dx.abs() < 0.5 && dy.abs() < 0.5 {
                        self.aim_angle // fallback if both clicks are on same pixel
                    } else {
                        dy.atan2(dx)
                    };
                    let cos_a = angle.cos();
                    let sin_a = angle.sin();
                    let half_len = 35i32;
                    let half_thick = 35i32;
                    for i in -half_len..=half_len {
                        for j in -half_thick..=half_thick {
                            let wx = (ax + i as f32 * cos_a - j as f32 * sin_a).round() as i32;
                            let wy = (ay + i as f32 * sin_a + j as f32 * cos_a).round() as i32;
                            if wx >= 0 && wx < self.terrain.width as i32
                                && wy >= 0 && wy < self.terrain.height as i32 {
                                self.terrain.set(wx, wy, terrain::WOOD);
                            }
                        }
                    }
                    self.terrain_dirty = true;
                    self.build_wall_anchor = None;
                    self.build_wall_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    // Record in wall log for reconnect sync
                    self.wall_log.push((ax as i32, ay as i32, (angle * 1000.0) as i32));
                    // Sync wall placement to other players
                    if self.net.connected {
                        let input_json = format!(
                            r#"{{"BuildWallPlace":{{"ax":{},"ay":{},"angle":{}}}}}"#,
                            ax, ay, angle
                        );
                        let mut escaped = String::new();
                        for c in input_json.chars() {
                            match c {
                                '"' => escaped.push_str("\\\""),
                                '\\' => escaped.push_str("\\\\"),
                                _ => escaped.push(c),
                            }
                        }
                        let msg = format!(r#"{{"type":"input","input":"{}","bi":{}}}"#, escaped, self.current_ball);
                        self.net.send_message(&msg);
                    }
                }
            }
            // Left-click only handles Build Wall placement (needs two canvas clicks).
            // Airstrike, Teleport, and all other weapons fire via the F key.
        }
        // F-key fire button: fires all weapons including airstrike, teleport, and build-wall.
        // On mobile the FIRE button holds/releases F.  Build Wall uses the current aim position
        // so mobile players can place walls without two separate canvas taps.
        if is_key_pressed(KeyCode::F)
            && !self.has_fired
            && self.is_my_turn()
            && self.phase.allows_input()
            && !self.weapon_menu_open
            && !self.charging
        {
            if self.build_wall_mode {
                // F key drives build-wall: first press sets anchor, second press finalises.
                let (mx, my) = mouse_position();
                let world_pos = self.cam.to_macroquad().screen_to_world(vec2(mx, my));
                if self.build_wall_anchor.is_none() {
                    self.build_wall_anchor = Some((world_pos.x, world_pos.y));
                } else {
                    let (ax, ay) = self.build_wall_anchor.unwrap();
                    let dx = world_pos.x - ax;
                    let dy = world_pos.y - ay;
                    let angle = if dx.abs() < 0.5 && dy.abs() < 0.5 {
                        self.aim_angle
                    } else {
                        dy.atan2(dx)
                    };
                    let cos_a = angle.cos();
                    let sin_a = angle.sin();
                    let half_len = 35i32;
                    let half_thick = 35i32;
                    for i in -half_len..=half_len {
                        for j in -half_thick..=half_thick {
                            let wx = (ax + i as f32 * cos_a - j as f32 * sin_a).round() as i32;
                            let wy = (ay + i as f32 * sin_a + j as f32 * cos_a).round() as i32;
                            if wx >= 0 && wx < self.terrain.width as i32
                                && wy >= 0 && wy < self.terrain.height as i32 {
                                self.terrain.set(wx, wy, terrain::WOOD);
                            }
                        }
                    }
                    self.terrain_dirty = true;
                    self.build_wall_anchor = None;
                    self.build_wall_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    self.wall_log.push((ax as i32, ay as i32, (angle * 1000.0) as i32));
                    if self.net.connected {
                        let input_json = format!(
                            r#"{{"BuildWallPlace":{{"ax":{},"ay":{},"angle":{}}}}}"#,
                            ax, ay, angle
                        );
                        let mut escaped = String::new();
                        for c in input_json.chars() {
                            match c {
                                '"' => escaped.push_str("\\\""),
                                '\\' => escaped.push_str("\\\\"),
                                _ => escaped.push(c),
                            }
                        }
                        self.net.send_message(&format!(r#"{{"type":"input","input":"{}","bi":{}}}"#, escaped, self.current_ball));
                    }
                }
            } else if let Some(airstrike_weapon) = self.airstrike_mode {
                // Use the locked target X (set by tapping). If player hasn't tapped yet, do nothing.
                let target_x = match self.airstrike_locked_x.take() {
                    Some(x) => x,
                    None => return, // no target locked yet — wait for player to tap first
                };
                self.airstrike_droplets.clear();
                match airstrike_weapon {
                    Weapon::Airstrike => {
                        let spacing = 80.0;
                        for i in 0..5 {
                            let x = target_x + (i as f32 - 2.0) * spacing;
                            self.airstrike_droplets.push(AirstrikeDroplet {
                                x, y: -50.0, vy: 0.0, alive: true,
                                weapon_type: AirstrikeType::Explosive,
                            });
                        }
                    },
                    Weapon::NapalmStrike => {
                        let spacing = 60.0;
                        for i in 0..7 {
                            let x = target_x + (i as f32 - 3.0) * spacing;
                            self.airstrike_droplets.push(AirstrikeDroplet {
                                x, y: -50.0, vy: 0.0, alive: true,
                                weapon_type: AirstrikeType::Napalm,
                            });
                        }
                    },
                    _ => {}
                }
                self.airstrike_mode = None;
                self.has_fired = true;
                self.phase = Phase::ProjectileFlying;
                // Restore zoom now that targeting is done.
                self.cam_target_zoom = DEFAULT_ZOOM;
                self.cam_return_timer = 2.0;
                if self.current_ball < self.balls.len() {
                    self.balls[self.current_ball].reset_movement_budget();
                }
                if self.net.connected {
                    let weapon_name = match airstrike_weapon {
                        Weapon::NapalmStrike => "NapalmStrike",
                        _ => "Airstrike",
                    };
                    let input_json = format!(
                        r#"{{"AirstrikeTarget":{{"weapon":"{}","x":{}}}}}"#,
                        weapon_name, target_x
                    );
                    let mut escaped = String::new();
                    for c in input_json.chars() {
                        match c {
                            '"' => escaped.push_str("\\\""),
                            '\\' => escaped.push_str("\\\\"),
                            _ => escaped.push(c),
                        }
                    }
                    self.net.send_message(&format!(r#"{{"type":"input","input":"{}","bi":{}}}"#, escaped, self.current_ball));
                }
            } else if self.teleport_mode {
                // Use the locked destination (set by tapping). If player hasn't tapped yet, do nothing.
                let (target_x, target_y) = match self.teleport_locked_pos.take() {
                    Some(pos) => pos,
                    None => return, // no destination locked yet — wait for player to tap first
                };
                let idx = self.current_ball;
                if idx < self.balls.len() && self.balls[idx].alive {
                    self.balls[idx].x = target_x;
                    self.balls[idx].y = target_y;
                    self.balls[idx].vx = 0.0;
                    self.balls[idx].vy = 0.0;
                    self.teleport_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    if self.net.connected {
                        let input_json = format!(
                            r#"{{"TeleportTo":{{"x":{},"y":{}}}}}"#,
                            target_x, target_y
                        );
                        let mut escaped = String::new();
                        for c in input_json.chars() {
                            match c {
                                '"' => escaped.push_str("\\\""),
                                '\\' => escaped.push_str("\\\\"),
                                _ => escaped.push(c),
                            }
                        }
                        self.net.send_message(&format!(r#"{{"type":"input","input":"{}","bi":{}}}"#, escaped, self.current_ball));
                    }
                }
            } else if self.baseball_bat_mode {
                // Baseball bat fires instantly on F press (no charge needed).
                let idx = self.current_ball;
                if idx < self.balls.len() && self.balls[idx].alive {
                    let ball_x = self.balls[idx].x;
                    let ball_y = self.balls[idx].y;
                    let bat_range = 100.0;
                    let angle = self.aim_angle;
                    let knock_x = angle.cos() * 850.0;
                    let knock_y = angle.sin() * 850.0 - 300.0;
                    for i in 0..self.balls.len() {
                        if i == idx || !self.balls[i].alive { continue; }
                        let dx = self.balls[i].x - ball_x;
                        let dy = self.balls[i].y - ball_y;
                        if (dx*dx + dy*dy).sqrt() < bat_range {
                            self.balls[i].apply_knockback(knock_x, knock_y);
                            self.balls[i].health = self.balls[i].health.saturating_sub(20);
                            if self.balls[i].health == 0 { self.balls[i].alive = false; }
                        }
                    }
                    self.baseball_bat_mode = false;
                    self.has_fired = true;
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    if self.net.connected {
                        let input_json = format!(r#"{{"BatSwing":{{"angle":{}}}}}"#, angle);
                        let mut escaped = String::new();
                        for c in input_json.chars() {
                            match c {
                                '"' => escaped.push_str("\\\""),
                                '\\' => escaped.push_str("\\\\"),
                                _ => escaped.push(c),
                            }
                        }
                        self.net.send_message(&format!(r#"{{"type":"input","input":"{}","bi":{}}}"#, escaped, self.current_ball));
                    }
                }
            } else {
                // Normal weapon: hold F to charge, release to fire.
                self.charging = true;
                self.charge_power = 0.0;
                self.phase = Phase::Charging;
                self.firing_by_key = true;
            }
        }
        if self.charging && !self.left_drag_panning {
            self.charge_power = (self.charge_power + CHARGE_SPEED * get_frame_time()).min(100.0);
            let fire_released = if self.firing_by_key {
                !is_key_down(KeyCode::F)
            } else {
                is_mouse_button_released(MouseButton::Left)
            };
            if fire_released || self.charge_power >= 100.0 {
                self.firing_by_key = false;
                self.fire();
            }
        }

        // Keep the FIRE button label in sync with the active weapon mode.
        self.sync_fire_label();
    }

    /// Returns the label that the FIRE button should show for the current weapon/mode.
    fn current_fire_label(&self) -> &'static str {
        if self.airstrike_mode.is_some() {
            if self.airstrike_locked_x.is_some() {
                "\u{1F525}\nFIRE"      // 🔥 FIRE — target locked, ready to launch
            } else {
                "\u{1F3AF}\nTARGET"    // 🎯 TARGET — tap the map to lock a target first
            }
        } else if self.teleport_mode {
            if self.teleport_locked_pos.is_some() {
                "\u{1F4CD}\nUSE"       // 📍 USE — destination locked, ready to warp
            } else {
                "\u{1F4CD}\nTARGET"    // 📍 TARGET — tap the map to lock a destination first
            }
        } else if self.build_wall_mode {
            if self.build_wall_anchor.is_none() {
                "\u{1F9F1}\nSET POS"   // 🧱 SET POS
            } else {
                "\u{1F9F1}\nSET ROT"   // 🧱 SET ROT
            }
        } else if self.baseball_bat_mode {
            "\u{1F4A5}\nSWING"     // 💥 SWING
        } else {
            use weapons::WeaponType;
            match self.selected_weapon.weapon_type() {
                WeaponType::Placed => "\u{1F9E8}\nPLACE",  // 🧨 PLACE
                _ => "\u{1F525}\nFIRE",                     // 🔥 FIRE
            }
        }
    }

    /// Sends an updated FIRE button label to the JS layer whenever the label changes.
    fn sync_fire_label(&mut self) {
        let label = self.current_fire_label();
        if label != self.last_fire_label {
            self.last_fire_label = label;
            self.net.send_fire_label(label);
        }
    }

    fn fire(&mut self) {
        self.charging = false;
        if self.has_fired {
            return;
        }
        let idx = self.current_ball;
        if idx >= self.balls.len() || !self.balls[idx].alive {
            return;
        }

        let power = self.charge_power.clamp(0.0, 100.0);
        let angle = self.aim_angle;
        let weapon = self.selected_weapon;

        self.cam_free_timer = 0.0;    // always follow the action when firing
        self.cam_return_timer = 0.0;   // skip the glide-back phase too
        self.do_fire(idx, angle, power, weapon);

        // Give the firing player a fresh movement budget so they can dodge
        // while the projectile is in the air.
        if self.phase == Phase::ProjectileFlying && idx < self.balls.len() {
            self.balls[idx].reset_movement_budget();
        }

        // Don't set has_fired for Baseball Bat, Teleport, and BuildWall - they need a second click
        if weapon != Weapon::BaseballBat && weapon != Weapon::Teleport && weapon != Weapon::BuildWall
            && weapon != Weapon::Airstrike && weapon != Weapon::NapalmStrike {
            self.has_fired = true;
        }
        self.charge_power = 0.0;
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("[FIRE] Fired {:?}, has_fired={}\0", weapon, self.has_fired);
            unsafe { console_log(msg.as_ptr()); }
        }

        if self.net.connected {
            // Two-click weapons handle their own network messages on the second click
            // (BatSwing, AirstrikeTarget, TeleportTo, BuildWallPlace).  Sending an
            // initial Fire message for these weapons would set baseball_bat_mode /
            // has_fired incorrectly on remote clients, causing desync and freezes.
            let is_two_click = matches!(
                weapon,
                Weapon::BaseballBat
                    | Weapon::Airstrike
                    | Weapon::NapalmStrike
                    | Weapon::Teleport
                    | Weapon::BuildWall
            );
            if !is_two_click {
            // Drill: send exact ball origin so all clients carve the identical tunnel.
            // Generic Fire message would make remotes use their own (potentially different)
            // ball position. DrillFire is broadcast just like any other input type.
            let input_json = if weapon == Weapon::Drill && idx < self.balls.len() {
                let bx = self.balls[idx].x as i32;
                let by = self.balls[idx].y as i32;
                format!(
                    r#"{{"DrillFire":{{"bx":{},"by":{},"angle":{}}}}}"#,
                    bx, by, angle
                )
            } else {
                let angle_deg = angle.to_degrees();
                let weapon_name = weapon.name();
                format!(
                    r#"{{"Fire":{{"weapon":"{}","angle_deg":{},"power_percent":{}}}}}"#,
                    weapon_name, angle_deg, power
                )
            };
            let mut escaped = String::new();
            for c in input_json.chars() {
                match c {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    _ => escaped.push(c),
                }
            }
            let (fire_bx, fire_by) = if idx < self.balls.len() {
                (self.balls[idx].x, self.balls[idx].y)
            } else {
                (0.0, 0.0)
            };
            let msg = format!(
                r#"{{"type":"input","input":"{}","bi":{},"bx":{:.1},"by":{:.1}}}"#,
                escaped, idx, fire_bx, fire_by,
            );
            self.net.send_message(&msg);
            } // end if !is_two_click
        }
    }

    fn do_fire(&mut self, idx: usize, angle: f32, power: f32, weapon: Weapon) {
        if idx >= self.balls.len() || !self.balls[idx].alive {
            return;
        }
        let ball = &self.balls[idx];
        let offset = BALL_RADIUS + 4.0;
        let sx = ball.x + angle.cos() * offset;
        let sy = ball.y + angle.sin() * offset;
        
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
            
            // Uzi - burst-fire 20 bullets in a straight line, one every 0.07 s
            Weapon::Uzi => {
                self.uzi_bullets.clear();
                let base_speed = 800.0; // fast bullets — always full power

                // Spawn the first bullet immediately
                self.uzi_bullets.push(UziBullet {
                    x: sx,
                    y: sy,
                    vx: angle.cos() * base_speed,
                    vy: angle.sin() * base_speed,
                    alive: true,
                });
                // Pushback on shooter for the first bullet
                if idx < self.balls.len() {
                    self.balls[idx].apply_knockback(-angle.cos() * 18.0, -angle.sin() * 18.0);
                }

                // Queue the remaining 19 bullets
                self.uzi_burst_remaining = 19;
                self.uzi_burst_timer = 0.07;
                self.uzi_burst_angle = angle;
                self.uzi_burst_speed = base_speed;
                self.uzi_burst_ox = sx;
                self.uzi_burst_oy = sy;
                self.uzi_burst_shooter = idx;

                self.phase = Phase::ProjectileFlying;
            },
            
            // Airstrike - enter click-targeting mode
            Weapon::Airstrike => {
                self.airstrike_mode = Some(Weapon::Airstrike);
                // Snap instantly to minimum zoom so the full map is visible for targeting.
                self.cam.zoom = GameCamera::min_zoom();
                self.cam_target_zoom = self.cam.zoom;
                // Stay in Aiming phase; droplets spawn on click
            },
            
            // Napalm Strike - enter click-targeting mode
            Weapon::NapalmStrike => {
                self.airstrike_mode = Some(Weapon::NapalmStrike);
                // Snap instantly to minimum zoom so the full map is visible for targeting.
                self.cam.zoom = GameCamera::min_zoom();
                self.cam_target_zoom = self.cam.zoom;
                // Stay in Aiming phase; droplets spawn on click
            },
            
            // Dynamite - place at ball position, then retreat so player can run
            Weapon::Dynamite => {
                self.placed_explosives.push(PlacedExplosive {
                    x: ball.x,
                    y: ball.y + BALL_RADIUS - 2.0,
                    fuse: 5.0,
                    alive: true,
                    radius: 45.0,
                    damage: 50,
                    proximity_radius: 0.0,
                    arming_timer: 0.0,
                });
                self.phase = Phase::Retreat;
                self.retreat_timer = 5.0;
                if self.current_ball < self.balls.len() {
                    self.balls[self.current_ball].reset_movement_budget();
                }
            },
            
            // Baseball Bat - enter melee mode
            Weapon::BaseballBat => {
                self.baseball_bat_mode = true;
                // Stay in aiming phase, will handle click for bat swing
            },

            // Build Wall - enter placement mode; actual terrain is placed on next click
            Weapon::BuildWall => {
                self.build_wall_mode = true;
                // Stay in aiming phase, will handle click for wall placement
            },

            // Drill - carve a large tunnel instantly along aim direction
            Weapon::Drill => {
                let bx = self.balls[idx].x;
                let by = self.balls[idx].y;
                self.apply_drill_at(bx, by, angle);
                self.phase = Phase::Settling;
                self.settle_timer = 0.0;
                // Record in drill log for reconnect sync
                self.drill_log.push((bx as i32, by as i32, (angle * 1000.0) as i32));
            },

            // Teleport - enter teleport mode
            Weapon::Teleport => {
                self.teleport_mode = true;
                // Stay in aiming phase, will handle click for teleport
            },
            
            // Sniper Rifle – instant raycast, no gravity, high damage but 50% miss chance
            Weapon::SniperRifle => {
                // Deterministic miss roll — must use rng_state (not rand::) so all
                // clients reach the same outcome on replay.
                self.rng_state = lcg(self.rng_state);
                let miss_roll = (self.rng_state >> 16) as f32 / 65536.0;
                let missed = miss_roll < 0.50;
                let shoot_angle = if missed {
                    // Random deflection of 7–20° away from aim line
                    self.rng_state = lcg(self.rng_state);
                    let sign = if (self.rng_state & 0x8000) != 0 { 1.0_f32 } else { -1.0_f32 };
                    self.rng_state = lcg(self.rng_state);
                    let mag = 0.12 + (self.rng_state >> 16) as f32 / 65536.0 * 0.22;
                    angle + sign * mag
                } else {
                    angle
                };
                let cos_a = shoot_angle.cos();
                let sin_a = shoot_angle.sin();
                let step = 3.0_f32;
                let max_dist = (self.terrain.width.max(self.terrain.height) as f32) * 2.0;

                let mut hit_x = sx;
                let mut hit_y = sy;
                let mut hit_ball: Option<usize> = None;
                let mut beam_len = max_dist;

                let mut dist = step;
                while dist <= max_dist {
                    let rx = sx + cos_a * dist;
                    let ry = sy + sin_a * dist;

                    // Left the terrain bounds
                    if rx < 0.0 || rx >= self.terrain.width as f32
                        || ry < 0.0 || ry >= self.terrain.height as f32 {
                        hit_x = rx;
                        hit_y = ry;
                        beam_len = dist;
                        break;
                    }

                    // Hit terrain
                    if self.terrain.is_solid(rx as i32, ry as i32) {
                        hit_x = rx;
                        hit_y = ry;
                        beam_len = dist;
                        break;
                    }

                    // Hit a ball
                    let mut found = false;
                    for (bi, w) in self.balls.iter().enumerate() {
                        if !w.alive || bi == idx { continue; }
                        let dx = w.x - rx;
                        let dy = w.y - ry;
                        if dx * dx + dy * dy < (BALL_RADIUS * 1.4) * (BALL_RADIUS * 1.4) {
                            hit_x = rx;
                            hit_y = ry;
                            hit_ball = Some(bi);
                            beam_len = dist;
                            found = true;
                            break;
                        }
                    }
                    if found { break; }

                    dist += step;
                }

                // Deal damage + knockback to hit ball
                if let Some(bi) = hit_ball {
                    self.balls[bi].take_damage(weapon.base_damage());
                    let knock = 320.0;
                    self.balls[bi].apply_knockback(
                        cos_a * knock,
                        sin_a * knock - 80.0,
                    );
                }

                // Spawn a thin laser-beam flash along the ray
                let num_sparks = (beam_len / 8.0) as usize;
                for i in 0..=num_sparks {
                    let t = i as f32 * 8.0;
                    self.particles.push(Particle {
                        x: sx + cos_a * t,
                        y: sy + sin_a * t,
                        vx: -sin_a * (rand::gen_range(0.0_f32, 1.0) - 0.5) * 20.0,
                        vy: -rand::gen_range(0.0_f32, 40.0),
                        life: 0.15 + rand::gen_range(0.0_f32, 0.1),
                        color: Color::new(0.8, 1.0, 0.3, 1.0),
                        size: 1.5,
                    });
                }
                // Bright flash at the hit point
                for _ in 0..12 {
                    let spread_angle = rand::gen_range(0.0_f32, std::f32::consts::TAU);
                    let speed = rand::gen_range(30.0_f32, 120.0);
                    self.particles.push(Particle {
                        x: hit_x,
                        y: hit_y,
                        vx: spread_angle.cos() * speed,
                        vy: spread_angle.sin() * speed,
                        life: 0.3 + rand::gen_range(0.0_f32, 0.2),
                        color: Color::new(1.0, 1.0, 0.4, 1.0),
                        size: 2.5,
                    });
                }

                // Pan camera to hit point so the player can see where the shot landed
                self.cam.follow(hit_x, hit_y, 1.0, 1.0);

                self.phase = Phase::Settling;
                self.settle_timer = 0.0;
            },

            // Mine - place at ball position as a timed trap, then retreat
            Weapon::Mine => {
                self.placed_explosives.push(PlacedExplosive {
                    x: ball.x,
                    y: ball.y + BALL_RADIUS - 2.0,
                    fuse: f32::MAX,
                    alive: true,
                    radius: 30.0,
                    damage: 45,
                    proximity_radius: 40.0,
                    arming_timer: 2.0,
                });
                self.phase = Phase::Retreat;
                self.retreat_timer = 5.0;
                if self.current_ball < self.balls.len() {
                    self.balls[self.current_ball].reset_movement_budget();
                }
            },

            // Mortar - fire as projectile but enter Retreat immediately so player
            // can move while the shell (and its cluster bomblets) are in flight.
            Weapon::Mortar => {
                let shooter_team = self.balls[idx].team;
                let proj = Projectile::new(sx, sy, angle, power, weapon, shooter_team);
                self.proj = Some(proj);
                self.phase = Phase::Retreat;
                self.retreat_timer = 5.0;
                if self.current_ball < self.balls.len() {
                    self.balls[self.current_ball].reset_movement_budget();
                }
            },

            // All other weapons use regular projectile
            _ => {
                let shooter_team = self.balls[idx].team;
                let proj = Projectile::new(sx, sy, angle, power, weapon, shooter_team);
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
        if self.net.connected {
            // Always send — the worker ignores end_turn from non-active players,
            // so it is safe to call unconditionally and removes a class of race conditions.
            self.send_terrain_damages();
            // Embed an authoritative ball-state snapshot in the end_turn message.
            // The server will forward it in turn_advanced so all clients apply the
            // correct final positions/health before the next turn starts, correcting
            // any physics divergence that accumulated during the turn.
            let mut balls_json = String::from("[");
            for (i, ball) in self.balls.iter().enumerate() {
                if i > 0 { balls_json.push(','); }
                balls_json.push_str(&format!(
                    r#"{{"x":{:.1},"y":{:.1},"vx":{:.2},"vy":{:.2},"hp":{},"alive":{}}}"#,
                    ball.x, ball.y, ball.vx, ball.vy, ball.health, ball.alive
                ));
            }
            balls_json.push(']');
            let et_msg = format!(
                r#"{{"type":"end_turn","bi":{},"wind":{:.3},"balls":{}}}"#,
                self.current_ball, self.wind, balls_json,
            );
            self.net.send_message(&et_msg);
        }
        self.phase = Phase::TurnEnd;
        self.turn_end_timer = TURN_END_DELAY;
        self.charging = false;
        self.charge_power = 0.0;
    }

    /// Send a snapshot of all ball positions/health to sync with other players
    fn send_ball_state(&self) {
        let mut ball_data = String::from("[");
        for (i, w) in self.balls.iter().enumerate() {
            if i > 0 { ball_data.push(','); }
            ball_data.push_str(&format!(
                "{{\"x\":{},\"y\":{},\"vx\":{},\"vy\":{},\"hp\":{},\"alive\":{}}}",
                w.x, w.y, w.vx, w.vy, w.health, w.alive
            ));
        }
        ball_data.push(']');
        let msg = format!("{{\"type\":\"ball_state\",\"balls\":{}}}", ball_data);
        self.net.send_message(&msg);
    }

    /// Carve a drill tunnel at the given ball origin and angle.
    /// Used by both do_fire (local) and the DrillFire network receive handler (remote)
    /// so all clients carve the exact same tunnel at the same world coordinates.
    fn apply_drill_at(&mut self, bx: f32, by: f32, angle: f32) {
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let perp_x = -sin_a;
        let perp_y =  cos_a;
        let tunnel_back: f32 = 30.0;
        let tunnel_fwd:  f32 = 260.0;
        let half_w:      f32 = 35.0;   // 70px wide — clean enough for any worm to pass

        // Compute the 4 corners of the rotated rectangle in world space
        let corners = [
            (bx + cos_a * (-tunnel_back) + perp_x * (-half_w),
             by + sin_a * (-tunnel_back) + perp_y * (-half_w)),
            (bx + cos_a * (-tunnel_back) + perp_x * half_w,
             by + sin_a * (-tunnel_back) + perp_y * half_w),
            (bx + cos_a * tunnel_fwd + perp_x * (-half_w),
             by + sin_a * tunnel_fwd + perp_y * (-half_w)),
            (bx + cos_a * tunnel_fwd + perp_x * half_w,
             by + sin_a * tunnel_fwd + perp_y * half_w),
        ];
        // Axis-aligned bounding box of those corners
        let bb_min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min).floor() as i32 - 1;
        let bb_max_x = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max).ceil()  as i32 + 1;
        let bb_min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min).floor() as i32 - 1;
        let bb_max_y = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max).ceil()  as i32 + 1;

        let w = self.terrain.width as i32;
        let h = self.terrain.height as i32;
        let mut min_x = i32::MAX; let mut max_x = i32::MIN;
        let mut min_y = i32::MAX; let mut max_y = i32::MIN;

        // Iterate over every pixel in the bounding box and test inclusion in the
        // rotated rectangle using dot-products. This guarantees no pixels are missed
        // regardless of tunnel angle, unlike iterating in rotated-space and truncating.
        for px in bb_min_x.max(0)..=bb_max_x.min(w - 1) {
            for py in bb_min_y.max(0)..=bb_max_y.min(h - 1) {
                let dx = px as f32 - bx;
                let dy = py as f32 - by;
                // Project onto tunnel axis and perpendicular axis
                let along = dx * cos_a  + dy * sin_a;
                let perp  = dx * perp_x + dy * perp_y;
                if along >= -tunnel_back && along <= tunnel_fwd
                    && perp >= -half_w && perp <= half_w
                {
                    self.terrain.set(px, py, terrain::AIR);
                    if px < min_x { min_x = px; }
                    if px > max_x { max_x = px; }
                    if py < min_y { min_y = py; }
                    if py > max_y { max_y = py; }
                }
            }
        }
        // Regrow grass on surfaces newly exposed around the tunnel edges
        if min_x <= max_x && min_y <= max_y {
            self.terrain.refresh_grass_in_area(min_x, min_y, max_x, max_y);
        }
        self.terrain_dirty = true;
    }

    /// Send the full terrain ops log to the server for persistence across reconnects.
    /// Format: [[type,a,b,c],...] where type 0=explosion, 1=drill, 2=wall.
    fn send_terrain_damages(&self) {
        let explosions = &self.terrain.damage_log;
        let total = explosions.len() + self.wall_log.len() + self.drill_log.len();
        if total == 0 {
            return;
        }
        let mut arr = String::from("[");
        let mut first = true;
        for &(cx, cy, r) in explosions.iter() {
            if !first { arr.push(','); }
            arr.push_str(&format!("[0,{},{},{}]", cx, cy, r));
            first = false;
        }
        for &(bx, by, amrad) in self.drill_log.iter() {
            if !first { arr.push(','); }
            arr.push_str(&format!("[1,{},{},{}]", bx, by, amrad));
            first = false;
        }
        for &(ax, ay, amrad) in self.wall_log.iter() {
            if !first { arr.push(','); }
            arr.push_str(&format!("[2,{},{},{}]", ax, ay, amrad));
            first = false;
        }
        arr.push(']');
        let msg = format!("{{\"type\":\"terrain_damages\",\"log\":{}}}", arr);
        self.net.send_message(&msg);
    }

    /// Apply terrain ops log received from server on reconnect.
    /// Handles [0,cx,cy,r] explosions, [1,bx,by,amrad] drills, [2,ax,ay,amrad] walls.
    /// Also handles legacy 3-element [cx,cy,r] entries (old format = explosion).
    fn apply_terrain_sync(&mut self, msg: &str) {
        let key = "\"log\":[";
        let start = match msg.find(key) {
            Some(i) => i + key.len(),
            None => return,
        };
        let mut depth = 1i32;
        let mut end = start;
        for (i, ch) in msg[start..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 { end = start + i; break; }
                }
                _ => {}
            }
        }
        let content = &msg[start..end];
        if content.is_empty() { return; }

        let mut explosions: Vec<(i32, i32, i32)> = Vec::new();
        let mut pos = 0;
        while pos < content.len() {
            let sub_start = match content[pos..].find('[') {
                Some(i) => pos + i + 1,
                None => break,
            };
            let sub_end = match content[sub_start..].find(']') {
                Some(i) => sub_start + i,
                None => break,
            };
            let entry = &content[sub_start..sub_end];
            let nums: Vec<i32> = entry.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            match nums.as_slice() {
                // Legacy 3-element = explosion
                [cx, cy, r] => { explosions.push((*cx, *cy, *r)); }
                // type 0 = explosion
                [0, cx, cy, r] => { explosions.push((*cx, *cy, *r)); }
                // type 1 = drill tunnel
                [1, bx, by, amrad] => {
                    let bxf = *bx as f32; let byf = *by as f32;
                    let angle = *amrad as f32 / 1000.0;
                    self.apply_drill_at(bxf, byf, angle);
                    // Track in log so this client can also upload it
                    if !self.drill_log.iter().any(|&(x,y,a)| x==*bx && y==*by && a==*amrad) {
                        self.drill_log.push((*bx, *by, *amrad));
                    }
                }
                // type 2 = build wall
                [2, ax, ay, amrad] => {
                    let ax = *ax as f32; let ay = *ay as f32;
                    let angle = *amrad as f32 / 1000.0;
                    let cos_a = angle.cos(); let sin_a = angle.sin();
                    let half_len = 35i32; let half_thick = 35i32;
                    for i in -half_len..=half_len {
                        for j in -half_thick..=half_thick {
                            let wx = (ax + i as f32 * cos_a - j as f32 * sin_a).round() as i32;
                            let wy = (ay + i as f32 * sin_a + j as f32 * cos_a).round() as i32;
                            if wx >= 0 && wx < self.terrain.width as i32
                                && wy >= 0 && wy < self.terrain.height as i32 {
                                self.terrain.set(wx, wy, terrain::WOOD);
                            }
                        }
                    }
                    self.terrain_dirty = true;
                    // Track in log so this client can also upload it
                    let aix = ax as i32; let aiy = ay as i32;
                    if !self.wall_log.iter().any(|&(x,y,a)| x==aix && y==aiy && a==*amrad) {
                        self.wall_log.push((aix, aiy, *amrad));
                    }
                }
                _ => {}
            }
            pos = sub_end + 1;
        }

        if !explosions.is_empty() {
            #[cfg(target_arch = "wasm32")]
            {
                let debug_msg = format!("[SYNC] Replaying {} terrain ops\0", explosions.len());
                unsafe { console_log(debug_msg.as_ptr()); }
            }
            self.terrain.replay_damage(&explosions);
            self.terrain_dirty = true;
        }
    }

    /// Apply ball state snapshot from the active player to sync positions/health
    fn apply_ball_state(&mut self, msg: &str) {
        // Parse the balls array from the message
        // Format: {"type":"ball_state","balls":[{"x":..,"y":..,"vx":..,"vy":..,"hp":..,"alive":..}, ...]}
        let balls_key = "\"balls\":[";
        let start = match msg.find(balls_key) {
            Some(i) => i + balls_key.len(),
            None => return,
        };
        // Find the closing bracket
        let end = match msg[start..].rfind(']') {
            Some(i) => start + i,
            None => return,
        };
        let array_content = &msg[start..end];
        
        // Split by "},{" to get individual ball objects
        let mut ball_idx = 0;
        let mut pos = 0;
        while pos < array_content.len() && ball_idx < self.balls.len() {
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
                self.balls[ball_idx].x = x as f32;
            }
            if let Some(y) = parse_json_number(obj, "y") {
                self.balls[ball_idx].y = y as f32;
            }
            if let Some(vx) = parse_json_number(obj, "vx") {
                self.balls[ball_idx].vx = vx as f32;
            }
            if let Some(vy) = parse_json_number(obj, "vy") {
                self.balls[ball_idx].vy = vy as f32;
            }
            if let Some(hp) = parse_json_number(obj, "hp") {
                self.balls[ball_idx].health = hp as i32;
            }
            // Parse alive (boolean)
            if obj.contains("\"alive\":true") {
                self.balls[ball_idx].alive = true;
            } else if obj.contains("\"alive\":false") {
                self.balls[ball_idx].alive = false;
            }
            
            ball_idx += 1;
            pos = obj_end;
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            let debug_msg = format!("[SYNC] Applied ball_state for {} balls\0", ball_idx);
            unsafe { console_log(debug_msg.as_ptr()); }
        }
    }

    /// Local turn advancement (offline or fallback)
    fn advance_turn(&mut self) {
        self.last_pos_sent = None; // force a fresh send at the start of each turn
        if self.check_game_over() {
            return;
        }

        let n = self.balls.len();
        if n == 0 {
            return;
        }
        let start = self.current_ball;
        let mut next = (start + 1) % n;
        loop {
            if self.balls[next].alive {
                break;
            }
            next = (next + 1) % n;
            if next == start {
                break;
            }
        }
        self.current_ball = next;
        // CRITICAL: keep current_turn_index in sync with the ball's team so that
        // is_my_turn() remains accurate when advance_turn() is used as a fallback.
        if next < self.balls.len() {
            self.current_turn_index = self.balls[next].team as usize;
        }
        self.reset_turn_state();
    }

    /// Sync to a player's turn from the worker. Finds the next alive ball belonging to
    /// the given team (player_index) using round-robin so all balls get a turn.
    fn sync_to_player_turn(&mut self, player_index: usize) {
        if self.check_game_over() {
            return;
        }
        let team = player_index as u32;
        let n = self.balls.len();
        if n == 0 {
            return;
        }

        // Ensure last_ball_per_team has enough entries
        while self.last_ball_per_team.len() <= player_index {
            self.last_ball_per_team.push(None);
        }

        // Collect indices of all alive balls on this team
        let team_balls: Vec<usize> = (0..n)
            .filter(|&i| self.balls[i].alive && self.balls[i].team == team)
            .collect();

        if team_balls.is_empty() {
            // Fallback: just find any alive ball
            for i in 0..n {
                if self.balls[i].alive {
                    self.current_ball = i;
                    self.reset_turn_state();
                    return;
                }
            }
            return;
        }

        // Pick the next ball in rotation after the last one used
        let last = self.last_ball_per_team[player_index];
        let chosen = match last {
            Some(prev) => {
                // Find the team ball that comes after prev in the global ball list
                let mut pick = team_balls[0]; // default to first
                for &wi in &team_balls {
                    if wi > prev {
                        pick = wi;
                        break;
                    }
                }
                // If none found after prev, wrap around to first
                if pick <= prev {
                    pick = team_balls[0];
                }
                pick
            }
            None => team_balls[0],
        };

        self.last_ball_per_team[player_index] = Some(chosen);
        self.current_ball = chosen;
        #[cfg(target_arch = "wasm32")]
        {
            let ball_name = if chosen < self.balls.len() { self.balls[chosen].name.as_str() } else { "?" };
            let debug_msg = format!("[TURN] sync_to_player_turn({}): chose ball {} '{}', team_balls={:?}, last={:?}\0",
                player_index, chosen, ball_name, team_balls, last);
            unsafe { console_log(debug_msg.as_ptr()); }
        }
        self.reset_turn_state();
    }

    fn reset_turn_state(&mut self) {
        // Emit turn_start event so the UI can show whose turn it is
        if self.current_ball < self.balls.len() {
            let ball = &self.balls[self.current_ball];
            let player_name = self.net.player_names
                .get(ball.team as usize)
                .cloned()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| ball.name.clone());
            let event = format!("{{\"type\":\"turn_start\",\"name\":\"{}\",\"ball\":\"{}\"}}",
                sanitize_event_name(&player_name),
                sanitize_event_name(&ball.name));
            self.net.send_game_event(&event);
        }
        self.phase = Phase::Aiming;
        self.turn_timer = TURN_TIME;
        self.has_fired = false;
        self.firing_by_key = false;
        self.aim_locked = false;
        self.retreat_timer = 0.0;
        self.charging = false;
        self.charge_power = 0.0;
        self.teleport_mode = false;
        self.baseball_bat_mode = false;
        self.build_wall_mode = false;
        self.build_wall_anchor = None;
        self.airstrike_mode = None;
        self.airstrike_locked_x = None;
        self.teleport_locked_pos = None;
        self.bot_think_timer = 3.0;
        self.stuck_phase_timer = 0.0;
        self.non_active_stuck_timer = 0.0;
        self.pending_sync_send_terrain = false;
        
        // Reset movement budget for the current ball
        if self.current_ball < self.balls.len() {
            self.balls[self.current_ball].reset_movement_budget();
            self.aim_angle = if self.balls[self.current_ball].facing > 0.0 {
                -0.3
            } else {
                std::f32::consts::PI + 0.3
            };
        }
        
        self.rng_state = lcg(self.rng_state);
        self.wind = ((self.rng_state >> 16) as f32 / 65536.0 - 0.5) * 6.0;

        // Snap camera back to the new active ball after every turn change.
        // Clear free-look so auto_follow re-activates immediately, then start a
        // 2-second glide so the transition feels smooth rather than instant.
        // cam_target_zoom drives the zoom smoothly back to the default level.
        self.cam_free_timer = 0.0;
        self.cam_return_timer = 2.0;
        self.cam_target_zoom = DEFAULT_ZOOM;
    }

    fn check_game_over(&mut self) -> bool {
        let mut alive_teams: Vec<u32> = Vec::new();
        for w in &self.balls {
            if w.alive && !alive_teams.contains(&w.team) {
                alive_teams.push(w.team);
            }
        }
        if alive_teams.len() <= 1 {
            self.phase = Phase::GameOver;
            self.winning_team = alive_teams.first().copied();
            // Emit game_over event for UI toast
            let winner_name = self.winning_team
                .and_then(|t| self.net.player_names.get(t as usize).cloned())
                .filter(|n| !n.is_empty())
                .or_else(|| {
                    // Fall back to ball name
                    self.winning_team.and_then(|t| {
                        self.balls.iter().find(|b| b.team == t).map(|b| b.name.clone())
                    })
                })
                .unwrap_or_else(|| String::from("Someone"));
            let event = format!("{{\"type\":\"game_over\",\"winner\":\"{}\"}}",
                sanitize_event_name(&winner_name));
            self.net.send_game_event(&event);
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
                // Any `init` message means we (re)connected. Force a full turn sync
                // once the subsequent state/game_resync arrives.
                self.just_reconnected = true;
                #[cfg(target_arch = "wasm32")]
                if self.had_network_connection {
                    self.net.send_game_event(r#"{"type":"reconnected"}"#);
                }
                self.had_network_connection = true;
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
                // Always flag reconnect so state/game_resync handlers force-sync
                // unconditionally, even if turn index happens to already be 0.
                self.just_reconnected = true;
                if seed_u32 != self.rng_state || num_players != self.num_teams {
                        // Regenerate terrain with proper seed and team count
                        *self = Game::new_with_teams(seed_u32, num_players);
                        // Flag that we just reconnected — next `state` or `game_resync`
                        // must unconditionally sync the current turn/ball regardless of index.
                        self.just_reconnected = true;
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
                        self.had_network_connection = true;
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"ws_close\"") || msg.contains("\"type\": \"ws_close\"") {
                self.net.connected = false;
                #[cfg(target_arch = "wasm32")]
                {
                    let event = r#"{"type":"connection_lost"}"#;
                    self.net.send_game_event(event);
                }
                continue;
            }
            if msg.contains("\"type\":\"force_advance\"") || msg.contains("\"type\": \"force_advance\"") {
                // Server watchdog forced a turn skip — immediately sync to the authoritative state.
                // The server will also broadcast turn_advanced/state, but we act immediately
                // so the game visually unsticks even before those messages arrive.
                match self.phase {
                    Phase::Retreat => {
                        // During Retreat, zero the timer so the retreat phase ends on the next
                        // frame naturally. This lets terrain_damages be sent before the sync is
                        // applied and preserves any remaining movement for the current frame.
                        self.retreat_timer = 0.0;
                    }
                    Phase::Aiming | Phase::Charging | Phase::TurnEnd => {
                        // Will be overwritten by the coming turn_advanced, but unstick now
                        self.phase = Phase::TurnEnd;
                        self.turn_end_timer = 0.1;
                    }
                    _ => {
                        // During projectile/settling, just note that a sync is coming
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"game_over\"") || msg.contains("\"type\": \"game_over\"") {
                // Server detected game over (e.g. after skipping dead teams).
                // Apply the authoritative ball snapshot so all clients have up-to-date
                // health/alive state before displaying the game-over overlay.
                if msg.contains("\"balls\":[") {
                    for t in &mut self.ball_lerp_targets {
                        *t = None;
                    }
                    self.apply_ball_state(&msg);
                }
                self.phase = Phase::GameOver;
                // Set winning_team from the server's authoritative winnerTeam index so all
                // clients show the correct winner banner (not "Draw!" due to stale local state).
                if let Some(winner_team_raw) = parse_json_number(&msg, "winnerTeam") {
                    self.winning_team = if winner_team_raw >= 0.0 { Some(winner_team_raw as u32) } else { None };
                } else {
                    // Fallback: find the winner team by matching the winner name from the
                    // server against player names or ball names.
                    if let Some(winner) = parse_json_string(&msg, "winner") {
                        let team = self.net.player_names.iter().position(|n| *n == winner)
                            .or_else(|| self.balls.iter().find(|b| b.name == winner).map(|b| b.team as usize));
                        self.winning_team = team.map(|t| t as u32);
                        // Last resort: first alive ball's team
                        if self.winning_team.is_none() {
                            self.winning_team = self.balls.iter().find(|b| b.alive).map(|b| b.team);
                        }
                    }
                }
                if let Some(winner) = parse_json_string(&msg, "winner") {
                    let event = format!("{{\"type\":\"game_over\",\"winner\":\"{}\"}}",
                        sanitize_event_name(&winner));
                    self.net.send_game_event(&event);
                }
                continue;
            }
            if msg.contains("\"type\":\"turn_advanced\"") || msg.contains("\"type\": \"turn_advanced\"") {
                if let Some(player_index) = parse_turn_index_from_message(&msg) {
                    // Apply authoritative ball state (positions + health) embedded by the
                    // active player in end_turn. This corrects any physics divergence that
                    // accumulated during the turn (wind, hit detection, etc.) before the
                    // next turn starts.
                    if msg.contains("\"balls\":[") {
                        for t in &mut self.ball_lerp_targets {
                            *t = None;
                        }
                        self.apply_ball_state(&msg);
                    }
                    // Apply the authoritative wind value from the active player so all
                    // clients use the same wind next turn, eliminating trajectory divergence
                    // caused by independently-computed rng_state values.
                    if let Some(w) = parse_json_number(&msg, "wind") {
                        self.wind = w as f32;
                    }
                    // Apply any terrain damage delta forwarded by the server.
                    // This corrects any terrain divergence when wind caused projectiles
                    // to land at different pixels on different clients.
                    if msg.contains("\"terrain\":[") {
                        // Reuse apply_terrain_sync by remapping "terrain" key to "log"
                        let terrain_as_log = msg.replace("\"terrain\":[", "\"log\":[");
                        self.apply_terrain_sync(&terrain_as_log);
                    }
                    // Capture whether this is still our turn BEFORE updating current_turn_index,
                    // using the dedicated helper that checks Retreat phase as a reliable signal.
                    let deferred_was_my_turn = self.should_send_terrain_on_deferred_sync();
                    // Always update our stored turn index
                    self.current_turn_index = player_index;
                    match self.phase {
                        Phase::Aiming | Phase::Charging | Phase::TurnEnd => {
                            self.sync_to_player_turn(player_index);
                        }
                        _ => {
                            // Defer during Retreat / ProjectileFlying / Settling so we
                            // don't abort an in-progress retreat or mid-flight projectile.
                            self.pending_turn_sync = Some(player_index);
                            // Remember to send terrain_damages when the deferred sync is applied
                            // if this advance happened while it was our turn (e.g. watchdog fired
                            // during our retreat — we need to flush our latest terrain state first).
                            self.pending_sync_send_terrain = deferred_was_my_turn;
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
            if msg.contains("\"type\":\"current_turn\"") || msg.contains("\"type\": \"current_turn\"") {
                // 1-second heartbeat from the server confirming the authoritative turn state.
                // Use it to reconcile local confusion about whose turn it is (e.g. if a
                // turn_advanced message was dropped or the client got stuck in TurnEnd).
                if let Some(server_turn_index) = parse_json_number(&msg, "currentTurnIndex").map(|v| v as usize) {
                    let should_sync = server_turn_index != self.current_turn_index;
                    if should_sync {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let debug_msg = format!("[NET] current_turn heartbeat: server={}, local={} – syncing\0",
                                server_turn_index, self.current_turn_index);
                            unsafe { console_log(debug_msg.as_ptr()); }
                        }
                        let deferred_was_my_turn = self.should_send_terrain_on_deferred_sync();
                        self.current_turn_index = server_turn_index;
                        match self.phase {
                            Phase::Aiming | Phase::Charging | Phase::TurnEnd => {
                                self.sync_to_player_turn(server_turn_index);
                            }
                            _ => {
                                self.pending_turn_sync = Some(server_turn_index);
                                self.pending_sync_send_terrain = deferred_was_my_turn;
                            }
                        }
                    }
                    // Also sync the turn timer so clients don't drift
                    if let Some(remaining_ms) = parse_json_number(&msg, "turnTimeRemainingMs") {
                        let remaining_s = (remaining_ms / 1000.0) as f32;
                        // Only update timer from heartbeat when our local timer is noticeably
                        // out of sync (> 3 s difference) to avoid jittering the countdown.
                        if (self.turn_timer - remaining_s).abs() > TURN_TIMER_RESYNC_THRESHOLD {
                            self.turn_timer = remaining_s.min(TURN_TIME).max(0.0);
                        }
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"state\"") || msg.contains("\"type\": \"state\"") {
                // Handle state message to sync with server's current turn
                if let Some(current_turn_index) = parse_state_turn_index(&msg) {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let debug_msg = format!("[apply_network] State message: current_turn_index={}, ours={}, just_reconnected={}\0", current_turn_index, self.current_turn_index, self.just_reconnected);
                        unsafe { console_log(debug_msg.as_ptr()); }
                    }
                    // Always sync on reconnect (handles the case where both sides are 0)
                    // otherwise only sync when the index changed.
                    let should_sync = self.just_reconnected || current_turn_index != self.current_turn_index;
                    self.just_reconnected = false;
                    // Sync the local turn timer from the relative `turnTimeRemainingMs`
                    // field injected by the server into every state broadcast.
                    // This is reliable across clients since it's already a relative
                    // duration, unlike the absolute epoch-ms `turnEndTime` whose
                    // conversion requires wall-clock time (unavailable in WASM via
                    // macroquad's get_time() which counts from program start).
                    if let Some(remaining_ms) = parse_json_number(&msg, "turnTimeRemainingMs") {
                        let remaining_s = (remaining_ms / 1000.0) as f32;
                        self.turn_timer = remaining_s.min(TURN_TIME).max(0.0);
                        #[cfg(target_arch = "wasm32")]
                        {
                            let debug_msg = format!("[NET] state: synced turn_timer from turnTimeRemainingMs -> {:.1}s\0", self.turn_timer);
                            unsafe { console_log(debug_msg.as_ptr()); }
                        }
                    }
                    if should_sync {
                        let deferred_was_my_turn = self.should_send_terrain_on_deferred_sync();
                        self.current_turn_index = current_turn_index;
                        match self.phase {
                            Phase::Aiming | Phase::Charging | Phase::TurnEnd => {
                                self.sync_to_player_turn(current_turn_index);
                            }
                            _ => {
                                self.pending_turn_sync = Some(current_turn_index);
                                self.pending_sync_send_terrain = deferred_was_my_turn;
                            }
                        }
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"game_resync\"") || msg.contains("\"type\": \"game_resync\"") {
                // Full reconnect sync: restore positions, health, phase, and turn timer.
                // Apply ball state (same key "balls" as ball_state message)
                self.apply_ball_state(&msg);
                // Clear lerp targets — no stale remote data should fight the authoritative snap
                for t in &mut self.ball_lerp_targets {
                    *t = None;
                }
                // Determine the authoritative turn to sync to
                let turn_idx = parse_json_number(&msg, "currentTurnIndex")
                    .map(|v| v as usize)
                    .unwrap_or(self.current_turn_index);
                // sync_to_player_turn resets phase to Aiming and timer to TURN_TIME.
                // We will override both immediately after.
                self.current_turn_index = turn_idx;
                self.sync_to_player_turn(turn_idx);
                // Override current_ball with the server's authoritative ball index.
                // This corrects round-robin divergence after reconnect.
                if let Some(cbi) = parse_json_number(&msg, "currentBallIndex").map(|v| v as usize) {
                    if cbi < self.balls.len() {
                        self.current_ball = cbi;
                        if let Some(team) = self.balls.get(cbi).map(|b| b.team as usize) {
                            while self.last_ball_per_team.len() <= team {
                                self.last_ball_per_team.push(None);
                            }
                            self.last_ball_per_team[team] = Some(cbi);
                        }
                    }
                }
                // Restore phase from server
                let restored_phase = if let Some(phase_str) = parse_json_string(&msg, "phase") {
                    match phase_str {
                        "game_over" => Phase::GameOver,
                        "retreat"   => Phase::Retreat,
                        "projectile" | "settling" => Phase::Settling, // missed the flight — settle
                        "turn_end"  => Phase::TurnEnd,
                        _           => Phase::Aiming,
                    }
                } else {
                    Phase::Aiming
                };
                self.phase = restored_phase;
                // Restore turn timer from remaining time on server
                if let Some(remaining_ms) = parse_json_number(&msg, "turnTimeRemainingMs") {
                    self.turn_timer = (remaining_ms / 1000.0) as f32;
                }
                // Clear reconnect flag — full state has been applied
                self.just_reconnected = false;
                #[cfg(target_arch = "wasm32")]
                {
                    let debug_msg = format!("[RESYNC] game_resync applied: turn={}, phase={:?}, timer={:.1}\0",
                        turn_idx, self.phase, self.turn_timer);
                    unsafe { console_log(debug_msg.as_ptr()); }
                }
                continue;
            }
            if msg.contains("\"type\":\"input\"") || msg.contains("\"type\": \"input\"") {
                // Bug 2 fix: only consume pending_turn_sync when we are actually in a
                // state that expects a new turn to begin.  If an `input` echo arrives
                // while we are still in Retreat or Settling (from the *previous* turn),
                // consuming the pending sync here would call reset_turn_state() and
                // teleport the phase back to Aiming mid-retreat, causing permanent
                // desync.  The sync will be applied on the very next `input` message
                // that arrives once we've reached an appropriate phase.
                if matches!(self.phase, Phase::Aiming | Phase::Charging | Phase::TurnEnd) {
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        self.sync_to_player_turn(player_idx);
                    }
                }

                if let Some((player_index, input_str)) = parse_input_message(&msg) {
                    // Skip our own input echo
                    if self.net.my_player_index == Some(player_index) {
                        continue;
                    }
                    
                    // Use transmitted ball index (bi) if present — authoritative, avoids
                    // round-robin divergence (e.g. after reconnect).
                    let ball_idx_opt = if let Some(bi) = parse_bi_from_message(&msg) {
                        if bi < self.balls.len() {
                            // Keep local round-robin in sync for future turns
                            if let Some(team) = self.balls.get(bi).map(|b| b.team as usize) {
                                while self.last_ball_per_team.len() <= team {
                                    self.last_ball_per_team.push(None);
                                }
                                self.last_ball_per_team[team] = Some(bi);
                            }
                            self.current_ball = bi;
                            Some(bi)
                        } else {
                            self.find_ball_for_player(player_index)
                        }
                    } else if player_index == self.current_turn_index {
                        Some(self.current_ball)
                    } else {
                        self.find_ball_for_player(player_index)
                    };
                    if let Some(ball_idx) = ball_idx_opt {
                        // Parse and apply different input types
                        if let Some((angle_rad, power, weapon)) = parse_fire_input(&input_str) {
                            // Two-click weapons (Bat, Airstrike, Teleport, BuildWall) no longer
                            // send an initial Fire message, but guard here defensively in case an
                            // older client does. Their real actions arrive as BatSwing / AirstrikeTarget etc.
                            let is_two_click = matches!(
                                weapon,
                                Weapon::BaseballBat | Weapon::Airstrike | Weapon::NapalmStrike
                                    | Weapon::Teleport | Weapon::BuildWall
                            );
                            if !is_two_click {
                            // Snap ball to sender's authoritative position so projectile
                            // starts at the correct spot on all clients.
                            if let (Some(bx_f), Some(by_f)) = (
                                parse_json_number(&msg, "bx").map(|v| v as f32),
                                parse_json_number(&msg, "by").map(|v| v as f32),
                            ) {
                                if ball_idx < self.balls.len() {
                                    self.balls[ball_idx].x = bx_f;
                                    self.balls[ball_idx].y = by_f;
                                }
                            }
                            self.do_fire(ball_idx, angle_rad, power, weapon);
                            self.has_fired = true;
                            // Reset budget on the firing ball so remote players also get
                            // a fresh dodge window once their shot is in the air.
                            if self.phase == Phase::ProjectileFlying && ball_idx < self.balls.len() {
                                self.balls[ball_idx].reset_movement_budget();
                            }
                            } // end !is_two_click
                        } else if let Some(dir) = parse_walk_input(&input_str) {
                            if ball_idx < self.balls.len() {
                                physics::walk(&mut self.balls[ball_idx], &self.terrain, dir);
                            }
                        } else if input_str.contains("\"Jump\"") || input_str.contains("Jump") {
                            if ball_idx < self.balls.len() {
                                physics::jump(&mut self.balls[ball_idx]);
                                self.balls[ball_idx].movement_used += 20.0;
                            }
                        } else if input_str.contains("\"Backflip\"") || input_str.contains("Backflip") {
                            if ball_idx < self.balls.len() {
                                physics::backflip(&mut self.balls[ball_idx]);
                                self.balls[ball_idx].movement_used += 30.0;
                            }
                        } else if input_str.contains("AirstrikeTarget") {
                            // Spawn airstrike/napalm droplets for the remote player's click
                            if let Some(target_x) = parse_json_number(&input_str, "x").map(|v| v as f32) {
                                let weapon_name = parse_json_string(&input_str, "weapon").unwrap_or("Airstrike");
                                self.airstrike_droplets.clear();
                                if weapon_name.contains("Napalm") {
                                    let spacing = 60.0;
                                    for i in 0..7 {
                                        let x = target_x + (i as f32 - 3.0) * spacing;
                                        self.airstrike_droplets.push(AirstrikeDroplet {
                                            x, y: -50.0, vy: 0.0, alive: true,
                                            weapon_type: AirstrikeType::Napalm,
                                        });
                                    }
                                } else {
                                    let spacing = 80.0;
                                    for i in 0..5 {
                                        let x = target_x + (i as f32 - 2.0) * spacing;
                                        self.airstrike_droplets.push(AirstrikeDroplet {
                                            x, y: -50.0, vy: 0.0, alive: true,
                                            weapon_type: AirstrikeType::Explosive,
                                        });
                                    }
                                }
                                self.has_fired = true;
                                self.phase = Phase::ProjectileFlying;
                                if ball_idx < self.balls.len() {
                                    self.balls[ball_idx].reset_movement_budget();
                                }
                            }
                        } else if input_str.contains("BuildWallPlace") {
                            // Stamp the wall onto terrain for the remote player's placement
                            let ax = parse_json_number(&input_str, "ax").map(|v| v as f32);
                            let ay = parse_json_number(&input_str, "ay").map(|v| v as f32);
                            let angle = parse_json_number(&input_str, "angle").map(|v| v as f32);
                            if let (Some(ax), Some(ay), Some(angle)) = (ax, ay, angle) {
                                let cos_a = angle.cos();
                                let sin_a = angle.sin();
                                let half_len = 35i32;
                                let half_thick = 35i32;
                                for i in -half_len..=half_len {
                                    for j in -half_thick..=half_thick {
                                        let wx = (ax + i as f32 * cos_a - j as f32 * sin_a).round() as i32;
                                        let wy = (ay + i as f32 * sin_a + j as f32 * cos_a).round() as i32;
                                        if wx >= 0 && wx < self.terrain.width as i32
                                            && wy >= 0 && wy < self.terrain.height as i32 {
                                            self.terrain.set(wx, wy, terrain::WOOD);
                                        }
                                    }
                                }
                                self.terrain_dirty = true;
                                self.has_fired = true;
                                self.phase = Phase::Settling;
                                self.settle_timer = 0.0;
                                // Record for reconnect sync
                                self.wall_log.push((ax as i32, ay as i32, (angle * 1000.0) as i32));
                            }
                        } else if input_str.contains("TeleportTo") {
                            // Move the remote player's ball to target position
                            let tx = parse_json_number(&input_str, "x").map(|v| v as f32);
                            let ty = parse_json_number(&input_str, "y").map(|v| v as f32);
                            if let (Some(tx), Some(ty)) = (tx, ty) {
                                if ball_idx < self.balls.len() && self.balls[ball_idx].alive {
                                    self.balls[ball_idx].x = tx.clamp(0.0, self.terrain.width as f32);
                                    self.balls[ball_idx].y = ty.clamp(0.0, self.terrain.height as f32);
                                    self.balls[ball_idx].vx = 0.0;
                                    self.balls[ball_idx].vy = 0.0;
                                }
                                self.has_fired = true;
                                self.phase = Phase::Settling;
                                self.settle_timer = 0.0;
                            }
                        } else if input_str.contains("BatSwing") {
                            // Apply baseball bat knockback for the remote player's swing
                            if let Some(angle) = parse_json_number(&input_str, "angle").map(|v| v as f32) {
                                if ball_idx < self.balls.len() && self.balls[ball_idx].alive {
                                    let ball_x = self.balls[ball_idx].x;
                                    let ball_y = self.balls[ball_idx].y;
                                    let bat_range = 100.0;
                                    let knock_x = angle.cos() * 850.0;
                                    let knock_y = angle.sin() * 850.0 - 300.0;
                                    for i in 0..self.balls.len() {
                                        if i == ball_idx || !self.balls[i].alive { continue; }
                                        let dx = self.balls[i].x - ball_x;
                                        let dy = self.balls[i].y - ball_y;
                                        if (dx*dx + dy*dy).sqrt() < bat_range {
                                            self.balls[i].apply_knockback(knock_x, knock_y);
                                            self.balls[i].health = self.balls[i].health.saturating_sub(20);
                                            if self.balls[i].health == 0 {
                                                self.balls[i].alive = false;
                                            }
                                        }
                                    }
                                }
                                self.has_fired = true;
                                self.phase = Phase::Settling;
                                self.settle_timer = 0.0;
                            }
                        } else if input_str.contains("DrillFire") {
                            // Carve drill tunnel using the exact origin the active player sent
                            let bx = parse_json_number(&input_str, "bx").map(|v| v as f32);
                            let by = parse_json_number(&input_str, "by").map(|v| v as f32);
                            let angle = parse_json_number(&input_str, "angle").map(|v| v as f32);
                            if let (Some(bx), Some(by), Some(angle)) = (bx, by, angle) {
                                self.apply_drill_at(bx, by, angle);
                                // Track for reconnect sync (dedup)
                                let bxi = bx as i32; let byi = by as i32;
                                let amrad = (angle * 1000.0) as i32;
                                if !self.drill_log.iter().any(|&(x,y,a)| x==bxi && y==byi && a==amrad) {
                                    self.drill_log.push((bxi, byi, amrad));
                                }
                                self.has_fired = true;
                                self.phase = Phase::Settling;
                                self.settle_timer = 0.0;
                            }
                        }
                    }
                }  
                continue;
            }
            if msg.contains("\"type\":\"ball_state\"") || msg.contains("\"type\": \"ball_state\"") {
                // Hard-sync from the active player — clear lerp targets to avoid fighting the snap
                for t in &mut self.ball_lerp_targets {
                    *t = None;
                }
                self.apply_ball_state(&msg);
                continue;
            }
            if msg.contains("\"type\":\"terrain_sync\"") || msg.contains("\"type\": \"terrain_sync\"") {
                // Replay terrain damage events received from server on reconnect
                self.apply_terrain_sync(&msg);
                continue;
            }
            if msg.contains("\"type\":\"pos_update\"") || msg.contains("\"type\": \"pos_update\"") {
                // Real-time position stream from another player — store as lerp target.
                // Skip our own echoes.
                if let Some((bi, x, y, vx, vy)) = parse_pos_update_message(&msg) {
                    let is_own_ball = self.net.my_player_index
                        .and_then(|pi| self.find_ball_for_player(pi))
                        == Some(bi);
                    if !is_own_ball && bi < self.ball_lerp_targets.len() {
                        self.ball_lerp_targets[bi] = Some((x, y, vx, vy));
                    }
                }
                continue;
            }
            if msg.contains("\"type\":\"aim\"") || msg.contains("\"type\": \"aim\"") {
                // Handle aim angle updates from other players
                if let Some((player_index, aim_angle)) = parse_aim_message(&msg) {
                    // Skip our own aim echo
                    if self.net.my_player_index == Some(player_index) {
                        continue;
                    }
                    
                    // Find the ball for this player and update their local aim
                    if let Some(ball_idx) = self.find_ball_for_player(player_index) {
                        if ball_idx == self.current_ball {
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

        // Snapshot health/alive state before any updates so we can detect changes
        let health_snapshot: Vec<(bool, i32)> = self.balls.iter()
            .map(|b| (b.alive, b.health))
            .collect();

        self.apply_network_messages();

        for p in &mut self.particles {
            p.vy += 200.0 * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);

        // Apply camera inertia coast (runs every frame; bled away by auto_follow when active)
        self.cam.apply_momentum(dt);

        // Smoothly return zoom toward the default level while the camera is gliding back
        // to the active ball after a turn change. Not triggered by mid-turn panning.
        if self.cam_return_timer > 0.0 {
            let diff = self.cam_target_zoom - self.cam.zoom;
            if diff.abs() > 0.005 {
                let rate = (4.0 * dt).min(1.0);
                self.cam.zoom += diff * rate;
            } else {
                self.cam.zoom = self.cam_target_zoom;
            }
        }

        // Tick napalm fire pools every frame (persist across turns)
        for fp in &mut self.fire_pools {
            fp.tick(&mut self.balls, dt);
        }
        self.fire_pools.retain(|fp| fp.alive);

        match self.phase {
            Phase::Aiming | Phase::Charging => {
                self.turn_timer -= dt;
                let bot_team = self.current_turn_index;
                let is_bot_turn = self.net.player_is_bot.get(bot_team).copied().unwrap_or(false);
                // Bug 4 fix: when connected, the server manages bot turn pacing via
                // maybeBotTurn() — don't let the human client call end_turn() on
                // behalf of a bot, or the client enters TurnEnd with stale state.
                if self.turn_timer <= 0.0 && (self.is_my_turn() || (is_bot_turn && !self.net.connected)) {
                    self.end_turn();
                }

                // Watchdog for non-active players: if the turn timer has expired but
                // we are not the active player, nudge the server so it force-advances.
                // This unsticks games where the active client's WebSocket silently dropped
                // or DO alarms are unavailable (local dev). Sends a ping after 3 s, then
                // every 10 s until the server responds with turn_advanced/force_advance.
                if self.turn_timer <= 0.0 && !self.is_my_turn() && self.net.connected {
                    self.non_active_stuck_timer += dt;
                    let next_ping = if self.non_active_stuck_timer >= 3.0 {
                        let cycles = ((self.non_active_stuck_timer - 3.0) / 10.0) as u32;
                        3.0 + cycles as f32 * 10.0
                    } else {
                        3.0
                    };
                    if (self.non_active_stuck_timer >= 3.0) && 
                       ((self.non_active_stuck_timer - dt) < next_ping) {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let msg = format!("[WATCHDOG] Non-active player stuck {:.1}s past turn end — pinging server\0", self.non_active_stuck_timer);
                            unsafe { console_log(msg.as_ptr()); }
                        }
                        self.net.send_message(r#"{"type":"ping"}"#);
                    }
                }

                // ── Bot AI ──────────────────────────────────────────────────────
                // Bug 1 fix: when connected the server's maybeBotTurn() sends an
                // `input` message that every client replays via apply_network_messages.
                // Running the inline bot AI here as well causes a double-fire and
                // immediate state divergence between clients.
                if is_bot_turn && !self.has_fired && !self.net.connected {
                    self.bot_think_timer -= dt;
                    if self.bot_think_timer <= 0.0 {
                        if let Some(bot_ball_idx) = self.find_ball_for_player(bot_team) {
                            let bx = self.balls[bot_ball_idx].x;
                            let by = self.balls[bot_ball_idx].y;
                            // Find nearest living enemy ball
                            let mut best_dist = f32::MAX;
                            let mut best_angle = -0.5f32; // default: slightly upward
                            let mut found_enemy = false;
                            for w in &self.balls {
                                if !w.alive || w.team == bot_team as u32 { continue; }
                                let dx = w.x - bx;
                                let dy = w.y - by;
                                let dist = (dx * dx + dy * dy).sqrt();
                                if dist < best_dist {
                                    best_dist = dist;
                                    // Aim slightly above the target so the missile arcs in
                                    best_angle = dy.atan2(dx) - 0.25;
                                    found_enemy = true;
                                }
                            }
                            if found_enemy {
                                self.current_ball = bot_ball_idx;
                                self.aim_angle = best_angle;
                                self.selected_weapon = Weapon::HomingMissile;
                                let shooter_team = self.balls[bot_ball_idx].team;
                                let offset = BALL_RADIUS + 4.0;
                                let sx = bx + best_angle.cos() * offset;
                                let sy = by + best_angle.sin() * offset;
                                let proj = Projectile::new(sx, sy, best_angle, 80.0, Weapon::HomingMissile, shooter_team);
                                self.proj = Some(proj);
                                self.has_fired = true;
                                self.phase = Phase::ProjectileFlying;
                            } else {
                                self.end_turn();
                            }
                        } else {
                            self.end_turn();
                        }
                    }
                }
                // ────────────────────────────────────────────────────────────────
                // Skip physics for remote balls that are network-driven (pos_update stream);
                // running local physics on them fights the lerp and causes visible teleporting.
                let my_ball_phys = self.net.my_player_index
                    .and_then(|pi| self.find_ball_for_player(pi));
                for (bi, w) in self.balls.iter_mut().enumerate() {
                    if self.net.connected
                        && Some(bi) != my_ball_phys
                        && self.ball_lerp_targets.get(bi).copied().flatten().is_some()
                    {
                        continue; // position driven by network; no local physics needed
                    }
                    w.tick(&self.terrain, dt);
                }
                // Proximity check for mines during aiming — balls may walk into them
                if !self.placed_explosives.is_empty() {
                    let mut explosions = Vec::new();
                    let prox_trigger: Vec<bool> = self.placed_explosives.iter()
                        .map(|e| e.check_proximity(&self.balls))
                        .collect();
                    let any_triggered = prox_trigger.iter().any(|&t| t);
                    if any_triggered {
                        for (i, explosive) in self.placed_explosives.iter_mut().enumerate() {
                            if prox_trigger[i] {
                                let exp = explosive.explode(&mut self.terrain, &mut self.balls);
                                explosive.alive = false;
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                        }
                        self.placed_explosives.retain(|e| e.alive);
                        for exp in &explosions {
                            self.spawn_explosion_particles(exp);
                        }
                        // Mine exploded — switch to settling so damage is resolved
                        self.phase = Phase::Settling;
                        self.settle_timer = 0.0;
                    }
                }
                // If the current ball died (walked into water/lava), end turn immediately
                if self.current_ball < self.balls.len() && !self.balls[self.current_ball].alive {
                    self.end_turn();
                }
                if self.current_ball < self.balls.len() {
                    let (wx, wy) = {
                        let w = &self.balls[self.current_ball];
                        (w.x, w.y)
                    };
                    let alive = self.balls[self.current_ball].alive;
                    if alive {
                        self.auto_follow(wx, wy - 30.0, 4.0, dt);
                    }
                }
            }
            Phase::ProjectileFlying => {
                let my_ball_phys2 = self.net.my_player_index
                    .and_then(|pi| self.find_ball_for_player(pi));
                for (bi, w) in self.balls.iter_mut().enumerate() {
                    if self.net.connected
                        && Some(bi) != my_ball_phys2
                        && self.ball_lerp_targets.get(bi).copied().flatten().is_some()
                    {
                        continue;
                    }
                    w.tick(&self.terrain, dt);
                }
                let mut explosion_opt = None;
                let mut proj_died = false;
                
                // Handle regular projectile
                let mut proj_follow: Option<(f32, f32)> = None;
                if let Some(ref mut proj) = self.proj {
                    let (explosion, bomblets) = proj.tick(&mut self.terrain, &mut self.balls, self.wind, dt);
                    proj_follow = Some((proj.x, proj.y));
                    explosion_opt = explosion;
                    proj_died = !proj.alive;
                    if !bomblets.is_empty() {
                        self.cluster_bomblets.extend(bomblets);
                    }
                }
                if let Some((px, py)) = proj_follow {
                    self.auto_follow(px, py, 8.0, dt);
                }

                // Handle shotgun pellets
                if !self.shotgun_pellets.is_empty() {
                    let mut any_active = false;
                    let mut pellet_follow: Option<(f32, f32)> = None;
                    for pellet in &mut self.shotgun_pellets {
                        if pellet.alive {
                            let hit = pellet.tick(&mut self.terrain, &mut self.balls, dt);
                            if hit {
                                self.terrain_dirty = true;
                            }
                            if pellet.alive {
                                any_active = true;
                                pellet_follow = Some((pellet.x, pellet.y));
                            }
                        }
                    }
                    if let Some((px, py)) = pellet_follow {
                        self.auto_follow(px, py, 6.0, dt);
                    }
                    if !any_active {
                        self.shotgun_pellets.clear();
                    }
                }

                // Handle Uzi bullets — tick existing ones then spawn burst bullets
                {
                    // Tick burst timer and spawn next bullet if due
                    if self.uzi_burst_remaining > 0 {
                        self.uzi_burst_timer -= dt;
                        while self.uzi_burst_timer <= 0.0 && self.uzi_burst_remaining > 0 {
                            let a = self.uzi_burst_angle;
                            let s = self.uzi_burst_speed;
                            self.uzi_bullets.push(UziBullet {
                                x: self.uzi_burst_ox,
                                y: self.uzi_burst_oy,
                                vx: a.cos() * s,
                                vy: a.sin() * s,
                                alive: true,
                            });
                            // Pushback on shooter per bullet
                            let shooter = self.uzi_burst_shooter;
                            if shooter < self.balls.len() && self.balls[shooter].alive {
                                self.balls[shooter].apply_knockback(-a.cos() * 18.0, -a.sin() * 18.0);
                            }
                            self.uzi_burst_remaining -= 1;
                            self.uzi_burst_timer += 0.07;
                        }
                    }

                    if !self.uzi_bullets.is_empty() {
                        let mut any_active = false;
                        let mut bullet_follow: Option<(f32, f32)> = None;
                        for bullet in &mut self.uzi_bullets {
                            if bullet.alive {
                                let hit = bullet.tick(&mut self.terrain, &mut self.balls, dt);
                                if hit {
                                    self.terrain_dirty = true;
                                }
                                if bullet.alive {
                                    any_active = true;
                                    bullet_follow = Some((bullet.x, bullet.y));
                                }
                            }
                        }
                        if let Some((bx, by)) = bullet_follow {
                            self.auto_follow(bx, by, 5.0, dt);
                        }
                        if !any_active {
                            self.uzi_bullets.clear();
                        }
                    }
                }

                // Handle airstrike droplets
                if !self.airstrike_droplets.is_empty() {
                    let mut any_active = false;
                    let mut droplet_follow: Option<(f32, f32)> = None;
                    let mut explosions = Vec::new();
                    let mut new_fires: Vec<FirePool> = Vec::new();
                    for droplet in &mut self.airstrike_droplets {
                        if droplet.alive {
                            let (exp_opt, fire_opt) = droplet.tick(&mut self.terrain, &mut self.balls, dt);
                            if let Some(exp) = exp_opt {
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                            if let Some(fire) = fire_opt {
                                new_fires.push(fire);
                            }
                            if droplet.alive {
                                any_active = true;
                                droplet_follow = Some((droplet.x, droplet.y));
                            }
                        }
                    }
                    if let Some((dx, dy)) = droplet_follow {
                        self.auto_follow(dx, dy, 7.0, dt);
                    }
                    for exp in explosions {
                        self.spawn_explosion_particles(&exp);
                    }
                    self.fire_pools.extend(new_fires);
                    if !any_active {
                        self.airstrike_droplets.clear();
                    }
                }
                
                // Handle placed explosives (timer-based detonation + proximity trigger for mines)
                if !self.placed_explosives.is_empty() {
                    let mut explosions = Vec::new();
                    // First pass: check proximity with immutable balls borrow
                    let prox_trigger: Vec<bool> = self.placed_explosives.iter()
                        .map(|e| e.check_proximity(&self.balls))
                        .collect();
                    // Second pass: tick timers and explode triggered ones
                    for (i, explosive) in self.placed_explosives.iter_mut().enumerate() {
                        let timer_fired = explosive.tick(dt);
                        if timer_fired || prox_trigger[i] {
                            let exp = explosive.explode(&mut self.terrain, &mut self.balls);
                            if prox_trigger[i] { explosive.alive = false; }
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
                    let mut bomblet_follow: Option<(f32, f32)> = None;
                    for bomblet in &mut self.cluster_bomblets {
                        if bomblet.alive {
                            if let Some(exp) = bomblet.tick(&mut self.terrain, &mut self.balls, dt) {
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                            if bomblet.alive {
                                any_active = true;
                                bomblet_follow = Some((bomblet.x, bomblet.y));
                            }
                        }
                    }
                    if let Some((bx, by)) = bomblet_follow {
                        self.auto_follow(bx, by, 6.0, dt);
                    }
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                    if !any_active {
                        self.cluster_bomblets.clear();
                    }
                }
                
                // Check if all projectiles/effects are done.
                // NOTE: placed_explosives (mines/dynamite) are intentionally excluded —
                // they are persistent hazards that must not block turn progression.
                let all_done = self.proj.is_none() 
                    && self.shotgun_pellets.is_empty() 
                    && self.uzi_bullets.is_empty()
                    && self.uzi_burst_remaining == 0
                    && self.airstrike_droplets.is_empty()
                    && self.cluster_bomblets.is_empty();
                
                if proj_died || explosion_opt.is_some() {
                    self.proj = None;
                }

                // Stuck-phase watchdog: if projectile flying goes on too long, force-end
                self.stuck_phase_timer += dt;
                if self.stuck_phase_timer > 12.0 {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let s = "[WATCHDOG] ProjectileFlying stuck >12s, force-ending turn\0";
                        unsafe { console_log(s.as_ptr()); }
                    }
                    self.proj = None;
                    self.shotgun_pellets.clear();
                    self.uzi_bullets.clear();
                    self.uzi_burst_remaining = 0;
                    self.airstrike_droplets.clear();
                    self.cluster_bomblets.clear();
                    self.end_turn();
                }
                
                if all_done {
                    self.phase = Phase::Settling;
                    self.settle_timer = 0.0;
                    // Linger the camera on the last explosion/impact for a moment before
                    // returning to the active player (most noticeable after an airstrike).
                    const EXPLOSION_CAM_LINGER_SECS: f32 = 1.5;
                    self.cam_free_timer = self.cam_free_timer.max(EXPLOSION_CAM_LINGER_SECS);
                    // All players send terrain damage log so the worker always
                    // has the latest cumulative state for reconnect sync.
                    if self.net.connected {
                        self.send_terrain_damages();
                        // Only the active player sends authoritative ball positions.
                        if self.is_my_turn() {
                            self.send_ball_state();
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
                for w in &mut self.balls {
                    w.tick(&self.terrain, dt);
                }
                // Tick placed explosives during settling (proximity mines + dynamite fuse)
                if !self.placed_explosives.is_empty() {
                    let mut explosions = Vec::new();
                    let prox_trigger: Vec<bool> = self.placed_explosives.iter()
                        .map(|e| e.check_proximity(&self.balls))
                        .collect();
                    for (i, explosive) in self.placed_explosives.iter_mut().enumerate() {
                        let timer_fired = explosive.tick(dt);
                        if timer_fired || prox_trigger[i] {
                            let exp = explosive.explode(&mut self.terrain, &mut self.balls);
                            if prox_trigger[i] { explosive.alive = false; }
                            explosions.push(exp);
                            self.terrain_dirty = true;
                        }
                    }
                    self.placed_explosives.retain(|e| e.alive);
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                }
                let all_settled = self.balls.iter().all(|w| w.is_settled());
                if all_settled || self.settle_timer > SETTLE_TIMEOUT {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let msg = format!("[PHASE] Settling done. pending_sync={:?}, connected={}, is_my_turn={}, has_fired={}\0",
                            self.pending_turn_sync, self.net.connected, self.is_my_turn(), self.has_fired);
                        unsafe { console_log(msg.as_ptr()); }
                    }
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        // Server already advanced the turn (watchdog/early advance).
                        // Send terrain before syncing so the server has our latest state.
                        if self.net.connected && self.pending_sync_send_terrain {
                            self.send_terrain_damages();
                        }
                        self.pending_sync_send_terrain = false;
                        self.sync_to_player_turn(player_idx);
                    } else if self.has_fired && self.is_my_turn() {
                        // Send fresh ball state and full terrain ops after settling
                        if self.net.connected {
                            self.send_ball_state();
                            // Upload terrain ops so server has drill/wall changes for reconnect sync
                            self.send_terrain_damages();
                        }
                        // Active player: enter retreat phase - 5 seconds to move
                        self.phase = Phase::Retreat;
                        self.retreat_timer = 5.0;
                        // Reset movement budget for retreat
                        if self.current_ball < self.balls.len() {
                            self.balls[self.current_ball].reset_movement_budget();
                        }
                    } else if !self.net.connected {
                        self.end_turn();
                    } else if self.is_my_turn() {
                        self.end_turn();
                    } else {
                        // Not our turn in multiplayer: report our own ball positions/health
                        // so the server has up-to-date alive status for win detection
                        // (important during bot turns when no active player sends ball_state).
                        if self.net.connected {
                            self.send_ball_state();
                        }
                        // Enter TurnEnd and wait for worker to advance the turn
                        self.phase = Phase::TurnEnd;
                        self.turn_end_timer = TURN_END_DELAY;
                    }
                }
                if self.current_ball < self.balls.len() {
                    let w = &self.balls[self.current_ball];
                    self.cam.follow(w.x, w.y - 30.0, 3.0, dt);
                }
            }
            Phase::Retreat => {
                self.retreat_timer -= dt;
                // Tick ball physics — skip remote network-driven balls to avoid teleporting
                let my_ball_phys3 = self.net.my_player_index
                    .and_then(|pi| self.find_ball_for_player(pi));
                for (bi, w) in self.balls.iter_mut().enumerate() {
                    if self.net.connected
                        && Some(bi) != my_ball_phys3
                        && self.ball_lerp_targets.get(bi).copied().flatten().is_some()
                    {
                        continue;
                    }
                    w.tick(&self.terrain, dt);
                }

                // Tick in-flight projectile (Mortar fires then enters Retreat so player
                // can move while the shell is travelling)
                let mut retreat_proj_follow: Option<(f32, f32)> = None;
                let mut retreat_proj_died = false;
                let mut retreat_proj_explosion = None;
                if let Some(ref mut proj) = self.proj {
                    let (explosion, bomblets) = proj.tick(&mut self.terrain, &mut self.balls, self.wind, dt);
                    retreat_proj_follow = Some((proj.x, proj.y));
                    retreat_proj_explosion = explosion;
                    retreat_proj_died = !proj.alive;
                    if !bomblets.is_empty() {
                        self.cluster_bomblets.extend(bomblets);
                    }
                }
                if retreat_proj_died { self.proj = None; }
                if let Some(ref exp) = retreat_proj_explosion {
                    self.spawn_explosion_particles(exp);
                    self.terrain_dirty = true;
                }

                // Tick cluster bomblets spawned by Mortar during Retreat
                if !self.cluster_bomblets.is_empty() {
                    let mut explosions = Vec::new();
                    for bomblet in &mut self.cluster_bomblets {
                        if bomblet.alive {
                            if let Some(exp) = bomblet.tick(&mut self.terrain, &mut self.balls, dt) {
                                explosions.push(exp);
                                self.terrain_dirty = true;
                            }
                        }
                    }
                    self.cluster_bomblets.retain(|b| b.alive);
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                }

                // Tick placed explosives (timer-based + proximity trigger for mines)
                if !self.placed_explosives.is_empty() {
                    let mut explosions = Vec::new();
                    let prox_trigger: Vec<bool> = self.placed_explosives.iter()
                        .map(|e| e.check_proximity(&self.balls))
                        .collect();
                    for (i, explosive) in self.placed_explosives.iter_mut().enumerate() {
                        let timer_fired = explosive.tick(dt);
                        if timer_fired || prox_trigger[i] {
                            let exp = explosive.explode(&mut self.terrain, &mut self.balls);
                            if prox_trigger[i] { explosive.alive = false; }
                            explosions.push(exp);
                            self.terrain_dirty = true;
                        }
                    }
                    self.placed_explosives.retain(|e| e.alive);
                    for exp in &explosions {
                        self.spawn_explosion_particles(exp);
                    }
                }

                // If current ball died during retreat (fell in water/lava), end turn now
                if self.current_ball < self.balls.len() && !self.balls[self.current_ball].alive {
                    self.retreat_timer = 0.0; // Force turn end
                }
                // Follow projectile while in flight, otherwise follow current ball
                if let Some((px, py)) = retreat_proj_follow {
                    let rpx = px; let rpy = py;
                    self.auto_follow(rpx, rpy, 7.0, dt);
                } else if self.current_ball < self.balls.len() {
                    let (wx, wy) = {
                        let w = &self.balls[self.current_ball];
                        (w.x, w.y)
                    };
                    if self.balls[self.current_ball].alive {
                        self.cam.follow(wx, wy - 30.0, 4.0, dt);
                    }
                }
                // When retreat time expires AND all in-flight effects are resolved, end turn.
                // NOTE: placed_explosives (mines) are excluded — they persist across turns
                // and must not prevent retreat from ending.
                let retreat_all_done = self.proj.is_none()
                    && self.cluster_bomblets.is_empty();
                if self.retreat_timer <= 0.0 && retreat_all_done {
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        // Server already advanced while we were retreating (watchdog).
                        // Send terrain so the server has our final map state before syncing.
                        if self.net.connected && self.pending_sync_send_terrain {
                            self.send_terrain_damages();
                        }
                        self.pending_sync_send_terrain = false;
                        self.sync_to_player_turn(player_idx);
                    } else {
                        self.end_turn();
                    }
                }
                // Safety watchdog: if retreat timer has been negative for >12 extra seconds
                // (e.g., cluster bomblets or mortar shell never resolved), force-end turn.
                if self.retreat_timer < -12.0 {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let s = "[WATCHDOG] Retreat stuck >12s extra, force-ending turn\0";
                        unsafe { console_log(s.as_ptr()); }
                    }
                    self.proj = None;
                    self.cluster_bomblets.clear();
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        if self.net.connected && self.pending_sync_send_terrain {
                            self.send_terrain_damages();
                        }
                        self.pending_sync_send_terrain = false;
                        self.sync_to_player_turn(player_idx);
                    } else {
                        self.end_turn();
                    }
                }
            }
            Phase::TurnEnd => {
                self.turn_end_timer -= dt;
                for w in &mut self.balls {
                    w.tick(&self.terrain, dt);
                }
                if self.turn_end_timer <= 0.0 {
                    if let Some(player_idx) = self.pending_turn_sync.take() {
                        // Server advanced while we were in TurnEnd (or we entered TurnEnd from
                        // force_advance with an in-progress retreat). Send terrain if we were
                        // the active player so the server has our final map state.
                        if self.net.connected && self.pending_sync_send_terrain {
                            self.send_terrain_damages();
                        }
                        self.pending_sync_send_terrain = false;
                        self.sync_to_player_turn(player_idx);
                    } else if !self.net.connected {
                        self.advance_turn();
                    } else {
                        // Still waiting for turn_advanced from server. Count down an extra
                        // safety window (turn_end_timer is already ≤0 and going further
                        // negative — we treat -4.0 as "server is silent, force locally").
                        //
                        // Send a ping once per second (at -1.0, -2.0, -3.0) so the server
                        // knows we are waiting. The server will either force-advance (if the
                        // turn expired) or re-broadcast state so we can reconcile whose turn it is.
                        if self.turn_end_timer <= TURN_END_PING_START {
                            // Fire a ping each time the timer crosses an integer second mark
                            // going negative (i.e., floor transitions from n to n-1).
                            let prev = self.turn_end_timer + dt;
                            let crossed_second = prev.floor() > self.turn_end_timer.floor();
                            if crossed_second && self.turn_end_timer > TURN_END_FORCE_ADVANCE {
                                self.net.send_message(r#"{"type":"ping"}"#);
                            }
                        }
                        if self.turn_end_timer < TURN_END_FORCE_ADVANCE {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let s = "[TURN] Safety timeout: force-advancing because server never sent turn_advanced\0";
                                unsafe { console_log(s.as_ptr()); }
                            }
                            // Re-send end_turn in case our original message was dropped.
                            // The server ignores it from non-active players, but if we
                            // ARE the active player it will unstick the server too.
                            if self.net.connected {
                                let mut balls_json = String::from("[");
                                for (i, ball) in self.balls.iter().enumerate() {
                                    if i > 0 { balls_json.push(','); }
                                    balls_json.push_str(&format!(
                                        r#"{{"x":{:.1},"y":{:.1},"vx":{:.2},"vy":{:.2},"hp":{},"alive":{}}}"#,
                                        ball.x, ball.y, ball.vx, ball.vy, ball.health, ball.alive
                                    ));
                                }
                                balls_json.push(']');
                                let et_msg = format!(
                                    r#"{{"type":"end_turn","bi":{},"balls":{}}}"#,
                                    self.current_ball, balls_json,
                                );
                                self.net.send_message(&et_msg);
                            }
                            self.advance_turn();
                        }
                    }
                }
            }
            Phase::GameOver => {}
        }

        self.cam
            .clamp_to_world(terrain::PLAYABLE_LAND_WIDTH, self.terrain.height as f32);

        // ── Position streaming ──────────────────────────────────────────────
        // The active player streams their ball position at ~30 Hz.  Remote
        // clients receive this and use it to lerp-correct their local copy.
        if self.net.connected {
            let current_time = get_time() as f32;
            if current_time - self.last_pos_send > 0.016 {  // ~60 Hz — one update per frame
                // Which ball should we stream?  During retreat we always control
                // current_ball; during other phases, find our own ball.
                let my_ball_opt = self.net.my_player_index.and_then(|pi| {
                    match self.phase {
                        Phase::Aiming | Phase::Charging => {
                            if self.is_my_turn() { Some(self.current_ball) } else { None }
                        }
                        Phase::Retreat => Some(self.current_ball),
                        Phase::ProjectileFlying => self.find_ball_for_player(pi),
                        _ => None,
                    }
                });
                if let Some(bi) = my_ball_opt {
                    if bi < self.balls.len() && self.balls[bi].alive {
                        let b = &self.balls[bi];
                        // Only transmit when something actually changed (rounds to 1dp precision)
                        let changed = match self.last_pos_sent {
                            None => true,
                            Some((lbi, lx, ly, lvx, lvy)) =>
                                lbi != bi
                                || (b.x - lx).abs() >= 0.05
                                || (b.y - ly).abs() >= 0.05
                                || (b.vx - lvx).abs() >= 0.05
                                || (b.vy - lvy).abs() >= 0.05,
                        };
                        if changed {
                            let msg = format!(
                                "{{\"type\":\"pos_update\",\"bi\":{},\"x\":{:.1},\"y\":{:.1},\"vx\":{:.1},\"vy\":{:.1}}}",
                                bi, b.x, b.y, b.vx, b.vy
                            );
                            self.net.send_message(&msg);
                            self.last_pos_sent = Some((bi, b.x, b.y, b.vx, b.vy));
                        }
                        self.last_pos_send = current_time;
                    }
                }
            }
        }

        // ── Lerp correction for remote balls ────────────────────────────────
        // After local physics ticked, nudge remote balls toward the
        // authoritative positions streamed by the other player.
        if self.net.connected {
            let my_ball = self.net.my_player_index
                .and_then(|pi| self.find_ball_for_player(pi));
            let n = self.balls.len().min(self.ball_lerp_targets.len());
            for bi in 0..n {
                // Never lerp our own ball
                if my_ball == Some(bi) {
                    continue;
                }
                if let Some((tx, ty, tvx, tvy)) = self.ball_lerp_targets[bi] {
                    let ball = &mut self.balls[bi];
                    if !ball.alive { continue; }
                    // Physics is skipped for network-driven balls, so snap directly to the
                    // authoritative position received from the active client (~60 Hz).
                    // With no local physics fighting the target, this produces smooth movement.
                    ball.x = tx;
                    ball.y = ty;
                    ball.vx = tvx;
                    ball.vy = tvy;
                }
            }
        }

        if self.terrain_dirty {
            self.terrain_image = self.terrain.bake_image();
            // Update texture data in-place to keep the same GL texture ID.
            // Recreating with from_image() deletes the old ID, which causes
            // "glBindTexture called with an already deleted texture" warnings
            // in WebGL when macroquad still has the old ID in its state.
            self.terrain_texture.update(&self.terrain_image);
            self.terrain_dirty = false;
        }

        // Detect damage/death and emit game events for UI toasts.
        // Resize cooldown vec in case balls were re-created (new game).
        if self.ball_event_cooldown.len() < self.balls.len() {
            self.ball_event_cooldown.resize(self.balls.len(), 0.0);
        }
        for cd in &mut self.ball_event_cooldown {
            if *cd > 0.0 { *cd -= dt; }
        }
        for (i, (&(was_alive, prev_hp), ball)) in health_snapshot.iter().zip(self.balls.iter()).enumerate() {
            if !was_alive { continue; }
            let cooldown = self.ball_event_cooldown.get(i).copied().unwrap_or(0.0);
            if !ball.alive && cooldown <= 0.0 {
                // Ball died this frame
                let name = sanitize_event_name(&ball.name);
                let event = format!("{{\"type\":\"died\",\"name\":\"{}\"}}", name);
                self.net.send_game_event(&event);
                if i < self.ball_event_cooldown.len() {
                    self.ball_event_cooldown[i] = 5.0;
                }
            } else if ball.alive && ball.health < prev_hp && cooldown <= 0.0 {
                let damage = prev_hp - ball.health;
                if damage >= 5 {
                    let name = sanitize_event_name(&ball.name);
                    let event = format!("{{\"type\":\"hit\",\"name\":\"{}\",\"damage\":{},\"hp\":{}}}", name, damage, ball.health);
                    self.net.send_game_event(&event);
                    if i < self.ball_event_cooldown.len() {
                        self.ball_event_cooldown[i] = 0.8;
                    }
                }
            }
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
        let team = self.balls.get(self.current_ball).map(|w| w.team as usize).unwrap_or(0);
        
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

    fn draw(&mut self) {
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

        hud::draw_ball_world(&self.balls, self.current_ball);

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
        
        // Draw fire pools (napalm burns)
        for fp in &self.fire_pools {
            if fp.alive {
                let alpha = (fp.lifetime / 5.0).min(1.0); // fade as it burns out
                // Outer glow
                draw_circle(fp.x, fp.y, fp.radius, Color::new(1.0, 0.35, 0.0, alpha * 0.35));
                // Inner hot core (flickers based on lifetime)
                let flicker = ((fp.lifetime * 12.0).sin() * 0.15 + 0.85).max(0.0);
                draw_circle(fp.x, fp.y, fp.radius * 0.55 * flicker, Color::new(1.0, 0.75, 0.1, alpha * 0.75));
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

        // Build Wall placement preview
        if self.build_wall_mode && self.is_my_turn() {
            let (mx, my) = mouse_position();
            let world_pos = self.cam.to_macroquad().screen_to_world(vec2(mx, my));

            let (cx, cy, angle) = match self.build_wall_anchor {
                None => {
                    // Phase 1: wall follows cursor, uses aim angle for rotation preview
                    (world_pos.x, world_pos.y, self.aim_angle)
                }
                Some((ax, ay)) => {
                    // Phase 2: wall is anchored, rotates toward cursor
                    let dx = world_pos.x - ax;
                    let dy = world_pos.y - ay;
                    let a = if dx.abs() < 0.5 && dy.abs() < 0.5 { self.aim_angle } else { dy.atan2(dx) };
                    (ax, ay, a)
                }
            };

            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let hl = 35.0f32;
            let ht = 4.0f32;

            let c0 = vec2(cx + hl * cos_a - ht * (-sin_a), cy + hl * sin_a - ht * cos_a);
            let c1 = vec2(cx - hl * cos_a - ht * (-sin_a), cy - hl * sin_a - ht * cos_a);
            let c2 = vec2(cx - hl * cos_a + ht * (-sin_a), cy - hl * sin_a + ht * cos_a);
            let c3 = vec2(cx + hl * cos_a + ht * (-sin_a), cy + hl * sin_a + ht * cos_a);

            let fill   = Color::new(0.55, 0.35, 0.15, 0.50);
            let border = Color::new(0.9, 0.7, 0.3, 1.0);

            draw_triangle(c0, c1, c2, fill);
            draw_triangle(c0, c2, c3, fill);
            draw_line(c0.x, c0.y, c1.x, c1.y, 2.0, border);
            draw_line(c1.x, c1.y, c2.x, c2.y, 2.0, border);
            draw_line(c2.x, c2.y, c3.x, c3.y, 2.0, border);
            draw_line(c3.x, c3.y, c0.x, c0.y, 2.0, border);

            // Phase 2: draw a rotation guide line from anchor to cursor
            if let Some((ax, ay)) = self.build_wall_anchor {
                draw_line(ax, ay, world_pos.x, world_pos.y, 1.0,
                    Color::new(0.9, 0.9, 0.4, 0.6));
                draw_circle(ax, ay, 3.0, Color::new(0.9, 0.7, 0.3, 1.0));
            }
        }

        // Teleport preview: ghost circle + crosshair at cursor
        if self.teleport_mode && self.is_my_turn() {
            let (mx, my) = mouse_position();
            let world_pos = self.cam.to_macroquad().screen_to_world(vec2(mx, my));
            let r = BALL_RADIUS;
            draw_circle(world_pos.x, world_pos.y, r, Color::new(0.4, 0.85, 1.0, 0.35));
            draw_circle_lines(world_pos.x, world_pos.y, r, 2.0, Color::new(0.4, 0.9, 1.0, 0.9));
            let gap = r * 0.5;
            let arm = r * 1.5;
            let c = Color::new(0.4, 0.9, 1.0, 0.8);
            draw_line(world_pos.x - arm - gap, world_pos.y, world_pos.x - gap, world_pos.y, 1.5, c);
            draw_line(world_pos.x + gap,       world_pos.y, world_pos.x + arm + gap, world_pos.y, 1.5, c);
            draw_line(world_pos.x, world_pos.y - arm - gap, world_pos.x, world_pos.y - gap, 1.5, c);
            draw_line(world_pos.x, world_pos.y + gap,       world_pos.x, world_pos.y + arm + gap, 1.5, c);
        }

        // Drill tunnel preview: blue rectangle along aim direction
        if self.selected_weapon == Weapon::Drill
            && (self.phase == Phase::Aiming || self.phase == Phase::Charging)
            && !self.has_fired && self.is_my_turn()
        {
            let idx = self.current_ball;
            if idx < self.balls.len() && self.balls[idx].alive {
                let bx = self.balls[idx].x;
                let by = self.balls[idx].y;
                let angle = self.aim_angle;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                // Match carve dims: back=30, fwd=250, half_w=30
                let half_len = 140.0f32; // (250+30)/2
                let half_w   = 30.0f32;
                let mid_off  = 110.0f32; // (250-30)/2 — center offset from ball
                let cx = bx + cos_a * mid_off;
                let cy = by + sin_a * mid_off;
                let px = -sin_a;
                let py =  cos_a;
                let c0 = vec2(cx + cos_a * half_len - px * half_w, cy + sin_a * half_len - py * half_w);
                let c1 = vec2(cx - cos_a * half_len - px * half_w, cy - sin_a * half_len - py * half_w);
                let c2 = vec2(cx - cos_a * half_len + px * half_w, cy - sin_a * half_len + py * half_w);
                let c3 = vec2(cx + cos_a * half_len + px * half_w, cy + sin_a * half_len + py * half_w);
                let fill   = Color::new(0.3, 0.4, 0.9, 0.22);
                let border = Color::new(0.5, 0.6, 1.0, 0.9);
                draw_triangle(c0, c1, c2, fill);
                draw_triangle(c0, c2, c3, fill);
                draw_line(c0.x, c0.y, c1.x, c1.y, 1.5, border);
                draw_line(c1.x, c1.y, c2.x, c2.y, 1.5, border);
                draw_line(c2.x, c2.y, c3.x, c3.y, 1.5, border);
                draw_line(c3.x, c3.y, c0.x, c0.y, 1.5, border);
            }
        }

        // Airstrike / NapalmStrike preview: vertical drop lines at each target X.
        // When no target is locked yet, the preview tracks the mouse/touch position so
        // the player can see exactly where tapping would place the strike.
        if let Some(airstrike_weapon) = self.airstrike_mode {
            if self.is_my_turn() {
                let locked = self.airstrike_locked_x.is_some();
                // Live preview tracks mouse position; confirmed target uses the locked X.
                let target_x = self.airstrike_locked_x.unwrap_or_else(|| {
                    let (cur_mx, cur_my) = mouse_position();
                    let (wx, _) = self.cam.screen_to_world(cur_mx, cur_my);
                    wx.clamp(0.0, self.terrain.width as f32)
                });
                let top_y = self.cam.y - self.cam.visible_height() / 2.0 - 50.0;
                let bot_y = self.cam.y + self.cam.visible_height() / 2.0;
                let (count, spacing, base_color) = match airstrike_weapon {
                    Weapon::NapalmStrike => (7usize, 60.0f32, Color::new(1.0, 0.45, 0.1, 1.0)),
                    _ =>                   (5usize, 80.0f32, Color::new(1.0, 0.15, 0.15, 1.0)),
                };
                // Locked = solid bright lines; preview = translucent dashed lines.
                let alpha = if locked { 1.0f32 } else { 0.55 };
                let color = Color::new(base_color.r, base_color.g, base_color.b, alpha);
                let half = count / 2;
                for i in 0..count {
                    let x = target_x + (i as f32 - half as f32) * spacing;
                    if locked {
                        // Solid line when target is confirmed
                        draw_line(x, top_y, x, bot_y, 2.5, color);
                    } else {
                        // Dashed line for live preview
                        let segments = 20;
                        let seg_h = (bot_y - top_y) / (segments as f32 * 2.0);
                        for s in 0..segments {
                            let y0 = top_y + s as f32 * seg_h * 2.0;
                            draw_line(x, y0, x, y0 + seg_h, 2.0, color);
                        }
                    }
                    // Arrow head
                    draw_triangle(
                        vec2(x, bot_y - 20.0),
                        vec2(x - 8.0, bot_y - 35.0),
                        vec2(x + 8.0, bot_y - 35.0),
                        color,
                    );
                }

                // Centre-drop reticle on the middle column — gives a clear screen-centre
                // indicator while panning, and a bright "locked here" marker after tapping.
                let mid_y = self.cam.y; // world-space screen centre
                let r_inner = if locked { 18.0f32 } else { 14.0 };
                let r_outer = r_inner + 8.0;
                let reticle_col = Color::new(base_color.r, base_color.g, base_color.b, if locked { 1.0 } else { 0.75 });
                draw_circle_lines(target_x, mid_y, r_inner, 2.0, reticle_col);
                draw_circle_lines(target_x, mid_y, r_outer, 1.2, reticle_col);
                // Cross-hair ticks
                let tick = 10.0f32;
                draw_line(target_x - r_outer - tick, mid_y, target_x - r_outer, mid_y, 2.0, reticle_col);
                draw_line(target_x + r_outer, mid_y, target_x + r_outer + tick, mid_y, 2.0, reticle_col);
                draw_line(target_x, mid_y - r_outer - tick, target_x, mid_y - r_outer, 2.0, reticle_col);
                draw_line(target_x, mid_y + r_outer, target_x, mid_y + r_outer + tick, 2.0, reticle_col);
            }
        }

        for p in &self.particles {
            let alpha = (p.life / 1.0).min(1.0);
            let c = Color::new(p.color.r, p.color.g, p.color.b, p.color.a * alpha);
            draw_circle(p.x, p.y, p.size, c);
        }

        // Teleport crosshair drawn in WORLD space (camera still active here).
        let teleport_locked_for_hint = if self.teleport_mode && self.is_my_turn() {
            let (wx, wy, locked) = if let Some((lx, ly)) = self.teleport_locked_pos {
                (lx, ly, true)
            } else {
                // Preview follows mouse/touch so the player sees exactly where they'll teleport.
                let (cur_mx, cur_my) = mouse_position();
                let (pwx, pwy) = self.cam.screen_to_world(cur_mx, cur_my);
                (pwx.clamp(0.0, self.terrain.width as f32),
                 pwy.clamp(0.0, self.terrain.height as f32),
                 false)
            };
            let alpha = if locked { 1.0f32 } else { 0.65 };
            let col = Color::new(0.35, 0.95, 1.0, alpha);
            let r = 20.0f32;
            let thick = if locked { 2.5 } else { 1.8 };
            // Outer ring + ticks
            draw_circle_lines(wx, wy, r, thick, col);
            draw_circle_lines(wx, wy, r * 0.35, thick, col);
            let tick = 10.0f32;
            draw_line(wx - r - tick, wy, wx - r, wy, thick, col);
            draw_line(wx + r, wy, wx + r + tick, wy, thick, col);
            draw_line(wx, wy - r - tick, wx, wy - r, thick, col);
            draw_line(wx, wy + r, wx, wy + r + tick, thick, col);
            Some(locked)
        } else {
            None
        };

        set_default_camera();

        // Build Wall mode: show placement hint at top of screen
        if self.build_wall_mode && self.is_my_turn() {
            let hint = if self.build_wall_anchor.is_none() {
                "[ BUILD WALL ]  Click to set position"
            } else {
                "[ BUILD WALL ]  Click to set rotation"
            };
            let sw = screen_width();
            let tw = measure_text(hint, None, 22, 1.0).width;
            draw_text(hint, sw / 2.0 - tw / 2.0, 58.0, 22.0, Color::new(0.9, 0.75, 0.3, 1.0));
        }

        // Teleport hint text (screen space, drawn after set_default_camera).
        if let Some(locked) = teleport_locked_for_hint {
            let hint = if locked {
                "DESTINATION SET \u{2014} Tap to reposition \u{2022} Press FIRE to warp"
            } else {
                "[ TELEPORT ]  Tap anywhere to set destination"
            };
            let sw = screen_width();
            let tw = measure_text(hint, None, 22, 1.0).width;
            let hint_col = if locked {
                Color::new(0.85, 1.0, 0.35, 1.0)
            } else {
                Color::new(0.4, 0.9, 1.0, 1.0)
            };
            draw_text(hint, sw / 2.0 - tw / 2.0, 58.0, 22.0, hint_col);
        }

        // Baseball Bat hint
        if self.selected_weapon == Weapon::BaseballBat
            && (self.phase == Phase::Aiming || self.phase == Phase::Charging)
            && !self.has_fired && self.is_my_turn()
        {
            let hint = "[ BASEBALL BAT ]  Aim at an enemy and click to swing";
            let sw = screen_width();
            let tw = measure_text(hint, None, 22, 1.0).width;
            draw_text(hint, sw / 2.0 - tw / 2.0, 58.0, 22.0, Color::new(1.0, 0.75, 0.3, 1.0));
        }

        // Nuke hint
        if self.selected_weapon == Weapon::Nuke
            && (self.phase == Phase::Aiming || self.phase == Phase::Charging)
            && !self.has_fired && self.is_my_turn()
        {
            let hint = "[ NUKE ]  Charge and fire - explosion destroys the entire map";
            let sw = screen_width();
            let tw = measure_text(hint, None, 22, 1.0).width;
            draw_text(hint, sw / 2.0 - tw / 2.0, 58.0, 22.0, Color::new(1.0, 0.3, 0.3, 1.0));
        }

        // Airstrike / NapalmStrike targeting hint
        if let Some(airstrike_weapon) = self.airstrike_mode {
            if self.is_my_turn() {
                let locked = self.airstrike_locked_x.is_some();
                let name = if airstrike_weapon == Weapon::NapalmStrike { "NAPALM STRIKE" } else { "AIRSTRIKE" };
                let hint = if locked {
                    "TARGET SET \u{2014} Tap to reposition \u{2022} Press FIRE to launch"
                } else {
                    match airstrike_weapon {
                        Weapon::NapalmStrike => "[ NAPALM STRIKE ]  Tap anywhere to set target",
                        _ =>                   "[ AIRSTRIKE ]  Tap anywhere to set target",
                    }
                };
                let _ = name; // suppress unused warning
                let sw = screen_width();
                let tw = measure_text(hint, None, 22, 1.0).width;
                let col = if locked {
                    Color::new(1.0, 0.85, 0.2, 1.0) // gold when locked
                } else {
                    Color::new(1.0, 0.5, 0.2, 1.0)
                };
                draw_text(hint, sw / 2.0 - tw / 2.0, 58.0, 22.0, col);
            }
        }

        let is_my_turn = self.is_my_turn();
        let turn_owner = self.turn_owner_label();
        hud::draw_hud(
            &self.balls,
            self.current_ball,
            self.phase,
            self.selected_weapon,
            self.charge_power,
            if self.phase == Phase::Retreat { self.retreat_timer } else { self.turn_timer },
            self.wind,
            self.winning_team,
            is_my_turn,
            &turn_owner,
            self.net.connected,
        );

        // ── egui weapon menu ──────────────────────────────────────────────────
        let selected = self.selected_weapon;
        let mut new_category = self.weapon_menu_category;
        let mut chosen_weapon: Option<Weapon> = None;
        let mut close_menu = false;
        let menu_open = self.weapon_menu_open;

        egui_macroquad::ui(|ctx| {
            if menu_open {
                match hud::draw_weapon_menu_egui(ctx, selected, &mut new_category) {
                    hud::WeaponMenuResult::Select(w) => {
                        chosen_weapon = Some(w);
                    }
                    hud::WeaponMenuResult::Close => {
                        close_menu = true;
                    }
                    hud::WeaponMenuResult::None => {}
                }
            }
        });
        egui_macroquad::draw();

        // Apply weapon selection from egui.
        if let Some(w) = chosen_weapon {
            self.selected_weapon = w;
            self.weapon_menu_open = false;
            // Reset special modes, then activate the new weapon's mode.
            self.airstrike_mode = None;
            self.airstrike_locked_x = None;
            self.teleport_mode = false;
            self.teleport_locked_pos = None;
            self.build_wall_mode = false;
            self.build_wall_anchor = None;
            match w {
                Weapon::Teleport    => { self.teleport_mode = true; }
                Weapon::BuildWall   => { self.build_wall_mode = true; }
                Weapon::Airstrike   => { self.airstrike_mode = Some(Weapon::Airstrike); }
                Weapon::NapalmStrike => { self.airstrike_mode = Some(Weapon::NapalmStrike); }
                _ => {}
            }
        }
        if close_menu {
            self.weapon_menu_open = false;
        }
        self.weapon_menu_category = new_category;
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
        let level_w = self.terrain.width as f32;

        // Draw water bounded to the level width (not viewport width)
        draw_rectangle(
            0.0,
            water_y,
            level_w,
            self.terrain.height as f32 - water_y + 100.0,
            Color::new(0.08, 0.25, 0.55, 0.85),
        );

        // Draw waves across the top of the water, bounded to level width
        let wave_h = 3.0;
        let wave_len = 40.0;
        let steps = (level_w / 4.0) as i32 + 2;
        for i in 0..steps {
            let wx = i as f32 * 4.0;
            let wy = water_y + (wx / wave_len + t * 2.0).sin() * wave_h;
            draw_rectangle(wx, wy - 2.0, 5.0, 4.0, Color::new(0.2, 0.45, 0.8, 0.6));
        }
    }

    fn draw_aim(&self) {
        let idx = self.current_ball;
        if idx >= self.balls.len() || !self.balls[idx].alive {
            return;
        }
        let ball = &self.balls[idx];
        let bx = ball.x;
        let by = ball.y;
        let angle = self.aim_angle;
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let pi = std::f32::consts::PI;

        match self.selected_weapon {
            // ── Baseball Bat ─────────────────────────────────────────────────
            Weapon::BaseballBat => {
                let bat_range = 100.0f32;
                draw_circle(bx, by, bat_range, Color::new(1.0, 0.55, 0.1, 0.07));
                draw_circle_lines(bx, by, bat_range, 1.5, Color::new(1.0, 0.65, 0.2, 0.55));
                for i in 0..8 {
                    let a = i as f32 * pi * 0.25;
                    draw_line(
                        bx + a.cos() * (bat_range - 5.0), by + a.sin() * (bat_range - 5.0),
                        bx + a.cos() * (bat_range + 5.0), by + a.sin() * (bat_range + 5.0),
                        1.5, Color::new(1.0, 0.65, 0.2, 0.7),
                    );
                }
                let tip_x = bx + cos_a * bat_range;
                let tip_y = by + sin_a * bat_range;
                draw_line(bx, by, tip_x, tip_y, 3.0, Color::new(0.9, 0.7, 0.3, 0.85));
                draw_circle(tip_x, tip_y, 5.0, Color::new(0.95, 0.8, 0.4, 0.9));
                draw_circle_lines(tip_x, tip_y, 6.5, 1.5, Color::new(1.0, 0.9, 0.5, 0.8));
                for (i, w) in self.balls.iter().enumerate() {
                    if i == idx || !w.alive || w.team == ball.team { continue; }
                    let dx = w.x - bx;
                    let dy = w.y - by;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist < bat_range {
                        let pulse = (get_time() as f32 * 4.0).sin() * 0.25 + 0.75;
                        draw_circle_lines(w.x, w.y, BALL_RADIUS + 4.0, 2.0,
                            Color::new(1.0, 0.15, 0.15, pulse));
                        let label = format!("{:.0}", dist);
                        draw_text(&label, w.x - 8.0, w.y - BALL_RADIUS - 10.0, 14.0,
                            Color::new(1.0, 0.7, 0.3, 0.9));
                    }
                }
            }

            // ── Sniper Rifle ──────────────────────────────────────────────────
            Weapon::SniperRifle => {
                let max_range = 1600.0f32;
                let mut hit_x = bx + cos_a * max_range;
                let mut hit_y = by + sin_a * max_range;
                let mut t = BALL_RADIUS + 4.0;
                while t < max_range {
                    let rx = bx + cos_a * t;
                    let ry = by + sin_a * t;
                    if self.terrain.is_solid(rx as i32, ry as i32) {
                        hit_x = rx; hit_y = ry; break;
                    }
                    t += 2.0;
                }
                // Glow + core beam
                draw_line(bx, by, hit_x, hit_y, 5.0, Color::new(0.2, 1.0, 0.9, 0.15));
                draw_line(bx, by, hit_x, hit_y, 2.0, Color::new(0.5, 1.0, 1.0, 0.9));
                // Impact crosshair
                let r = 9.0f32;
                draw_circle_lines(hit_x, hit_y, r, 1.5, Color::new(0.2, 1.0, 0.9, 0.9));
                let arm = r * 1.6;
                let gap = r * 0.5;
                draw_line(hit_x - arm, hit_y, hit_x - gap, hit_y, 1.5, Color::new(0.2, 1.0, 0.9, 0.9));
                draw_line(hit_x + gap, hit_y, hit_x + arm, hit_y, 1.5, Color::new(0.2, 1.0, 0.9, 0.9));
                draw_line(hit_x, hit_y - arm, hit_x, hit_y - gap, 1.5, Color::new(0.2, 1.0, 0.9, 0.9));
                draw_line(hit_x, hit_y + gap, hit_x, hit_y + arm, 1.5, Color::new(0.2, 1.0, 0.9, 0.9));
            }

            // ── Uzi ───────────────────────────────────────────────────────────
            Weapon::Uzi => {
                // Single tight beam — no spread fan
                let range = 320.0f32;
                let ca = angle.cos();
                let sa = angle.sin();
                let mut ex = bx + ca * range;
                let mut ey = by + sa * range;
                let mut tr = BALL_RADIUS + 4.0;
                while tr < range {
                    let rx = bx + ca * tr;
                    let ry = by + sa * tr;
                    if self.terrain.is_solid(rx as i32, ry as i32) {
                        ex = rx; ey = ry; break;
                    }
                    ex = rx; ey = ry;
                    tr += 3.0;
                }
                draw_line(bx, by, ex, ey, 2.0, Color::new(0.95, 0.9, 0.25, 0.80));
                draw_circle(ex, ey, 3.0, Color::new(1.0, 0.95, 0.4, 0.90));
            }

            // ── Shotgun ───────────────────────────────────────────────────────
            Weapon::Shotgun => {
                let range = 240.0f32;
                let spread = 0.22f32;
                let pellets = 6usize;
                for i in 0..pellets {
                    let t_param = i as f32 / (pellets - 1) as f32;
                    let ray_angle = angle - spread + t_param * spread * 2.0;
                    let ca = ray_angle.cos();
                    let sa = ray_angle.sin();
                    let mut ex = bx + ca * range;
                    let mut ey = by + sa * range;
                    let mut tr = BALL_RADIUS + 4.0;
                    while tr < range {
                        let rx = bx + ca * tr;
                        let ry = by + sa * tr;
                        if self.terrain.is_solid(rx as i32, ry as i32) {
                            ex = rx; ey = ry; break;
                        }
                        ex = rx; ey = ry;
                        tr += 3.0;
                    }
                    draw_line(bx, by, ex, ey, 1.5, Color::new(0.95, 0.85, 0.5, 0.50));
                    draw_circle(ex, ey, 3.5, Color::new(1.0, 0.9, 0.4, 0.85));
                }
                // Spread cone outline
                let left_x  = bx + (angle - spread).cos() * range;
                let left_y  = by + (angle - spread).sin() * range;
                let right_x = bx + (angle + spread).cos() * range;
                let right_y = by + (angle + spread).sin() * range;
                draw_line(bx, by, left_x,  left_y,  1.0, Color::new(1.0, 0.8, 0.3, 0.30));
                draw_line(bx, by, right_x, right_y, 1.0, Color::new(1.0, 0.8, 0.3, 0.30));
                draw_line(left_x, left_y, right_x, right_y, 1.0, Color::new(1.0, 0.8, 0.3, 0.25));
            }

            // ── Homing Missile ────────────────────────────────────────────────
            Weapon::HomingMissile => {
                // Show trajectory arc
                let power_for_preview = if self.charging { self.charge_power } else { 50.0 };
                let traj = projectile::simulate_trajectory(
                    bx + cos_a * (BALL_RADIUS + 4.0), by + sin_a * (BALL_RADIUS + 4.0),
                    angle, power_for_preview, Weapon::HomingMissile, self.wind, &self.terrain,
                );
                for (i, &(tx, ty)) in traj.iter().enumerate() {
                    if i % 2 == 0 {
                        let alpha = 1.0 - i as f32 / traj.len().max(1) as f32;
                        draw_circle(tx, ty, 1.5, Color::new(1.0, 0.5, 0.2, alpha * 0.6));
                    }
                }
                // Lock-on reticle for nearest enemy
                let mut closest: Option<(f32, f32)> = None;
                let mut best_dist = f32::MAX;
                for (i, w) in self.balls.iter().enumerate() {
                    if i == idx || !w.alive || w.team == ball.team { continue; }
                    let dx = w.x - bx; let dy = w.y - by;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist < best_dist { best_dist = dist; closest = Some((w.x, w.y)); }
                }
                if let Some((tx, ty)) = closest {
                    let pulse = (get_time() as f32 * 4.0).sin() * 0.25 + 0.75;
                    let r = BALL_RADIUS + 8.0;
                    draw_circle_lines(tx, ty, r, 2.0, Color::new(1.0, 0.2, 0.2, pulse));
                    let arm = r * 0.5;
                    let gap = r * 0.65;
                    for &(sx, sy) in &[(-1.0f32,-1.0f32),(1.0,-1.0),(1.0,1.0),(-1.0,1.0)] {
                        draw_line(tx+sx*gap, ty+sy*gap, tx+sx*(gap+arm), ty+sy*gap, 2.0, Color::new(1.0,0.2,0.2,1.0));
                        draw_line(tx+sx*gap, ty+sy*gap, tx+sx*gap, ty+sy*(gap+arm), 2.0, Color::new(1.0,0.2,0.2,1.0));
                    }
                    // Line to target
                    draw_line(bx, by, tx, ty, 1.0, Color::new(1.0, 0.2, 0.2, 0.25));
                }
            }

            // ── Mine / Dynamite ───────────────────────────────────────────────
            Weapon::Mine | Weapon::Dynamite => {
                let radius = self.selected_weapon.explosion_radius();
                let pulse = (get_time() as f32 * 2.5).sin() * 0.15 + 0.55;
                let foot_x = bx;
                let foot_y = by + BALL_RADIUS + 2.0;
                draw_circle(foot_x, foot_y, radius, Color::new(1.0, 0.3, 0.1, 0.06));
                draw_circle_lines(foot_x, foot_y, radius, 1.5, Color::new(1.0, 0.3, 0.1, pulse));
                draw_circle(foot_x, foot_y, 4.0, Color::new(1.0, 0.3, 0.1, pulse));
                // Danger stripes on the foot marker
                for i in 0..4 {
                    let a = i as f32 * pi * 0.5;
                    draw_line(foot_x + a.cos()*5.0, foot_y + a.sin()*5.0,
                              foot_x + a.cos()*12.0, foot_y + a.sin()*12.0,
                              1.5, Color::new(1.0, 0.85, 0.0, 0.7));
                }
            }

            // ── All other projectile weapons ──────────────────────────────────
            _ => {
                let line_len = 50.0 + self.charge_power * 0.5;
                let ex = bx + cos_a * line_len;
                let ey = by + sin_a * line_len;
                draw_line(bx, by, ex, ey, 2.0, Color::new(1.0, 1.0, 0.4, 0.8));
                draw_circle(ex, ey, 4.0, Color::new(1.0, 0.2, 0.2, 0.8));
                draw_circle_lines(ex, ey, 6.0, 1.5, WHITE);

                let power_for_preview = if self.charging { self.charge_power } else { 50.0 };
                let traj = projectile::simulate_trajectory(
                    bx + cos_a * (BALL_RADIUS + 4.0),
                    by + sin_a * (BALL_RADIUS + 4.0),
                    angle, power_for_preview,
                    self.selected_weapon,
                    self.wind,
                    &self.terrain,
                );
                let impact = traj.last().copied();
                for (i, &(tx, ty)) in traj.iter().enumerate() {
                    if i % 2 == 0 {
                        let alpha = 1.0 - i as f32 / traj.len().max(1) as f32;
                        draw_circle(tx, ty, 1.5, Color::new(1.0, 1.0, 0.6, alpha * 0.6));
                    }
                }
                // Explosion radius circle at predicted impact point
                let radius = self.selected_weapon.explosion_radius();
                if radius > 0.0 {
                    if let Some((ix, iy)) = impact {
                        draw_circle(ix, iy, radius, Color::new(1.0, 0.45, 0.1, 0.08));
                        draw_circle_lines(ix, iy, radius, 1.5, Color::new(1.0, 0.55, 0.2, 0.65));
                        // Tick marks on explosion circle
                        for i in 0..8 {
                            let a = i as f32 * pi * 0.25;
                            draw_line(
                                ix + a.cos() * (radius - 4.0), iy + a.sin() * (radius - 4.0),
                                ix + a.cos() * (radius + 4.0), iy + a.sin() * (radius + 4.0),
                                1.5, Color::new(1.0, 0.55, 0.2, 0.8),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn lcg(s: u32) -> u32 {
    s.wrapping_mul(1103515245).wrapping_add(12345)
}

/// Escape a player/ball name for safe embedding in a JSON string value.
fn sanitize_event_name(name: &str) -> String {
    name.chars()
        .flat_map(|c| match c {
            '"'  => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            _    => vec![c],
        })
        .collect()
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

fn parse_bi_from_message(msg: &str) -> Option<usize> {
    let search = "\"bi\":";
    let pos = msg.find(search)?;
    let after = msg[pos + search.len()..].trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    if end == 0 { return None; }
    after[..end].parse().ok()
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

/// Parse a pos_update message: {"type":"pos_update","bi":N,"x":..,"y":..,"vx":..,"vy":..}
/// Returns (ball_index, x, y, vx, vy)
fn parse_pos_update_message(msg: &str) -> Option<(usize, f32, f32, f32, f32)> {
    let bi = parse_json_number(msg, "bi")? as usize;
    let x  = parse_json_number(msg, "x")? as f32;
    let y  = parse_json_number(msg, "y")? as f32;
    let vx = parse_json_number(msg, "vx")? as f32;
    let vy = parse_json_number(msg, "vy")? as f32;
    Some((bi, x, y, vx, vy))
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Balls".to_string(),
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
