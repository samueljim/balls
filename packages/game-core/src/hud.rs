use egui_macroquad::egui;
use macroquad::prelude::*;

use crate::physics::{Ball, TEAM_COLORS, BALL_RADIUS};
use crate::state::Phase;
use crate::weapons::{Weapon, WeaponCategory};

/// Result returned by `draw_weapon_menu_egui`.
pub enum WeaponMenuResult {
    /// Player selected this weapon.
    Select(Weapon),
    /// Player closed the menu without selecting.
    Close,
    /// No action this frame.
    None,
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
    connected: bool,
) {
    let sw = screen_width();
    let sh = screen_height();

    // Responsive breakpoint — screen_width/height return CSS px in macroquad WASM.
    let is_mobile_hud = sw < 600.0 || sh < 700.0;
    let bar_h = if is_mobile_hud { 68.0 } else { 44.0 };

    draw_rectangle(0.0, 0.0, sw, bar_h, Color::new(0.0, 0.0, 0.0, 0.80));

    if phase == Phase::GameOver {
        // ── GAME OVER overlay ────────────────────────────────────────────────
        // Dark semi-transparent backdrop over the full screen.
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.65));

        // ── Winner banner ────────────────────────────────────────────────────
        let (winner_text, banner_col) = if let Some(team) = winning_team {
            let (r, g, b) = TEAM_COLORS[team as usize % TEAM_COLORS.len()];
            // Try to find a name for the winning team.
            let name = balls
                .iter()
                .find(|b| b.team == team)
                .map(|b| b.name.as_str())
                .unwrap_or("Unknown");
            (format!("{} Wins!", name), Color::new(r, g, b, 1.0))
        } else {
            ("Draw!".to_string(), WHITE)
        };
        let banner_font = if sw < 500.0 { 30u16 } else { 42u16 };
        let tw = measure_text(&winner_text, None, banner_font, 1.0).width;
        let banner_y = sh * 0.22;
        // Shadow
        draw_text(&winner_text, sw / 2.0 - tw / 2.0 + 2.0, banner_y + 2.0, banner_font as f32, Color::new(0.0, 0.0, 0.0, 0.7));
        draw_text(&winner_text, sw / 2.0 - tw / 2.0, banner_y, banner_font as f32, banner_col);

        // ── Score table ──────────────────────────────────────────────────────
        // Show each ball sorted by team, with HP bars and status.
        let table_y_start = sh * 0.32;
        let row_h = if sw < 500.0 { 28.0_f32 } else { 34.0_f32 };
        let name_font = if sw < 500.0 { 16u16 } else { 20u16 };
        let bar_w = sw * 0.30;
        let bar_h_px = if sw < 500.0 { 12.0_f32 } else { 16.0_f32 };
        let col_name_x = sw * 0.12;
        let col_bar_x  = sw * 0.50;

        // Sort balls by team for a consistent ordering.
        let mut sorted: Vec<&Ball> = balls.iter().collect();
        sorted.sort_by_key(|b| b.team);

        for (i, ball) in sorted.iter().enumerate() {
            let row_y = table_y_start + i as f32 * row_h;
            if row_y + row_h > sh * 0.78 { break; } // don't overflow into button

            let (tr, tg, tb) = TEAM_COLORS[ball.team as usize % TEAM_COLORS.len()];
            let team_col = Color::new(tr, tg, tb, 1.0);

            // Team colour dot
            draw_circle(col_name_x - 14.0, row_y + row_h * 0.45, 6.0, team_col);

            // Ball name
            let name_col = if ball.alive { WHITE } else { Color::new(0.55, 0.55, 0.55, 1.0) };
            draw_text(&ball.name, col_name_x, row_y + name_font as f32, name_font as f32, name_col);

            // HP bar (greyed out when eliminated)
            let hp_frac = (ball.health as f32 / ball.max_health as f32).clamp(0.0, 1.0);
            let bar_filled_w = bar_w * hp_frac;
            draw_rectangle(col_bar_x, row_y + (row_h - bar_h_px) / 2.0, bar_w, bar_h_px,
                Color::new(0.25, 0.25, 0.25, 0.9));
            if bar_filled_w > 0.0 {
                let bar_col = if ball.alive { Color::new(tr * 0.9, tg * 0.9, tb * 0.9, 1.0) }
                              else { Color::new(0.4, 0.4, 0.4, 0.8) };
                draw_rectangle(col_bar_x, row_y + (row_h - bar_h_px) / 2.0, bar_filled_w, bar_h_px, bar_col);
            }
            // HP text
            let hp_label = if ball.alive {
                format!("{} HP", ball.health)
            } else {
                "Eliminated".to_string()
            };
            let hp_font = (name_font - 2).max(12);
            draw_text(&hp_label, col_bar_x + bar_w + 8.0, row_y + hp_font as f32, hp_font as f32,
                Color::new(0.85, 0.85, 0.85, 0.9));
        }

        // ── NEW GAME button ──────────────────────────────────────────────────
        let btn_w = (sw * 0.45).min(220.0);
        let btn_h = if sw < 500.0 { 44.0_f32 } else { 52.0_f32 };
        let btn_x = sw / 2.0 - btn_w / 2.0;
        let btn_y = sh * 0.80;
        // Button shadow
        draw_rectangle(btn_x + 3.0, btn_y + 4.0, btn_w, btn_h, Color::new(0.0, 0.0, 0.0, 0.5));
        // Button background
        draw_rectangle(btn_x, btn_y, btn_w, btn_h, Color::new(0.15, 0.60, 0.25, 1.0));
        // Button border
        draw_rectangle_lines(btn_x, btn_y, btn_w, btn_h, 2.0, Color::new(0.4, 0.95, 0.5, 0.9));
        // Button label
        let label = "NEW GAME";
        let label_font = if sw < 500.0 { 20u16 } else { 24u16 };
        let lw = measure_text(label, None, label_font, 1.0).width;
        draw_text(label, sw / 2.0 - lw / 2.0, btn_y + btn_h / 2.0 + label_font as f32 * 0.38, label_font as f32, WHITE);

        return;
    }

    if is_mobile_hud {
        // ── MOBILE: two-row layout ──────────────────────────────────────────
        // Row 1 (top): whose turn (left) | timer (right)
        // Row 2 (bottom): HP + move bar (left) | phase (center) | wind (right)

        let timer_text = format!("{:.0}", turn_timer.max(0.0));
        let timer_font = 26u16;
        let timer_color = if turn_timer < 10.0 {
            Color::new(1.0, 0.3, 0.2, 1.0)
        } else {
            WHITE
        };
        let tw = measure_text(&timer_text, None, timer_font, 1.0).width;
        draw_text(&timer_text, sw - tw - 10.0, 24.0, timer_font as f32, timer_color);

        // Whose turn label — use the player's actual name, not team number.
        let (turn_label, turn_color) = if is_my_turn {
            ("YOUR TURN".to_string(), Color::new(0.4, 1.0, 0.5, 1.0))
        } else {
            let name = if !turn_owner_name.is_empty() {
                turn_owner_name.to_string()
            } else if current_ball < balls.len() {
                balls[current_ball].name.clone()
            } else {
                "Opponent".to_string()
            };
            let label = if connected && !phase.allows_input() {
                format!("Waiting for {}…", name)
            } else {
                format!("{}'s Turn", name)
            };
            (label, Color::new(1.0, 0.85, 0.35, 1.0))
        };
        // Clamp label so it never overlaps the timer
        let max_label_w = (sw - tw - 24.0).max(0.0);
        let label_font = 18u16;
        let raw_lw = measure_text(&turn_label, None, label_font, 1.0).width;
        // Shrink font if the label is still too wide (very long names)
        let actual_font = if raw_lw > max_label_w { 14u16 } else { label_font };
        draw_text(&turn_label, 10.0, 23.0, actual_font as f32, turn_color);

        // Row 2 — HP + mini move bar
        if current_ball < balls.len() {
            let ball = &balls[current_ball];
            let hp_text = format!("HP:{}", ball.health);
            let hp_font = 15u16;
            draw_text(&hp_text, 10.0, 52.0, hp_font as f32, WHITE);
            let hp_w = measure_text(&hp_text, None, hp_font, 1.0).width;

            let move_remaining = ball.movement_remaining();
            let move_frac = (move_remaining / ball.movement_budget).clamp(0.0, 1.0);
            let bar_x = 10.0 + hp_w + 6.0;
            let bar_w = 40.0;
            let bar_h_px = 7.0;
            let bar_y = 46.0;
            draw_rectangle(bar_x - 1.0, bar_y - 1.0, bar_w + 2.0, bar_h_px + 2.0,
                Color::new(0.0, 0.0, 0.0, 0.6));
            let bar_color = if move_frac > 0.5 {
                Color::new(0.2, 0.7, 1.0, 0.9)
            } else if move_frac > 0.15 {
                Color::new(0.9, 0.7, 0.2, 0.9)
            } else {
                Color::new(0.9, 0.3, 0.2, 0.9)
            };
            draw_rectangle(bar_x, bar_y, bar_w * move_frac, bar_h_px, bar_color);
        }

        // Row 2 center — phase label
        let phase_text = phase.label();
        let phase_color = if is_my_turn {
            Color::new(0.5, 1.0, 0.5, 1.0)
        } else {
            Color::new(0.85, 0.75, 0.4, 1.0)
        };
        let ph_font = 13u16;
        let ph_w = measure_text(phase_text, None, ph_font, 1.0).width;
        draw_text(phase_text, sw / 2.0 - ph_w / 2.0, 52.0, ph_font as f32, phase_color);

        // Row 2 right — wind (compact)
        let wind_label = if wind.abs() < 0.5 {
            "~calm".to_string()
        } else {
            let arrow = if wind > 0.0 { ">>" } else { "<<" };
            format!("{} {:.0}", arrow, wind.abs())
        };
        let wf = 13u16;
        let ww = measure_text(&wind_label, None, wf, 1.0).width;
        draw_text(&wind_label, sw - ww - 10.0, 52.0, wf as f32,
            Color::new(0.5, 0.8, 1.0, 0.9));
    } else {
        // ── DESKTOP: original single-row layout ────────────────────────────
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

            let bar_w = 60.0;
            let bar_h = 8.0;
            let bar_y = 20.0;
            draw_rectangle(move_x - 1.0, bar_y - 1.0, bar_w + 2.0, bar_h + 2.0,
                Color::new(0.0, 0.0, 0.0, 0.6));

            let move_frac = move_remaining / ball.movement_budget;
            let bar_color = if move_frac > 0.5 {
                Color::new(0.2, 0.7, 1.0, 0.9)
            } else if move_frac > 0.15 {
                Color::new(0.9, 0.7, 0.2, 0.9)
            } else {
                Color::new(0.9, 0.3, 0.2, 0.9)
            };
            draw_rectangle(move_x, bar_y, bar_w * move_frac, bar_h, bar_color);
            draw_text("MOVE", move_x, 16.0, 11.0, Color::new(0.7, 0.7, 0.7, 0.8));
            let move_text = format!("{:.0}%", move_percent);
            draw_text(&move_text, move_x + bar_w + 4.0, 28.0, 14.0,
                if move_frac < 0.1 { Color::new(1.0, 0.3, 0.3, 1.0) } else { WHITE });
        }

        // Phase label — use player name, not team number
        let (phase_label, phase_color) = if is_my_turn {
            let label = if turn_owner_name.is_empty() {
                format!("YOUR TURN — {}", phase.label())
            } else {
                format!("YOUR TURN ({}) — {}", turn_owner_name, phase.label())
            };
            (label, Color::new(0.7, 1.0, 0.7, 1.0))
        } else {
            let owner = if turn_owner_name.is_empty() {
                "Opponent".to_string()
            } else {
                turn_owner_name.to_string()
            };
            let label = if connected && !phase.allows_input() {
                format!("Waiting for {}…", owner)
            } else {
                format!("{} — {}", owner, phase.label())
            };
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
        draw_text(&wind_label, sw - 200.0, 30.0, 16.0,
            Color::new(0.5, 0.8, 1.0, 0.9));
    }

    // Draw weapon button — desktop only (mobile uses the JS overlay WEAPON button)
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
}

