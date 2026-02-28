#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Weapon {
    // Basic Explosives
    Bazooka,
    Grenade,
    Shotgun,
    
    // Bouncing Weapons
    ClusterBomb,
    BananaBomb,
    HolyHandGrenade,
    
    // Placed Weapons
    Dynamite,
    Mine,
    
    // Projectile Weapons
    HomingMissile,
    Mortar,
    Sheep,
    
    // Air Weapons
    Airstrike,
    NapalmStrike,
    
    // Utility Items
    Teleport,
    Jetpack,
    Parachute,
    BaseballBat,
    Rope,
    
    // Precision Weapons
    SniperRifle,
    Uzi,
    
    // Fun Weapons
    BananaBonanza,
    Drill,
    SuperSheep,
    BuildWall,
}

#[derive(Clone, Copy, PartialEq)]
pub enum WeaponType {
    Projectile,
    Placed,
    Utility,
    Airstrike,
    Melee,
    Instant,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponCategory {
    Explosives,
    Ballistics,
    Utilities,
    Special,
}

impl Weapon {
    pub fn name(&self) -> &str {
        match self {
            Weapon::Bazooka => "Bazooka",
            Weapon::Grenade => "Grenade",
            Weapon::Shotgun => "Shotgun",
            Weapon::ClusterBomb => "Cluster Bomb",
            Weapon::BananaBomb => "Banana Bomb",
            Weapon::HolyHandGrenade => "Holy Hand Grenade",
            Weapon::Dynamite => "Dynamite",
            Weapon::Mine => "Mine",
            Weapon::HomingMissile => "Homing Missile",
            Weapon::Mortar => "Mortar",
            Weapon::Sheep => "Sheep",
            Weapon::Airstrike => "Airstrike",
            Weapon::NapalmStrike => "Napalm Strike",
            Weapon::Teleport => "Teleport",
            Weapon::Jetpack => "Jetpack",
            Weapon::Parachute => "Parachute",
            Weapon::BaseballBat => "Baseball Bat",
            Weapon::Rope => "Rope",
            Weapon::SniperRifle => "Sniper Rifle",
            Weapon::Uzi => "Uzi",
            Weapon::BananaBonanza => "Banana Bonanza",
            Weapon::Drill => "Drill",
            Weapon::SuperSheep => "Super Sheep",
            Weapon::BuildWall => "Build Wall",
        }
    }

    pub fn weapon_type(&self) -> WeaponType {
        match self {
            Weapon::Bazooka | Weapon::Grenade | Weapon::Shotgun | Weapon::ClusterBomb 
            | Weapon::BananaBomb | Weapon::HolyHandGrenade | Weapon::HomingMissile 
            | Weapon::Mortar | Weapon::Sheep | Weapon::BananaBonanza
            | Weapon::SuperSheep => WeaponType::Projectile,
            
            Weapon::Dynamite | Weapon::Mine => WeaponType::Placed,
            
            Weapon::Airstrike | Weapon::NapalmStrike => WeaponType::Airstrike,
            
            Weapon::Teleport | Weapon::Jetpack | Weapon::Parachute | Weapon::Rope | Weapon::BuildWall | Weapon::Drill => WeaponType::Utility,
            
            Weapon::BaseballBat => WeaponType::Melee,
            
            Weapon::SniperRifle | Weapon::Uzi => WeaponType::Instant,
        }
    }

    pub fn explosion_radius(&self) -> f32 {
        match self {
            Weapon::Bazooka => 30.0,
            Weapon::Grenade => 25.0,
            Weapon::Shotgun => 12.0,
            Weapon::ClusterBomb => 35.0,
            Weapon::BananaBomb => 40.0,
            Weapon::HolyHandGrenade => 100.0,
            Weapon::Dynamite => 95.0,
            Weapon::Mine => 30.0,
            Weapon::HomingMissile => 35.0,
            Weapon::Mortar => 38.0,
            Weapon::Sheep => 50.0,
            Weapon::Airstrike => 25.0,
            Weapon::NapalmStrike => 20.0,
            Weapon::BananaBonanza => 45.0,
            Weapon::Drill => 0.0,
            Weapon::SuperSheep => 55.0,
            Weapon::BuildWall => 0.0,
            _ => 0.0,
        }
    }

