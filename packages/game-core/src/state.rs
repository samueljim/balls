// Game state exposed to JS for rendering and sync.

use serde::{Deserialize, Serialize};

use crate::physics::Worm;
use crate::terrain::Terrain;

#[derive(Clone, Serialize, Deserialize)]
pub struct GameState {
    pub phase: Phase,
    pub current_turn_index: u32,
    pub worms: Vec<WormState>,
    #[serde(skip)]
    pub terrain: Option<Terrain>, // We export terrain as a separate buffer for JS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Movement,
    Aiming,
    ProjectileFlying,
    TurnEnd,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WormState {
    pub x: f32,
    pub y: f32,
    pub health: i32,
    pub facing: i32,
    pub player_index: u32,
}

impl GameState {
    pub fn from_internal(
        phase: Phase,
        current_turn_index: u32,
        worms: &[Worm],
        terrain: &Terrain,
    ) -> Self {
        let worms: Vec<WormState> = worms
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let (x, y) = w.position.to_f32();
                WormState {
                    x,
                    y,
                    health: w.health,
                    facing: w.facing,
                    player_index: i as u32,
                }
            })
            .collect();
        GameState {
            phase,
            current_turn_index,
            worms,
            terrain: Some(terrain.clone()),
        }
    }

    /// Serialize terrain as flat u8: 0 = air, 1 = solid (for canvas/JS).
    pub fn terrain_buffer(&self) -> Option<Vec<u8>> {
        self.terrain.as_ref().map(|t| {
            t.pixels
                .iter()
                .map(|&s| if s { 1u8 } else { 0u8 })
                .collect()
        })
    }

    pub fn terrain_width(&self) -> u32 {
        self.terrain.as_ref().map(|t| t.width).unwrap_or(0)
    }

    pub fn terrain_height(&self) -> u32 {
        self.terrain.as_ref().map(|t| t.height).unwrap_or(0)
    }
}
