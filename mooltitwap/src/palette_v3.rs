//! Spotlight preset command palette for the v3 editor — ported from y-flanger
//! (itself ported 1:1 from cumulus). Search + scope chips (ALL/FAVORITES/RECENT)
//! + rows with favorite/tags/rename/delete + save-as, keyboard-navigable, every
//! action also reachable by mouse (hosts eat ⌘S/⌘⌫).
//!
//! The preset store is `presets::Library` (BTreeMap snapshots). This module owns
//! only the popup UI + the load/save/step/init glue on `EditorState`.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use epoche_ui::theme::{self, FAINT, G, INK, INK2, INSET, SOFT};
use nice_plug::prelude::*;
use std::sync::Arc;

use crate::gui::EditorState;
use crate::params::PetalParams;
use crate::presets::{self, Library, OldEditors};


pub(crate) fn ensure(st: &mut EditorState) {
    if st.library.is_none() {
        st.library = Some(Library::load());
    }
}

// ─── load / step / save / init ─────────────────────────────────────────────
pub fn preset_load(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter, name: &str) {
    let data = st.library.as_ref().and_then(|l| l.get(name)).map(|p| (p.values.clone(), p.values_format));
    if let Some((values, fmt)) = data {
        presets::apply(params, setter, &values, fmt, &mut presets::Sent::default());
        st.current_preset = Some(name.to_string());
        st.recent.retain(|n| n != name);
        st.recent.insert(0, name.to_string());
        st.recent.truncate(12);
        // snapshot the values we APPLIED (raw sets are async — re-capturing now
        // would read stale live values and show a false dirty `*`).
        st.loaded_snapshot = Some(values);
    }
}

pub fn preset_step(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter, dir: i32) {
    ensure(st);
    let names: Vec<String> = st.library.as_ref().map(|l| l.names()).unwrap_or_default();
    if names.is_empty() {
        return;
    }
    let cur = st.current_preset.as_ref().and_then(|n| names.iter().position(|x| x == n));
    let idx = match cur {
        Some(i) => (i as i64 + dir as i64).rem_euclid(names.len() as i64) as usize,
        None => 0,
    };
    let name = names[idx].clone();
    preset_load(st, params, setter, &name);
}

pub(crate) fn preset_save(st: &mut EditorState, params: &Arc<PetalParams>, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    ensure(st);
    let ok = st.library.as_mut().map(|l| l.save_named(name, presets::capture(params))).unwrap_or(false);
    if ok {
        st.current_preset = Some(name.to_string());
        st.loaded_snapshot = Some(presets::capture(params));
        st.recent.retain(|n| n != name);
        st.recent.insert(0, name.to_string());
        st.recent.truncate(12);
    }
    ok
}

