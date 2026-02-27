use macroquad::prelude::*;

use crate::physics::{Ball, TEAM_COLORS, BALL_RADIUS};
use crate::state::Phase;
use crate::weapons::{Weapon, WeaponCategory};

/// Shared layout constants for the weapon menu (used by hud rendering and click hit-testing).
pub struct WeaponMenuLayout {
    pub menu_x: f32,
    pub menu_y: f32,
    pub menu_w: f32,
    pub menu_h: f32,
    pub header_h: f32,
    pub footer_h: f32,
    pub content_y: f32,
    pub content_h: f32,
    pub padding: f32,
    pub item_h: f32,
    pub cat_header_h: f32,
    pub item_padding: f32,
    pub cat_spacing: f32,
    pub is_mobile: bool,
    /// Device pixel ratio (1.0 on desktop, 2-3 on HiDPI mobile).
    /// Used ONLY to convert CSS px → physical pixels for the GL scissor rect.
    pub dpi: f32,
}

impl WeaponMenuLayout {
    pub fn new() -> Self {
        let sw = screen_width();
        let sh = screen_height();
        // screen_width/height return CSS (logical) pixels in macroquad WASM.
        // screen_dpi_scale() gives the device pixel ratio — use it ONLY for
        // is_mobile detection (where we compare against CSS px thresholds) and
        // for the GL scissor rect which needs physical pixels.
        let dpi = screen_dpi_scale();
        let css_w = sw;
        let css_h = sh;
        let is_mobile = css_w < 600.0 || css_h < 700.0;

        // All sizes below are in CSS pixels — no * dpi.
        let menu_w = if is_mobile { sw * 0.97 } else { 520.0_f32.min(sw * 0.85) };
        let menu_h = if is_mobile { sh * 0.92 } else { 620.0_f32.min(sh * 0.85) };
        let menu_x = sw / 2.0 - menu_w / 2.0;
        let menu_y = sh / 2.0 - menu_h / 2.0;
        let header_h = if is_mobile { 44.0 } else { 52.0 };
        let footer_h = if is_mobile { 36.0 } else { 44.0 };
        let content_y = menu_y + header_h + 6.0;
        let content_h = menu_h - header_h - 6.0 - footer_h;
        let padding = if is_mobile { 6.0 } else { 12.0 };
        let item_h = if is_mobile { 50.0 } else { 58.0 };
        let cat_header_h = if is_mobile { 24.0 } else { 30.0 };
        let item_padding = 2.0;
        let cat_spacing = 6.0;
        Self { menu_x, menu_y, menu_w, menu_h, header_h, footer_h, content_y, content_h,
               padding, item_h, cat_header_h, item_padding, cat_spacing, is_mobile, dpi }
    }

    /// Total height of all content (categories + weapons) to determine max scroll.
    pub fn total_content_height(&self) -> f32 {
        let categories = [
            WeaponCategory::Explosives,
            WeaponCategory::Ballistics,
            WeaponCategory::Special,
            WeaponCategory::Utilities,
        ];
        let all_weapons = Weapon::all();
        let mut by_category: std::collections::HashMap<WeaponCategory, Vec<&Weapon>> = std::collections::HashMap::new();
        for w in all_weapons {
            by_category.entry(w.category()).or_insert_with(Vec::new).push(w);
        }
        let mut h = 0.0_f32;
        for (i, cat) in categories.iter().enumerate() {
            if let Some(weapons) = by_category.get(cat) {
                h += self.cat_header_h + self.item_padding;
                h += weapons.len() as f32 * (self.item_h + self.item_padding);
                if i < categories.len() - 1 {
                    h += self.cat_spacing;
                }
            }
        }
        h
    }

    pub fn max_scroll(&self) -> f32 {
        (self.total_content_height() - self.content_h).max(0.0)
    }
}

