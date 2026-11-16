//! The Motion tab on the shared crate's editors (beta pass 8, layout A, owner 2026-11-14):
//! in the lane, the six slot tiles left (2 × 3), the open slot's editor (curve, steps or
//! follow; its kind menu on its name), Scatter at the right edge; "Routes…" opens the grid
//! as a sheet over the picture and the lane, with Depth and Time in its header.

use super::{button, knob_cell, pill_drag, r, rule, text, Layout, ScopeState};
use crate::modulation::{self, ALLOWED, FOLLOW_INPUTS, N_SLOTS, N_TGT};
use crate::params::AfterechoParams;
use crate::ui::theme;
use nice_plug::prelude::*;
use egui::{Align2, Pos2, Rect, Vec2};
use softsystems_mod::{Modulator, Patch};
use softsystems_ui::modulation::curve::{self, CurveIn, CurveState};
use softsystems_ui::modulation::follow::{self, FollowIn};
use softsystems_ui::modulation::header::{EditOut, HeaderIn, Marks};
use softsystems_ui::modulation::steps::{self, StepsIn, StepsState};
use softsystems_ui::modulation::tiles::{self, TileIn};
use softsystems_ui::modulation::trace::Trace;
use softsystems_ui::modulation::{grid, CellLook, Column, Grid, GridState, Headings, Widths};
use std::sync::Arc;

/// The Motion tab's editor state: the open slot, each slot's editor state, the grid's, and
/// whether the Routes sheet is open.
pub(crate) struct MotionUi {
    pub(crate) sel: usize,
    pub(crate) routes: bool,
    curves: Vec<CurveState>,
    steps: Vec<StepsState>,
    traces: Vec<Trace>,
    grid: GridState,
    look: CellLook,
}

impl Default for MotionUi {
    fn default() -> Self {
        Self {
            sel: 0,
            routes: false,
            curves: (0..N_SLOTS).map(|_| CurveState::default()).collect(),
            steps: (0..N_SLOTS).map(|_| StepsState::default()).collect(),
            traces: (0..N_SLOTS).map(|_| Trace::default()).collect(),
            grid: GridState::default(),
            look: CellLook::default(),
        }
    }
}

/// A slot's name: the crate's, with its curves called LFO (owner, 2026-11-13).
pub(crate) fn slot_name(patch: &Patch, slot: usize) -> String {
    let n = softsystems_mod::name(&patch.modulators, slot);
    match n.strip_prefix("Curve") {
        Some(rest) => format!("LFO{rest}"),
        None => n,
    }
}

/// Slot `i`'s colour (the tiles, the editor's name, the grid's column).
pub(crate) fn slot_col(i: usize) -> egui::Color32 {
    theme::accent(i)
}

/// The patch changed: the audio thread takes it; a finished edit marks the host's state.
fn edited(setter: &ParamSetter, m: &modulation::MotionParams, ended: bool) {
    m.shared.changed();
    if ended {
        setter.mark_state_dirty();
    }
}

/// The Motion tab in the lane card.
pub(super) fn motion_panel(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState,
) {
    let (x, y, w, _) = l.lane;
    let m = &params.motion;
    let mu = &mut sc.motion;
    if button(ui, r(o, s, x + w - 14.0 - 92.0, y + 7.0, 92.0, 26.0), "Routes\u{2026}", mu.routes, "sc_routes") {
        mu.routes = true;
    }
    let Ok(mut patch) = m.patch.lock() else { return };
    // automation moves the editors' knobs
    m.mods.pull_editor(&mut patch.modulators);
    let names: Vec<String> = (0..N_SLOTS).map(|i| slot_name(&patch, i)).collect();
    let mon = &m.shared.monitor;

    // the tiles: a tap opens a slot
    let items: Vec<TileIn> = (0..N_SLOTS)
        .map(|i| TileIn {
            generator: &patch.modulators[i],
            name: &names[i],
            col: slot_col(i),
            value: mon.value(i),
            outputs: &[],
            position: mon.position(i),
            armed: false,
            armed_output: None,
            open: mu.sel == i,
        })
        .collect();
    let outs = tiles::tiles(ui, r(o, s, x + 14.0, y + 50.0, 196.0, 184.0), 2, 3, &items, None, ui.id().with("mo_tiles"));
    drop(items);
    if let Some(i) = outs.iter().position(|o| o.open) {
        mu.sel = i;
    }
    let p = ui.painter().clone();
    for xr in [x + 220.0, x + 712.0] {
        rule(&p, Pos2::new(o.x + xr * s, o.y + (y + 52.0) * s), Pos2::new(o.x + xr * s, o.y + (y + 232.0) * s), s);
    }

    // the open slot's editor, a playhead per tap
    let sel = mu.sel.min(N_SLOTS - 1);
    let head = HeaderIn {
        name: &names[sel],
        col: slot_col(sel),
        notes: false,
        voices: false,
        tempo: m.shared.tempo(),
        sig: m.shared.sig(),
        marks: Marks::NONE,
        kinds: &ALLOWED,
    };
    let er = r(o, s, x + 230.0, y + 46.0, 472.0, 194.0);
    let id = ui.id().with(("mo_editor", sel));
    let heads = mon.heads(sel);
    let mut g = patch.modulators[sel];
    let out: EditOut = match &mut g {
        Modulator::Curve(c) => {
            curve::curve_editor(ui, er, c, &mut mu.curves[sel], &CurveIn { head, heads: &heads, value: mon.value(sel) }, id)
        }
        Modulator::Steps(st) => steps::steps_editor(ui, er, st, &mut mu.steps[sel], &StepsIn { head, heads: &heads }, id),
        Modulator::Follow(f) => follow::follower_view(
            ui,
            er,
            f,
            &mut mu.traces[sel],
            &FollowIn { head, inputs: &FOLLOW_INPUTS, level: mon.value(sel) },
            id,
        ),
        // Expressions and Logic aren't allowed here (`slots().load` turns them back)
        _ => EditOut::default(),
    };
    if let Some(kind) = out.kind {
        if modulation::slots().set_kind(&mut patch, sel, kind) {
            m.mods.push(setter, sel, &patch.modulators[sel], true);
            edited(setter, m, true);
        }
    } else if g != patch.modulators[sel] {
        patch.modulators[sel] = g;
        m.mods.push(setter, sel, &g, out.ended);
        edited(setter, m, out.ended);
    } else if out.ended {
        m.mods.push(setter, sel, &g, true);
        edited(setter, m, true);
    }
    drop(patch);

    // Scatter: how far apart the taps read every slot
    knob_cell(ui, r(o, s, x + 720.0, y + 90.0, 72.0, 100.0), s, setter, &m.scatter, "mx_scatter", "Scatter", false, true, None, &mut sc.focus);
}

