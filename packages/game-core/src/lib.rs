//! Deterministic Worms-like game engine for WASM.
//! Exposes init_round, apply_input, get_state to JS.

mod physics;
mod projectile;
mod state;
mod terrain;
mod weapons;

use state::{GameState, Phase};
use terrain::{generate_terrain, DEFAULT_HEIGHT, DEFAULT_WIDTH};
use wasm_bindgen::prelude::*;

use physics::Worm;
use projectile::{fire_bazooka, tick_projectile, Projectile};
use state::WormState;
use weapons::{GameInput, Weapon};

#[wasm_bindgen]
pub struct Game {
    rng_seed: u32,
    terrain: terrain::Terrain,
    worms: Vec<Worm>,
    current_turn_index: u32,
    phase: Phase,
    projectile: Option<Projectile>,
    tick_count: u32,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Game {
        Game {
            rng_seed: 0,
            terrain: terrain::Terrain::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
            worms: Vec::new(),
            current_turn_index: 0,
            phase: Phase::Aiming,
            projectile: None,
            tick_count: 0,
        }
    }

    /// Initialize a round: seed, terrain_id (0 = default hills), worm positions per player.
    /// worm_positions: array of [x, y] in pixel coords, one per player.
    #[wasm_bindgen]
    pub fn init_round(
        &mut self,
        seed: u32,
        _terrain_id: u32,
        worm_positions: Vec<i32>,
    ) {
        self.rng_seed = seed;
        self.terrain = generate_terrain(seed, DEFAULT_WIDTH, DEFAULT_HEIGHT);
        self.worms.clear();
        let mut i = 0;
        while i + 1 < worm_positions.len() {
            let x = worm_positions[i];
            let y = worm_positions[i + 1];
            self.worms.push(Worm::new(x * 100, y * 100));
            i += 2;
        }
        self.current_turn_index = 0;
        self.phase = Phase::Aiming;
        self.projectile = None;
        self.tick_count = 0;
    }

    /// Apply one game input (from network or local). Call get_state after to read state.
    #[wasm_bindgen]
    pub fn apply_input(&mut self, input_json: &str) -> Result<(), JsValue> {
        let input: GameInput = serde_wasm_bindgen::from_str(input_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.apply_input_inner(input);
        Ok(())
    }

    /// Get current state as JSON for rendering. Includes worms and phase; terrain as separate buffer.
    #[wasm_bindgen]
    pub fn get_state_json(&self) -> String {
        let state = GameState::from_internal(
            self.phase,
            self.current_turn_index,
            &self.worms,
            &self.terrain,
        );
        serde_json::to_string(&state).unwrap_or_default()
    }

    /// Get terrain as flat Uint8Array: 0 = air, 1 = solid. Length = width * height.
    #[wasm_bindgen]
    pub fn get_terrain_buffer(&self) -> Vec<u8> {
        self.terrain
            .pixels
            .iter()
            .map(|&s| if s { 1 } else { 0 })
            .collect()
    }

    #[wasm_bindgen]
    pub fn terrain_width(&self) -> u32 {
        self.terrain.width
    }

    #[wasm_bindgen]
    pub fn terrain_height(&self) -> u32 {
        self.terrain.height
    }

    /// Advance one simulation tick (e.g. for projectile flight). Call from JS in requestAnimationFrame.
    #[wasm_bindgen]
    pub fn tick(&mut self) {
        self.tick_count += 1;
        if let Some(ref mut proj) = self.projectile {
            let done = tick_projectile(proj, &mut self.terrain, &mut self.worms);
            if done {
                self.projectile = None;
                self.phase = Phase::TurnEnd;
            }
        } else {
            // Settle worms (gravity)
            for w in &mut self.worms {
                w.tick(&self.terrain);
            }
        }
    }

    /// Whether the game is waiting for an input (aiming phase) or simulating (projectile).
    #[wasm_bindgen]
    pub fn is_waiting_for_input(&self) -> bool {
        matches!(self.phase, Phase::Aiming | Phase::Movement) && self.projectile.is_none()
    }
}

impl Game {
    fn apply_input_inner(&mut self, input: GameInput) {
        match input {
            GameInput::EndTurn => {
                self.phase = Phase::TurnEnd;
            }
            GameInput::Move { left, right, jump } => {
                if let Some(w) = self.worms.get_mut(self.current_turn_index as usize) {
                    if left {
                        w.velocity.x = (w.velocity.x - 800).max(-3000);
                        w.facing = -1;
                    }
                    if right {
                        w.velocity.x = (w.velocity.x + 800).min(3000);
                        w.facing = 1;
                    }
                    if jump {
                        w.velocity.y = -2500;
                    }
                }
            }
            GameInput::Fire {
                weapon,
                angle_deg,
                power_percent,
            } => {
                let idx = self.current_turn_index as usize;
                if let Some(w) = self.worms.get(idx) {
                    let sx = w.position.x / 100;
                    let sy = w.position.y / 100;
                    let power = power_percent.clamp(10, 100);
                    let angle = angle_deg.clamp(0, 360);
                    let mut proj = match weapon {
                        Weapon::Bazooka => fire_bazooka(sx, sy, angle, power),
                        Weapon::Grenade => fire_bazooka(sx, sy, angle, power), // same for now
                    };
                    // Small offset so we don't explode immediately on terrain
                    proj.pos.x += (w.facing * 15) * 100;
                    self.projectile = Some(proj);
                    self.phase = Phase::ProjectileFlying;
                }
            }
        }
    }
}