/// INIT — reset every param to default, driven like knob gestures so the host
/// records it, and drop the loaded-preset link for a clean dirty baseline.
pub(crate) fn preset_init(st: &mut EditorState, params: &Arc<PetalParams>, setter: &ParamSetter) {
    for (_id, ptr, _g) in params.param_map() {
        // SAFETY: ptr belongs to params; begin/set/end like a knob drag.
        unsafe {
            let d = ptr.default_normalized_value();
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter.raw_context.raw_set_parameter_normalized(ptr, d);
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
    st.current_preset = None;
    st.loaded_snapshot = None;
}

/// The dirty `*`: a preset is loaded and the live params have since diverged.
pub fn is_dirty(st: &EditorState, params: &Arc<PetalParams>) -> bool {
    st.current_preset.is_some()
        && st.loaded_snapshot.as_ref().map(|snap| !presets::matches(params, snap)).unwrap_or(false)
}

struct Row {
    name: String,
    fav: bool,
    build: String,
    tags: Vec<String>,
}

// ─── the popup ─────────────────────────────────────────────────────────────
#[allow(clippy::too_many_lines)]
pub fn draw(ui: &mut egui::Ui, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    // iPad: enlarge the whole palette (+25%) and give tap targets an extra bump.
    let s = theme::ui_scale(ui) * if theme::touch() { 1.25 } else { 1.0 };
    let tap = if theme::touch() { 1.2 } else { 1.0 };
    let fsc = theme::font_scale();
    if st.preset_guard > 0 {
        st.preset_guard -= 1;
    }
    ensure(st);

    // snapshot + filter (scope ∘ search), capped at 9 rows
    let lib: Vec<Row> = st.library.as_ref().map(|l| {
        l.presets().map(|p| Row { name: p.name.clone(), fav: l.is_favourite(&p.id()), build: p.plugin_version.clone(), tags: p.tags.clone() }).collect()
    }).unwrap_or_default();
    let q = st.preset_search.trim().to_lowercase();
    let q_disp = st.preset_search.trim().to_string();
    let scope = st.palette_scope;
    let mut rows: Vec<Row> = lib.into_iter().filter(|r| {
        let scope_ok = match scope {
            1 => r.fav,
            2 => st.recent.iter().any(|n| n == &r.name),
            _ => true,
        };
        let q_ok = q.is_empty() || r.name.to_lowercase().contains(&q) || r.tags.iter().any(|t| t.to_lowercase().contains(&q));
        scope_ok && q_ok
    }).collect();
    if scope == 2 {
        rows.sort_by_key(|r| st.recent.iter().position(|n| n == &r.name).unwrap_or(usize::MAX));
    }
    rows.truncate(9);
    let n_rows = rows.len();

    // keyboard — read BEFORE the TextEdits run so events aren't consumed
    let name_mode = st.palette_name_open;
    let tag_mode = st.palette_tag_for.is_some();
    let text_mode = name_mode || tag_mode;
    let (k_up, k_down, k_enter, k_esc, k_save, k_init) = ui.input(|i| {
        let cmd = i.modifiers.command;
        (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown),
         i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape),
         cmd && i.key_pressed(egui::Key::S), cmd && i.key_pressed(egui::Key::Backspace))
    });
    if k_init && !text_mode {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Backspace));
    }
    let pointer_moved = ui.input(|i| i.pointer.is_moving());
    if k_up && !text_mode {
        st.palette_sel = st.palette_sel.saturating_sub(1);
    }
    if k_down && !text_mode && n_rows > 0 {
        st.palette_sel = (st.palette_sel + 1).min(n_rows - 1);
    }
    let sel = st.palette_sel.min(n_rows.saturating_sub(1));
    st.palette_sel = sel;

    // geometry
    let screen = ui.ctx().screen_rect();
    let w = 520.0 * s;
    let input_h = 46.0 * s;
    let scope_h = 36.0 * s;
    let footer_h = 34.0 * s;
    let row_h = 34.0 * s * tap;
    let list_pad = 6.0 * s;
    let list_h = if n_rows == 0 { 44.0 * s } else { list_pad * 2.0 + n_rows as f32 * row_h };
    let panel_h = input_h + scope_h + list_h + footer_h;
    let left = (screen.center().x - w * 0.5).round();
    let top = if st.palette_top > 1.0 { st.palette_top.round() } else { (screen.top() + 70.0 * theme::ui_scale(ui)).round() };
    let panel = Rect::from_min_size(Pos2::new(left, top), Vec2::new(w, panel_h));
    let r14 = 14.0 * s;
    let bw = theme::STROKE_HAIR * s;

    // deferred actions
    let mut do_load: Option<String> = None;
    let mut do_fav: Option<String> = None;
    let mut new_scope: Option<u8> = None;
    let mut hover_sel: Option<usize> = None;
    let mut do_close = false;
    let mut do_init_click = false;
    let mut te_changed = false;
    let mut open_save = false;
    let mut open_rename: Option<String> = None;
    let mut name_commit = false;
    let mut name_cancel = false;
    let mut del_arm: Option<String> = None;
    let mut do_delete: Option<String> = None;
    let mut tag_remove: Option<(String, String)> = None;
    let mut open_tag_add: Option<String> = None;
    let mut tag_commit = false;
    let mut tag_cancel = false;
    // singleline TextEdit surrenders focus on Return (and on tap-away) → this is
    // the reliable cross-platform "validate" signal; the soft keyboard's Return
    // on iPad does NOT map to a global egui::Key::Enter.
    let mut tag_lost = false;

    egui::Area::new(egui::Id::new("v3_cmd_palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ui.ctx(), |ui| {
            let backdrop = ui.allocate_rect(screen, Sense::click());
            let block = ui.allocate_rect(panel, Sense::click());
            ui.painter().rect_filled(screen, 0.0, theme::scrim());
            ui.painter().rect_filled(panel.translate(Vec2::new(0.0, 12.0 * s)).expand(2.0 * s), r14, Color32::from_black_alpha(120));
            ui.painter().rect_filled(panel, r14, theme::FRAME());

            let in_rect = Rect::from_min_size(panel.min, Vec2::new(w, input_h));
            let sc_rect = Rect::from_min_size(Pos2::new(panel.left(), in_rect.bottom()), Vec2::new(w, scope_h));
            let list_rect = Rect::from_min_size(Pos2::new(panel.left(), sc_rect.bottom()), Vec2::new(w, list_h));
            let ft_rect = Rect::from_min_size(Pos2::new(panel.left(), list_rect.bottom()), Vec2::new(w, footer_h));
            ui.painter().rect_filled(sc_rect, 0.0, theme::PANEL2());
            let br = egui::CornerRadius { nw: 0, ne: 0, sw: r14.round() as u8, se: r14.round() as u8 };
            ui.painter().rect_filled(ft_rect, br, theme::PANEL2());
            for y in [in_rect.bottom(), sc_rect.bottom(), ft_rect.top()] {
                ui.painter().hline(panel.x_range(), y, Stroke::new(bw, theme::line()));
            }

            // ===== input band =====
            let pad = 16.0 * s;
            if st.palette_name_open {
                let is_rename = st.palette_rename_from.is_some();
                let icon_c = Pos2::new(in_rect.left() + pad + 6.0 * s, in_rect.center().y);
                if is_rename { paint_pencil(ui.painter(), icon_c, s, G()); } else { paint_disk(ui.painter(), icon_c, 6.5 * s, G(), s); }
                let lab = if is_rename { "RENAME" } else { "SAVE AS" };
                let lg = ui.painter().layout_no_wrap(lab.to_string(), FontId::monospace(8.5 * s * fsc), G());
                let lab_x = icon_c.x + 12.0 * s;
                let lab_w = lg.size().x;
                ui.painter().galley(Pos2::new(lab_x, in_rect.center().y - lg.size().y * 0.5), lg, G());
                let ok_c = Pos2::new(in_rect.right() - pad - 6.0 * s, in_rect.center().y);
                let ok_resp = ui.interact(Rect::from_center_size(ok_c, Vec2::splat(22.0 * s * tap)), ui.id().with("pnameok"), Sense::click());
                paint_check(ui.painter(), ok_c, 6.0 * s, if ok_resp.hovered() { G() } else { SOFT() }, s);
                if ok_resp.clicked() { name_commit = true; }
                let x_c = Pos2::new(ok_c.x - 26.0 * s * tap, in_rect.center().y);
                let x_resp = ui.interact(Rect::from_center_size(x_c, Vec2::splat(22.0 * s * tap)), ui.id().with("pnamex"), Sense::click());
                paint_cross(ui.painter(), x_c, 5.0 * s, if x_resp.hovered() { INK() } else { SOFT() }, s);
                if x_resp.clicked() { name_cancel = true; }
                let te_left = lab_x + lab_w + 12.0 * s;
                let te_right = x_c.x - 18.0 * s;
                let te_h = 22.0 * s * fsc;
                let te_rect = Rect::from_min_max(Pos2::new(te_left, in_rect.center().y - te_h * 0.5), Pos2::new(te_right, in_rect.center().y + te_h * 0.5));
                let hint = if is_rename { "Rename to…" } else { "Name this preset…" };
                let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(te_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let te = cu.add(egui::TextEdit::singleline(&mut st.palette_name_buf).frame(egui::Frame::NONE).hint_text(hint).font(FontId::monospace(15.0 * s * fsc)).text_color(INK()).desired_width(te_rect.width()));
                if st.palette_name_focus { te.request_focus(); st.palette_name_focus = false; }
            } else {
                let icon_r = 5.0 * s;
                let icon_c = Pos2::new(in_rect.left() + pad + icon_r, in_rect.center().y);
                ui.painter().circle_stroke(icon_c, icon_r, Stroke::new(1.3 * s, G()));
                ui.painter().line_segment([icon_c + Vec2::splat(icon_r * 0.7), icon_c + Vec2::splat(icon_r * 0.7 + 4.5 * s)], Stroke::new(1.3 * s, G()));
                ui.painter().text(Pos2::new(in_rect.right() - pad, in_rect.center().y), Align2::RIGHT_CENTER, format!("{n_rows}"), FontId::monospace(9.0 * s * fsc), FAINT());
                let disk_c = Pos2::new(in_rect.right() - pad - 26.0 * s, in_rect.center().y);
                let disk_resp = ui.interact(Rect::from_center_size(disk_c, Vec2::splat(22.0 * s * tap)), ui.id().with("psaveicon"), Sense::click());
                paint_disk(ui.painter(), disk_c, 6.5 * s, if disk_resp.hovered() { G() } else { SOFT() }, s);
                if disk_resp.clicked() { open_save = true; }
                let te_left = icon_c.x + icon_r + 12.0 * s;
                let te_right = in_rect.right() - pad - 46.0 * s;
                let te_h = 22.0 * s * fsc;
                let te_rect = Rect::from_min_max(Pos2::new(te_left, in_rect.center().y - te_h * 0.5), Pos2::new(te_right, in_rect.center().y + te_h * 0.5));
                let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(te_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let te = cu.add(egui::TextEdit::singleline(&mut st.preset_search).frame(egui::Frame::NONE).hint_text("Jump to or filter presets…").font(FontId::monospace(15.0 * s * fsc)).text_color(INK()).desired_width(te_rect.width()));
                if st.palette_focus {
                    if !theme::touch() { te.request_focus(); }
                    st.palette_focus = false;
                }
                te_changed = te.changed();
            }

            // ===== scope chips =====
            let chips = [(0u8, "ALL"), (1u8, "FAVORITES"), (2u8, "RECENT")];
            let mut cx = sc_rect.left() + 16.0 * s;
            for (id, lab) in chips {
                let active = scope == id;
                let txt_col = if active { INK() } else { SOFT() };
                let g = ui.painter().layout_no_wrap(lab.to_string(), FontId::proportional(9.0 * s * fsc), txt_col);
                let star_w = if id == 1 { 14.0 * s } else { 0.0 };
                let cw = g.size().x + star_w + 22.0 * s;
                let ch = 20.0 * s * tap;
                let chip_rect = Rect::from_min_size(Pos2::new(cx, sc_rect.center().y - ch * 0.5), Vec2::new(cw, ch));
                let resp = ui.interact(chip_rect, ui.id().with(("pscope", id)), Sense::click());
                let (bg, border) = if active { (theme::sel_bg(), INK2()) } else { (Color32::TRANSPARENT, theme::line2()) };
                ui.painter().rect_filled(chip_rect, 10.0 * s, bg);
                ui.painter().rect_stroke(chip_rect, 10.0 * s, Stroke::new(bw, border), egui::StrokeKind::Inside);
                let mut tx = chip_rect.left() + 11.0 * s;
                if id == 1 {
                    paint_star(ui.painter(), Pos2::new(tx + 5.0 * s, chip_rect.center().y), 5.0 * s, active, txt_col, s);
                    tx += star_w;
                }
                ui.painter().galley(Pos2::new(tx, chip_rect.center().y - g.size().y * 0.5), g, txt_col);
                if resp.clicked() { new_scope = Some(id); }
                cx += cw + 6.0 * s;
            }

            // ===== rows =====
            if n_rows == 0 {
                let msg = if q_disp.is_empty() { "no presets yet — save the current patch with the disk icon".to_string() }
                          else { format!("no match for \u{201C}{}\u{201D}", if q_disp.chars().count() > 22 { format!("{}\u{2026}", q_disp.chars().take(21).collect::<String>()) } else { q_disp.clone() }) };
                ui.painter().text(list_rect.center(), Align2::CENTER_CENTER, msg, FontId::monospace(10.0 * s * fsc), FAINT());
            } else {
                for (k, r) in rows.iter().enumerate() {
                    let ry = list_rect.top() + list_pad + k as f32 * row_h;
                    let rrect = Rect::from_min_size(Pos2::new(panel.left(), ry), Vec2::new(w, row_h));
                    let on = k == sel;
                    let resp = ui.interact(rrect, ui.id().with(("prow", k)), Sense::click());
                    if resp.hovered() { hover_sel = Some(k); }
                    if on {
                        ui.painter().rect_filled(rrect, 0.0, theme::sel_bg());
                        ui.painter().rect_filled(Rect::from_min_size(rrect.min, Vec2::new(2.0 * s, row_h)), 0.0, INK2());
                    }
                    let star_c = Pos2::new(rrect.left() + 16.0 * s + 6.0 * s, rrect.center().y);
                    let star_r = 6.0 * s;
                    paint_star(ui.painter(), star_c, star_r, r.fav, if r.fav { G() } else { FAINT() }, s);
                    let sresp = ui.interact(Rect::from_center_size(star_c, Vec2::splat(20.0 * s)), ui.id().with(("pstar", k)), Sense::click());
                    if sresp.clicked() { do_fav = Some(r.name.clone()); }

                    let mut rx = rrect.right() - 16.0 * s;
                    if on && !name_mode {
                        // delete ✕ (two-click confirm)
                        let armed = st.palette_del_confirm.as_deref() == Some(r.name.as_str());
                        let danger = Color32::from_rgb(214, 69, 69);
                        let del_c = Pos2::new(rx - 6.0 * s, rrect.center().y);
                        let del_resp = ui.interact(Rect::from_center_size(del_c, Vec2::splat(20.0 * s)), ui.id().with(("pdel", k)), Sense::click());
                        if armed { ui.painter().circle_filled(del_c, 9.0 * s, theme::with_alpha(danger, 38)); }
                        let del_col = if armed { danger } else if del_resp.hovered() { INK() } else { FAINT() };
                        paint_cross(ui.painter(), del_c, 5.0 * s, del_col, s);
                        if del_resp.clicked() { if armed { do_delete = Some(r.name.clone()); } else { del_arm = Some(r.name.clone()); } }
                        rx -= 24.0 * s;
                        // rename ✎
                        let ren_c = Pos2::new(rx - 6.0 * s, rrect.center().y);
                        let ren_resp = ui.interact(Rect::from_center_size(ren_c, Vec2::splat(20.0 * s)), ui.id().with(("pren", k)), Sense::click());
                        paint_pencil(ui.painter(), ren_c, s, if ren_resp.hovered() { INK() } else { FAINT() });
                        if ren_resp.clicked() { open_rename = Some(r.name.clone()); }
                        rx -= 22.0 * s;
                        // add-tag: inline field when armed, else a ＋ chip
                        if st.palette_tag_for.as_deref() == Some(r.name.as_str()) {
                            let fw = 92.0 * s;
                            let fr = Rect::from_min_max(Pos2::new(rx - fw, rrect.center().y - 11.0 * s), Pos2::new(rx, rrect.center().y + 11.0 * s));
                            ui.painter().rect_filled(fr, 5.0 * s, INSET());
                            ui.painter().rect_stroke(fr, 5.0 * s, Stroke::new(bw, G()), egui::StrokeKind::Inside);
                            let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(fr.shrink2(Vec2::new(6.0 * s, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            let te = cu.add(egui::TextEdit::singleline(&mut st.palette_tag_buf).frame(egui::Frame::NONE).hint_text("tag").font(FontId::monospace(10.0 * s * fsc)).text_color(INK()).desired_width(fw - 12.0 * s));
                            if te.lost_focus() { tag_lost = true; }
                            if st.palette_tag_focus { te.request_focus(); st.palette_tag_focus = false; }
                            rx -= fw + 6.0 * s;
                        } else {
                            let plus_c = Pos2::new(rx - 8.0 * s, rrect.center().y);
                            let prect = Rect::from_center_size(plus_c, Vec2::splat(16.0 * s));
                            let plus_resp = ui.interact(prect, ui.id().with(("ptagadd", k)), Sense::click());
                            ui.painter().rect_stroke(prect, 5.0 * s, Stroke::new(bw, theme::line2()), egui::StrokeKind::Inside);
                            paint_plus(ui.painter(), plus_c, 3.5 * s, if plus_resp.hovered() { INK() } else { SOFT() }, s);
                            if plus_resp.clicked() { open_tag_add = Some(r.name.clone()); }
                            rx -= 22.0 * s;
                        }
                        // editable tag chips (right→left) each with a ✕
                        let name_zone = star_c.x + star_r + 12.0 * s + 60.0 * s;
                        for t in r.tags.iter().rev() {
                            let tg = ui.painter().layout_no_wrap(t.clone(), FontId::monospace(8.5 * s * fsc), SOFT());
                            let chip_w = tg.size().x + 20.0 * s;
                            if rx - chip_w < name_zone { break; }
                            rx -= chip_w;
                            let chip = Rect::from_min_size(Pos2::new(rx, rrect.center().y - 9.0 * s), Vec2::new(chip_w, 18.0 * s));
                            ui.painter().rect_filled(chip, 5.0 * s, theme::sel_bg());
                            ui.painter().galley(Pos2::new(chip.left() + 6.0 * s, rrect.center().y - tg.size().y * 0.5), tg, SOFT());
                            let xc = Pos2::new(chip.right() - 6.0 * s, rrect.center().y);
                            let xr = ui.interact(Rect::from_center_size(xc, Vec2::splat(16.0 * s)), ui.id().with(("ptagx", k, t.as_str())), Sense::click());
                            paint_cross(ui.painter(), xc, 3.0 * s, if xr.hovered() { INK() } else { FAINT() }, s);
                            if xr.clicked() { tag_remove = Some((r.name.clone(), t.clone())); }
                            rx -= 6.0 * s;
                        }
                    } else {
                        for t in r.tags.iter().rev() {
                            let tg = ui.painter().layout_no_wrap(t.clone(), FontId::monospace(8.5 * s * fsc), SOFT());
                            rx -= tg.size().x;
                            ui.painter().galley(Pos2::new(rx, rrect.center().y - tg.size().y * 0.5), tg, SOFT());
                            rx -= 6.0 * s;
                        }
                    }
                    let cluster_left = rx;
                    let name_x = star_c.x + star_r + 12.0 * s;
                    let name_col = if on { INK() } else { INK2() };
                    let build_budget = if r.build.is_empty() { 0.0 } else { 50.0 * s };
                    let name_max = (cluster_left - name_x - build_budget).max(8.0 * s);
                    let mut job = egui::text::LayoutJob::simple_singleline(r.name.clone(), FontId::proportional(13.0 * s * fsc), name_col);
                    job.wrap.max_width = name_max;
                    job.wrap.max_rows = 1;
                    job.wrap.overflow_character = Some('\u{2026}');
                    let ng = ui.painter().layout_job(job);
                    let nw = ng.size().x;
                    ui.painter().galley(Pos2::new(name_x, rrect.center().y - ng.size().y * 0.5), ng, name_col);
                    if !r.build.is_empty() {
                        let tag_x = name_x + nw + 12.0 * s;
                        let bg = ui.painter().layout_no_wrap(r.build.clone(), FontId::monospace(9.5 * s * fsc), SOFT());
                        if tag_x + bg.size().x <= cluster_left {
                            ui.painter().galley(Pos2::new(tag_x, rrect.center().y - bg.size().y * 0.5), bg, SOFT());
                        }
                    }
                    if resp.clicked() { do_load = Some(r.name.clone()); }
                }
            }

            // ===== footer (clickable keycaps) =====
            let cap_h = 16.0 * s;
            let draw_cap = |ui: &egui::Ui, x: f32, kind: u8, txt: &str, salt: &str, act: bool| -> (f32, bool) {
                let cy = ft_rect.center().y;
                let cpad = 6.0 * s;
                let cw = (match kind {
                    0 | 1 => 16.0 * s,
                    _ => ui.painter().layout_no_wrap(txt.to_string(), FontId::monospace(9.0 * s * fsc), INK2()).size().x + cpad * 2.0,
                }).max(16.0 * s);
                let cr = Rect::from_min_size(Pos2::new(x, cy - cap_h * 0.5), Vec2::new(cw, cap_h));
                let (hot, clicked) = if act {
                    let resp = ui.interact(cr, ui.id().with(("pcap", salt)), Sense::click());
                    (resp.hovered(), resp.clicked())
                } else { (false, false) };
                let fg = if hot { INK() } else { INK2() };
                let border = if hot { theme::with_alpha(INK(), 90) } else { theme::line2() };
                ui.painter().rect_filled(cr, 5.0 * s, INSET());
                ui.painter().rect_stroke(cr, 5.0 * s, Stroke::new(bw, border), egui::StrokeKind::Inside);
                match kind {
                    0 => {
                        let stk = Stroke::new(1.2 * s, fg);
                        let c = cr.center();
                        let dx = 3.0 * s;
                        ui.painter().line_segment([Pos2::new(c.x - 5.0 * s, c.y + 1.0 * s), Pos2::new(c.x - 5.0 * s + dx, c.y - 2.0 * s)], stk);
                        ui.painter().line_segment([Pos2::new(c.x - 5.0 * s + dx, c.y - 2.0 * s), Pos2::new(c.x - 5.0 * s + 2.0 * dx, c.y + 1.0 * s)], stk);
                        ui.painter().line_segment([Pos2::new(c.x + 5.0 * s - 2.0 * dx, c.y - 1.0 * s), Pos2::new(c.x + 5.0 * s - dx, c.y + 2.0 * s)], stk);
                        ui.painter().line_segment([Pos2::new(c.x + 5.0 * s - dx, c.y + 2.0 * s), Pos2::new(c.x + 5.0 * s, c.y - 1.0 * s)], stk);
                    }
                    1 => { paint_return(ui.painter(), Pos2::new(cr.center().x + 4.5 * s, cr.center().y), s, fg); }
                    _ => { ui.painter().text(cr.center(), Align2::CENTER_CENTER, txt, FontId::monospace(9.0 * s * fsc), fg); }
                }
                (cw, clicked)
            };
            let mut fx = ft_rect.left() + 16.0 * s;
            if name_mode {
                let commit_label = if st.palette_rename_from.is_some() { "rename" } else { "save" };
                let (cw, clicked) = draw_cap(ui, fx, 1, "", "namecommit", true);
                if clicked { name_commit = true; }
                fx += cw + 6.0 * s;
                let lg = ui.painter().layout_no_wrap(commit_label.to_string(), FontId::monospace(9.0 * s * fsc), FAINT());
                ui.painter().galley(Pos2::new(fx, ft_rect.center().y - lg.size().y * 0.5), lg, FAINT());
            } else {
                let legend: [(u8, &str, &str, &str, bool); 3] = [
                    (0, "", "navigate", "nav", false),
                    (1, "", "load", "load", true),
                    (2, "save", "save", "save", true),
                ];
                for (kind, txt, label, salt, act) in legend {
                    let (cw, clicked) = draw_cap(ui, fx, kind, txt, salt, act);
                    if clicked {
                        match salt {
                            "load" => { if let Some(r) = rows.get(sel) { do_load = Some(r.name.clone()); } }
                            "save" => open_save = true,
                            _ => {}
                        }
                    }
                    fx += cw + 6.0 * s;
                    let lg = ui.painter().layout_no_wrap(label.to_string(), FontId::monospace(9.0 * s * fsc), FAINT());
                    let lw = lg.size().x;
                    ui.painter().galley(Pos2::new(fx, ft_rect.center().y - lg.size().y * 0.5), lg, FAINT());
                    fx += lw + 16.0 * s;
                }
            }
            let esc_label = if text_mode { "cancel" } else { "close" };
            let close_lab = ui.painter().layout_no_wrap(esc_label.to_string(), FontId::monospace(9.0 * s * fsc), FAINT());
            let clx = ft_rect.right() - 16.0 * s - close_lab.size().x;
            ui.painter().galley(Pos2::new(clx, ft_rect.center().y - close_lab.size().y * 0.5), close_lab, FAINT());
            let (_, esc_clicked) = draw_cap(ui, clx - 14.0 * s - 28.0 * s, 2, "esc", "esc", true);
            if esc_clicked {
                if tag_mode { tag_cancel = true; } else if name_mode { name_cancel = true; } else { do_close = true; }
            }

            ui.painter().rect_stroke(panel, r14, Stroke::new(bw, theme::line2()), egui::StrokeKind::Inside);
            if st.preset_guard == 0 && backdrop.clicked() && !block.clicked() { do_close = true; }
        });

    // ── apply deferred ──
    if k_enter && name_mode { name_commit = true; }
    // tag validates on Return OR tap-away (tag_lost), but Esc must cancel, not
    // commit — so guard the blur-commit on !k_esc.
    if (k_enter && tag_mode) || (tag_lost && tag_mode && !k_esc) { tag_commit = true; }
    if k_esc && name_mode { name_cancel = true; }
    if k_esc && tag_mode { tag_cancel = true; }
    if k_save && !text_mode { open_save = true; }

    if let Some(id) = new_scope {
        st.palette_scope = id;
        st.palette_sel = 0;
        st.palette_focus = true;
        st.palette_del_confirm = None;
        st.palette_tag_for = None;
        st.palette_tag_buf.clear();
    }
    if te_changed {
        st.palette_sel = 0;
    } else if let Some(h) = hover_sel {
        if pointer_moved && !k_up && !k_down { st.palette_sel = h; }
    }
    if let Some(nm) = do_fav {
        if let Some(l) = st.library.as_mut() { l.toggle_favorite(&nm); }
        if st.palette_scope == 1 { st.palette_sel = 0; }
    }
    if let Some(nm) = open_tag_add {
        st.palette_tag_for = Some(nm);
        st.palette_tag_buf.clear();
        st.palette_tag_focus = true;
        st.palette_name_open = false;
        st.palette_del_confirm = None;
    }
    if tag_commit {
        if let Some(nm) = st.palette_tag_for.clone() {
            let t = st.palette_tag_buf.trim().to_string();
            if !t.is_empty() { if let Some(l) = st.library.as_mut() { let _ = l.add_tag(&nm, &t); } }
            st.palette_tag_buf.clear();
            st.palette_tag_focus = true;
        }
    }
    if tag_cancel { st.palette_tag_for = None; st.palette_tag_buf.clear(); st.palette_focus = true; }
    if let Some((nm, t)) = tag_remove {
        if let Some(l) = st.library.as_mut() { let _ = l.remove_tag(&nm, &t); }
    }
    if let Some(nm) = open_rename {
        st.palette_name_open = true;
        st.palette_rename_from = Some(nm.clone());
        st.palette_name_buf = nm;
        st.palette_name_focus = true;
        st.palette_tag_for = None;
        st.palette_del_confirm = None;
    } else if open_save && !st.palette_name_open {
        st.palette_name_open = true;
        st.palette_rename_from = None;
        let typed = st.preset_search.trim();
        st.palette_name_buf = if !typed.is_empty() { typed.to_string() } else { st.current_preset.clone().unwrap_or_default() };
        st.palette_name_focus = true;
        st.palette_tag_for = None;
    }
    if name_commit {
        let typed = st.palette_name_buf.trim().to_string();
        if typed.is_empty() {
            st.palette_name_focus = true;
        } else if let Some(old) = st.palette_rename_from.clone() {
            let ok = st.library.as_mut().is_some_and(|l| l.rename(&old, &typed).is_ok());
            if ok {
                if st.current_preset.as_deref() == Some(old.as_str()) { st.current_preset = Some(typed.clone()); }
                for n in st.recent.iter_mut() { if *n == old { *n = typed.clone(); } }
                st.palette_name_open = false;
                st.palette_rename_from = None;
                st.palette_name_buf.clear();
                st.palette_focus = true;
            } else { st.palette_name_focus = true; }
        } else if preset_save(st, params, &typed) {
            st.palette_name_open = false;
            st.palette_name_buf.clear();
            st.palette_focus = true;
        } else { st.palette_name_focus = true; }
    }
    if name_cancel {
        st.palette_name_open = false;
        st.palette_rename_from = None;
        st.palette_name_buf.clear();
        st.palette_focus = true;
    }
    if let Some(nm) = del_arm { st.palette_del_confirm = Some(nm); }
    if let Some(nm) = do_delete {
        if let Some(l) = st.library.as_mut() { let _ = l.delete(&nm); }
        if st.current_preset.as_deref() == Some(nm.as_str()) { st.current_preset = None; st.loaded_snapshot = None; }
        st.recent.retain(|n| n != &nm);
        st.palette_del_confirm = None;
        st.palette_sel = 0;
    }
    {
        let sel_now = st.palette_sel.min(n_rows.saturating_sub(1));
        let sel_name = rows.get(sel_now).map(|r| r.name.as_str());
        if st.palette_del_confirm.is_some() && st.palette_del_confirm.as_deref() != sel_name {
            st.palette_del_confirm = None;
        }
    }
    if (k_init && !text_mode) || do_init_click { preset_init(st, params, setter); }
    if k_enter && !text_mode {
        if let Some(r) = rows.get(sel) { do_load = Some(r.name.clone()); }
    }
    if let Some(nm) = do_load {
        if !name_mode {
            preset_load(st, params, setter, &nm);
            st.preset_open = false;
            st.palette_name_open = false;
            st.palette_rename_from = None;
            st.palette_tag_for = None;
            st.palette_del_confirm = None;
        }
    }
    if do_close || (k_esc && !text_mode) {
        st.preset_open = false;
        st.palette_name_open = false;
        st.palette_rename_from = None;
        st.palette_tag_for = None;
        st.palette_del_confirm = None;
    }
}

// ─── painted icons (no glyph dependency) ────────────────────────────────────
fn paint_star(p: &egui::Painter, c: Pos2, r: f32, filled: bool, col: Color32, s: f32) {
    use std::f32::consts::PI;
    let mut pts = [Pos2::ZERO; 10];
    for (k, pt) in pts.iter_mut().enumerate() {
        let rr = if k % 2 == 0 { r } else { r * 0.42 };
        let a = -PI / 2.0 + k as f32 * PI / 5.0;
        *pt = Pos2::new(c.x + a.cos() * rr, c.y + a.sin() * rr);
    }
    if filled {
        for k in 0..10 {
            p.add(egui::Shape::convex_polygon(vec![c, pts[k], pts[(k + 1) % 10]], col, Stroke::NONE));
        }
    } else {
        let mut poly: Vec<Pos2> = pts.to_vec();
        poly.push(pts[0]);
        p.add(egui::Shape::line(poly, Stroke::new(1.2 * s, col)));
    }
}

fn paint_return(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let w = 9.0 * s;
    let st = Stroke::new(1.3 * s, col);
    let x0 = c.x - w;
    let x1 = c.x;
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x1, c.y)], st);
    p.line_segment([Pos2::new(x1, c.y), Pos2::new(x1, c.y - 5.0 * s)], st);
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x0 + 3.0 * s, c.y - 3.0 * s)], st);
    p.line_segment([Pos2::new(x0, c.y), Pos2::new(x0 + 3.0 * s, c.y + 3.0 * s)], st);
}