pub fn draw_hud(
    balls: &[Ball],
    current_ball: usize,
    phase: Phase,
    selected_weapon: Weapon,
    charge_power: f32,
    turn_timer: f32,
    wind: f32,
    winning_team: Option<u32>,
    is_my_turn: bool,
    turn_owner_name: &str,
    weapon_menu_open: bool,
    weapon_menu_scroll: f32,
) {
    let sw = screen_width();
    let sh = screen_height();

    draw_rectangle(0.0, 0.0, sw, 44.0, Color::new(0.0, 0.0, 0.0, 0.75));

    if phase == Phase::GameOver {
        if let Some(team) = winning_team {
            let (r, g, b) = TEAM_COLORS[team as usize % TEAM_COLORS.len()];
            let text = format!("Team {} Wins!", team + 1);
            let tw = measure_text(&text, None, 36, 1.0).width;
            draw_text(&text, sw / 2.0 - tw / 2.0, 32.0, 36.0, Color::new(r, g, b, 1.0));
        } else {
            let text = "Draw!";
            let tw = measure_text(text, None, 36, 1.0).width;
            draw_text(text, sw / 2.0 - tw / 2.0, 32.0, 36.0, WHITE);
        }

        let hint = "Press R to restart";
        let hw = measure_text(hint, None, 24, 1.0).width;
        draw_text(
            hint,
            sw / 2.0 - hw / 2.0,
            sh / 2.0 + 40.0,
            24.0,
            Color::new(0.8, 0.8, 0.8, 0.8),
        );
        return;
    }

    if current_ball < balls.len() {
        let ball = &balls[current_ball];
        let (r, g, b) = TEAM_COLORS[ball.team as usize % TEAM_COLORS.len()];
        let team_color = Color::new(r, g, b, 1.0);
        let label = format!("{}", ball.name);
        draw_text(&label, 12.0, 30.0, 26.0, team_color);

        let hp = format!("HP:{}", ball.health);
        let hp_x = 12.0 + measure_text(&label, None, 26, 1.0).width + 14.0;
        draw_text(&hp, hp_x, 30.0, 20.0, WHITE);
        
        // Draw movement bar
        let move_remaining = ball.movement_remaining();
        let move_percent = (move_remaining / ball.movement_budget * 100.0).min(100.0);
        let move_x = hp_x + measure_text(&hp, None, 20, 1.0).width + 20.0;
        
        // Movement bar background
        let bar_w = 60.0;
        let bar_h = 8.0;
        let bar_y = 20.0;
        draw_rectangle(move_x - 1.0, bar_y - 1.0, bar_w + 2.0, bar_h + 2.0, Color::new(0.0, 0.0, 0.0, 0.6));
        
        // Movement bar foreground
        let move_frac = move_remaining / ball.movement_budget;
        let bar_color = if move_frac > 0.5 {
            Color::new(0.2, 0.7, 1.0, 0.9)
        } else if move_frac > 0.15 {
            Color::new(0.9, 0.7, 0.2, 0.9)
        } else {
            Color::new(0.9, 0.3, 0.2, 0.9)
        };
        draw_rectangle(move_x, bar_y, bar_w * move_frac, bar_h, bar_color);
        
        // Movement label
        draw_text("MOVE", move_x, 16.0, 11.0, Color::new(0.7, 0.7, 0.7, 0.8));
        let move_text = format!("{:.0}%", move_percent);
        draw_text(&move_text, move_x + bar_w + 4.0, 28.0, 14.0, 
            if move_frac < 0.1 { Color::new(1.0, 0.3, 0.3, 1.0) } else { WHITE });
    }

    // Always display the current phase to all players. Highlight when it's your turn.
    let (phase_label, phase_color) = if is_my_turn {
        let label = if turn_owner_name.is_empty() {
            format!("YOUR TURN — {}", phase.label())
        } else {
            format!("YOUR TURN ({}) — {}", turn_owner_name, phase.label())
        };
        (label, Color::new(0.7, 1.0, 0.7, 1.0))
    } else {
        let owner = if turn_owner_name.is_empty() { "Opponent".to_string() } else { turn_owner_name.to_string() };
        let label = format!("{} — {}", owner, phase.label());
        (label, Color::new(0.85, 0.75, 0.4, 1.0))
    };
    let pw = measure_text(&phase_label, None, 20, 1.0).width;
    draw_text(&phase_label, sw / 2.0 - pw / 2.0, 30.0, 20.0, phase_color);

    let timer_text = format!("{:.0}", turn_timer.max(0.0));
    let timer_color = if turn_timer < 10.0 {
        Color::new(1.0, 0.3, 0.2, 1.0)
    } else {
        WHITE
    };
    draw_text(&timer_text, sw - 60.0, 30.0, 28.0, timer_color);

    let wind_label = if wind.abs() < 0.5 {
        "Wind: calm".to_string()
    } else {
        let arrow = if wind > 0.0 { ">>>" } else { "<<<" };
        format!("Wind: {} {:.0}", arrow, wind.abs())
    };
    draw_text(
        &wind_label,
        sw - 200.0,
        30.0,
        16.0,
        Color::new(0.5, 0.8, 1.0, 0.9),
    );

    // Draw weapon button — desktop only (mobile uses the JS overlay WEAPON button)
    let dpi = screen_dpi_scale();
    let css_sw = sw / dpi;
    let is_mobile_hud = css_sw < 600.0 || (sh / dpi) < 700.0;
    if !is_mobile_hud {
        let weapon_button = get_weapon_button_bounds();
        let (mx, my) = mouse_position();
        let is_hovering = mx >= weapon_button.0 && mx <= weapon_button.0 + weapon_button.2
            && my >= weapon_button.1 && my <= weapon_button.1 + weapon_button.3;
        let bg_color = if is_hovering {
            Color::new(0.3, 0.5, 0.8, 0.9)
        } else {
            Color::new(0.2, 0.3, 0.5, 0.8)
        };
        draw_rectangle(weapon_button.0, weapon_button.1, weapon_button.2, weapon_button.3, bg_color);
        draw_rectangle_lines(weapon_button.0, weapon_button.1, weapon_button.2, weapon_button.3, 2.0,
            Color::new(0.7, 0.8, 0.9, 1.0));
        let weapon_text = format!(">> {} (Click or TAB)", selected_weapon.name());
        draw_text(&weapon_text, weapon_button.0 + 8.0, weapon_button.1 + 22.0, 16.0, WHITE);
    }

    if is_my_turn && (phase == Phase::Aiming || phase == Phase::Charging) {
        let meter_w = 220.0;
        let meter_h = 24.0;
        let mx = sw / 2.0 - meter_w / 2.0;
        let my = sh - 56.0;
        draw_rectangle(mx - 2.0, my - 2.0, meter_w + 4.0, meter_h + 4.0, Color::new(0.0, 0.0, 0.0, 0.8));
        let fill = charge_power / 100.0;
        let bar_color = Color::new(0.2 + fill * 0.8, 0.9 - fill * 0.7, 0.1, 1.0);
        draw_rectangle(mx, my, meter_w * fill, meter_h, bar_color);
        draw_rectangle_lines(mx - 2.0, my - 2.0, meter_w + 4.0, meter_h + 4.0, 2.0, WHITE);
        let ptext = format!("POWER {:.0}%", charge_power);
        let ptw = measure_text(&ptext, None, 18, 1.0).width;
        draw_text(&ptext, sw / 2.0 - ptw / 2.0, my - 6.0, 18.0, WHITE);
        if phase == Phase::Aiming {
            let hint = "Hold LEFT CLICK to charge, release to FIRE";
            let hw = measure_text(hint, None, 14, 1.0).width;
            draw_text(hint, sw / 2.0 - hw / 2.0, my - 24.0, 14.0, Color::new(0.9, 0.9, 0.5, 0.95));
        }
    }

    // Bottom hint — desktop only
    if !is_mobile_hud {
        draw_text(
            "WASD/Arrows move  Space jump  TAB weapons  Scroll zoom  Right-drag pan",
            10.0,
            sh - 6.0,
            13.0,
            Color::new(0.5, 0.5, 0.5, 0.5),
        );
    }
    
    // Draw weapon menu
    if weapon_menu_open {
        draw_weapon_menu(selected_weapon, weapon_menu_scroll);
    }
}