    pub fn base_damage(&self) -> i32 {
        match self {
            Weapon::Bazooka => 50,
            Weapon::Grenade => 35,
            Weapon::Shotgun => 60,
            Weapon::ClusterBomb => 30,
            Weapon::BananaBomb => 50,
            Weapon::HolyHandGrenade => 50,
            Weapon::Dynamite => 50,
            Weapon::Mine => 45,
            Weapon::HomingMissile => 40,
            Weapon::Mortar => 35,
            Weapon::Sheep => 60,
            Weapon::Airstrike => 30,
            Weapon::NapalmStrike => 25,
            Weapon::BaseballBat => 20,
            Weapon::SniperRifle => 1000,
            Weapon::Uzi => 5,
            Weapon::BananaBonanza => 35,
            Weapon::Drill => 0,
            Weapon::SuperSheep => 70,
            Weapon::BuildWall => 0,
            _ => 0,
        }
    }

    pub fn speed_factor(&self) -> f32 {
        match self {
            Weapon::Bazooka => 12.0,
            Weapon::Grenade => 9.0,
            Weapon::Shotgun => 14.0,
            Weapon::ClusterBomb => 8.0,
            Weapon::BananaBomb => 10.0,
            Weapon::HolyHandGrenade => 7.0,
            Weapon::HomingMissile => 15.0,
            Weapon::Mortar => 41.0,
            Weapon::Sheep => 5.0,
            Weapon::SniperRifle => 1000.0,
            Weapon::BananaBonanza => 10.0,
            Weapon::Drill => 20.0,
            Weapon::SuperSheep => 13.0,
            Weapon::BuildWall => 1.0,
            _ => 10.0,
        }
    }

    pub fn fuse_time(&self) -> f32 {
        match self {
            Weapon::Grenade => 3.0,
            Weapon::ClusterBomb => 2.5,
            Weapon::BananaBomb => 3.0,
            Weapon::HolyHandGrenade => 3.0,
            Weapon::Dynamite => 5.0,
            Weapon::Mine => -1.0, // Proximity triggered
            Weapon::Sheep => 5.0,
            Weapon::BananaBonanza => 2.0,
            Weapon::SuperSheep => 10.0,
            Weapon::BuildWall => -1.0,
            _ => -1.0,
        }
    }

    pub fn max_bounces(&self) -> i32 {
        match self {
            Weapon::Grenade => 3,
            Weapon::ClusterBomb => 2,
            Weapon::BananaBomb => 5,
            Weapon::HolyHandGrenade => 1,
            _ => 0,
        }
    }

    pub fn cluster_count(&self) -> usize {
        match self {
            Weapon::ClusterBomb => 5,
            Weapon::BananaBomb => 6,
            Weapon::BananaBonanza => 10,
            Weapon::Mortar => 3,
            _ => 0,
        }
    }

    pub fn from_key(k: u8) -> Option<Weapon> {
        match k {
            1 => Some(Weapon::Bazooka),
            2 => Some(Weapon::Grenade),
            3 => Some(Weapon::Shotgun),
            4 => Some(Weapon::ClusterBomb),
            5 => Some(Weapon::BananaBomb),
            6 => Some(Weapon::HolyHandGrenade),
            7 => Some(Weapon::Dynamite),
            8 => Some(Weapon::Mine),
            9 => Some(Weapon::HomingMissile),
            10 => Some(Weapon::BuildWall),
            _ => None,
        }
    }

    pub fn from_name(s: &str) -> Option<Weapon> {
        match s {
            "Bazooka" => Some(Weapon::Bazooka),
            "Grenade" => Some(Weapon::Grenade),
            "Shotgun" => Some(Weapon::Shotgun),
            "Cluster Bomb" => Some(Weapon::ClusterBomb),
            "Banana Bomb" => Some(Weapon::BananaBomb),
            "Holy Hand Grenade" => Some(Weapon::HolyHandGrenade),
            "Dynamite" => Some(Weapon::Dynamite),
            "Mine" => Some(Weapon::Mine),
            "Homing Missile" => Some(Weapon::HomingMissile),
            "Mortar" => Some(Weapon::Mortar),
            "Sheep" => Some(Weapon::Sheep),
            "Airstrike" => Some(Weapon::Airstrike),
            "Napalm Strike" => Some(Weapon::NapalmStrike),
            "Teleport" => Some(Weapon::Teleport),
            "Jetpack" => Some(Weapon::Jetpack),
            "Parachute" => Some(Weapon::Parachute),
            "Baseball Bat" => Some(Weapon::BaseballBat),
            "Rope" => Some(Weapon::Rope),
            "Sniper Rifle" => Some(Weapon::SniperRifle),
            "Uzi" => Some(Weapon::Uzi),
            "Banana Bonanza" => Some(Weapon::BananaBonanza),
            "Concrete Shell" => Some(Weapon::Drill),
            "Super Sheep" => Some(Weapon::SuperSheep),
            "Build Wall" => Some(Weapon::BuildWall),
            _ => None,
        }
    }

