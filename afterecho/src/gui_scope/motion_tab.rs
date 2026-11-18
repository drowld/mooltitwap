//! The Motion tab on the shared crate's editors (beta pass 8, layout A, owner 2026-11-14):
//! in the lane, the six slot tiles left (2 × 3), the open slot's editor (curve, steps or
//! follow; its kind menu on its name), Scatter at the right edge; "Routes…" opens the grid
//! as a sheet on the picture's card (the lane stays), with Depth and Time in its header.

use super::{button, knob_cell, n_active, pill_drag, r, rule, tap_col, text, Layout, ScopeState};
use crate::modulation::{self, ALLOWED, FOLLOW_INPUTS, N_SLOTS, N_TGT, TGT_IDS};
use crate::params::AfterechoParams;
use crate::ui::theme;
use nice_plug::prelude::*;
use egui::{Align2, Pos2, Rect, Vec2};
use softsystems_mod::{mod_target_base, mod_targets, Assign, Modulator, Param as ModParam, Patch, Port, Routes, Target, Targets, Touch};
use softsystems_ui::modulation::curve::{self, CurveIn, CurveState};
use softsystems_ui::modulation::follow::{self, FollowIn};
use softsystems_ui::modulation::header::{EditOut, HeaderIn, Marks as HeadMarks, RouteGesture};
use softsystems_ui::modulation::steps::{self, StepsIn, StepsState};
use softsystems_ui::modulation::tiles::{self, TileIn, DROP_AMOUNT};
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
    /// A slot setting's routes menu, open (pass 11): the slot, the setting, its anchor.
    mod_menu: Option<(usize, ModParam, Rect)>,
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
            mod_menu: None,
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

    // the tiles: a tap opens a slot, ↗ arms it, a drag carries it to a control
    let armed_port = wiring(ui.ctx(), |w| w.assign.armed());
    let items: Vec<TileIn> = (0..N_SLOTS)
        .map(|i| TileIn {
            generator: &patch.modulators[i],
            name: &names[i],
            col: slot_col(i),
            value: mon.value(i),
            outputs: &[],
            position: mon.position(i),
            armed: armed_port == Some(Port::slot(i as u16)),
            armed_output: None,
            open: mu.sel == i,
        })
        .collect();
    let outs = tiles::tiles(ui, r(o, s, x + 14.0, y + 50.0, 196.0, 184.0), 2, 3, &items, None, ui.id().with("mo_tiles"));
    drop(items);
    if let Some(i) = outs.iter().position(|o| o.open) {
        mu.sel = i;
    }
    for (i, o) in outs.iter().enumerate() {
        let port = Port::slot(i as u16);
        if o.arm {
            wiring(ui.ctx(), |w| w.assign.arm(port));
        }
        if let Some((_, hit)) = o.drag.and_then(|at| target_at(ui.ctx(), at)) {
            light(ui.ctx(), hit, slot_col(i), s);
        }
        if let Some((t, _)) = o.drop.and_then(|at| target_at(ui.ctx(), at)) {
            patch.routes.set(port, TGT_IDS[t], DROP_AMOUNT, modulation::ROUTE_CAP);
            edited(setter, m, true);
        }
    }
    let p = ui.painter().clone();
    for xr in [x + 220.0, x + 712.0] {
        rule(&p, Pos2::new(o.x + xr * s, o.y + (y + 52.0) * s), Pos2::new(o.x + xr * s, o.y + (y + 232.0) * s), s);
    }

    // the open slot's editor, a playhead per tap
    let sel = mu.sel.min(N_SLOTS - 1);
    // routes into this slot's own settings, and the armed slot (pass 11)
    let into: Vec<(ModParam, egui::Color32)> = patch
        .routes
        .iter()
        .filter_map(|r| ModParam::parse(&r.target).filter(|(slot, _)| *slot as usize == sel).map(|(_, p)| (p, slot_col(r.source.slot as usize))))
        .collect();
    let head = HeaderIn {
        name: &names[sel],
        col: slot_col(sel),
        notes: false,
        voices: false,
        tempo: m.shared.tempo(),
        sig: m.shared.sig(),
        marks: HeadMarks { routes: &into, armed: armed_port.map(|p| slot_col(p.slot as usize)) },
        kinds: &ALLOWED,
        // Afterecho's slots keep nothing fixed: every kind, ±, trigger stays editable
        lock: softsystems_mod::Lock::NONE,
    };
    let er = r(o, s, x + 230.0, y + 46.0, 472.0, 194.0);
    let id = ui.id().with(("mo_editor", sel));
    let heads = mon.heads(sel);
    // each playhead in its tap's colour, as the tap field draws it (owner, 2026-11-16: A).
    // The shown voices are the taps 0..n in order (`place_heads`); `heads` skips the rest.
    let n = n_active(params);
    let head_cols: Vec<egui::Color32> =
        (0..mon.voices()).filter(|&k| mon.voice_position(sel, k).is_finite()).map(|k| tap_col(k, n)).collect();
    let mut g = patch.modulators[sel];
    let out: EditOut = match &mut g {
        Modulator::Curve(c) => {
            curve::curve_editor(ui, er, c, &mut mu.curves[sel], &CurveIn { head, heads: &heads, value: mon.value(sel) }, id)
        }
        Modulator::Steps(st) => steps::steps_editor(ui, er, st, &mut mu.steps[sel], &StepsIn { head, heads: &heads, head_cols: &head_cols }, id),
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
    // a setting touched with a slot armed makes or sizes its route; a right-click lists them
    if let Some(gesture) = out.route {
        wiring(ui.ctx(), |w| match gesture {
            RouteGesture::Touch(p) => {
                w.assign.touch(&p.id(sel as u16), &mut patch.routes, modulation::ROUTE_CAP);
            }
            RouteGesture::Drag(d) => {
                w.assign.drag(&mut patch.routes, d);
            }
            RouteGesture::Release => w.assign.release(),
        });
        edited(setter, m, matches!(gesture, RouteGesture::Release));
    }
    if let Some((p, anchor)) = out.menu {
        mu.mod_menu = Some((sel, p, anchor));
    }
    if let Some((slot, p, anchor)) = mu.mod_menu {
        let gens = patch.modulators.clone();
        let names: Vec<String> = (0..N_SLOTS).map(|i| slot_name(&patch, i)).collect();
        let targets = Targets::new(mod_targets(&gens, &names));
        let id = p.id(slot as u16);
        let base = mod_target_base(&gens, &id).unwrap_or(0.5);
        let columns: Vec<Column> = (0..N_SLOTS).map(|i| Column::new(&names[i], slot_col(i))).collect();
        let mid = ui.id().with(("mo_mod_menu", slot, id.as_str()));
        let o = softsystems_ui::modulation::knob_menu(ui, mid, anchor, &id, &targets, &columns, base, &mut patch.routes);
        if o.changed {
            edited(setter, m, true);
        }
        if o.matrix {
            mu.routes = true;
        }
        if o.closed || o.matrix || slot != sel {
            mu.mod_menu = None;
        }
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

/// The Routes sheet on the picture's card (the lane stays): every slot into every target, Depth
/// and Time in its header. ✕ or Escape closes it.
pub(super) fn routes_sheet(
    ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<AfterechoParams>, setter: &ParamSetter, sc: &mut ScopeState,
) {
    let (x, y, w, h) = l.picture;
    let rect = r(o, s, x, y, w, h);
    let p = ui.painter().clone();
    softsystems_ui::paint::card(&p, rect, "");
    // one line (owner, 2026-11-18): "Routes", the grid's header (Depth, Time, Numbers/Dots),
    // then the crate's ×, so all ten rows fit the picture's card
    text(&p, rect.min + Vec2::new(16.0, 26.0) * s, Align2::LEFT_CENTER, "Routes", 14.0, s, theme::INK());
    let bh = softsystems_ui::sheet::BTN_H;
    let close_r = r(o, s, x + w - 14.0 - bh, y + 12.0, bh, bh);
    let m = &params.motion;
    let shown_tap = sc.sel.unwrap_or(sc.last_tap);
    let mu = &mut sc.motion;
    let close = softsystems_ui::sheet::button(ui, close_r, "×", ui.id().with("mo_routes_close"), softsystems_ui::sheet::Look::Plain, true);
    if close || (!mu.grid.menu_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
        mu.routes = false;
    }
    let Ok(mut patch) = m.patch.lock() else { return };
    let columns: Vec<Column> = (0..N_SLOTS).map(|i| Column::new(&slot_name(&patch, i), slot_col(i)).per_voice()).collect();
    // the plug-in's targets, then every slot setting (pass 11: a row once routed or added)
    let names: Vec<String> = (0..N_SLOTS).map(|i| slot_name(&patch, i)).collect();
    let mut settings = mod_targets(&patch.modulators, &names);
    // the sheet draws no group headings (a rule between groups), so a setting's row says whose
    // it is: "LFO 1 Rate"
    for tg in &mut settings {
        tg.name = format!("{} {}", tg.group, tg.name);
    }
    let mut bases: Vec<f32> = (0..N_TGT).map(|t| knob_base(params, t, shown_tap)).collect();
    bases.extend(settings.iter().map(|tg| mod_target_base(&patch.modulators, &tg.id).unwrap_or(0.5)));
    let mut list: Vec<Target> = TARGETS.with(|t| t.iter().cloned().collect());
    list.extend(settings);
    let targets = Targets::new(list);
    let active = vec![true; bases.len()];
    // the pending row: the last knob touched or "+ Target" pick (none or one)
    let added: Vec<String> = wiring(ui.ctx(), |w| w.assign.added().iter().map(|s| s.to_string()).collect());
    let added: Vec<&str> = added.iter().map(String::as_str).collect();
    // the grid's 34 px header shares the title's line, centred with the ×
    let gr = Rect::from_min_max(rect.min + Vec2::new(14.0, 9.0) * s, Pos2::new(close_r.left() - 8.0 * s, rect.bottom() - 12.0 * s));
    let out = {
        let g = Grid {
            targets: &targets,
            sources: &columns,
            bases: &bases,
            active: &active,
            added: &added,
            cap: modulation::ROUTE_CAP,
            look: mu.look,
            columns: &[],
            row_order: &[],
            row_h: None,
            widths: Widths::Equal,
            headings: Headings::Rule,
            // off, like Cumulus: every one of Afterecho's targets has a knob, and the crate's menu
            // leaves slot settings out, so "+ Target" would only show greyed with nothing to add
            add_menu: false,
        };
        grid(ui, gr, &g, &mut patch.routes, &mut mu.grid, ui.id().with("mo_grid"))
    };
    if let Some(look) = out.look {
        mu.look = look;
    }
    // "+ Target": the pick becomes the pending row, and an armed slot routes there
    let added_route = out.add.and_then(|i| targets.get(i)).is_some_and(|tg| {
        let id = tg.id.clone();
        matches!(wiring(ui.ctx(), |w| w.assign.add(&id, &mut patch.routes, modulation::ROUTE_CAP, DROP_AMOUNT)), Touch::Route(_))
    });
    if out.changed || added_route {
        edited(setter, m, out.ended || added_route);
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

// ─────────────────────────────────────────────────────────────────────────────
// Arm-and-touch, drag-to-knob, the halo and the routes menu (beta pass 10, Strata's way)
// ─────────────────────────────────────────────────────────────────────────────

/// The page's routing state, in the editor's own egui context: the crate's `Assign` (the armed
/// slot, the knob being sized), the target controls laid out this frame and the last (for a
/// tile's drop and for lighting the control under a carried tile), a tapped route dot, and
/// the routes menu open on a control.
#[derive(Clone, Default)]
struct Wiring {
    assign: Assign,
    pass: u64,
    hits: Vec<(usize, Rect)>,
    hits_prev: Vec<(usize, Rect)>,
    /// A route dot was tapped: open this slot (`u16::MAX`: open Routes).
    show: Option<u16>,
    menu: Option<(usize, Rect)>,
}

/// Knob `tgt` was pressed with nothing armed (its `KnobOut::touched`): it becomes the Matrix's
/// pending row (owner, 2026-11-17: one row, replaced by the next touch; nice-plug 9d7f979).
pub(super) fn seen(ctx: &egui::Context, tgt: usize) {
    wiring(ctx, |w| w.assign.seen(TGT_IDS[tgt]));
}

fn wiring<R>(ctx: &egui::Context, f: impl FnOnce(&mut Wiring) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Wiring>(egui::Id::new("afterecho_wiring"))))
}

/// Target `tgt`'s control is at `r` this frame (a drop on it routes there).
pub(super) fn place(ctx: &egui::Context, tgt: usize, r: Rect) {
    let nr = ctx.cumulative_pass_nr();
    wiring(ctx, |w| {
        if w.pass != nr {
            w.hits_prev = std::mem::take(&mut w.hits);
            w.pass = nr;
        }
        w.hits.push((tgt, r));
    });
}

/// The target control under `at`, laid out this frame or the last.
fn target_at(ctx: &egui::Context, at: Pos2) -> Option<(usize, Rect)> {
    wiring(ctx, |w| w.hits.iter().chain(w.hits_prev.iter()).copied().find(|(_, r)| r.contains(at)))
}

/// The armed slot's colour, for the controls' ring.
pub(super) fn armed(ctx: &egui::Context) -> Option<egui::Color32> {
    wiring(ctx, |w| w.assign.armed()).map(|p| slot_col(p.slot as usize))
}

/// A control's arm-and-touch: the press makes the armed slot's route at 0 (or takes the one
/// there), the drag sizes it, a release with no drag leaves it at the crate's drop amount. The
/// slot stays armed for the next control; ↗ again (or Escape) disarms. A tapped route dot
/// opens its slot.
#[allow(clippy::too_many_arguments)]
pub(super) fn route_knob(
    ctx: &egui::Context, setter: &ParamSetter, m: &modulation::MotionParams, tgt: usize, touch: bool, drag: f32, release: bool,
    tap: Option<u16>,
) {
    if touch || drag != 0.0 || release {
        let Ok(mut patch) = m.patch.lock() else { return };
        wiring(ctx, |w| touch_route(&mut w.assign, &mut patch.routes, tgt, touch, drag, release));
        drop(patch);
        edited(setter, m, release);
    }
    if tap.is_some() {
        wiring(ctx, |w| w.show = tap);
    }
}

fn touch_route(a: &mut Assign, routes: &mut Routes, tgt: usize, touch: bool, drag: f32, release: bool) {
    if touch {
        a.touch(TGT_IDS[tgt], routes, modulation::ROUTE_CAP);
    }
    if drag != 0.0 {
        a.drag(routes, drag);
    }
    if release {
        if let Some(port) = a.armed() {
            if let Some(r) = routes.find(port, TGT_IDS[tgt]).and_then(|i| routes.get_mut(i)) {
                if r.amount == 0.0 {
                    r.amount = DROP_AMOUNT;
                }
            }
        }
        a.release();
    }
}

/// A target's routes menu (the crate's `knob_menu`), on a right-click or a long press: a row
/// per route into it (a pick clears it), Clear every route, Show in Routes. `base` is the
/// control's position, for the amounts' units.
pub(super) fn routes_menu(ui: &mut egui::Ui, setter: &ParamSetter, m: &modulation::MotionParams, tgt: usize, base: f32, open: bool, anchor: Rect) {
    if open {
        wiring(ui.ctx(), |w| w.menu = Some((tgt, anchor)));
    }
    let Some(anchor) = wiring(ui.ctx(), |w| w.menu.filter(|(t, _)| *t == tgt).map(|(_, r)| r)) else { return };
    let Ok(mut patch) = m.patch.lock() else { return };
    let columns: Vec<Column> = (0..N_SLOTS).map(|i| Column::new(&slot_name(&patch, i), slot_col(i))).collect();
    let id = ui.id().with(("afterecho_routes_menu", tgt));
    let out = TARGETS.with(|t| softsystems_ui::modulation::knob_menu(ui, id, anchor, TGT_IDS[tgt], t, &columns, base, &mut patch.routes));
    drop(patch);
    if out.changed {
        edited(setter, m, true);
    }
    wiring(ui.ctx(), |w| {
        if out.matrix {
            w.show = Some(u16::MAX);
        }
        if out.closed {
            w.menu = None;
        }
    });
}

/// A carried tile lights the control under it: a ring in the slot's colour, over everything.
fn light(ctx: &egui::Context, r: Rect, col: egui::Color32, s: f32) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("afterecho_lit")));
    p.rect_stroke(r.expand(3.0 * s), 8.0 * s, egui::Stroke::new(2.0 * s, col), egui::StrokeKind::Outside);
}

