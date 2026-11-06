//! The preset browser: the header's preset box and the sheet over the cards,
//! both softsystems-ui's (`preset_sheet`, on the store's `Library` and
//! `Session`, v2.49). Afterecho keeps the library, the store's flow state and
//! the sheet's view, and keeps the host-session field in step. Every action
//! runs on the GUI thread; a load drives each param as a gesture.

use super::*;
use crate::params::{DelayMode, DiffuseRouting};
use crate::presets::{Library, PresetSession, Store};
use softsystems_presets::Session;
use softsystems_ui::preset_sheet::{self as shared, Ctx, PresetSheet, SummaryLine};

/// GUI-thread preset state: the library (opened lazily, on first use, so
/// instantiating the plug-in never touches the disk), the loaded preset and
/// Try (`session`), and the browser's view (`sheet`).
#[derive(Default)]
pub(crate) struct PresetUi {
    /// The library's folder and an old one to copy from once; None = the
    /// user's own (`presets::open_default`). Tests and previews set their own.
    pub(super) dir: Option<(PathBuf, Option<PathBuf>)>,
    pub(super) lib: Option<Library>,
    pub(super) session: Session<Store>,
    pub(super) sheet: PresetSheet<Store>,
    /// The host-session field as last written or adopted (§10).
    field: Option<PresetSession>,
    /// This frame's edited state (`update`).
    edited: bool,
    edited_frame: Option<u64>,
    /// Preview: once the sheet is open, 1 opens Save as…, 2 tries the
    /// second row, 3 asks to delete the third.
    pub(super) preview: u8,
}

impl PresetUi {
    /// Open the library on first use. The Time and Cross filters Afterecho
    /// remembered under its own keys before v2.49 become the sheet's.
    pub(super) fn ensure(&mut self) {
        if self.lib.is_some() {
            return;
        }
        let mut lib = match &self.dir {
            Some((d, old)) => Library::open(d.clone(), old.as_slice()),
            None => crate::presets::open_default(),
        };
        if lib.state.facet.is_empty() {
            let pairs: Vec<String> = ["time", "cross"]
                .into_iter()
                .filter_map(|k| Some(crate::presets::state_word(&lib, k)).filter(|v| !v.is_empty() && v != "any").map(|v| format!("{k}={v}")))
                .collect();
            if !pairs.is_empty() {
                let _ = lib.update_state(|s| s.facet = pairs.join(","));
            }
        }
        self.lib = Some(lib);
        self.sheet.summary = Some(summary);
    }

    /// This frame's edited state, computed once a frame.
    pub(super) fn update(&mut self, params: &PetalParams, frame: u64) -> bool {
        if self.edited_frame != Some(frame) {
            self.edited = self.session.edited(params);
            self.edited_frame = Some(frame);
        }
        self.edited
    }

    /// Keep the host-session field and this state in step: a value the host
    /// restored under us (a project opened, an undo) is adopted; ours is
    /// written when it changes. GUI thread; the audio thread never reads it.
    pub(super) fn sync_session(&mut self, params: &PetalParams, frame: u64) {
        let host = params.preset_session.lock().map(|g| g.clone()).unwrap_or_default();
        if self.field.as_ref() != Some(&host) {
            // this editor's first frame adopts only a named preset: an empty
            // field is a fresh instance, whatever this state already says
            if self.field.is_some() || !host.id.is_empty() {
                self.ensure();
                if let Some(lib) = &self.lib {
                    self.session.adopt(lib, &host);
                }
                self.edited_frame = None;
            }
            self.field = Some(host);
        }
        let edited = self.update(params, frame);
        let want = self.session.preset_ref(edited);
        if self.field.as_ref() != Some(&want) {
            if let Ok(mut g) = params.preset_session.lock() {
                *g = want.clone();
            }
            self.field = Some(want);
        }
    }

    /// ‹ › in the header: the previous / next preset of the browser's list.
    pub(super) fn step(&mut self, params: &PetalParams, setter: &ParamSetter, d: i32) {
        self.ensure();
        let Some(lib) = self.lib.as_mut() else { return };
        self.sheet.step(&mut Ctx { lib, session: &mut self.session, params, setter }, d);
        self.edited_frame = None;
    }

    /// The sheet shows or not this frame (it keeps a Try on close).
    pub(super) fn track(&mut self, open: bool) {
        if open {
            self.ensure();
        }
        if let Some(lib) = self.lib.as_mut() {
            self.sheet.track(open, lib, &mut self.session);
        }
    }
}

