// Weapons and game input types.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weapon {
    Bazooka,
    Grenade,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GameInput {
    /// Fire a weapon (angle in degrees 0-360, power 0-100)
    Fire {
        weapon: Weapon,
        angle_deg: i32,
        power_percent: i32,
    },
    /// Move during movement phase
    Move { left: bool, right: bool, jump: bool },
    /// End turn without firing
    EndTurn,
}