/// Every frame, whatever tab is open: a tapped route dot opens its slot in the Motion tab
/// ("+N" or Show in Routes opens the sheet); Escape disarms.
pub(super) fn follow_wiring(ctx: &egui::Context, sc: &mut ScopeState) {
    match wiring(ctx, |w| w.show.take()) {
        Some(u16::MAX) => {
            sc.lane_tab = 1;
            sc.motion.routes = true;
        }
        Some(src) if (src as usize) < N_SLOTS => {
            sc.lane_tab = 1;
            sc.motion.sel = src as usize;
        }
        _ => {}
    }
    if !sc.motion.routes && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        wiring(ctx, |w| w.assign.disarm());
    }
}

/// Preview only: slot 1 armed.
pub(super) fn arm_demo(ctx: &egui::Context) {
    wiring(ctx, |w| w.assign.arm(Port::slot(0)));
}

/// What modulation marks on one target's control.
pub(super) struct Marks {
    tgt: usize,
    routes: Routes,
    generators: Vec<Modulator>,
    depth: f32,
    /// The live modulation now (Depth applied): the target's, or the shown tap's.
    offset: f32,
    /// The target does nothing now (the matrix says inactive): dots faint, no halo.
    dim: bool,
}

impl Marks {
    fn deepest(&self) -> Option<u16> {
        self.routes.iter().max_by(|a, b| a.amount.abs().total_cmp(&b.amount.abs())).map(|r| r.source.slot)
    }
}