fn paint_disk(p: &egui::Painter, c: Pos2, half: f32, col: Color32, s: f32) {
    let body = Rect::from_center_size(c, Vec2::splat(half * 2.0));
    p.rect_stroke(body, 2.0 * s, Stroke::new(1.3 * s, col), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_max(Pos2::new(body.left() + half * 0.5, body.top() + half * 0.28), Pos2::new(body.right() - half * 0.5, body.top() + half * 0.85)), 0.0, col);
    p.rect_stroke(Rect::from_min_max(Pos2::new(body.left() + half * 0.42, c.y + half * 0.12), Pos2::new(body.right() - half * 0.42, body.bottom() - half * 0.28)), 0.0, Stroke::new(1.0 * s, col), egui::StrokeKind::Inside);
}

fn paint_cross(p: &egui::Painter, c: Pos2, r: f32, col: Color32, s: f32) {
    let st = Stroke::new(1.3 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)], st);
    p.line_segment([Pos2::new(c.x - r, c.y + r), Pos2::new(c.x + r, c.y - r)], st);
}

fn paint_plus(p: &egui::Painter, c: Pos2, r: f32, col: Color32, s: f32) {
    let st = Stroke::new(1.3 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], st);
    p.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], st);
}

fn paint_check(p: &egui::Painter, c: Pos2, r: f32, col: Color32, s: f32) {
    let st = Stroke::new(1.6 * s, col);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x - r * 0.3, c.y + r * 0.8)], st);
    p.line_segment([Pos2::new(c.x - r * 0.3, c.y + r * 0.8), Pos2::new(c.x + r, c.y - r * 0.8)], st);
}

fn paint_pencil(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let u = 1.7 * s;
    let dir = Vec2::new(0.7071, -0.7071);
    let perp = Vec2::new(0.7071, 0.7071);
    let len = 5.5 * s;
    let tip = c - dir * len;
    let top = c + dir * len * 0.85;
    let b = tip + dir * (2.4 * s);
    p.add(egui::Shape::convex_polygon(vec![top + perp * u, top - perp * u, b - perp * u, b + perp * u], col, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![b + perp * u, b - perp * u, tip], col, Stroke::NONE));
}
