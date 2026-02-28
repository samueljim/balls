use macroquad::prelude::*;

pub const WIDTH: u32 = 1400;
pub const HEIGHT: u32 = 800;
pub const WATER_LEVEL: f32 = 740.0;
pub const PLAYABLE_LAND_WIDTH: f32 = 1360.0; // Land is centered, minimal water margins
pub const LAND_START_X: f32 = 20.0; // Land starts 20px from left edge (minimal water)
pub const LAND_END_X: f32 = 1380.0; // Land ends 20px from right edge (minimal water)

pub const AIR: u8 = 0;
pub const DIRT: u8 = 1;
pub const GRASS: u8 = 2;
pub const STONE: u8 = 3;
pub const LAVA: u8 = 4;
pub const WOOD: u8 = 5;

pub struct Terrain {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<u8>,
    /// Log of all (cx, cy, radius) damage events for replay on reconnect
    pub damage_log: Vec<(i32, i32, i32)>,
}

impl Terrain {
    pub fn new(w: u32, h: u32) -> Self {
        Terrain {
            width: w,
            height: h,
            cells: vec![AIR; (w * h) as usize],
            damage_log: Vec::new(),
        }
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some((y as u32 * self.width + x as u32) as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        self.idx(x, y)
            .map(|i| self.cells[i])
            .unwrap_or(if y >= self.height as i32 { STONE } else { AIR })
    }

    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if let Some(i) = self.idx(x, y) {
            self.cells[i] = v;
        }
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.get(x, y) != AIR
    }

    pub fn find_surface_y(&self, x: i32) -> Option<i32> {
        for y in 0..self.height as i32 {
            if self.is_solid(x, y) {
                return Some(y);
            }
        }
        None
    }

    pub fn apply_damage(&mut self, cx: i32, cy: i32, radius: i32) {
        self.damage_log.push((cx, cy, radius));
        self.apply_damage_no_log(cx, cy, radius);
    }

    /// Apply damage without recording to the log (used for replay on reconnect)
    fn apply_damage_no_log(&mut self, cx: i32, cy: i32, radius: i32) {
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r2 {
                    self.set(cx + dx, cy + dy, AIR);
                }
            }
        }
        self.regrow_grass_near(cx, cy, radius);
    }

    /// Replay a damage log on this terrain (e.g. after regenerating from seed on reconnect)
    pub fn replay_damage(&mut self, log: &[(i32, i32, i32)]) {
        for &(cx, cy, r) in log {
            self.apply_damage_no_log(cx, cy, r);
        }
        // Store the replayed events so future sends include them
        self.damage_log.extend_from_slice(log);
    }

    /// Regrow grass over any rectangular area (used after drill carvings).
    pub fn refresh_grass_in_area(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let margin = 4;
        for y in (y1 - margin).max(1)..=(y2 + margin).min(self.height as i32 - 1) {
            for x in (x1 - margin).max(0)..=(x2 + margin).min(self.width as i32 - 1) {
                if self.get(x, y) != AIR {
                    continue;
                }
                if self.get(x, y + 1) == DIRT || self.get(x, y + 1) == STONE {
                    self.set(x, y, GRASS);
                }
            }
        }
    }

    fn regrow_grass_near(&mut self, cx: i32, cy: i32, radius: i32) {
        let margin = 3;
        for dy in -(radius + margin)..=(radius + margin) {
            for dx in -(radius + margin)..=(radius + margin) {
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || x >= self.width as i32 || y < 1 || y >= self.height as i32 {
                    continue;
                }
                if self.get(x, y) != AIR {
                    continue;
                }
                if self.get(x, y + 1) == DIRT || self.get(x, y + 1) == STONE {
                    self.set(x, y, GRASS);
                }
            }
        }
    }

    pub fn bake_image(&self) -> Image {
        let mut img = Image::gen_image_color(self.width as u16, self.height as u16, BLANK);
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.cells[(y * self.width + x) as usize];
                let color = cell_color(cell, x as i32, y as i32);
                let idx = ((y * self.width + x) * 4) as usize;
                img.bytes[idx] = (color.r * 255.0) as u8;
                img.bytes[idx + 1] = (color.g * 255.0) as u8;
                img.bytes[idx + 2] = (color.b * 255.0) as u8;
                img.bytes[idx + 3] = (color.a * 255.0) as u8;
            }
        }
        img
    }
}