    pub fn all() -> &'static [Weapon] {
        &[
            Weapon::Bazooka,
            Weapon::Grenade,
            Weapon::Shotgun,
            Weapon::ClusterBomb,
            Weapon::BananaBomb,
            Weapon::HolyHandGrenade,
            Weapon::Dynamite,
            Weapon::Mine,
            Weapon::HomingMissile,
            Weapon::Mortar,
            Weapon::Sheep,
            Weapon::Airstrike,
            Weapon::NapalmStrike,
            Weapon::BaseballBat,
            Weapon::SniperRifle,
            Weapon::Uzi,
            Weapon::Teleport,
            Weapon::BananaBonanza,
            Weapon::Drill,
            Weapon::SuperSheep,
            Weapon::BuildWall,
        ]
    }
    
    pub fn category(&self) -> WeaponCategory {
        match self {
            Weapon::Bazooka | Weapon::Grenade | Weapon::ClusterBomb | Weapon::BananaBomb
            | Weapon::HolyHandGrenade | Weapon::Dynamite | Weapon::Mine | Weapon::BananaBonanza
            | Weapon::Mortar | Weapon::Airstrike | Weapon::NapalmStrike => WeaponCategory::Explosives,
            
            Weapon::Shotgun | Weapon::HomingMissile | Weapon::SniperRifle | Weapon::Uzi
            | Weapon::Drill => WeaponCategory::Ballistics,
            
            Weapon::Teleport | Weapon::Jetpack | Weapon::Parachute | Weapon::Rope | Weapon::BuildWall => WeaponCategory::Utilities,
            
            Weapon::Sheep | Weapon::SuperSheep | Weapon::BaseballBat => WeaponCategory::Special,
        }
    }
    
    pub fn icon(&self) -> &str {
        match self {
            Weapon::Bazooka => ">>",
            Weapon::Grenade => "*",
            Weapon::Shotgun => "##",
            Weapon::ClusterBomb => "**",
            Weapon::BananaBomb => "))",
            Weapon::HolyHandGrenade => "+",
            Weapon::Dynamite => "!!",
            Weapon::Mine => "/!\\",
            Weapon::HomingMissile => "->",
            Weapon::Mortar => "^^",
            Weapon::Sheep => "@@",
            Weapon::Airstrike => "vv",
            Weapon::NapalmStrike => "~~",
            Weapon::Teleport => "<>",
            Weapon::Jetpack => "||",
            Weapon::Parachute => "}{" ,
            Weapon::BaseballBat => "/",
            Weapon::Rope => "&",
            Weapon::SniperRifle => "--",
            Weapon::Uzi => "=",
            Weapon::BananaBonanza => ")))",
            Weapon::Drill => "[]",
            Weapon::SuperSheep => "@!",
            Weapon::BuildWall => "###",
        }
    }
    
    pub fn description(&self) -> &str {
        match self {
            Weapon::Bazooka => "Direct fire explosive. No bounce.",
            Weapon::Grenade => "Bounces 3 times before exploding",
            Weapon::Shotgun => "Fires 6 pellets in a spread",
            Weapon::ClusterBomb => "Splits into 5 bomblets on impact",
            Weapon::BananaBomb => "Bounces 5x, clusters into 6 bombs",
            Weapon::HolyHandGrenade => "Massive holy explosion!",
            Weapon::Dynamite => "Place and run! 5s fuse",
            Weapon::Mine => "Proximity triggered trap",
            Weapon::HomingMissile => "Tracks nearest ball",
            Weapon::Mortar => "High arc, clusters on impact",
            Weapon::Sheep => "Walks on terrain, then explodes",
            Weapon::Airstrike => "5 explosions from the sky",
            Weapon::NapalmStrike => "Fire trail from above",
            Weapon::Teleport => "Click to relocate your ball",
            Weapon::Jetpack => "Fly freely for 5 seconds",
            Weapon::Parachute => "Deploy to slow descent",
            Weapon::BaseballBat => "Melee knockback attack",
            Weapon::Rope => "Ninja rope swing",
            Weapon::SniperRifle => "Instant laser shot",
            Weapon::Uzi => "Rapid-fire 10 shots",
            Weapon::BananaBonanza => "10 cluster bomblets!",
            Weapon::Drill => "Drills a walkable tunnel through terrain. No damage.",
            Weapon::SuperSheep => "Flying explosive sheep!",
            Weapon::BuildWall => "Place a short wooden wall at target location",
        }
    }
}

impl WeaponCategory {
    pub fn name(&self) -> &str {
        match self {
            WeaponCategory::Explosives => "* EXPLOSIVES",
            WeaponCategory::Ballistics => "+ BALLISTICS",
            WeaponCategory::Utilities => "~ UTILITIES",
            WeaponCategory::Special => "# SPECIAL",
        }
    }
}
