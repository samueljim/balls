// 2D destructible terrain: bitmap with circle damage (craters).

const WIDTH: u32 = 800;
const HEIGHT: u32 = 400;

/// Terrain stored as 1 bit per pixel: true = solid, false = air/dug.
/// Row-major, index = y * WIDTH + x.
#[derive(Clone)]
pub struct Terrain {
    pub width: u32,
    pub height: u32,
    /// true = solid
    pub pixels: Vec<bool>,
}

impl Terrain {
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;
        Terrain {
            width,
            height,
            pixels: vec![false; len],
        }
    }

    #[inline]
    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some((y as u32 * self.width + x as u32) as usize)
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        match self.index(x, y) {
            Some(i) => self.pixels[i],
            None => true, // out of bounds = solid
        }
    }

    pub fn set_solid(&mut self, x: i32, y: i32, solid: bool) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = solid;
        }
    }

    /// Remove terrain in a circle (explosion crater). Uses integer math for determinism.
    pub fn apply_damage(&mut self, cx: i32, cy: i32, radius: i32) {
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r2 {
                    self.set_solid(cx + dx, cy + dy, false);
                }
            }
        }
    }
}

/// Generate a simple hilly terrain for a given seed (deterministic).
pub fn generate_terrain(seed: u32, width: u32, height: u32) -> Terrain {
    let mut t = Terrain::new(width, height);
    let mut s = seed;
    for x in 0..width {
        // Simple "noise" for ground height: deterministic from seed
        let h = (height * 3 / 4) as i32
            - ((s.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as i32 % 40);
        let h = h.clamp(0, height as i32 - 1);
        s = s.wrapping_mul(1103515245).wrapping_add(12345);
        for y in h..height as i32 {
            t.set_solid(x as i32, y, true);
        }
    }
    t
}

pub const DEFAULT_WIDTH: u32 = WIDTH;
pub const DEFAULT_HEIGHT: u32 = HEIGHT;