/// Draws the weapon selection menu using egui.
///
/// Returns a [`WeaponMenuResult`] indicating whether a weapon was selected, the
/// menu was closed, or nothing happened this frame.  The caller must call
/// `egui_macroquad::draw()` after this function returns.
pub fn draw_weapon_menu_egui(
    ctx: &egui::Context,
    selected_weapon: Weapon,
    active_category: &mut WeaponCategory,
) -> WeaponMenuResult {
    let sw = screen_width();
    let sh = screen_height();
    let is_mobile = sw < 600.0 || sh < 700.0;

    // Menu dimensions — match the old layout so nothing looks jarring.
    let menu_w = if is_mobile { sw * 0.97 } else { 520.0_f32.min(sw * 0.85) };
    let controls_reserved = if is_mobile { 225.0_f32 } else { 0.0_f32 };
    let menu_h = if is_mobile {
        (sh - 50.0 - controls_reserved).max(200.0)
    } else {
        620.0_f32.min(sh * 0.85)
    };
    let menu_x = sw / 2.0 - menu_w / 2.0;
    let menu_y = if is_mobile { 50.0 } else { sh / 2.0 - menu_h / 2.0 };

    // ── Visuals ───────────────────────────────────────────────────────────────
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill         = egui::Color32::from_rgb(20, 25, 36);
    visuals.panel_fill          = egui::Color32::from_rgb(20, 25, 36);
    visuals.extreme_bg_color    = egui::Color32::from_rgb(13, 16, 23);
    // Default widget backgrounds
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 25, 36);
    visuals.widgets.inactive.bg_fill       = egui::Color32::from_rgb(28, 36, 52);
    visuals.widgets.hovered.bg_fill        = egui::Color32::from_rgb(38, 60, 90);
    visuals.widgets.active.bg_fill         = egui::Color32::from_rgb(48, 105, 130);
    // Category tab "selected" colour
    visuals.selection.bg_fill   = egui::Color32::from_rgb(45, 110, 80);
    visuals.selection.stroke    = egui::Stroke::new(1.0, egui::Color32::from_rgb(77, 220, 127));
    visuals.window_stroke       = egui::Stroke::new(2.0, egui::Color32::from_rgb(64, 115, 166));
    visuals.window_corner_radius = egui::CornerRadius::same(6);
    ctx.set_visuals(visuals);

    // ── Text styles ───────────────────────────────────────────────────────────
    let body_size   = if is_mobile { 14.0 } else { 15.0 };
    let button_size = if is_mobile { 12.5 } else { 13.5 };
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(body_size),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(button_size),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(11.0),
    );
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.item_spacing   = egui::vec2(4.0, 3.0);
    ctx.set_style(style);

    let mut result = WeaponMenuResult::None;

    egui::Window::new("weapon_menu")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .fixed_pos([menu_x, menu_y])
        .fixed_size([menu_w, menu_h])
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 25, 36))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(64, 115, 166)))
                .inner_margin(egui::Margin::same(0))
                .corner_radius(egui::CornerRadius::same(6)),
        )
        .show(ctx, |ui| {
            // ── Header ────────────────────────────────────────────────────────
            let header_h = if is_mobile { 40.0 } else { 48.0 };
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(15, 20, 30))
                .inner_margin(egui::Margin::symmetric(12, 0))
                .show(ui, |ui| {
                    ui.set_min_width(menu_w);
                    ui.set_min_height(header_h);
                    ui.horizontal_centered(|ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new("\u{2694}  WEAPONS")
                                .size(if is_mobile { 17.0 } else { 20.0 })
                                .color(egui::Color32::from_rgb(230, 240, 255))
                                .strong(),
                        ));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close_btn = egui::Button::new(
                                egui::RichText::new("\u{2715}")
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(180, 190, 210)),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE);
                            if ui.add(close_btn).clicked() {
                                result = WeaponMenuResult::Close;
                            }
                        });
                    });
                });

            ui.add(egui::Separator::default().spacing(0.0));

            // ── Category tabs ─────────────────────────────────────────────────
            let cats = [
                WeaponCategory::Explosives,
                WeaponCategory::Ballistics,
                WeaponCategory::Special,
                WeaponCategory::Utilities,
            ];
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 23, 34))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_min_width(menu_w);
                    ui.horizontal_wrapped(|ui| {
                        for cat in &cats {
                            let is_active = *active_category == *cat;
                            let label = egui::RichText::new(cat.name())
                                .size(if is_mobile { 12.0 } else { 13.0 })
                                .color(if is_active {
                                    egui::Color32::from_rgb(120, 230, 160)
                                } else {
                                    egui::Color32::from_rgb(155, 170, 195)
                                });
                            if ui.add(egui::SelectableLabel::new(is_active, label)).clicked() {
                                *active_category = *cat;
                            }
                        }
                    });
                });

            ui.add(egui::Separator::default().spacing(0.0));

            // ── Weapon list ───────────────────────────────────────────────────
            let item_h    = if is_mobile { 46.0_f32 } else { 52.0_f32 };
            let name_size = if is_mobile { 14.5 } else { 15.5 };
            let desc_size = if is_mobile { 10.5 } else { 11.5 };
            let icon_size = if is_mobile { 14.0 } else { 15.0 };

            let weapons: Vec<Weapon> = Weapon::all()
                .iter()
                .filter(|w| w.category() == *active_category)
                .cloned()
                .collect();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_source("weapon_list")
                .show(ui, |ui| {
                    ui.set_min_width(menu_w);
                    for w in weapons {
                        let is_selected = w == selected_weapon;
                        let avail_w = ui.available_width();
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(avail_w, item_h),
                            egui::Sense::click(),
                        );
                        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

                        if ui.is_rect_visible(rect) {
                            let bg = if is_selected {
                                egui::Color32::from_rgb(38, 88, 62)
                            } else if response.hovered() {
                                egui::Color32::from_rgb(28, 48, 72)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 0.0, bg);

                            // Green accent bar on left edge for selected weapon
                            if is_selected {
                                let accent_rect = egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(3.0, item_h),
                                );
                                ui.painter().rect_filled(
                                    accent_rect,
                                    0.0,
                                    egui::Color32::from_rgb(77, 200, 120),
                                );
                                ui.painter().rect_stroke(
                                    rect.shrink(1.0),
                                    egui::CornerRadius::same(2),
                                    egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_rgba_unmultiplied(77, 200, 120, 60),
                                    ),
                                    egui::StrokeKind::Middle,
                                );
                            }

                            let icon_x  = rect.left() + 12.0;
                            let text_x  = icon_x + (if is_mobile { 28.0 } else { 32.0 });
                            let name_y  = rect.top() + item_h * 0.31;
                            let desc_y  = rect.top() + item_h * 0.60;

                            // Icon (monospace keeps alignment consistent across symbols)
                            ui.painter().text(
                                egui::pos2(icon_x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                w.icon(),
                                egui::FontId::monospace(icon_size),
                                egui::Color32::from_rgb(195, 210, 230),
                            );

                            // Weapon name
                            let name_color = if is_selected {
                                egui::Color32::from_rgb(130, 245, 170)
                            } else {
                                egui::Color32::from_rgb(220, 228, 240)
                            };
                            ui.painter().text(
                                egui::pos2(text_x, name_y),
                                egui::Align2::LEFT_TOP,
                                w.name(),
                                egui::FontId::proportional(name_size),
                                name_color,
                            );

                            // Description
                            ui.painter().text(
                                egui::pos2(text_x, desc_y),
                                egui::Align2::LEFT_TOP,
                                w.description(),
                                egui::FontId::proportional(desc_size),
                                egui::Color32::from_rgb(115, 130, 158),
                            );

                            // Subtle row divider
                            ui.painter().hline(
                                rect.left()..=rect.right(),
                                rect.bottom(),
                                egui::Stroke::new(
                                    0.5,
                                    egui::Color32::from_rgba_unmultiplied(60, 80, 110, 80),
                                ),
                            );
                        }

                        if response.clicked() {
                            result = WeaponMenuResult::Select(w);
                        }
                    }
                });

            // ── Footer hint ───────────────────────────────────────────────────
            ui.add(egui::Separator::default().spacing(0.0));
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(15, 18, 26))
                .inner_margin(egui::Margin::symmetric(0, 6))
                .show(ui, |ui| {
                    ui.set_min_width(menu_w);
                    ui.vertical_centered(|ui| {
                        let hint = if is_mobile {
                            "Tap to select  \u{2022}  Swipe to scroll  \u{2022}  Tap outside to close"
                        } else {
                            "Click to select  \u{2022}  Scroll to browse  \u{2022}  ESC to close"
                        };
                        ui.add(egui::Label::new(
                            egui::RichText::new(hint)
                                .size(if is_mobile { 10.0 } else { 11.5 })
                                .color(egui::Color32::from_rgb(95, 110, 138)),
                        ));
                    });
                });
        });

    // Close when the user LEFT-clicks anywhere outside the menu window.
    // We use primary_clicked (left button only) so that right-clicking outside
    // does not accidentally trigger a close.
    if ctx.input(|i| i.pointer.primary_clicked()) && !ctx.is_pointer_over_area() {
        result = WeaponMenuResult::Close;
    }

    // Close on Escape.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = WeaponMenuResult::Close;
    }

    result
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