/// Target `tgt`'s marks; a tap target's live position is tap `tap`'s.
pub(super) fn marks(m: &modulation::MotionParams, tgt: usize, tap: Option<usize>) -> Marks {
    let (routes, generators) = match m.patch.lock() {
        Ok(g) => {
            let mut r = Routes::new();
            for x in g.routes.iter().filter(|x| x.target == TGT_IDS[tgt] && x.amount != 0.0) {
                r.set(x.source, &x.target, x.amount, usize::MAX);
            }
            (r, g.modulators.clone())
        }
        Err(_) => (Routes::new(), Vec::new()),
    };
    let mx = &m.shared.matrix;
    let offset = match tap {
        Some(v) => mx.voice_offset(tgt, v),
        None => mx.offset(tgt),
    };
    Marks { tgt, routes, generators, depth: m.depth.value(), offset, dim: !mx.is_active(tgt) }
}

/// The routes as the shared knob's dots: filled when the route pushes up, a ring when it pulls
/// down (Depth's sign included); faint while the target does nothing.
pub(super) fn route_dots(m: &Marks) -> Vec<softsystems_ui::knob::RouteDot> {
    m.routes
        .iter()
        .map(|r| {
            let col = slot_col(r.source.slot as usize);
            softsystems_ui::knob::RouteDot {
                source: r.source.slot,
                col: if m.dim || m.depth == 0.0 { theme::with_alpha(col, 90) } else { col },
                neg: r.amount * m.depth < 0.0,
            }
        })
        .collect()
}

