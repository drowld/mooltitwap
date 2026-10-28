//! The preset browser of the scope-first page: Tide's spotlight palette
//! (`y-flanger/src/editor.rs`, `draw_command_palette`), ported as-is onto this
//! crate's copy of Tide's UI layer. Search, All · Favorites · Recent, rows with
//! favourite, tags, rename and delete, and save-as. The load / save / init glue
//! is palette_v3's, which the v3 editor shares until it retires.
#![allow(deprecated)]

use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use nice_plug::prelude::*;
use std::sync::Arc;

use crate::gui::EditorState;
use crate::palette_v3::{ensure, preset_init, preset_load, preset_save};
use crate::params::PetalParams;
use crate::gui_scope::{g, seg};
use crate::ui::theme::{self, ui_scale, FAINT, INK, INK2, INSET, PANEL, SOFT};
use crate::ui::tokens as tk;

struct PaletteRow {
    name: String,
    fav: bool,
    build: String,
    tags: Vec<String>,
}

/// Filled or outlined 5-point star (filled = a center triangle-fan so concave
/// tessellation is exact). `r` = outer radius, logical-scaled by the caller.
fn paint_star(p: &egui::Painter, c: Pos2, r: f32, filled: bool, col: egui::Color32, s: f32) {
    use std::f32::consts::PI;
    let mut pts = [Pos2::ZERO; 10];
    for (k, pt) in pts.iter_mut().enumerate() {
        let rr = if k % 2 == 0 { r } else { r * 0.42 };
        let a = -PI / 2.0 + k as f32 * PI / 5.0;
        *pt = Pos2::new(c.x + a.cos() * rr, c.y + a.sin() * rr);
    }
    if filled {
        for k in 0..10 {
            p.add(egui::Shape::convex_polygon(
                vec![c, pts[k], pts[(k + 1) % 10]],
                col,
                Stroke::NONE,
            ));
        }
    } else {
        let mut poly: Vec<Pos2> = pts.to_vec();
        poly.push(pts[0]);
        p.add(egui::Shape::line(poly, Stroke::new(1.2 * s, col)));
    }
}

/// A small return/enter arrow (↵) anchored at its RIGHT edge `c`. Painted (not a
/// glyph) so it never tofus in Karla/Inconsolata. Returns the width it occupies.
fn paint_return(p: &egui::Painter, c: Pos2, s: f32, col: egui::Color32) -> f32 {
    let w = 9.0 * s;
    let st = Stroke::new(1.3 * s, col);
    let x0 = c.x - w;
    let x1 = c.x;
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x1, c.y)], st); // shaft
    p.line_segment([Pos2::new(x1, c.y), Pos2::new(x1, c.y - 5.0 * s)], st); // riser
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x0 + 3.0 * s, c.y - 3.0 * s)], st); // head
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x0 + 3.0 * s, c.y + 3.0 * s)], st);
    w
}

/// A small floppy-disk "save" icon centered at `c` (`half` = half-size). Painted
/// so it never depends on an emoji/glyph.
fn paint_disk(p: &egui::Painter, c: Pos2, half: f32, col: egui::Color32, s: f32) {
    let body = Rect::from_center_size(c, Vec2::splat(half * 2.0));
    p.rect_stroke(body, 2.0 * s, Stroke::new(1.3 * s, col), egui::StrokeKind::Inside);
    // sliding shutter (filled band near the top)
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(body.left() + half * 0.5, body.top() + half * 0.28),
            Pos2::new(body.right() - half * 0.5, body.top() + half * 0.85),
        ),
        0.0,
        col,
    );
    // label (outlined rect, lower half)
    p.rect_stroke(
        Rect::from_min_max(
            Pos2::new(body.left() + half * 0.42, c.y + half * 0.12),
            Pos2::new(body.right() - half * 0.42, body.bottom() - half * 0.28),
        ),
        0.0,
        Stroke::new(1.0 * s, col),
        egui::StrokeKind::Inside,
    );
}

/// A small ✕ centered at `c` (arm radius `r`). Painted (no glyph dependency).
fn paint_cross(p: &egui::Painter, c: Pos2, r: f32, col: egui::Color32, s: f32) {
    let st = Stroke::new(1.3 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)], st);
    p.line_segment([Pos2::new(c.x - r, c.y + r), Pos2::new(c.x + r, c.y - r)], st);
}

/// A small ＋ centered at `c`.
fn paint_plus(p: &egui::Painter, c: Pos2, r: f32, col: egui::Color32, s: f32) {
    let st = Stroke::new(1.3 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], st);
    p.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], st);
}

/// A small check (✓) centered near `c`.
fn paint_check(p: &egui::Painter, c: Pos2, r: f32, col: egui::Color32, s: f32) {
    let st = Stroke::new(1.6 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x - r * 0.3, c.y + r * 0.8)], st);
    p.line_segment([Pos2::new(c.x - r * 0.3, c.y + r * 0.8), Pos2::new(c.x + r, c.y - r * 0.8)], st);
}

