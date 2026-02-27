#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Aiming,
    Charging,
    ProjectileFlying,
    Settling,
    Retreat,
    TurnEnd,
    GameOver,
}

impl Phase {
    pub fn label(&self) -> &str {
        match self {
            Phase::Aiming => "Your turn -- aim, then hold click to charge",
            Phase::Charging => "Charging power -- release to fire!",
            Phase::ProjectileFlying => "Watch out!",
            Phase::Settling => "Settling...",
            Phase::Retreat => "Retreat! Move to safety!",
            Phase::TurnEnd => "Next turn...",
            Phase::GameOver => "Game Over",
        }
    }

    /// Full input (aiming, weapon selection, firing)
    pub fn allows_input(&self) -> bool {
        matches!(self, Phase::Aiming | Phase::Charging)
    }

    /// Movement only (walk, jump, backflip)
    pub fn allows_movement(&self) -> bool {
        matches!(self, Phase::Aiming | Phase::Charging | Phase::ProjectileFlying | Phase::Retreat)
    }
}