fn draw_weapon_menu(selected_weapon: Weapon, scroll_offset: f32) {
    let sw = screen_width();
    let sh = screen_height();
    let layout = WeaponMenuLayout::new();
    let menu_x = layout.menu_x;
    let menu_y = layout.menu_y;
    let menu_w = layout.menu_w;
    let menu_h = layout.menu_h;
    let is_mobile = layout.is_mobile;
    let dpi = layout.dpi;
    
    // Dark overlay
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.75));
    
    // Modern rounded panel with shadow
    for i in 0..4 {
        let offset = (4 - i) as f32;
        draw_rectangle(
            menu_x + offset,
            menu_y + offset,
            menu_w,
            menu_h,
            Color::new(0.0, 0.0, 0.0, 0.15 * (i as f32 + 1.0)),
        );
    }
    
    // Main panel
    draw_rectangle(menu_x, menu_y, menu_w, menu_h, Color::new(0.08, 0.1, 0.14, 0.98));
    draw_rectangle(menu_x, menu_y, menu_w, 2.0, Color::new(0.3, 0.6, 0.9, 0.8));
    draw_rectangle_lines(menu_x, menu_y, menu_w, menu_h, 2.0, Color::new(0.25, 0.45, 0.65, 0.9));
    
    // Header
    let header_h = layout.header_h;
    draw_rectangle(menu_x, menu_y, menu_w, header_h, Color::new(0.12, 0.15, 0.2, 1.0));
    draw_rectangle(menu_x, menu_y + header_h, menu_w, 1.0, Color::new(0.3, 0.5, 0.7, 0.5));
    
    let title = "== WEAPON ARSENAL ==";
    let title_size = if is_mobile { 18.0 } else { 22.0 };
    let title_w = measure_text(title, None, title_size as u16, 1.0).width;
    draw_text(
        title,
        menu_x + menu_w / 2.0 - title_w / 2.0,
        menu_y + header_h - 12.0,
        title_size,
        Color::new(0.9, 0.95, 1.0, 1.0),
    );
    
    // Content area dimensions
    let content_y = layout.content_y;
    let content_h = layout.content_h;
    let padding = layout.padding;
    let item_h = layout.item_h;
    let cat_header_h = layout.cat_header_h;
    let item_padding = layout.item_padding;
    let cat_spacing = layout.cat_spacing;
    
    // Organize weapons by category
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
    
    // Enable scissor clipping for the scrollable content area.
    // Scissor coordinates must be in PHYSICAL pixels; drawing coords are CSS px.
    let phys_clip_x = (menu_x * dpi) as i32;
    let phys_clip_sy = ((sh - (content_y + content_h)) * dpi) as i32; // GL: bottom-left origin
    let phys_clip_w = (menu_w * dpi) as i32;
    let phys_clip_h = (content_h * dpi) as i32;
    unsafe {
        get_internal_gl().quad_gl.scissor(Some((phys_clip_x, phys_clip_sy, phys_clip_w, phys_clip_h)));
    }
    
    let mut current_y = content_y - scroll_offset;
    
    for cat in &categories {
        if let Some(weapons) = by_category.get(cat) {
            // Category header
            let cat_y = current_y;
            
            if cat_y + cat_header_h > content_y - 10.0 && cat_y < content_y + content_h + 10.0 {
                draw_rectangle(
                    menu_x + padding,
                    cat_y,
                    menu_w - padding * 2.0,
                    cat_header_h,
                    Color::new(0.15, 0.2, 0.28, 0.8),
                );
                
                let cat_size = if is_mobile { 12.0 } else { 14.0 };
                draw_text(
                    cat.name(),
                    menu_x + padding + 8.0,
                    cat_y + cat_header_h - 6.0,
                    cat_size,
                    Color::new(0.7, 0.8, 0.95, 1.0),
                );
            }
            
            current_y += cat_header_h + item_padding;
            
            // Weapon items
            for w in weapons {
                let item_y = current_y;
                let item_x = menu_x + padding;
                let item_w = menu_w - padding * 2.0;
                
                if item_y + item_h > content_y - 10.0 && item_y < content_y + content_h + 10.0 {
                    let is_selected = **w == selected_weapon;
                    
                    let bg_color = if is_selected {
                        Color::new(0.2, 0.45, 0.35, 0.95)
                    } else {
                        Color::new(0.12, 0.14, 0.18, 0.7)
                    };
                    
                    draw_rectangle(item_x, item_y, item_w, item_h, bg_color);
                    
                    if is_selected {
                        draw_rectangle_lines(item_x, item_y, item_w, item_h, 2.0, Color::new(0.3, 0.9, 0.5, 1.0));
                        draw_rectangle_lines(item_x - 1.0, item_y - 1.0, item_w + 2.0, item_h + 2.0, 1.0, Color::new(0.5, 1.0, 0.7, 0.5));
                    } else {
                        draw_rectangle_lines(item_x, item_y, item_w, item_h, 1.0, Color::new(0.25, 0.28, 0.35, 0.7));
                    }
                    
                    // Weapon icon
                    let icon_size = if is_mobile { 16.0 } else { 20.0 };
                    let icon_x = item_x + 8.0;
                    draw_text(
                        w.icon(),
                        icon_x,
                        item_y + item_h / 2.0 + icon_size / 3.0,
                        icon_size,
                        Color::new(1.0, 1.0, 1.0, 1.0),
                    );
                    
                    // Weapon name — vertically centred in item
                    let name_x = icon_x + if is_mobile { 26.0 } else { 32.0 };
                    let name_size = if is_mobile { 15.0 } else { 17.0 };
                    let name_y = if is_mobile {
                        // On mobile: show name only, centred; show description below if selected
                        item_y + item_h / 2.0 + name_size * 0.35
                    } else {
                        item_y + item_h / 2.0 + name_size * 0.35
                    };
                    draw_text(
                        w.name(),
                        name_x,
                        name_y,
                        name_size,
                        if is_selected {
                            Color::new(1.0, 1.0, 1.0, 1.0)
                        } else {
                            Color::new(0.85, 0.88, 0.95, 1.0)
                        },
                    );

                    if is_mobile {
                        // Mobile: show a short description line below name when selected
                        if is_selected {
                            let desc_size = 10.0;
                            draw_text(
                                w.description(),
                                name_x,
                                item_y + item_h - 8.0,
                                desc_size,
                                Color::new(0.6, 0.85, 0.7, 0.9),
                            );
                        }
                    } else {
                        // Desktop: always show description + stats badges
                        let desc_size = 12.0;
                        draw_text(
                            w.description(),
                            name_x,
                            item_y + 42.0,
                            desc_size,
                            Color::new(0.55, 0.6, 0.7, 0.95),
                        );

                        let stats_x = item_x + item_w - 120.0;
                        let badge_y = item_y + 16.0;
                        let badge_w = 52.0;
                        let badge_h = 24.0;
                        let badge_spacing = 4.0;

                        if w.base_damage() > 0 {
                            draw_rectangle(stats_x, badge_y, badge_w, badge_h, Color::new(0.7, 0.2, 0.2, 0.7));
                            let dmg_text = format!("{}", w.base_damage());
                            let dmg_w = measure_text(&dmg_text, None, desc_size as u16, 1.0).width;
                            draw_text(&dmg_text, stats_x + badge_w / 2.0 - dmg_w / 2.0, badge_y + badge_h - 5.0, desc_size, WHITE);
                        }
                        if w.explosion_radius() > 0.0 {
                            draw_rectangle(stats_x + badge_w + badge_spacing, badge_y, badge_w, badge_h, Color::new(0.2, 0.4, 0.7, 0.7));
                            let rad_text = format!("{:.0}", w.explosion_radius());
                            let rad_w = measure_text(&rad_text, None, desc_size as u16, 1.0).width;
                            draw_text(&rad_text, stats_x + badge_w + badge_spacing + badge_w / 2.0 - rad_w / 2.0, badge_y + badge_h - 5.0, desc_size, WHITE);
                        }
                    }
                }
                
                current_y += item_h + item_padding;
            }
            
            current_y += cat_spacing;
        }
    }
    
    // Disable scissor clipping before drawing footer and scrollbar
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }
    
    // Scrollbar
    let max_scroll = layout.max_scroll();
    if max_scroll > 0.0 {
        let track_w = 6.0;
        let track_x = menu_x + menu_w - track_w - 4.0;
        let track_y = content_y;
        let track_h = content_h;
        
        draw_rectangle(track_x, track_y, track_w, track_h, Color::new(0.15, 0.18, 0.22, 0.6));
        
        let visible_ratio = (content_h / layout.total_content_height()).min(1.0);
        let thumb_h = (track_h * visible_ratio).max(24.0);
        let scroll_ratio = scroll_offset / max_scroll;
        let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);
        draw_rectangle(track_x, thumb_y, track_w, thumb_h, Color::new(0.5, 0.6, 0.8, 0.8));
    }
    
    // Footer with hints
    let footer_h = layout.footer_h;
    let footer_y = menu_y + menu_h - footer_h;
    draw_rectangle(menu_x, footer_y, menu_w, footer_h, Color::new(0.1, 0.12, 0.16, 1.0));
    draw_rectangle(menu_x, footer_y, menu_w, 1.0, Color::new(0.3, 0.5, 0.7, 0.4));
    
    let hint = if is_mobile {
        "Tap to select  •  Swipe to scroll  •  Tap outside to close"
    } else {
        "Click to select • Scroll to browse • TAB/ESC to close"
    };
    let hint_size = if is_mobile { 11.0 } else { 13.0 };
    let hint_w = measure_text(hint, None, hint_size as u16, 1.0).width;
    draw_text(
        hint,
        menu_x + menu_w / 2.0 - hint_w / 2.0,
        footer_y + footer_h - 10.0,
        hint_size,
        Color::new(0.6, 0.65, 0.75, 0.95),
    );
}