/// A small filled pencil (rename/edit) centered at `c`, pointing down-left.
fn paint_pencil(p: &egui::Painter, c: Pos2, s: f32, col: egui::Color32) {
    let u = 1.7 * s; // half-thickness of the shaft
    let r2 = std::f32::consts::FRAC_1_SQRT_2;
    let dir = Vec2::new(r2, -r2); // along the shaft (up-right)
    let perp = Vec2::new(r2, r2);
    let len = 5.5 * s;
    let tip = c - dir * len; // bottom-left point
    let top = c + dir * len * 0.85; // top-right end
    let b = tip + dir * (2.4 * s); // where the body meets the tip
    p.add(egui::Shape::convex_polygon(
        vec![top + perp * u, top - perp * u, b - perp * u, b + perp * u],
        col,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(vec![b + perp * u, b - perp * u, tip], col, Stroke::NONE));
}

pub(crate) fn draw(
    ui: &mut egui::Ui,
    params: &Arc<PetalParams>,
    setter: &ParamSetter,
    state: &mut EditorState,
) {
    // iPad: enlarge the whole palette (~+25%) so it's less cramped, and give the
    // tap targets (icons / scope chips / rows) an EXTRA bump (~×1.2 → ≈+50% overall)
    // since the small ok/✓/✕/disk icons were fiddly to hit. Boosting `s` scales the
    // box AND all the pixel-positioned internals coherently; `tap` grows only the
    // hit dimensions on top. Desktop is unchanged (both factors = 1.0).
    let s = ui_scale(ui) * if theme::touch() { 1.25 } else { 1.0 };
    let tap = if theme::touch() { 1.2 } else { 1.0 };
    let fsc = theme::font_scale();
    if state.preset_guard > 0 {
        state.preset_guard -= 1;
    }
    ensure(state);

    // ---- snapshot + filter (scope ∘ search), capped at 9 rows (design slice) ----
    let lib: Vec<PaletteRow> = state
        .library
        .as_ref()
        .map(|l| {
            l.presets
                .iter()
                .map(|p| PaletteRow {
                    name: p.name.clone(),
                    fav: p.favorite,
                    build: p.build.clone(),
                    tags: p.tags.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let q = state.preset_search.trim().to_lowercase();
    let q_disp = state.preset_search.trim().to_string(); // original case, for the empty-state hint
    let scope = state.palette_scope;
    let mut rows: Vec<PaletteRow> = lib
        .into_iter()
        .filter(|r| {
            let scope_ok = match scope {
                1 => r.fav,
                2 => state.recent.iter().any(|n| n == &r.name),
                _ => true,
            };
            let q_ok = q.is_empty()
                || r.name.to_lowercase().contains(&q)
                || r.tags.iter().any(|t| t.to_lowercase().contains(&q));
            scope_ok && q_ok
        })
        .collect();
    if scope == 2 {
        rows.sort_by_key(|r| state.recent.iter().position(|n| n == &r.name).unwrap_or(usize::MAX));
    }
    rows.truncate(9);
    let n_rows = rows.len();

    // ---- mode + keyboard (read BEFORE the TextEdit runs, so events aren't consumed) ----
    // A sub-editor (Save-As/Rename name field, or the add-tag field) owns the
    // keyboard when open: arrows move its caret, Enter commits it, Esc cancels it.
    let name_mode = state.palette_name_open;
    let tag_mode = state.palette_tag_for.is_some();
    let text_mode = name_mode || tag_mode;
    let (k_up, k_down, k_enter, k_esc, k_save, k_init) = ui.input(|i| {
        let cmd = i.modifiers.command;
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
            cmd && i.key_pressed(egui::Key::S),
            cmd && i.key_pressed(egui::Key::Backspace),
        )
    });
    // ⌘⌫ = INIT, but ONLY in search mode — while a name/tag field is focused it
    // must stay an ordinary delete-line, never reset the whole patch. Consume it
    // in search mode so it can't ALSO wipe the focused query.
    if k_init && !text_mode {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Backspace));
    }
    // hover only wins when the pointer actually moved this frame (else a parked
    // cursor freezes keyboard nav by snapping the selection back every frame).
    let pointer_moved = ui.input(|i| i.pointer.is_moving());
    // arrow-key row nav is search-mode only (else arrows move the text caret).
    if k_up && !text_mode {
        state.palette_sel = state.palette_sel.saturating_sub(1);
    }
    if k_down && !text_mode && n_rows > 0 {
        state.palette_sel = (state.palette_sel + 1).min(n_rows - 1);
    }
    let sel = state.palette_sel.min(n_rows.saturating_sub(1));
    state.palette_sel = sel;

    // ---- geometry (logical px from the HTML, * ui_scale) ----
    let screen = ui.ctx().screen_rect();
    let w = 560.0 * s;
    let input_h = 50.0 * s;
    let scope_h = 38.0 * s;
    let footer_h = 36.0 * s;
    let row_h = 34.0 * s * tap; // taller = easier to tap a preset
    let list_pad = 6.0 * s;
    let list_h = if n_rows == 0 { 44.0 * s } else { list_pad * 2.0 + n_rows as f32 * row_h };
    let panel_h = input_h + scope_h + list_h + footer_h;
    let left = (screen.center().x - w * 0.5).round();
    // drop from the body top captured during draw (below titlebar + mode bar) (falls back to a
    // fixed offset on the very first frame before it's been set). Uses UNBOOSTED
    // scale for the fallback so the enlarged palette doesn't push it off-anchor.
    let top = if state.palette_top > 1.0 {
        state.palette_top.round()
    } else {
        (screen.top() + 46.0 * ui_scale(ui)).round()
    };
    let panel = Rect::from_min_size(Pos2::new(left, top), Vec2::new(w, panel_h));
    // Same language as the settings menu: radius 16, one flat panel, hairlines.
    let rad = g::R_CARD * s;
    let bw = tk::stroke_outline() * s;
    let ui_font = |px: f32| egui::FontId::proportional(px * s * fsc);

    // deferred actions (applied after the borrow-y draw closure)
    let mut do_load: Option<String> = None;
    let mut do_fav: Option<String> = None;
    let mut new_scope: Option<u8> = None;
    let mut hover_sel: Option<usize> = None;
    let mut do_close = false;
    let mut do_init_click = false;
    let mut te_changed = false;
    let mut open_save = false; // open the Save-As name editor (fresh, empty)
    let mut open_rename: Option<String> = None; // open the Rename editor for a preset
    let mut name_commit = false; // commit the name editor (save or rename)
    let mut name_cancel = false; // cancel the name editor → back to search
    let mut del_arm: Option<String> = None; // first ✕ click → arm the confirm
    let mut do_delete: Option<String> = None; // armed ✕ click → delete
    let mut tag_remove: Option<(String, String)> = None; // (preset, tag)
    let mut open_tag_add: Option<String> = None; // open the inline add-tag field
    let mut tag_commit = false; // commit the add-tag field
    let mut tag_cancel = false; // cancel the add-tag field

    egui::Area::new(egui::Id::new("afterecho_cmd_palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ui.ctx(), |ui| {
            let backdrop = ui.allocate_rect(screen, Sense::click());
            // a click-blocker over the panel chrome: only a true OUTSIDE (scrim)
            // click should dismiss. Allocated after the backdrop so it's topmost
            // over the panel; rows/chips/stars/field (allocated later) still win.
            let block = ui.allocate_rect(panel, Sense::click());
            ui.painter().rect_filled(screen, 0.0, theme::scrim());
            // panel body (flat: no shadow)
            ui.painter().rect_filled(panel, rad, PANEL());

            // section rects
            let in_rect = Rect::from_min_size(panel.min, Vec2::new(w, input_h));
            let sc_rect = Rect::from_min_size(Pos2::new(panel.left(), in_rect.bottom()), Vec2::new(w, scope_h));
            let list_rect = Rect::from_min_size(Pos2::new(panel.left(), sc_rect.bottom()), Vec2::new(w, list_h));
            let ft_rect = Rect::from_min_size(Pos2::new(panel.left(), list_rect.bottom()), Vec2::new(w, footer_h));

            // dividers (hairlines between search · scope · list · footer)
            for y in [in_rect.bottom(), sc_rect.bottom(), ft_rect.top()] {
                ui.painter().hline(panel.x_range(), y, Stroke::new(bw, theme::line()));
            }

            // ===== input band: search filter, OR the Save-As/Rename name editor =====
            let pad = 18.0 * s;
            if state.palette_name_open {
                // ---- the dedicated name editor (Save-As / Rename split) ----
                let is_rename = state.palette_rename_from.is_some();
                let icon_c = Pos2::new(in_rect.left() + pad + 6.0 * s, in_rect.center().y);
                if is_rename {
                    paint_pencil(ui.painter(), icon_c, s, INK());
                } else {
                    paint_disk(ui.painter(), icon_c, 6.5 * s, INK(), s);
                }
                let lab = if is_rename { "Rename" } else { "Save as" };
                let lg = ui.painter().layout_no_wrap(lab.to_string(), ui_font(12.0), INK());
                let lab_x = icon_c.x + 12.0 * s;
                let lab_w = lg.size().x;
                ui.painter().galley(Pos2::new(lab_x, in_rect.center().y - lg.size().y * 0.5), lg, INK());
                // commit (✓) + cancel (✕) on the right
                let ok_c = Pos2::new(in_rect.right() - pad - 6.0 * s, in_rect.center().y);
                let ok_resp = ui
                    .interact(Rect::from_center_size(ok_c, Vec2::splat(22.0 * s * tap)), ui.id().with("pnameok"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint_check(ui.painter(), ok_c, 6.0 * s, if ok_resp.hovered() { INK() } else { SOFT() }, s);
                if ok_resp.clicked() {
                    name_commit = true;
                }
                let x_c = Pos2::new(ok_c.x - 26.0 * s * tap, in_rect.center().y); // keep ✓/✕ hits from overlapping
                let x_resp = ui
                    .interact(Rect::from_center_size(x_c, Vec2::splat(22.0 * s * tap)), ui.id().with("pnamex"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint_cross(ui.painter(), x_c, 5.0 * s, if x_resp.hovered() { INK() } else { SOFT() }, s);
                if x_resp.clicked() {
                    name_cancel = true;
                }
                let te_left = lab_x + lab_w + 12.0 * s;
                let te_right = x_c.x - 18.0 * s;
                let te_h = 22.0 * s * fsc; // scale with the font nudge so the taller text still centers (was clipping high)
                let te_rect = Rect::from_min_max(
                    Pos2::new(te_left, in_rect.center().y - te_h * 0.5),
                    Pos2::new(te_right, in_rect.center().y + te_h * 0.5),
                );
                let hint = if is_rename { "Rename to…" } else { "Name this preset…" };
                let te_resp = ui
                    .scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(te_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.palette_name_buf)
                                    .frame(egui::Frame::NONE)
                                    .hint_text(hint)
                                    .font(ui_font(15.0))
                                    .text_color(INK())
                                    .desired_width(te_rect.width()),
                            )
                        },
                    )
                    .inner;
                if state.palette_name_focus {
                    te_resp.request_focus();
                    state.palette_name_focus = false;
                }
            } else {
                // ---- search filter (pure: it no longer doubles as the save name) ----
                let icon_r = 5.0 * s;
                let icon_c = Pos2::new(in_rect.left() + pad + icon_r, in_rect.center().y);
                ui.painter().circle_stroke(icon_c, icon_r, Stroke::new(1.3 * s, INK2()));
                ui.painter().line_segment(
                    [
                        icon_c + Vec2::splat(icon_r * 0.7),
                        icon_c + Vec2::splat(icon_r * 0.7 + 4.5 * s),
                    ],
                    Stroke::new(1.3 * s, INK2()),
                );
                // count
                ui.painter().text(
                    Pos2::new(in_rect.right() - pad, in_rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{n_rows}"),
                    ui_font(11.0),
                    SOFT(),
                );
                // save icon (disk) — OPENS the Save-As name editor (the host usually
                // eats ⌘S, so a mouse target is the reliable path).
                let disk_c = Pos2::new(in_rect.right() - pad - 26.0 * s, in_rect.center().y);
                let disk_resp = ui
                    .interact(
                        Rect::from_center_size(disk_c, Vec2::splat(22.0 * s * tap)),
                        ui.id().with("psaveicon"),
                        Sense::click(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint_disk(ui.painter(), disk_c, 6.5 * s, if disk_resp.hovered() { INK() } else { SOFT() }, s);
                if disk_resp.clicked() {
                    open_save = true;
                }
                // search field (leave room for the disk + count on the right)
                let te_left = icon_c.x + icon_r + 12.0 * s;
                let te_right = in_rect.right() - pad - 46.0 * s;
                let te_h = 22.0 * s * fsc; // scale with the font nudge so the taller text still centers (was clipping high)
                let te_rect = Rect::from_min_max(
                    Pos2::new(te_left, in_rect.center().y - te_h * 0.5),
                    Pos2::new(te_right, in_rect.center().y + te_h * 0.5),
                );
                let te_resp = ui
                    .scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(te_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.preset_search)
                                    .frame(egui::Frame::NONE)
                                    .hint_text("Search presets…")
                                    .font(ui_font(15.0))
                                    .text_color(INK())
                                    .desired_width(te_rect.width()),
                            )
                        },
                    )
                    .inner;
                if state.palette_focus {
                    // Desktop: auto-focus the search box so you can type immediately.
                    // TOUCH: do NOT — auto-focusing forces the iOS soft keyboard up the
                    // instant the palette opens (and the fork re-asserts first-responder
                    // every frame, so it fights being dismissed). On iPad you tap the
                    // field yourself when you actually want to type.
                    if !theme::touch() {
                        te_resp.request_focus();
                    }
                    state.palette_focus = false;
                }
                te_changed = te_resp.changed();
            }

            // ===== scope: the page's own selector (laid out, the pick underlined) =====
            let seg_rect = Rect::from_min_size(
                Pos2::new(sc_rect.left() + 16.0 * s, sc_rect.center().y - 13.0 * s * tap),
                Vec2::new(270.0 * s, 26.0 * s * tap),
            );
            if let Some(i) = seg(ui, seg_rect, &["All", "Favorites", "Recent"], scope as usize, "pscope", s, 12.0, true) {
                new_scope = Some(i as u8);
            }

            // ===== results =====
            if n_rows == 0 {
                let msg = if q_disp.is_empty() {
                    "No presets yet — \u{2318}S saves the current sound".to_string()
                } else {
                    // truncate (char-safe) so a long query can't overflow the panel
                    let q_short = if q_disp.chars().count() > 20 {
                        format!("{}\u{2026}", q_disp.chars().take(19).collect::<String>())
                    } else {
                        q_disp.clone()
                    };
                    format!("Nothing matches \u{201C}{q_short}\u{201D} — \u{2318}S saves it")
                };
                ui.painter().text(
                    list_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    msg,
                    ui_font(12.0),
                    SOFT(),
                );
            } else {
                for (k, r) in rows.iter().enumerate() {
                    let ry = list_rect.top() + list_pad + k as f32 * row_h;
                    let rrect = Rect::from_min_size(Pos2::new(panel.left(), ry), Vec2::new(w, row_h));
                    let on = k == sel;
                    let resp = ui.interact(rrect, ui.id().with(("prow", k)), Sense::click());
                    if resp.hovered() {
                        hover_sel = Some(k);
                    }
                    if on {
                        ui.painter().rect_filled(rrect.shrink2(Vec2::new(8.0 * s, 1.5 * s)), g::R_CTL * s, theme::with_alpha(INK(), 20));
                    }
                    // favorite star (its own click target → toggle)
                    let star_c = Pos2::new(rrect.left() + 16.0 * s + 6.0 * s, rrect.center().y);
                    let star_r = 6.0 * s;
                    paint_star(ui.painter(), star_c, star_r, r.fav, if r.fav { INK() } else { FAINT() }, s);
                    let sresp = ui.interact(
                        Rect::from_center_size(star_c, Vec2::splat(20.0 * s)),
                        ui.id().with(("pstar", k)),
                        Sense::click(),
                    );
                    if sresp.clicked() {
                        do_fav = Some(r.name.clone());
                    }
                    // right cluster FIRST (actions on the selected row, else read-only
                    // tags) so we can budget the name width and never overlap it. While
                    // the name editor is open the row shows read-only (no stray actions).
                    let mut rx = rrect.right() - 16.0 * s;
                    if on && !name_mode {
                        // delete ✕ (rightmost) — TWO-click confirm guards a destructive op
                        let armed = state.palette_del_confirm.as_deref() == Some(r.name.as_str());
                        let danger = egui::Color32::from_rgb(214, 69, 69);
                        let del_c = Pos2::new(rx - 6.0 * s, rrect.center().y);
                        let del_resp = ui
                            .interact(Rect::from_center_size(del_c, Vec2::splat(20.0 * s)), ui.id().with(("pdel", k)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if armed {
                            ui.painter().circle_filled(del_c, 9.0 * s, theme::with_alpha(danger, 38));
                        }
                        let del_col = if armed {
                            danger
                        } else if del_resp.hovered() {
                            INK()
                        } else {
                            FAINT()
                        };
                        paint_cross(ui.painter(), del_c, 5.0 * s, del_col, s);
                        if del_resp.clicked() {
                            if armed {
                                do_delete = Some(r.name.clone());
                            } else {
                                del_arm = Some(r.name.clone());
                            }
                        }
                        rx -= 24.0 * s;
                        // rename ✎
                        let ren_c = Pos2::new(rx - 6.0 * s, rrect.center().y);
                        let ren_resp = ui
                            .interact(Rect::from_center_size(ren_c, Vec2::splat(20.0 * s)), ui.id().with(("pren", k)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        paint_pencil(ui.painter(), ren_c, s, if ren_resp.hovered() { INK() } else { FAINT() });
                        if ren_resp.clicked() {
                            open_rename = Some(r.name.clone());
                        }
                        rx -= 22.0 * s;
                        // add-tag: an inline field when armed for this row, else a ＋ chip
                        if state.palette_tag_for.as_deref() == Some(r.name.as_str()) {
                            let fw = 96.0 * s;
                            let fr = Rect::from_min_max(
                                Pos2::new(rx - fw, rrect.center().y - 11.0 * s),
                                Pos2::new(rx, rrect.center().y + 11.0 * s),
                            );
                            ui.painter().rect_filled(fr, g::R_CTL * s, INSET());
                            ui.painter().rect_stroke(fr, g::R_CTL * s, Stroke::new(bw, INK()), egui::StrokeKind::Inside);
                            let resp = ui
                                .scope_builder(
                                    egui::UiBuilder::new()
                                        .max_rect(fr.shrink2(Vec2::new(6.0 * s, 0.0)))
                                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                    |ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut state.palette_tag_buf)
                                                .frame(egui::Frame::NONE)
                                                .hint_text("tag")
                                                .font(ui_font(11.0))
                                                .text_color(INK())
                                                .desired_width(fw - 12.0 * s),
                                        )
                                    },
                                )
                                .inner;
                            if state.palette_tag_focus {
                                resp.request_focus();
                                state.palette_tag_focus = false;
                            }
                            rx -= fw + 6.0 * s;
                        } else {
                            let plus_c = Pos2::new(rx - 8.0 * s, rrect.center().y);
                            let prect = Rect::from_center_size(plus_c, Vec2::splat(16.0 * s));
                            let plus_resp = ui
                                .interact(prect, ui.id().with(("ptagadd", k)), Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            ui.painter().rect_stroke(prect, g::R_CTL * s, Stroke::new(bw, theme::line2()), egui::StrokeKind::Inside);
                            paint_plus(ui.painter(), plus_c, 3.5 * s, if plus_resp.hovered() { INK() } else { SOFT() }, s);
                            if plus_resp.clicked() {
                                open_tag_add = Some(r.name.clone());
                            }
                            rx -= 22.0 * s;
                        }
                        // editable tag chips (right→left), each with an ✕ to remove. Stop
                        // before crossing the name's min zone so chips never overlap it.
                        let name_zone = star_c.x + star_r + 12.0 * s + 60.0 * s;
                        for t in r.tags.iter().rev() {
                            let tg = ui.painter().layout_no_wrap(t.clone(), ui_font(11.0), INK2());
                            let chip_w = tg.size().x + 20.0 * s;
                            if rx - chip_w < name_zone {
                                break;
                            }
                            rx -= chip_w;
                            let chip =
                                Rect::from_min_size(Pos2::new(rx, rrect.center().y - 9.0 * s), Vec2::new(chip_w, 18.0 * s));
                            ui.painter().rect_filled(chip, g::R_CTL * s, theme::sel_bg());
                            ui.painter().galley(
                                Pos2::new(chip.left() + 6.0 * s, rrect.center().y - tg.size().y * 0.5),
                                tg,
                                SOFT(),
                            );
                            let xc = Pos2::new(chip.right() - 6.0 * s, rrect.center().y);
                            let xr = ui.interact(
                                Rect::from_center_size(xc, Vec2::splat(16.0 * s)),
                                ui.id().with(("ptagx", k, t.as_str())),
                                Sense::click(),
                            );
                            paint_cross(ui.painter(), xc, 3.0 * s, if xr.hovered() { INK() } else { FAINT() }, s);
                            if xr.clicked() {
                                tag_remove = Some((r.name.clone(), t.clone()));
                            }
                            rx -= 6.0 * s;
                        }
                    } else {
                        // non-selected row: read-only tags right→left (unchanged)
                        for t in r.tags.iter().rev() {
                            let tg = ui.painter().layout_no_wrap(t.clone(), ui_font(11.0), INK2());
                            rx -= tg.size().x;
                            ui.painter()
                                .galley(Pos2::new(rx, rrect.center().y - tg.size().y * 0.5), tg, SOFT());
                            rx -= 6.0 * s;
                        }
                    }
                    let cluster_left = rx;
                    // name — truncated with an ellipsis to its budget (the design uses
                    // CSS text-overflow:ellipsis; egui needs an explicit LayoutJob).
                    let name_x = star_c.x + star_r + 12.0 * s;
                    let name_col = if on { INK() } else { INK2() };
                    let build_budget = if r.build.is_empty() { 0.0 } else { 56.0 * s };
                    let name_max = (cluster_left - name_x - build_budget).max(8.0 * s);
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        r.name.clone(),
                        ui_font(14.0),
                        name_col,
                    );
                    job.wrap.max_width = name_max;
                    job.wrap.max_rows = 1;
                    job.wrap.overflow_character = Some('\u{2026}');
                    let ng = ui.painter().layout_job(job);
                    let nw = ng.size().x;
                    ui.painter()
                        .galley(Pos2::new(name_x, rrect.center().y - ng.size().y * 0.5), ng, name_col);
                    // build/source tag (mono), only if it still fits before the cluster
                    if !r.build.is_empty() {
                        let tag_x = name_x + nw + 12.0 * s;
                        let bg = ui.painter().layout_no_wrap(r.build.clone(), ui_font(11.0), SOFT());
                        if tag_x + bg.size().x <= cluster_left {
                            ui.painter().galley(
                                Pos2::new(tag_x, rrect.center().y - bg.size().y * 0.5),
                                bg,
                                SOFT(),
                            );
                        }
                    }
                    if resp.clicked() {
                        do_load = Some(r.name.clone());
                    }
                }
            }

            // ===== footer legend — the keycaps are CLICKABLE (hosts often eat the
            // real shortcuts, so every action is reachable by mouse too) =====
            let cap_h = 16.0 * s;
            // draw one keycap. kind: 0 = up/down · 1 = return · 2 = text. `act`
            // makes it an interactive button (salt = unique id). Returns (w, clicked).
            let draw_cap =
                |ui: &egui::Ui, x: f32, kind: u8, txt: &str, salt: &str, act: bool| -> (f32, bool) {
                    let cy = ft_rect.center().y;
                    let pad = 6.0 * s;
                    let cw = (match kind {
                        0 | 1 => 16.0 * s,
                        _ => ui
                            .painter()
                            .layout_no_wrap(txt.to_string(), egui::FontId::monospace(9.0 * s * fsc), INK2())
                            .size()
                            .x
                            + pad * 2.0,
                    })
                    .max(16.0 * s);
                    let cr = Rect::from_min_size(Pos2::new(x, cy - cap_h * 0.5), Vec2::new(cw, cap_h));
                    let (hot, clicked) = if act {
                        let resp = ui
                            .interact(cr, ui.id().with(("pcap", salt)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        (resp.hovered(), resp.clicked())
                    } else {
                        (false, false)
                    };
                    let fg = if hot { INK() } else { INK2() };
                    let border = if hot { theme::with_alpha(INK(), 90) } else { theme::line2() };
                    ui.painter().rect_filled(cr, g::R_CTL * s, INSET());
                    ui.painter().rect_stroke(cr, g::R_CTL * s, Stroke::new(bw, border), egui::StrokeKind::Inside);
                    match kind {
                        0 => {
                            let st = Stroke::new(1.2 * s, fg);
                            let c = cr.center();
                            let dx = 3.0 * s;
                            ui.painter().line_segment([Pos2::new(c.x - 5.0 * s, c.y + 1.0 * s), Pos2::new(c.x - 5.0 * s + dx, c.y - 2.0 * s)], st);
                            ui.painter().line_segment([Pos2::new(c.x - 5.0 * s + dx, c.y - 2.0 * s), Pos2::new(c.x - 5.0 * s + 2.0 * dx, c.y + 1.0 * s)], st);
                            ui.painter().line_segment([Pos2::new(c.x + 5.0 * s - 2.0 * dx, c.y - 1.0 * s), Pos2::new(c.x + 5.0 * s - dx, c.y + 2.0 * s)], st);
                            ui.painter().line_segment([Pos2::new(c.x + 5.0 * s - dx, c.y + 2.0 * s), Pos2::new(c.x + 5.0 * s, c.y - 1.0 * s)], st);
                        }
                        1 => {
                            paint_return(ui.painter(), Pos2::new(cr.center().x + 4.5 * s, cr.center().y), s, fg);
                        }
                        _ => {
                            ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, txt, egui::FontId::monospace(9.0 * s * fsc), fg);
                        }
                    }
                    (cw, clicked)
                };
            let mut fx = ft_rect.left() + 16.0 * s;
            if name_mode {
                // while naming: ⏎ commit · esc cancel
                let commit_label = if state.palette_rename_from.is_some() { "rename" } else { "save" };
                let (cw, clicked) = draw_cap(ui, fx, 1, "", "namecommit", true);
                if clicked {
                    name_commit = true;
                }
                fx += cw + 6.0 * s;
                let lg = ui.painter().layout_no_wrap(commit_label.to_string(), ui_font(11.0), SOFT());
                ui.painter().galley(Pos2::new(fx, ft_rect.center().y - lg.size().y * 0.5), lg, SOFT());
            } else {
                // (kind, cap-text, label, salt, interactive)
                let legend: [(u8, &str, &str, &str, bool); 4] = [
                    (0, "", "navigate", "nav", false),
                    (1, "", "load", "load", true),
                    (2, "\u{2318}S", "save", "save", true),
                    (2, "\u{2318}\u{232B}", "init", "init", true),
                ];
                for (kind, txt, label, salt, act) in legend {
                    let (cw, clicked) = draw_cap(ui, fx, kind, txt, salt, act);
                    if clicked {
                        match salt {
                            "load" => {
                                if let Some(r) = rows.get(sel) {
                                    do_load = Some(r.name.clone());
                                }
                            }
                            "save" => open_save = true,
                            "init" => do_init_click = true,
                            _ => {}
                        }
                    }
                    fx += cw + 6.0 * s;
                    let lg = ui.painter().layout_no_wrap(label.to_string(), ui_font(11.0), SOFT());
                    let lw = lg.size().x;
                    ui.painter().galley(Pos2::new(fx, ft_rect.center().y - lg.size().y * 0.5), lg, SOFT());
                    fx += lw + 16.0 * s;
                }
            }
            // esc · cancel (in a sub-mode) or close (right-aligned, clickable)
            let esc_label = if text_mode { "cancel" } else { "close" };
            let close_lab = ui.painter().layout_no_wrap(esc_label.to_string(), ui_font(11.0), SOFT());
            let clx = ft_rect.right() - 16.0 * s - close_lab.size().x;
            ui.painter().galley(Pos2::new(clx, ft_rect.center().y - close_lab.size().y * 0.5), close_lab, SOFT());
            // extra gap so the esc keycap doesn't crowd the "close" label (the cap's
            // real width is ~27px, so the old 6+24 offset left it nearly touching).
            let (_, esc_clicked) = draw_cap(ui, clx - 14.0 * s - 28.0 * s, 2, "esc", "esc", true);
            if esc_clicked {
                if tag_mode {
                    tag_cancel = true;
                } else if name_mode {
                    name_cancel = true;
                } else {
                    do_close = true;
                }
            }

            // border LAST so band backgrounds don't cover it
            ui.painter().rect_stroke(panel, rad, Stroke::new(bw, theme::line2()), egui::StrokeKind::Inside);

            if state.preset_guard == 0 && backdrop.clicked() && !block.clicked() {
                do_close = true;
            }
        });

    // ---- apply deferred actions ----
    // fold the keyboard into sub-mode intents (modes are mutually exclusive:
    // name-editor XOR add-tag XOR plain search).
    if k_enter && name_mode {
        name_commit = true;
    }
    if k_enter && tag_mode {
        tag_commit = true;
    }
    if k_esc && name_mode {
        name_cancel = true;
    }
    if k_esc && tag_mode {
        tag_cancel = true;
    }
    if k_save && !text_mode {
        open_save = true;
    }

    if let Some(id) = new_scope {
        state.palette_scope = id;
        state.palette_sel = 0;
        state.palette_focus = true;
        state.palette_del_confirm = None;
        state.palette_tag_for = None;
        state.palette_tag_buf.clear(); // don't leave a half-typed tag orphaned
    }
    if te_changed {
        state.palette_sel = 0;
    } else if let Some(h) = hover_sel {
        // hover wins only on genuine movement, never on a keyboard-nav frame
        if pointer_moved && !k_up && !k_down {
            state.palette_sel = h;
        }
    }
    if let Some(nm) = do_fav {
        if let Some(l) = state.library.as_mut() {
            l.toggle_favorite(&nm);
        }
        // in FAVORITES scope the row just left the list — reset to the top so the
        // highlight doesn't silently land on an unrelated preset.
        if state.palette_scope == 1 {
            state.palette_sel = 0;
        }
    }

    // --- tag editing (inline chips on the selected row) ---
    if let Some(nm) = open_tag_add {
        state.palette_tag_for = Some(nm);
        state.palette_tag_buf.clear();
        state.palette_tag_focus = true;
        state.palette_name_open = false; // exclusive with the name editor
        state.palette_del_confirm = None;
    }
    if tag_commit {
        if let Some(nm) = state.palette_tag_for.clone() {
            let t = state.palette_tag_buf.trim().to_string();
            if !t.is_empty() {
                if let Some(l) = state.library.as_mut() {
                    l.add_tag(&nm, &t);
                }
            }
            // keep the field open + clear so several tags can be added in a row
            state.palette_tag_buf.clear();
            state.palette_tag_focus = true;
        }
    }
    if tag_cancel {
        state.palette_tag_for = None;
        state.palette_tag_buf.clear();
        state.palette_focus = true;
    }
    if let Some((nm, t)) = tag_remove {
        if let Some(l) = state.library.as_mut() {
            l.remove_tag(&nm, &t);
        }
    }

    // --- the name editor (Save-As / Rename) ---
    if let Some(nm) = open_rename {
        state.palette_name_open = true;
        state.palette_rename_from = Some(nm.clone());
        state.palette_name_buf = nm;
        state.palette_name_focus = true;
        state.palette_tag_for = None;
        state.palette_del_confirm = None;
    } else if open_save && !state.palette_name_open {
        state.palette_name_open = true;
        state.palette_rename_from = None;
        // Seed the name from what the user TYPED in the filter (the "⌘S to save"
        // hint promises exactly this) — so "type name → save" saves that name.
        // Fall back to the loaded preset's name (overwrite), else empty.
        let typed = state.preset_search.trim();
        state.palette_name_buf = if !typed.is_empty() {
            typed.to_string()
        } else {
            state.current_preset.clone().unwrap_or_default()
        };
        state.palette_name_focus = true;
        state.palette_tag_for = None;
    }
    if name_commit {
        let typed = state.palette_name_buf.trim().to_string();
        if typed.is_empty() {
            state.palette_name_focus = true; // never silently close on an empty name
        } else if let Some(old) = state.palette_rename_from.clone() {
            // RENAME
            let ok = state.library.as_mut().map(|l| l.rename(&old, &typed)).unwrap_or(false);
            if ok {
                if state.current_preset.as_deref() == Some(old.as_str()) {
                    state.current_preset = Some(typed.clone());
                }
                for n in state.recent.iter_mut() {
                    if *n == old {
                        *n = typed.clone();
                    }
                }
                state.palette_name_open = false;
                state.palette_rename_from = None;
                state.palette_name_buf.clear();
                state.palette_focus = true;
            } else {
                state.palette_name_focus = true; // collision / write failure → keep editing
            }
        } else if preset_save(state, params, &typed) {
            // SAVE-AS (create or overwrite)
            state.palette_name_open = false;
            state.palette_name_buf.clear();
            state.palette_focus = true;
        } else {
            state.palette_name_focus = true;
        }
    }
    if name_cancel {
        state.palette_name_open = false;
        state.palette_rename_from = None;
        state.palette_name_buf.clear();
        state.palette_focus = true;
    }

    // --- delete (two-click confirm) ---
    if let Some(nm) = del_arm {
        state.palette_del_confirm = Some(nm);
    }
    if let Some(nm) = do_delete {
        if let Some(l) = state.library.as_mut() {
            l.delete(&nm);
        }
        if state.current_preset.as_deref() == Some(nm.as_str()) {
            state.current_preset = None;
            state.loaded_snapshot = None;
        }
        state.recent.retain(|n| n != &nm);
        state.palette_del_confirm = None;
        state.palette_sel = 0;
    }
    // disarm a pending delete if the selection moved off that preset
    {
        let sel_now = state.palette_sel.min(n_rows.saturating_sub(1));
        let sel_name = rows.get(sel_now).map(|r| r.name.as_str());
        if state.palette_del_confirm.is_some() && state.palette_del_confirm.as_deref() != sel_name {
            state.palette_del_confirm = None;
        }
    }

    // --- init / load / close ---
    if (k_init && !text_mode) || do_init_click {
        preset_init(state, params, setter);
    }
    let enter_load = k_enter && !text_mode && rows.get(sel).is_some();
    if enter_load {
        if let Some(r) = rows.get(sel) {
            do_load = Some(r.name.clone());
        }
    }
    if let Some(nm) = do_load {
        // a row click while the name editor is open shouldn't hijack the save
        if !name_mode {
            preset_load(state, params, setter, &nm);
            state.preset_open = false;
            state.palette_name_open = false;
            state.palette_rename_from = None;
            state.palette_tag_for = None;
            state.palette_del_confirm = None;
        }
    }
    if do_close || (k_esc && !text_mode) {
        state.preset_open = false;
        state.palette_name_open = false;
        state.palette_rename_from = None;
        state.palette_tag_for = None;
        state.palette_del_confirm = None;
    }
}
