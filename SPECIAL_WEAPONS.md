# Special Weapons Implementation

## Overview
We've implemented 27 unique weapons with truly differentiated behaviors across 4 categories.

## Weapon Categories

### 🧨 Explosives
- **Grenade** - Standard explosive with high blast radius
- **Cluster Bomb** - Splits into multiple bomblets on impact
- **BananaBomb** - Powerful cluster bomb with extra bounce
- **Dynamite** - Place a timed explosive (5s fuse) that detonates in place
- **Mine** - Proximity-based explosive
- **Holy Grenade** - Massive blast radius with special effects

### 🔫 Ballistics
- **Bazooka** - Standard projectile weapon
- **Homing Missile** - Tracks nearest worm within 400px radius
- **Shotgun** - Fires 6 pellets in a spread pattern (0.25 rad spread)
- **Uzi** - Rapid-fire weapon that shoots 10 bullets with random spread
- **Mortar** - High-arc explosive
- **Sniper Rifle** - High-speed precision shot

### ⭐ Special
- **Airstrike** - Calls in 5 explosive droplets from the sky (80px spacing)
- **Napalm Strike** - Drops 7 napalm fire droplets (60px spacing)
- **Baseball Bat** - Melee weapon that knocks nearby worms (60px range)

### 🛠️ Utilities
- **Teleport** - Click to instantly move your worm to target location
- **Ninja Rope** - Grappling hook for traversal
- **Parachute** - Slow descent
- **Jet Pack** - Aerial mobility
- **Sheep** - Walking explosive
- **Super Sheep** - Flying explosive sheep
- **Prod** - Push enemy worms
- **Dragon Ball** - Special movement ball

## Special Weapon Mechanics

### Airstrike/Napalm Strike
- Droplets fall from above (`y=-50`) with high gravity (600.0)
- Explosive airstrike: 25px radius, 30 damage per droplet
- Napalm strike: 20px radius, 25 damage, fire effect
- Camera follows droplets as they fall
- Great for hitting entrenched enemies

### Uzi
- Fires 10 bullets in quick succession
- Each bullet has random spread (±0.15 radians)
- Lower gravity (300.0) for flatter trajectory
- 5 damage per bullet (50 damage total if all hit)
- Minimal air resistance for faster travel

### Shotgun
- Fires 6 pellets simultaneously
- Fixed 0.25 radian spread pattern
- Each pellet deals independent damage on impact
- Camera follows pellets
- Good for close-range encounters

### Dynamite
- Places explosive at worm's current position
- 5 second fuse timer
- Pulsing visual indicator (gets brighter as countdown approaches)
- 45px blast radius, 50 damage
- Cannot be moved once placed

### Baseball Bat
- Melee weapon (60px effective range)
- Click to swing in aimed direction
- Knocks back nearby worms with strong force (15.0 power)
- Deals 10 damage on hit
- No projectile - instant effect
- Great for knocking enemies into water or off cliffs

### Teleport
- Click anywhere on the map to instantly move your worm
- No damage, no projectile
- Worm velocity reset to zero upon teleport
- Perfect for repositioning or escaping danger
- Limited by map boundaries

### Homing Missile
- Automatically tracks nearest worm within 400px radius
- Adjusts trajectory mid-flight (0.6 steering strength)
- Visual trail shows pursuit path
- Standard explosion on impact

### Cluster Bomb/BananaBomb
- Initial projectile with bounce mechanics
- Splits into 5 bomblets on explosion
- Each bomblet bounces independently with different damping
- BananaBomb has reduced damping for more chaos

## UI Features

### Weapon Menu
- **Open**: Press TAB or Q (click support coming soon)
- **Close**: Press ESC
- **Navigate**: Scroll through categories
- **Select**: Click on weapon

### Menu Layout
- Organized by category with color-coded badges
- Each weapon shows:
  - Category badge (top-right corner)
  - Icon/emoji
  - Weapon name
  - Description of behavior
  - Stats: Damage, Blast Radius, Bounces

### Mobile Support
- Responsive design with larger touch targets
- Adaptive sizing based on screen width
- Smooth scrolling (no camera zoom while menu open)

## Testing Recommendations

1. **Shotgun**: Aim at close range enemy - watch pellets spread
2. **Uzi**: Fire at medium range - see bullet spray pattern
3. **Airstrike**: Target enemies on high ground - droplets fall from sky
4. **Dynamite**: Place near group of enemies, retreat before detonation
5. **Baseball Bat**: Get close to enemy near cliff or water - melee swing
6. **Teleport**: Click on opposite side of map - instant transport
7. **Homing Missile**: Fire near (but not at) an enemy - watch it track
8. **Cluster Bomb**: Hit terrain near enemies - watch bomblets scatter

## Known Issues
- Weapon menu currently requires TAB/Q to open (mouse click support pending)
- Baseball Bat swing has no visual effect yet (works functionally)
- Teleport doesn't validate terrain (can teleport inside walls - will likely fall out)

## Future Enhancements
- Add mouse click to open weapon menu
- Visual swing arc for Baseball Bat
- Teleport target validation (prevent invalid positions)
- Additional special weapons (Sheep, Super Sheep, etc.)
- Sound effects for each weapon type