fn cell_color(cell: u8, x: i32, y: i32) -> Color {
    let n = ((x.wrapping_mul(7) ^ y.wrapping_mul(13)) & 0x1F) as f32 / 31.0;
    match cell {
        GRASS => Color::new(0.18 + n * 0.08, 0.50 + n * 0.12, 0.12 + n * 0.06, 1.0),
        DIRT => Color::new(0.48 + n * 0.08, 0.32 + n * 0.06, 0.18 + n * 0.04, 1.0),
        STONE => Color::new(0.38 + n * 0.08, 0.38 + n * 0.08, 0.42 + n * 0.08, 1.0),
        LAVA => {
            // Animated lava glow effect
            let glow = ((x + y) as f32 * 0.1).sin() * 0.15 + 0.85;
            Color::new(0.95 * glow, 0.25 * glow, 0.05 * glow, 1.0)
        }
        WOOD => Color::new(0.35 + n * 0.1, 0.20 + n * 0.05, 0.10 + n * 0.03, 1.0),
        _ => BLANK,
    }
}

/// Integer hash that avoids f32 precision issues with large seed values.
/// Returns a value in [0, 1).
fn noise_1d(x: f32, seed: f32) -> f32 {
    // Convert to integers for hashing to avoid sin() precision loss
    let ix = x as i32;
    let is = seed as i32;
    let mut h = (ix as u32).wrapping_mul(374761393)
        .wrapping_add((is as u32).wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h = h ^ (h >> 16);
    (h & 0x00FFFFFF) as f32 / 16777216.0 // 2^24
}

fn smooth_noise(x: f32, scale: f32, seed: f32) -> f32 {
    let sx = x / scale;
    let ix = sx.floor();
    let fx = sx - ix;
    let t = fx * fx * (3.0 - 2.0 * fx);
    let a = noise_1d(ix, seed);
    let b = noise_1d(ix + 1.0, seed);
    a + (b - a) * t
}

/// Produces a value in [0, 1) with sharper transitions — used for plateau/cliff shapes
fn ridged_noise(x: f32, scale: f32, seed: f32) -> f32 {
    let v = smooth_noise(x, scale, seed);
    // Fold around 0.5 to create ridges
    let centered = (v - 0.5).abs() * 2.0; // 0..1, peaks at v=0 and v=1
    centered
}

fn lcg(s: u32) -> u32 {
    s.wrapping_mul(1103515245).wrapping_add(12345)
}

/// Simple integer-hash based 2D noise in [0,1). Lightweight and deterministic.
fn noise_2d(x: f32, y: f32, seed: f32) -> f32 {
    let ix = x as i32;
    let iy = y as i32;
    let is = seed as i32;
    let mut h = (ix as u32).wrapping_mul(374761393)
        .wrapping_add((iy as u32).wrapping_mul(668265263))
        .wrapping_add((is as u32).wrapping_mul(982451653));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h = h ^ (h >> 16);
    (h & 0x00FFFFFF) as f32 / 16777216.0
}

pub fn generate(seed: u32) -> Terrain {
    let w = WIDTH;
    let h = HEIGHT;
    let mut t = Terrain::new(w, h);
    let sf = seed as f32;

    // Base ground level — biased toward bottom so hills can rise prominently
    let base_ground = h as f32 * 0.58;
    let mut heights = vec![0.0f32; w as usize];

    // Use seed to vary the terrain style per game
    let mut s = lcg(seed.wrapping_add(7777));

    // ---- Layer 1: Define 4-8 distinct hill "segments" across the map ----
    // This creates the classic Balls look of distinct hills separated by valleys
    s = lcg(s);
    let num_hills = 3 + (s >> 16) as u32 % 3; // 3-5 hills
    struct HillDef { center: f32, width: f32, height: f32, flat_top: f32 }
    let mut hill_defs: Vec<HillDef> = Vec::new();
    
    let segment_width = PLAYABLE_LAND_WIDTH / num_hills as f32;
    for i in 0..num_hills {
        s = lcg(s);
        let jitter = ((s >> 16) as f32 / 65535.0 - 0.5) * segment_width * 0.4;
        let center = LAND_START_X + segment_width * (i as f32 + 0.5) + jitter;
        s = lcg(s);
        let hill_height = 60.0 + (s >> 16) as f32 / 65535.0 * 100.0; // 60-160px tall
        s = lcg(s);
        let hill_width = 60.0 + (s >> 16) as f32 / 65535.0 * 120.0; // 60-180px half-width
        s = lcg(s);
        let flat_top = (s >> 16) as f32 / 65535.0 * 0.5; // 0-0.5 flatness on top
        hill_defs.push(HillDef { center, width: hill_width, height: hill_height, flat_top });
    }

    for x in 0..w {
        let fx = x as f32;
        // Start from base ground (low = deep valley between hills)
        let mut hill_influence = 0.0f32;
        for hd in &hill_defs {
            let dx = (fx - hd.center) / hd.width;
            let dist = dx * dx;
            // Blend of Gaussian (smooth) and plateau (flat-top) shapes
            let gaussian = (-dist * 2.5).exp();
            let plateau = (1.0 - dist).max(0.0);
            let shape = gaussian * (1.0 - hd.flat_top) + plateau * hd.flat_top;
            hill_influence = hill_influence.max(shape * hd.height);
        }
        heights[x as usize] = base_ground - hill_influence; // Negative y = higher on screen
    }

    // ---- Layer 2: Add noise-based variation on top of the hill shapes ----
    for x in 0..w {
        let fx = x as f32;
        let h2 = (smooth_noise(fx, 90.0, sf + 100.0) - 0.5) * 22.0;  // medium undulation
        let h3 = (smooth_noise(fx, 35.0, sf + 200.0) - 0.5) * 12.0;  // small bumps
        let h4 = (smooth_noise(fx, 12.0, sf + 300.0) - 0.5) * 5.0;   // fine detail
        // Ridged noise for occasional sharp features
        let ridge = (ridged_noise(fx, 180.0, sf + 400.0) - 0.5) * 18.0;
        heights[x as usize] += h2 + h3 + h4 + ridge;
    }

    // ---- Layer 3: Seed-driven valleys (cut between hills) ----
    s = lcg(s);
    let num_valleys = 1 + (s >> 16) as u32 % 3; // 1-3 deep valleys
    for _ in 0..num_valleys {
        s = lcg(s);
        let valley_x = LAND_START_X + 100.0 + (s >> 16) as f32 / 65535.0 * (PLAYABLE_LAND_WIDTH - 200.0);
        s = lcg(s);
        let valley_depth = 60.0 + (s >> 16) as f32 / 65535.0 * 100.0; // 60-160px deep
        s = lcg(s);
        let valley_width = 40.0 + (s >> 16) as f32 / 65535.0 * 60.0;  // 40-100px wide
        for x in 0..w {
            let dx = (x as f32 - valley_x) / valley_width;
            let influence = (-dx * dx * 3.0).exp();
            heights[x as usize] += influence * valley_depth; // Positive y = lower on screen
        }
    }

    // Minimal smoothing — just 1 pass to avoid jagged pixels while keeping sharp hills
    {
        let prev = heights.clone();
        for x in 1..w as usize - 1 {
            heights[x] = (prev[x - 1] + prev[x] * 2.0 + prev[x + 1]) / 4.0;
        }
    }

    // Slope down to water at edges, keeping center playable
    for x in 0..w as usize {
        let from_left = x as f32;
        let from_right = (w as usize - 1 - x) as f32;
        let edge_dist = from_left.min(from_right);
        
        // More aggressive falloff at edges to create water boundaries
        if edge_dist < LAND_START_X {
            let factor = (LAND_START_X - edge_dist) / LAND_START_X;
            // Smoothly transition to deep water
            heights[x] -= factor * factor * 300.0;
        }
    }

    // Apply a multi-island mask so central island(s) stay higher and channels form between them.
    {
        let mut s_is = lcg(seed.wrapping_add(8000));
        let num_islands = 1 + (s_is >> 16) as u32 % 3; // 1-3 islands
        let mut island_centers: Vec<f32> = Vec::new();
        for _ in 0..num_islands {
            s_is = lcg(s_is);
            let cx = LAND_START_X + 80.0 + (s_is >> 16) as f32 / 65535.0 * (PLAYABLE_LAND_WIDTH - 160.0);
            island_centers.push(cx);
        }

        let half_width = (PLAYABLE_LAND_WIDTH * 0.5) as f32;
        for x in 0..w as usize {
            let xf = x as f32;
            if xf >= LAND_START_X && xf <= LAND_END_X {
                // distance to nearest island center (normalized)
                let mut best = 99999.0f32;
                for &c in &island_centers {
                    let d = (xf - c).abs();
                    if d < best { best = d; }
                }
                let norm = (best / half_width).clamp(0.0, 1.0);
                // Sharpen the falloff so islands are pronounced and channels appear
                let mask = 1.0 - norm.powf(1.8);
                // Center boost: lower the height value to raise island tops
                let center_boost = 80.0;
                // Edge depth: increase height value to deepen channels/water
                let edge_depth = 110.0;
                heights[x] -= mask * center_boost; // raise island centres
                heights[x] += (1.0 - mask) * edge_depth; // deepen channels
            }
        }
    }

    // Carve a porous cave field using 2D noise so maps get lots of natural caverns.
    // This runs before the chamber+worm pass to produce interconnected voids.
    {
        let mut s_cave = lcg(seed.wrapping_add(2500));
        let cave_scale = 0.035; // coarser noise for big caverns
        let cave_threshold = 0.42; // lower -> more air; slightly stricter to protect surface
        for x in LAND_START_X as i32..=LAND_END_X as i32 {
            if x < 0 || x >= w as i32 { continue; }
            let ground = heights[x as usize] as i32;
            // create caves starting a bit below surface and extending downward
            let y0 = (ground + 10).max(10);
            let y1 = (ground + 220).min(h as i32 - 10);
            for y in y0..=y1 {
                let nx = x as f32 * cave_scale;
                let ny = y as f32 * cave_scale;
                // combine a couple of noise samples for variety
                let n1 = noise_2d(nx, ny, sf + 6000.0);
                let n2 = noise_2d(nx * 1.7 + 17.0, ny * 0.9 + 53.0, sf + 7000.0) * 0.6;
                let n = n1 * 0.7 + n2 * 0.3;
                if n < cave_threshold {
                    t.set(x, y, AIR);
                }
            }
        }
    }

    for x in 0..w {
        let ground = heights[x as usize].clamp(60.0, WATER_LEVEL - 20.0) as i32;
        // Only generate land in the center playable area
        let x_f = x as f32;
        if x_f < LAND_START_X || x_f > LAND_END_X {
            // Skip generating solid terrain in water areas
            continue;
        }
        
        for y in ground..h as i32 {
            let depth = y - ground;
            let cell = if depth <= 1 {
                GRASS
            } else if depth < 16 {
                DIRT
            } else {
                STONE
            };
            t.set(x as i32, y, cell);
        }
    }

    let mut s = lcg(seed.wrapping_add(1000));
    let num_platforms = 3 + (s >> 16) as u32 % 4;
    let land_width = (LAND_END_X - LAND_START_X) as i32;
    for _ in 0..num_platforms {
        s = lcg(s);
        let px = LAND_START_X as i32 + 80 + (s >> 16) as i32 % (land_width - 160);
        s = lcg(s);
        let py_offset = 35 + (s >> 16) as i32 % 55;
        s = lcg(s);
        let plat_width = 50 + (s >> 16) as i32 % 70;
        s = lcg(s);
        let surface_y = heights[px as usize] as i32;
        let py = surface_y - py_offset;
        if py > 30 && py < WATER_LEVEL as i32 - 30 {
            for dx in 0..plat_width {
                let x = px + dx;
                if x >= 0 && x < w as i32 {
                    t.set(x, py, GRASS);
                    for d in 1..4 {
                        t.set(x, py + d, DIRT);
                    }
                }
            }
        }
    }

    // Generate improved caves with more variety
    s = lcg(s.wrapping_add(2000));
    let num_caves = 12 + (s >> 16) as u32 % 6; // Many caves (12-17)
    let mut cave_positions = Vec::new();
    
    for i in 0..num_caves {
        s = lcg(s);
        let cx = LAND_START_X as i32 + 100 + (s >> 16) as i32 % (land_width - 200);
        s = lcg(s);
        let cy = 100 + (s >> 16) as i32 % ((heights[cx as usize] as i32) - 120);
        s = lcg(s);
        let cave_w = 110 + (s >> 16) as i32 % 160; // Generous width (110-270)
        s = lcg(s);
        let cave_h = 70 + (s >> 16) as i32 % 110; // Generous height (70-180)
        
        cave_positions.push((cx, cy));
        
        // Carve out main cave chamber with more organic shape
        for dy in 0..cave_h {
            for dx in 0..cave_w {
                let xd = (dx - cave_w / 2) as f32;
                let yd = (dy - cave_h / 2) as f32;
                // Use different radii for more irregular shapes
                let x_radius = cave_w as f32 * 0.5;
                let y_radius = cave_h as f32 * 0.5;
                let dist = ((xd * xd) / (x_radius * x_radius) + 
                           (yd * yd) / (y_radius * y_radius)).sqrt();
                
                // Add some noise to cave edges for organic feel
                s = lcg(s);
                let noise = ((s >> 16) as f32 / 65535.0) * 0.25;
                if dist < 1.0 + noise {
                    t.set(cx + dx - cave_w / 2, cy + dy - cave_h / 2, AIR);
                }
            }
        }
        
        // Add small alcoves to some caves for more complexity
        s = lcg(s);
        if (s >> 16) % 2 == 0 {
            s = lcg(s);
            let alcove_dx = if (s >> 16) % 2 == 0 { -cave_w / 3 } else { cave_w / 3 };
            let alcove_w = 15 + (s >> 16) as i32 % 20;
            let alcove_h = 12 + (s >> 16) as i32 % 15;
            
            for dy in 0..alcove_h {
                for dx in 0..alcove_w {
                    let xd = (dx - alcove_w / 2) as f32;
                    let yd = (dy - alcove_h / 2) as f32;
                    let dist = ((xd * xd + yd * yd) as f32).sqrt();
                    if dist < (alcove_w.min(alcove_h) as f32) * 0.5 {
                        t.set(cx + alcove_dx + dx - alcove_w / 2, cy + dy - alcove_h / 2, AIR);
                    }
                }
            }
        }
        
        // Connect most caves with tunnels
        if i > 0 && (s >> 16) % 4 != 0 && cave_positions.len() >= 2 {
            let prev_idx = cave_positions.len() - 2;
            let (prev_cx, prev_cy) = cave_positions[prev_idx];
            
            // Create winding, worm-like tunnel between caves using a noisy walk.
            // This produces curvier tunnels that can cross over themselves and branch.
            {
                let mut px = prev_cx as f32;
                let mut py = prev_cy as f32;
                let txf = cx as f32;
                let tyf = cy as f32;
                let total_dist = ((txf - px).hypot(tyf - py)).max(1.0);
                // More steps for longer tunnels
                let steps = (total_dist / 3.0).max(30.0) as i32;

                for _ in 0..steps {
                    // steer toward target but with jitter
                    let desired = (tyf - py).atan2(txf - px);
                    s = lcg(s);
                    let jitter = ((s >> 16) as f32 / 65535.0 - 0.5) * 1.6; // +/- ~0.8 rad
                    let angle = desired + jitter;

                    s = lcg(s);
                    let step_len = 2.5 + ((s >> 16) as f32 / 65535.0) * 4.0; // 2.5-6.5 px
                    px += angle.cos() * step_len;
                    py += angle.sin() * step_len;

                    // varying radius for organic tunnels
                    s = lcg(s);
                    let base_r = 7.0 + ((s >> 16) as f32 / 65535.0) * 8.0; // 7-15
                    let wobble = ((px * 0.12).sin() + (py * 0.15).cos()) * 1.5;
                    let radius_f = (base_r + wobble).max(3.0);
                    let radius = radius_f as i32;

                    let ix = px as i32;
                    let iy = py as i32;
                    for dy in -radius - 1..=radius + 1 {
                        for dx in -radius - 1..=radius + 1 {
                            if dx * dx + dy * dy <= radius * radius {
                                t.set(ix + dx, iy + dy, AIR);
                            }
                        }
                    }

                    // occasional side-branches for variety
                    s = lcg(s);
                    if (s >> 16) % 30 == 0 {
                        // small branching tunnel
                        s = lcg(s);
                        let mut bx = px;
                        let mut by = py;
                        let branch_angle = angle + ((s >> 16) as f32 / 65535.0 - 0.5) * 3.14 * 0.5;
                        s = lcg(s);
                        let blen = 8 + (s >> 16) as i32 % 20;
                        for _b in 0..blen {
                            s = lcg(s);
                            bx += branch_angle.cos() * (1.8 + ((s >> 16) as f32 / 65535.0) * 2.2);
                            by += branch_angle.sin() * (1.8 + ((s >> 16) as f32 / 65535.0) * 2.2);
                            let br = 2 + ((s >> 16) as i32 % 4);
                            let bix = bx as i32;
                            let biy = by as i32;
                            for dy in -br..=br {
                                for dx in -br..=br {
                                    if dx * dx + dy * dy <= br * br {
                                        t.set(bix + dx, biy + dy, AIR);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Generate trenches — narrow deep cuts across the terrain surface
    s = lcg(s.wrapping_add(3000));
    let num_trenches = 2 + (s >> 16) as u32 % 2; // 2-3 trenches
    for _ in 0..num_trenches {
        s = lcg(s);
        let tx = LAND_START_X as i32 + 120 + (s >> 16) as i32 % (land_width - 240);
        s = lcg(s);
        let trench_w = 10 + (s >> 16) as i32 % 12; // 10-22px wide
        s = lcg(s);
        let trench_d = 35 + (s >> 16) as i32 % 40; // 35-75px deep
        let surface_y = heights[tx as usize] as i32;
        for dx in 0..trench_w {
            let xd = (dx as f32 - trench_w as f32 * 0.5) / (trench_w as f32 * 0.5);
            // Slightly deeper in the center
            let col_depth = (trench_d as f32 * (1.0 - xd * xd * 0.35)) as i32;
            for dy in 0..col_depth {
                t.set(tx + dx - trench_w / 2, surface_y + dy, AIR);
            }
            // Stone walls on each side for a fortified look
            t.set(tx - trench_w / 2 - 1 + dx / trench_w, surface_y, STONE);
        }
        // Stone floor
        for dx in 0..trench_w {
            t.set(tx + dx - trench_w / 2, surface_y + trench_d, STONE);
        }
    }

    // Generate stone ruin clusters — scattered rubble from crumbled structures
    s = lcg(s.wrapping_add(3500));
    let num_ruins = 2 + (s >> 16) as u32 % 3; // 2-4 ruin sites
    for _ in 0..num_ruins {
        s = lcg(s);
        let rx = LAND_START_X as i32 + 100 + (s >> 16) as i32 % (land_width - 200);
        let surface_y = heights[rx as usize] as i32;
        s = lcg(s);
        let ruin_spread = 30 + (s >> 16) as i32 % 40; // 30-70px spread
        // Place 8-14 irregular stone chunks
        s = lcg(s);
        let num_chunks = 8 + (s >> 16) as i32 % 7;
        for _ in 0..num_chunks {
            s = lcg(s);
            let cx = rx + ((s >> 16) as i32 % ruin_spread) - ruin_spread / 2;
            s = lcg(s);
            let cy_off = -((s >> 16) as i32 % 12); // slightly above surface
            s = lcg(s);
            let cw = 3 + (s >> 16) as i32 % 7; // 3-9 wide
            s = lcg(s);
            let ch = 2 + (s >> 16) as i32 % 5; // 2-6 tall
            for dy in 0..ch {
                for dx in 0..cw {
                    let px = cx + dx;
                    let py = surface_y + cy_off - dy;
                    if px >= LAND_START_X as i32 && px < LAND_END_X as i32 && py > 0 {
                        if t.get(px, py) == AIR || t.get(px, py) == GRASS {
                            t.set(px, py, STONE);
                        }
                    }
                }
            }
        }
    }

    // Generate pre-made craters — as if the battlefield has already seen combat
    s = lcg(s.wrapping_add(3800));
    let num_craters = 2 + (s >> 16) as u32 % 3; // 2-4 craters
    for _ in 0..num_craters {
        s = lcg(s);
        let crx = LAND_START_X as i32 + 100 + (s >> 16) as i32 % (land_width - 200);
        let surface_y = heights[crx as usize] as i32;
        s = lcg(s);
        let radius = 12 + (s >> 16) as i32 % 18; // 12-30px radius
        // Carve a circular bowl into the surface
        for dy in 0..radius {
            for dx in -radius..=radius {
                let xd = dx as f32;
                let yd = dy as f32;
                let dist = (xd * xd + yd * yd).sqrt();
                if dist < radius as f32 {
                    // Deeper in the middle; stone rim at edges
                    let depth_factor = 1.0 - dist / radius as f32;
                    let depth = (radius as f32 * depth_factor * 0.6) as i32;
                    if yd as i32 <= depth {
                        t.set(crx + dx, surface_y + dy, AIR);
                    }
                }
            }
        }
        // Scattered dirt/stone ejecta around rim
        for _ in 0..8 {
            s = lcg(s);
            let ex = crx + ((s >> 16) as i32 % (radius * 3)) - radius;
            s = lcg(s);
            let esize = 2 + (s >> 16) as i32 % 4;
            for dy in 0..esize {
                for dx in 0..esize {
                    let px = ex + dx;
                    let py = surface_y - 1 - dy;
                    if px >= LAND_START_X as i32 && px < LAND_END_X as i32 && py > 0 {
                        if t.get(px, py) == AIR {
                            t.set(px, py, DIRT);
                        }
                    }
                }
            }
        }
    }

    // Generate trees (wooden trunks with green tops)
    s = lcg(s.wrapping_add(4000));
    let num_trees = 3 + (s >> 16) as u32 % 6;
    for _ in 0..num_trees {
        s = lcg(s);
        let tx = LAND_START_X as i32 + 80 + (s >> 16) as i32 % (land_width - 160);
        let surface_y = heights[tx as usize] as i32;
        
        // Skip if not on solid ground
        if t.get(tx, surface_y) != GRASS && t.get(tx, surface_y) != DIRT {
            continue;
        }
        
        s = lcg(s);
        let tree_h = 12 + (s >> 16) as i32 % 15;
        
        // Draw trunk (non-solid — left as AIR so balls don't snag on the
        // 1-pixel-wide column; the crown foliage still provides a surface to stand on)
        
        // Draw foliage (grass colored blocks forming crown)
        let crown_y = surface_y - tree_h;
        for dy in -3..=2 {
            for dx in -3..=3 {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist < 3.5 && crown_y + dy > 0 && crown_y + dy < h as i32 {
                    if t.get(tx + dx, crown_y + dy) == AIR {
                        t.set(tx + dx, crown_y + dy, GRASS);
                    }
                }
            }
        }
    }

    // Generate buildings (multi-story structures)
    s = lcg(s.wrapping_add(5000));
    let num_buildings = 1 + (s >> 16) as u32 % 3; // 1-3 buildings
    for _ in 0..num_buildings {
        s = lcg(s);
        let bx = LAND_START_X as i32 + 100 + (s >> 16) as i32 % (land_width - 200);
        let surface_y = heights[bx as usize] as i32;
        
        s = lcg(s);
        let building_width = 25 + (s >> 16) as i32 % 35; // 25-60 wide
        s = lcg(s);
        let num_floors = 2 + (s >> 16) as i32 % 3; // 2-4 floors
        let floor_height = 12;
        let building_height = num_floors * floor_height;
        
        // Build foundation and walls
        for floor in 0..num_floors {
            let floor_y = surface_y - (floor * floor_height);
            
            // Floor
            for dx in 0..building_width {
                t.set(bx + dx, floor_y, STONE);
            }
            
            // Walls (left and right)
            for dy in 1..floor_height {
                t.set(bx, floor_y - dy, STONE);
                t.set(bx + building_width - 1, floor_y - dy, STONE);
            }
            
            // Windows (gaps in walls)
            if floor > 0 {
                let window_y = floor_y - floor_height / 2;
                for window_pos in [building_width / 3, 2 * building_width / 3] {
                    for dy in 0..4 {
                        t.set(bx + window_pos, window_y - dy, AIR);
                    }
                }
            }
        }
        
        // Roof
        let roof_y = surface_y - building_height;
        for dx in 0..building_width {
            t.set(bx + dx, roof_y, STONE);
        }
        
        // Door on ground floor
        for dy in 0..8 {
            t.set(bx + building_width / 2, surface_y - dy, AIR);
        }
    }

    // Generate bunkers (underground reinforced structures)
    s = lcg(s.wrapping_add(6000));
    let num_bunkers = 0 + (s >> 16) as u32 % 2; // 0-1 bunkers
    for _ in 0..num_bunkers {
        s = lcg(s);
        let bunker_x = LAND_START_X as i32 + 150 + (s >> 16) as i32 % (land_width - 300);
        let surface_y = heights[bunker_x as usize] as i32;
        let bunker_y = surface_y + 10;
        
        let bunker_w = 30;
        let bunker_h = 18;
        
        // Carved out interior
        for dy in 0..bunker_h {
            for dx in 0..bunker_w {
                t.set(bunker_x + dx, bunker_y + dy, AIR);
            }
        }
        
        // Reinforced stone walls
        for dy in 0..bunker_h {
            t.set(bunker_x - 1, bunker_y + dy, STONE);
            t.set(bunker_x + bunker_w, bunker_y + dy, STONE);
        }
        for dx in 0..bunker_w {
            t.set(bunker_x + dx, bunker_y - 1, STONE);
            t.set(bunker_x + dx, bunker_y + bunker_h, STONE);
        }
        
        // Entrance tunnel
        for dx in 0..8 {
            for dy in 0..6 {
                t.set(bunker_x + bunker_w / 2 - 4 + dx, bunker_y - 6 - dy, AIR);
            }
        }
    }

    // Generate bridges connecting elevated areas
    s = lcg(s.wrapping_add(7000));
    let num_bridges = 1 + (s >> 16) as u32 % 2; // 1-2 bridges
    for _ in 0..num_bridges {
        s = lcg(s);
        let bridge_x = LAND_START_X as i32 + 120 + (s >> 16) as i32 % (land_width - 240);
        s = lcg(s);
        let bridge_y = 180 + (s >> 16) as i32 % 150; // Elevated position
        s = lcg(s);
        let bridge_length = 40 + (s >> 16) as i32 % 50;
        
        // Main bridge deck (wooden planks)
        for dx in 0..bridge_length {
            t.set(bridge_x + dx, bridge_y, WOOD);
            // Support beams every 8 blocks
            if dx % 8 == 0 {
                for dy in 1..4 {
                    t.set(bridge_x + dx, bridge_y + dy, WOOD);
                }
            }
        }
        
        // Railings
        for dx in 0..bridge_length {
            if dx % 3 == 0 {
                t.set(bridge_x + dx, bridge_y - 1, WOOD);
                t.set(bridge_x + dx, bridge_y - 2, WOOD);
            }
        }
    }

    // Generate scattered crates/boxes for cover
    s = lcg(s.wrapping_add(8000));
    let num_crates = 4 + (s >> 16) as u32 % 6; // 4-9 crates
    for _ in 0..num_crates {
        s = lcg(s);
        let crate_x = LAND_START_X as i32 + 60 + (s >> 16) as i32 % (land_width - 120);
        let surface_y = heights[crate_x as usize] as i32;
        
        // Skip if not on solid ground
        if t.get(crate_x, surface_y) != GRASS && t.get(crate_x, surface_y) != DIRT {
            continue;
        }
        
        s = lcg(s);
        let crate_size = 4 + (s >> 16) as i32 % 5; // 4-8 blocks
        
        // Draw crate
        for dy in 0..crate_size {
            for dx in 0..crate_size {
                if dy == 0 || dy == crate_size - 1 || dx == 0 || dx == crate_size - 1 {
                    // Wooden crate edges
                    t.set(crate_x + dx, surface_y - dy - 1, WOOD);
                } else {
                    // Interior can be dirt
                    t.set(crate_x + dx, surface_y - dy - 1, DIRT);
                }
            }
        }
    }

    // Generate stone towers/pillars
    s = lcg(s.wrapping_add(9000));
    let num_towers = 1 + (s >> 16) as u32 % 3; // 1-3 towers
    for _ in 0..num_towers {
        s = lcg(s);
        let tower_x = LAND_START_X as i32 + 100 + (s >> 16) as i32 % (land_width - 200);
        let surface_y = heights[tower_x as usize] as i32;
        
        s = lcg(s);
        let tower_h = 30 + (s >> 16) as i32 % 40; // 30-70 tall
        let tower_w = 8 + (s >> 16) as i32 % 6; // 8-13 wide
        
        // Build tower
        for dy in 0..tower_h {
            for dx in 0..tower_w {
                // Hollow interior (except base)
                if dy > 5 && dx > 0 && dx < tower_w - 1 && dy % 15 != 0 {
                    t.set(tower_x + dx, surface_y - dy, AIR);
                } else {
                    t.set(tower_x + dx, surface_y - dy, STONE);
                }
            }
        }
        
        // Platform on top
        let top_y = surface_y - tower_h;
        for dx in -2..tower_w + 2 {
            t.set(tower_x + dx, top_y, STONE);
        }
        
        // Entrance
        for dy in 0..8 {
            t.set(tower_x + tower_w / 2, surface_y - dy, AIR);
        }
    }

    t
}