/// The halo for a control at `n`: how far its routes can move it (Depth included), the live
/// position, the deepest route's colour. None while nothing can move it. Colour has no halo:
/// its route scales the colour amounts, it doesn't move a knob (DECISIONS 2026-11-16).
pub(super) fn halo(m: &Marks, n: f32) -> Option<((f32, f32), f32, egui::Color32)> {
    if m.dim || m.tgt == modulation::T_COLOUR {
        return None;
    }
    let src = m.deepest()?;
    let reach = TARGETS.with(|t| t.reach(m.tgt, n, &m.routes, &m.generators, m.depth));
    if reach.1 - reach.0 < 0.003 {
        return None;
    }
    let live = if m.offset.is_finite() { (n + m.offset).clamp(0.0, 1.0) } else { n };
    Some((reach, live, theme::with_alpha(slot_col(src as usize), 200)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use modulation::{T_COLOUR, T_CUTOFF, T_FDBK, T_PAN};

    #[test]
    fn an_armed_touch_makes_a_route() {
        let mut a = Assign::new();
        let mut routes = Routes::new();
        // nothing armed: no route
        touch_route(&mut a, &mut routes, T_PAN, true, 0.0, true);
        assert_eq!(routes.iter().count(), 0);
        // armed, a tap: the drop amount
        a.arm(Port::slot(2));
        touch_route(&mut a, &mut routes, T_PAN, true, 0.0, true);
        let r = routes.find(Port::slot(2), TGT_IDS[T_PAN]).and_then(|i| routes.get(i)).expect("made");
        assert_eq!(r.amount, DROP_AMOUNT);
        // still armed: a press and a drag on Feedback sizes its route
        assert_eq!(a.armed(), Some(Port::slot(2)));
        touch_route(&mut a, &mut routes, T_FDBK, true, 0.0, false);
        touch_route(&mut a, &mut routes, T_FDBK, false, -0.4, false);
        touch_route(&mut a, &mut routes, T_FDBK, false, 0.0, true);
        let r = routes.find(Port::slot(2), TGT_IDS[T_FDBK]).and_then(|i| routes.get(i)).expect("made");
        assert!((r.amount + 0.4).abs() < 0.02, "{}", r.amount);
    }

    #[test]
    fn a_drop_finds_the_control_under_it() {
        let ctx = egui::Context::default();
        place(&ctx, T_CUTOFF, Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::splat(40.0)));
        assert_eq!(target_at(&ctx, Pos2::new(30.0, 30.0)).map(|h| h.0), Some(T_CUTOFF));
        assert!(target_at(&ctx, Pos2::new(80.0, 30.0)).is_none());
    }

    #[test]
    fn dots_follow_the_sign_of_the_route() {
        let p = AfterechoParams::default();
        p.motion.patch.lock().unwrap().routes.set(2u16, "cutoff", -0.4, modulation::ROUTE_CAP);
        let dots = route_dots(&marks(&p.motion, T_CUTOFF, Some(0)));
        assert_eq!(dots.len(), 1);
        assert_eq!(dots[0].source, 2);
        assert!(dots[0].neg);
        assert!(route_dots(&marks(&p.motion, T_PAN, Some(0))).is_empty());
    }

    #[test]
    fn the_halo_is_the_routes_reach_and_colour_has_none() {
        let p = AfterechoParams::default();
        {
            let mut g = p.motion.patch.lock().unwrap();
            g.routes.set(0u16, "fdbk", 0.25, modulation::ROUTE_CAP);
            g.routes.set(0u16, "colour", 0.5, modulation::ROUTE_CAP);
        }
        let ((lo, hi), _, _) = halo(&marks(&p.motion, T_FDBK, None), 0.5).expect("a route reaches");
        assert!(lo < 0.5 && hi > 0.5, "{lo}..{hi}");
        assert!(halo(&marks(&p.motion, T_PAN, Some(0)), 0.5).is_none());
        assert!(halo(&marks(&p.motion, T_COLOUR, None), 0.5).is_none());
        assert_eq!(route_dots(&marks(&p.motion, T_COLOUR, None)).len(), 1);
    }
}

#[cfg(test)]
mod mod_to_mod {
    use super::*;

    /// One pending row (owner, 2026-11-17): a knob touched with nothing armed shows in the
    /// Matrix, the next touch replaces it; a "+ Target" pick with a slot armed routes there.
    #[test]
    fn a_touched_knob_is_the_one_pending_row() {
        let mut a = Assign::new();
        let mut routes = Routes::new();
        a.seen(TGT_IDS[0]);
        assert_eq!(a.added(), vec![TGT_IDS[0]]);
        a.seen(TGT_IDS[2]);
        assert_eq!(a.added(), vec![TGT_IDS[2]]);
        assert!(routes.is_empty());
        a.arm(Port::slot(0));
        let made = a.add(TGT_IDS[3], &mut routes, modulation::ROUTE_CAP, DROP_AMOUNT);
        assert!(matches!(made, Touch::Route(_)));
        assert_eq!(a.added(), vec![TGT_IDS[3]]);
    }

    /// Pass 11: a slot's setting is a target like a knob; its route rides in the patch and the
    /// matrix leaves it to the engine.
    #[test]
    fn an_armed_touch_on_a_setting_makes_a_mod_route() {
        let mut a = Assign::new();
        let mut routes = Routes::new();
        a.arm(Port::slot(1));
        let id = ModParam::Rate.id(0);
        a.touch(&id, &mut routes, modulation::ROUTE_CAP);
        a.drag(&mut routes, 0.3);
        a.release();
        let r = routes.find(Port::slot(1), &id).and_then(|i| routes.get(i)).expect("made");
        assert!((r.amount - 0.3).abs() < 0.02, "{}", r.amount);
        assert!(softsystems_mod::is_mod_target(&r.target));
        // it isn't one of the plug-in's targets: no knob shows it
        assert!(!TGT_IDS.contains(&r.target.as_str()));
    }
}