/// Six short lines: taps, time, the loop, its colour, Motion, the output.
/// "In this preset" under the sheet's list: the six lines joined into the two
/// it shows (the sound's shape and time; its colour, motion and output).
fn summary(p: &crate::presets::Preset) -> Vec<SummaryLine> {
    thread_local! {
        static PROBE: PetalParams = PetalParams::default();
    }
    let lines = PROBE.with(|q| {
        crate::presets::load_into(q, &p.values);
        summary_lines(q)
    });
    let line = |a: &[String]| vec![(a.join(" · "), None)];
    vec![line(&lines[..3]), line(&lines[3..])]
}

fn summary_lines(q: &PetalParams) -> Vec<String> {
    let taps = q.num_taps_active.value();
    let spacing = match q.spacing_mode.value() {
        SpacingMode::Linear => "Lin".to_string(),
        SpacingMode::Exponential => "Exp".to_string(),
        SpacingMode::Logarithmic => "Log".to_string(),
        SpacingMode::Euclidean => {
            let r = q.euclid_rotate.value();
            let turned = if r != 0 { format!(", turned {r}") } else { String::new() };
            format!("Euclid, {} steps{turned}", q.euclid_steps.value())
        }
    };
    let swing = q.swing.value();
    let swing = if swing > 0.5005 { format!(" · swing {:.0} %", swing * 100.0) } else { String::new() };
    let sync = q.sync_mode.value();
    let time = PetalParams::format_base_time_mode(sync, q.base_time_l.value(), 120.0);
    let split = if q.time_linked.value() { "" } else { " · L/R split" };
    let mode = match q.delay_mode.value() {
        DelayMode::Repitch => "Repitch",
        DelayMode::Fade => "Fade",
        DelayMode::Jump => "Jump",
    };
    let sync_word = if sync == SyncMode::Sync { "Sync" } else { "Free" };
    let cross = match q.crossfeed_mode.value() {
        CrossfeedMode::Mix => "Mix",
        CrossfeedMode::PingPong => "Ping",
        CrossfeedMode::Diffuse => "Diffuse",
    };
    let route = |r: DiffuseRouting| match r {
        DiffuseRouting::Input => "In",
        DiffuseRouting::Feedback => "FB",
        DiffuseRouting::Output => "Out",
    };
    let mut colour: Vec<String> = Vec::new();
    for (name, amount, r) in [
        ("Diffuse", q.diffuse_amount.value(), q.diffuse_routing.value()),
        ("Tape", q.degrade_amount.value(), q.degrade_routing.value()),
        ("Saturate", q.sat_amount.value(), q.sat_routing.value()),
    ] {
        if amount > 0.001 {
            colour.push(format!("{name} {}", route(r)));
        }
    }
    let tone = crate::params::tone_text(q.loop_tone.value());
    let colour = if colour.is_empty() { format!("Tone {tone} · no colour") } else { format!("Tone {tone} · {}", colour.join(", ")) };
    // Motion: each routed source and what it moves, the first two in full
    let routes: Vec<String> = q
        .mx_rows()
        .iter()
        .enumerate()
        .filter_map(|(r, row)| {
            let to: Vec<&str> = row.cells().iter().enumerate().filter(|(_, c)| c.value().abs() > 1e-4).map(|(t, _)| crate::matrix::TGT_NAMES[t]).collect();
            (!to.is_empty()).then(|| format!("{} → {}", crate::matrix::SRC_NAMES[r], to.join(", ")))
        })
        .collect();
    let motion = match routes.len() {
        0 => "No motion".to_string(),
        1 | 2 => format!("Motion · {}", routes.join(" · ")),
        n => format!("Motion · {} · +{} more", routes[..2].join(" · "), n - 2),
    };
    vec![
        format!("{taps} taps · {spacing}{swing}"),
        format!("{time} · {sync_word} · {mode}{split}"),
        format!("{cross} · {} · Feedback {}", repeats_word(q.repeats.value()), q.feedback),
        colour,
        motion,
        format!("Duck {} · Mix {}", q.duck, q.dry_wet),
    ]
}

/// The header's preset box, centred: ‹ name › with its state under the name.
/// The name opens the sheet; ‹ › load the previous / next preset.
pub(super) fn preset_box(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let pb = r(o, s, (l.w - l.preset_w) * 0.5, g::HEAD_Y, l.preset_w, g::HEAD_H);
    let pu = &mut st.scope.presets;
    pu.sync_session(params, ui.ctx().cumulative_frame_nr());
    let header = pu.session.header(pu.edited);
    let out = shared::preset_box(ui, pb, &header, st.preset_open, ui.id().with("sc_preset"));
    if out.step != 0 {
        pu.step(params, setter, out.step);
    }
    if out.toggle {
        st.preset_open = !st.preset_open;
    }
}