pub fn draw_ball_world(balls: &[Ball], current_ball: usize) {
    for (i, ball) in balls.iter().enumerate() {
        if !ball.alive {
            continue;
        }
        let (r, g, b) = TEAM_COLORS[ball.team as usize % TEAM_COLORS.len()];
        let color = Color::new(r, g, b, 1.0);
        let outline = Color::new(r * 0.4, g * 0.4, b * 0.4, 1.0);
        let rad = BALL_RADIUS;

        draw_circle(ball.x, ball.y, rad + 1.5, outline);
        draw_circle(ball.x, ball.y, rad, color);

        if i == current_ball {
            draw_circle_lines(ball.x, ball.y, rad + 3.0, 1.5, WHITE);
        }

        let eye_x_base = ball.x + ball.facing * 2.5;
        let eye_y = ball.y - 1.5;
        draw_circle(eye_x_base - 1.5, eye_y, 2.2, WHITE);
        draw_circle(eye_x_base + 1.5, eye_y, 2.2, WHITE);
        draw_circle(
            eye_x_base - 1.5 + ball.facing * 0.6,
            eye_y,
            1.1,
            Color::new(0.1, 0.1, 0.1, 1.0),
        );
        draw_circle(
            eye_x_base + 1.5 + ball.facing * 0.6,
            eye_y,
            1.1,
            Color::new(0.1, 0.1, 0.1, 1.0),
        );

        let bar_w = 26.0;
        let bar_h = 4.0;
        let bar_x = ball.x - bar_w / 2.0;
        let bar_y = ball.y - rad - 14.0;
        draw_rectangle(
            bar_x - 1.0,
            bar_y - 1.0,
            bar_w + 2.0,
            bar_h + 2.0,
            Color::new(0.0, 0.0, 0.0, 0.6),
        );
        let hp_frac = ball.health as f32 / ball.max_health as f32;
        let hp_color = if hp_frac > 0.5 {
            Color::new(0.15, 0.8, 0.15, 1.0)
        } else if hp_frac > 0.25 {
            Color::new(0.9, 0.7, 0.1, 1.0)
        } else {
            Color::new(0.9, 0.2, 0.1, 1.0)
        };
        draw_rectangle(bar_x, bar_y, bar_w * hp_frac, bar_h, hp_color);

        let name_size = 11.0;
        let nm = measure_text(&ball.name, None, name_size as u16, 1.0);
        draw_text(
            &ball.name,
            ball.x - nm.width / 2.0,
            bar_y - 3.0,
            name_size,
            Color::new(1.0, 1.0, 1.0, 0.85),
        );

        if ball.damage_timer > 0.0 && ball.last_damage > 0 {
            let popup_y = ball.y - rad - 22.0 - (2.0 - ball.damage_timer) * 20.0;
            let alpha = ball.damage_timer.min(1.0);
            let txt = format!("-{}", ball.last_damage);
            let tw = measure_text(&txt, None, 18, 1.0).width;
            draw_text(
                &txt,
                ball.x - tw / 2.0,
                popup_y,
                18.0,
                Color::new(1.0, 0.2, 0.1, alpha),
            );
        }
    }
}

// Returns (x, y, width, height) of weapon button
pub fn get_weapon_button_bounds() -> (f32, f32, f32, f32) {
    let sh = screen_height();
    let x = 10.0;
    let y = sh - 42.0;
    let w = 280.0;
    let h = 32.0;
    (x, y, w, h)
}