/// Target `t`'s knob position for the grid's readouts; the taps' are the shown tap's.
fn knob_base(p: &AfterechoParams, t: usize, tap: usize) -> f32 {
    use modulation::*;
    let tp = &p.taps[tap.min(crate::NUM_TAPS - 1)];
    match t {
        T_PAN => tp.pan.unmodulated_normalized_value(),
        T_GAIN => tp.gain_db.unmodulated_normalized_value(),
        T_CUTOFF => tp.filter_cutoff.unmodulated_normalized_value(),
        T_RESO => tp.filter_q.unmodulated_normalized_value(),
        T_COLOUR => COLOUR_BASE,
        T_FDBK => p.feedback.unmodulated_normalized_value(),
        T_CROSS => p.crossfeed.unmodulated_normalized_value(),
        T_TONE => p.loop_tone.unmodulated_normalized_value(),
        T_MIX => p.dry_wet.unmodulated_normalized_value(),
        _ => p.duck.unmodulated_normalized_value(),
    }
}

thread_local! {
    static TARGETS: softsystems_mod::Targets = modulation::targets();
}

/// The Routes sheet over the picture and the lane: every slot into every target, Depth
/// and Time in its header. ✕ or Escape closes it.
pub(super) fn routes_sheet(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState,
) {
    let (x, y, w, _) = l.picture;
    let bottom = l.lane.1 + l.lane.3;
    let rect = r(o, s, x, y, w, bottom - y);
    let p = ui.painter().clone();
    softsystems_ui::paint::card(&p, rect, "");
    text(&p, rect.min + Vec2::new(16.0, 24.0) * s, Align2::LEFT_CENTER, "Routes", 14.0, s, theme::INK());
    let m = &params.motion;
    let shown_tap = sc.sel.unwrap_or(sc.last_tap);
    let mu = &mut sc.motion;
    let close = button(ui, r(o, s, x + w - 14.0 - 64.0, y + 10.0, 64.0, 28.0), "Close", false, "mo_routes_close");
    if close || (!mu.grid.menu_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
        mu.routes = false;
    }
    let Ok(mut patch) = m.patch.lock() else { return };
    let columns: Vec<Column> = (0..N_SLOTS).map(|i| Column::new(&slot_name(&patch, i), slot_col(i)).per_voice()).collect();
    let bases: [f32; N_TGT] = std::array::from_fn(|t| knob_base(params, t, shown_tap));
    let active = [true; N_TGT];
    let gr = Rect::from_min_max(rect.min + Vec2::new(14.0, 48.0) * s, rect.max - Vec2::new(14.0, 12.0) * s);
    let out = TARGETS.with(|targets| {
        let g = Grid {
            targets,
            sources: &columns,
            bases: &bases,
            active: &active,
            added: &[],
            cap: modulation::CAPACITY,
            look: mu.look,
            columns: &[],
            row_order: &[],
            row_h: None,
            widths: Widths::Equal,
            headings: Headings::Rule,
        };
        grid(ui, gr, &g, &mut patch.routes, &mut mu.grid, ui.id().with("mo_grid"))
    });
    if let Some(look) = out.look {
        mu.look = look;
    }
    if out.changed {
        edited(setter, m, out.ended);
    } else if out.ended {
        setter.mark_state_dirty();
    }
    drop(patch);
    // the globals: in the grid's header, else in the sheet's title row
    let hr = out.header.unwrap_or(r(o, s, x + 100.0, y + 10.0, w - 190.0, 28.0));
    let (pw, ph) = (112.0 * s, 26.0 * s);
    let top = hr.center().y - ph * 0.5;
    let time = Rect::from_min_size(Pos2::new(hr.right() - pw, top), Vec2::new(pw, ph));
    let depth = time.translate(Vec2::new(-pw - 8.0 * s, 0.0));
    pill_drag(ui, depth, s, setter, &m.depth, "mx_depth", "Depth", &mut sc.focus);
    pill_drag(ui, time, s, setter, &m.time, "mx_time", "Time", &mut sc.focus);
}