/// The sheet over every card under the header (SOF-243: the exact box round
/// the cards).
pub(super) fn sheet(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let pu = &mut st.scope.presets;
    pu.ensure();
    let Some(lib) = pu.lib.as_mut() else { return };
    let mut cx = Ctx { lib, session: &mut pu.session, params: &**params, setter };
    match std::mem::take(&mut pu.preview) {
        1 => pu.sheet.begin_save_as(&cx),
        k @ (2 | 3) => {
            let list = pu.sheet.list(cx.lib);
            if let Some(first) = list.first() {
                pu.sheet.load(&mut cx, first);
            }
            if let Some(id) = list.get(k as usize - 1) {
                pu.sheet.select(&mut cx, id, k == 2);
            }
            if k == 3 {
                pu.sheet.ask_delete(cx.lib);
            }
        }
        _ => {}
    }
    let top = l.time.0;
    let bottom = l.lane.1 + l.lane.3;
    let area = r(o, s, l.pad, top, l.right_x + l.right_w - l.pad, bottom - top);
    let out = pu.sheet.show(ui, area, ui.id().with("sc_sheet"), &mut cx);
    pu.edited_frame = None;
    if out.close {
        st.preset_open = false;
    }
}

/// Repeats in a word: "4 repeats", or "Loops" for ∞.
fn repeats_word(n: i32) -> String {
    match n {
        n if n >= crate::params::REPEATS_LOOP => "Loops".into(),
        1 => "1 repeat".into(),
        n => format!("{n} repeats"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::tests::{TempDir, TestGui};

    /// The binary's factory bank, user presets in a folder of the test's own.
    fn ui_in(t: &TempDir) -> PresetUi {
        PresetUi { dir: Some((t.0.clone(), None)), ..Default::default() }
    }

    /// The host session names the loaded preset, and a reopened editor
    /// adopts it.
    #[test]
    fn the_session_names_the_preset() {
        let t = TempDir::new("ui-session");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.ensure();
        let lib = pu.lib.as_mut().unwrap();
        pu.sheet.load(&mut Ctx { lib, session: &mut pu.session, params: &p, setter: &setter }, "ping garden");
        pu.sync_session(&p, 1);
        let s = p.preset_session.lock().unwrap().clone();
        assert_eq!((s.id.as_str(), s.kind.as_str(), s.edited), ("ping garden", "factory", false));
        let mut again = ui_in(&t);
        again.sync_session(&p, 1);
        assert_eq!(again.session.header(false).name, "Ping Garden");
    }

    /// ‹ › before the sheet was opened walk every preset A–Z.
    #[test]
    fn step_walks_the_presets() {
        let t = TempDir::new("ui-step");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.step(&p, &setter, 1);
        assert_eq!(pu.session.loaded().unwrap().id, "bloom hall", "A–Z first");
        pu.step(&p, &setter, 1);
        assert_eq!(pu.session.loaded().unwrap().id, "dark repeats");
    }

    /// Closing the sheet while trying keeps what you hear (§13.3).
    #[test]
    fn closing_while_trying_keeps_it() {
        let t = TempDir::new("ui-close");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.track(true);
        let lib = pu.lib.as_mut().unwrap();
        pu.sheet.select(&mut Ctx { lib, session: &mut pu.session, params: &p, setter: &setter }, "ping garden", true);
        assert!(pu.session.is_trying());
        pu.track(false);
        assert!(!pu.session.is_trying());
        assert_eq!(pu.session.loaded().unwrap().id, "ping garden");
        assert_eq!(pu.lib.as_ref().unwrap().state.recent, vec!["ping garden".to_string()]);
    }

    /// The filters Afterecho remembered before v2.49 carry over.
    #[test]
    fn old_filters_carry_over() {
        let t = TempDir::new("ui-facets");
        std::fs::write(t.join("library.json"), r#"{"time":"any","cross":"ping"}"#).unwrap();
        let mut pu = ui_in(&t);
        pu.ensure();
        assert_eq!(pu.lib.as_ref().unwrap().state.facet, "cross=ping");
    }

    #[test]
    fn the_sheet_shows_two_summary_lines() {
        let pg = crate::factory::factory_presets().into_iter().find(|p| p.name == "Ping Garden").unwrap();
        let two = summary(&pg);
        assert_eq!(two.len(), 2);
        assert!(two[0][0].0.starts_with("5 taps") && two[0][0].0.contains("Ping"), "{two:?}");
    }

    #[test]
    fn the_summary_reads_the_preset() {
        let q = PetalParams::default();
        let pg = crate::factory::factory_presets().into_iter().find(|p| p.name == "Ping Garden").unwrap();
        crate::presets::load_into(&q, &pg.values);
        let lines = summary_lines(&q);
        assert_eq!(lines.len(), 6);
        assert!(lines[0].starts_with("5 taps"), "{lines:?}");
        assert!(lines[2].starts_with("Ping"), "{lines:?}");
    }
}
