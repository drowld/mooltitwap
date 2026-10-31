//! The preset browser (the Soft Systems preset spec §6–8, as Vortex built
//! it): the header's preset box, the loaded-preset state behind it (Try,
//! edited, the host session), and the sheet over the field and the lane, so
//! the Time, Colour and Output cards stay in view and move while you browse.
//! The look is the approved mockup (2026-10-31) in the page's grammar:
//! underlined choices, ink fills, hairlines, flat. Every action runs on the
//! GUI thread; a load drives each param as a gesture (`presets::apply`).

use super::*;
use crate::params::{DelayMode, DiffuseRouting};
use crate::ui::theme::FAINT;
use crate::presets::{self, Cross, Kind, Library, Preset, PresetSession, Time, VALUES_FORMAT_PLAIN};
use std::collections::BTreeMap;

const TXT: f32 = 13.0;
const SMALL: f32 = 11.0;
const TITLE: f32 = 14.0;

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

/// The collections (§2), in the column's order: (value, label, the word
/// library.json keeps).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Coll {
    #[default]
    All,
    Favourites,
    Recent,
    Factory,
    User,
}

const COLLS: [(Coll, &str, &str); 5] = [
    (Coll::All, "All", "all"),
    (Coll::Favourites, "Favourites", "favourites"),
    (Coll::Recent, "Recent", "recent"),
    (Coll::Factory, "Factory", "factory"),
    (Coll::User, "User", "user"),
];

/// The list's order (§6.2); Recent is always newest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
    #[default]
    Az,
    FavFirst,
}

/// The loaded preset: what the header names.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Loaded {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) kind: Kind,
    /// Deleted since it was loaded: the sound stays (§7.7).
    pub(super) deleted: bool,
}

/// What the loaded preset sounds like, for the edited check (§7.11).
#[derive(Clone)]
struct Base {
    values: BTreeMap<String, f32>,
    fmt: u32,
}

impl Base {
    fn of(p: &Preset) -> Base {
        Base { values: p.values.clone(), fmt: p.values_format }
    }

    /// Init: every param at its default.
    fn init() -> Base {
        Base { values: BTreeMap::new(), fmt: VALUES_FORMAT_PLAIN }
    }
}

/// The whole state at the first Try (§7.2), for Back.
struct TrySnap {
    values: BTreeMap<String, f32>,
    loaded: Option<Loaded>,
    base: Option<Base>,
    fallback: bool,
}

/// The search field's second job (§7.4–7.5).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum NameEdit {
    SaveAs,
    /// The preset being renamed, by identity.
    Rename(String),
}

/// The sheet's text fields (widget id salts).
const SEARCH: &str = "ps_search";
const NAME: &str = "ps_name";
const TAG: &str = "ps_tag";
const NOTES: &str = "ps_notes";

/// GUI-thread preset state: the library (opened lazily, on first use, so
/// instantiating the plug-in never touches the disk), the loaded preset,
/// Try, and the browser's view.
#[derive(Default)]
pub(crate) struct PresetUi {
    /// The library's folder and an old one to copy from once; None = the
    /// user's own (`Library::open_default`). Tests and previews set their own.
    pub(super) dir: Option<(PathBuf, Option<PathBuf>)>,
    /// The factory set when `dir` is set (tests); None = the binary's.
    pub(super) factory: Option<Vec<Preset>>,
    pub(super) lib: Option<Library>,
    /// None = Init.
    pub(super) loaded: Option<Loaded>,
    base: Option<Base>,
    /// The edited flag when there's nothing to compare with (a session
    /// restored for a preset that's gone).
    fallback: bool,
    /// This frame's edited state (`update`).
    pub(super) edited: bool,
    edited_frame: Option<u64>,
    trying: Option<Box<TrySnap>>,
    /// The host-session field as last written or adopted (§10).
    session: Option<PresetSession>,
    /// The sheet was opened in this editor session: ‹ › follow its list
    /// from then on (all A–Z before).
    pub(super) opened: bool,
    was_open: bool,
    pub(super) query: String,
    pub(super) coll: Coll,
    /// The facets: None = Any.
    pub(super) time: Option<Time>,
    pub(super) cross: Option<Cross>,
    pub(super) tags: Vec<String>,
    pub(super) sort: Sort,
    /// Try (audition): on unless library.json says otherwise.
    pub(super) try_off: bool,
    /// The selected row, by identity.
    pub(super) sel: Option<String>,
    /// The list's first row shown, the wheel's remainder, and the tag
    /// column's first line.
    scroll: usize,
    scroll_px: f32,
    tag_line: usize,
    pub(super) edit: Option<NameEdit>,
    pub(super) name_buf: String,
    pub(super) name_err: String,
    pub(super) confirm_del: Option<String>,
    /// The "+ tag" field, open.
    pub(super) tag_buf: Option<String>,
    /// The notes field's buffer: (preset identity, text).
    notes: Option<(String, String)>,
    /// One line of feedback under the sheet.
    pub(super) status: String,
    /// Give this field focus on the next draw (and select all its text).
    focus: Option<&'static str>,
    select_all: bool,
    /// Preview: once the sheet is open, 1 opens Save as…, 2 tries the
    /// second row, 3 asks to delete the third.
    pub(super) preview: u8,
    /// "In this preset": the selection's identity and its lines, and the
    /// stand-alone params they're read from.
    summary: Option<(String, Vec<String>)>,
    probe: Option<Box<PetalParams>>,
}

/// A path with the home folder as "~", for the status line.
fn home_short(p: &std::path::Path) -> String {
    let full = p.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && full.starts_with(&h) => format!("~{}", &full[h.len()..]),
        _ => full,
    }
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Factory => "factory",
        Kind::User => "user",
    }
}

impl PresetUi {
    /// Open the library on first use, with the browser's remembered view.
    pub(super) fn ensure(&mut self) {
        if self.lib.is_some() {
            return;
        }
        let lib = match &self.dir {
            Some((d, old)) => Library::open(d.clone(), old.clone(), self.factory.clone().unwrap_or_else(crate::factory::factory_presets)),
            None => Library::open_default(),
        };
        let st = &lib.state;
        self.coll = COLLS.iter().find(|c| c.2 == st.collection).map_or(Coll::All, |c| c.0);
        self.time = Time::ALL.into_iter().find(|t| t.word() == st.time);
        self.cross = Cross::ALL.into_iter().find(|c| c.word() == st.cross);
        self.sort = if st.sort == "favourites" { Sort::FavFirst } else { Sort::Az };
        self.try_off = !st.try_on;
        self.lib = Some(lib);
    }

    fn lib(&self) -> Option<&Library> {
        self.lib.as_ref()
    }

    pub(super) fn trying(&self) -> bool {
        self.trying.is_some()
    }

    /// A library write's failure goes to the status line.
    fn report(&mut self, r: Result<(), String>) {
        if let Err(e) = r {
            self.status = e;
        }
    }

    // ---- loading (§7.1, 7.2, 7.10)

    /// Load preset `id`: its params as gestures; it becomes the loaded
    /// preset. `keep`: a kept load (to the top of Recent; it ends a Try, the
    /// same as Keep).
    pub(super) fn load(&mut self, params: &PetalParams, setter: &ParamSetter, id: &str, keep: bool) {
        self.ensure();
        let Some(p) = self.lib().and_then(|l| l.get(id)).cloned() else { return };
        presets::apply(params, setter, &p.values, p.values_format);
        self.loaded = Some(Loaded { id: p.id(), name: p.name.clone(), kind: p.kind(), deleted: false });
        self.base = Some(Base::of(&p));
        self.edited_frame = None;
        self.status = format!("loaded “{}”", p.name);
        if keep {
            self.keep();
        }
    }

    /// Keep (§7.2): the tried preset stays loaded; it goes to Recent.
    pub(super) fn keep(&mut self) {
        self.trying = None;
        if let Some(l) = self.loaded.clone().filter(|l| !l.deleted) {
            let r = self.lib.as_mut().map_or(Ok(()), |lib| lib.push_recent(&l.id));
            self.report(r);
        }
    }

    /// Select and hear (§7.2): the first Try snapshots the whole state.
    pub(super) fn try_preset(&mut self, params: &PetalParams, setter: &ParamSetter, id: &str) {
        if self.trying.is_none() {
            self.trying = Some(Box::new(TrySnap {
                values: presets::capture(params),
                loaded: self.loaded.clone(),
                base: self.base.clone(),
                fallback: self.fallback,
            }));
        }
        self.load(params, setter, id, false);
        if let Some(l) = &self.loaded {
            self.status = format!("trying “{}” — Keep, or close the sheet to keep it; Back returns", l.name);
        }
    }

    /// Back (§7.2): the snapshot restored exactly, the header included.
    pub(super) fn back(&mut self, params: &PetalParams, setter: &ParamSetter) {
        let Some(t) = self.trying.take() else { return };
        presets::apply(params, setter, &t.values, VALUES_FORMAT_PLAIN);
        self.loaded = t.loaded;
        self.base = t.base;
        self.fallback = t.fallback;
        self.edited_frame = None;
        if let Some(l) = &self.loaded {
            self.sel = Some(l.id.clone());
        }
        self.status = format!("back to “{}”", self.loaded.as_ref().map_or("Init", |l| l.name.as_str()));
    }

    /// Init (§7.10): every param at its default.
    pub(super) fn init(&mut self, params: &PetalParams, setter: &ParamSetter) {
        presets::apply(params, setter, &BTreeMap::new(), VALUES_FORMAT_PLAIN);
        self.loaded = None;
        self.base = Some(Base::init());
        self.trying = None;
        self.edited_frame = None;
        self.status = "Init: every control at its default".into();
    }

    /// ‹ › (§6.1): the previous / next preset of the browser's list as
    /// filtered and sorted (all A–Z until the sheet is first opened, or
    /// when the filter leaves nothing), loaded directly.
    pub(super) fn step(&mut self, params: &PetalParams, setter: &ParamSetter, d: i32) {
        self.ensure();
        let mut ids = if self.opened { self.list() } else { Vec::new() };
        if ids.is_empty() {
            ids = self.lib().map(|l| l.presets().map(Preset::id).collect()).unwrap_or_default();
        }
        if ids.is_empty() {
            self.status = "No presets yet — Save as… names this sound".into();
            return;
        }
        let n = ids.len() as i32;
        let cur = self.loaded.as_ref().and_then(|l| ids.iter().position(|i| *i == l.id));
        let next = match cur {
            Some(i) => (i as i32 + d).rem_euclid(n),
            None if d > 0 => 0,
            None => n - 1,
        } as usize;
        let id = ids[next].clone();
        self.load(params, setter, &id, true);
        self.sel = Some(id);
        self.reveal_sel();
    }

    // ---- edited and the host session (§7.11, §10)

    /// This frame's edited state, computed once a frame.
    pub(super) fn update(&mut self, params: &PetalParams, frame: u64) -> bool {
        if self.edited_frame != Some(frame) {
            self.edited = match (&self.base, &self.loaded) {
                (Some(b), _) => !presets::matches(params, &b.values, b.fmt),
                (None, None) => !presets::matches(params, &BTreeMap::new(), VALUES_FORMAT_PLAIN),
                (None, Some(_)) => self.fallback,
            };
            self.edited_frame = Some(frame);
        }
        self.edited
    }

    /// Keep the host-session field and this state in step: a value the host
    /// restored under us (a project opened, an undo) is adopted; ours is
    /// written when it changes. GUI thread; the audio thread never reads it.
    pub(super) fn sync_session(&mut self, params: &PetalParams, frame: u64) {
        let host = params.preset_session.lock().map(|g| g.clone()).unwrap_or_default();
        if self.session.as_ref() != Some(&host) {
            // this editor's first frame adopts only a named preset: an empty
            // field is a fresh instance, whatever this state already says
            if self.session.is_some() || !host.id.is_empty() {
                self.adopt(&host);
            }
            self.session = Some(host);
        }
        let edited = self.update(params, frame);
        let want = match &self.loaded {
            None => PresetSession { edited, ..PresetSession::default() },
            Some(l) => PresetSession { id: l.id.clone(), name: l.name.clone(), kind: kind_word(l.kind).into(), edited },
        };
        if self.session.as_ref() != Some(&want) {
            if let Ok(mut g) = params.preset_session.lock() {
                *g = want.clone();
            }
            self.session = Some(want);
        }
    }

    fn adopt(&mut self, s: &PresetSession) {
        self.trying = None;
        self.edited_frame = None;
        self.fallback = s.edited;
        if s.id.is_empty() {
            self.loaded = None;
            self.base = None;
            return;
        }
        self.ensure();
        let kind = if s.kind == "factory" { Kind::Factory } else { Kind::User };
        match self.lib().and_then(|l| l.get(&s.id)).cloned() {
            Some(p) => {
                self.loaded = Some(Loaded { id: p.id(), name: p.name.clone(), kind: p.kind(), deleted: false });
                self.base = Some(Base::of(&p));
            }
            None => {
                self.loaded = Some(Loaded { id: s.id.clone(), name: s.name.clone(), kind, deleted: true });
                self.base = None;
            }
        }
        self.sel = Some(s.id.clone());
    }

    /// The header's two lines (§6.1): the name, the state under it and its
    /// colour (the accent while trying, coral once deleted).
    pub(super) fn header(&self) -> (String, String, Color32) {
        let ed = if self.edited { " · edited" } else { "" };
        match &self.loaded {
            _ if self.trying.is_some() => {
                (self.loaded.as_ref().map_or("Init".into(), |l| l.name.clone()), "trying · not kept yet".into(), G())
            }
            None => ("Init".into(), format!("Default{ed}"), INK2()),
            Some(l) if l.deleted => (l.name.clone(), "deleted · Save as… to keep".into(), theme::accent(1)),
            Some(l) => (l.name.clone(), format!("{}{ed}", if l.kind == Kind::Factory { "Factory" } else { "User" }), INK2()),
        }
    }

    // ---- the browser's list (§6.2)

    /// The list as filtered and sorted, by identity.
    pub(super) fn list(&self) -> Vec<String> {
        let Some(lib) = self.lib() else { return Vec::new() };
        let q = self.query.trim().to_lowercase();
        let recent = &lib.state.recent;
        let mut v: Vec<&Preset> = lib
            .presets()
            .filter(|p| {
                let id = p.id();
                let in_coll = match self.coll {
                    Coll::All => true,
                    Coll::Favourites => lib.is_favourite(&id),
                    Coll::Recent => recent.contains(&id),
                    Coll::Factory => p.kind() == Kind::Factory,
                    Coll::User => p.kind() == Kind::User,
                };
                in_coll
                    && self.time.is_none_or(|t| p.time() == t)
                    && self.cross.is_none_or(|c| p.cross() == c)
                    && self.tags.iter().all(|t| p.tags.contains(t))
                    && (q.is_empty()
                        || p.name.to_lowercase().contains(&q)
                        || p.tags.iter().any(|t| t.contains(&q))
                        || p.notes.to_lowercase().contains(&q))
            })
            .collect();
        match self.coll {
            Coll::Recent => v.sort_by_key(|p| recent.iter().position(|r| *r == p.id())),
            _ if self.sort == Sort::FavFirst => v.sort_by_key(|p| (!lib.is_favourite(&p.id()), p.name.to_lowercase())),
            _ => {} // the library is A–Z
        }
        v.into_iter().map(Preset::id).collect()
    }

    fn count(&self, c: Coll) -> usize {
        let Some(lib) = self.lib() else { return 0 };
        match c {
            Coll::All => lib.len(),
            Coll::Favourites => lib.presets().filter(|p| lib.is_favourite(&p.id())).count(),
            Coll::Recent => lib.presets().filter(|p| lib.state.recent.contains(&p.id())).count(),
            Coll::Factory => lib.presets().filter(|p| p.kind() == Kind::Factory).count(),
            Coll::User => lib.presets().filter(|p| p.kind() == Kind::User).count(),
        }
    }

    /// Scroll the list so the selection shows (on the next draw).
    fn reveal_sel(&mut self) {
        self.scroll = usize::MAX;
    }

    // ---- the sheet opening and closing

    /// The sheet opened: rescan the folder (§11), select the loaded preset.
    pub(super) fn opened(&mut self) {
        self.ensure();
        if let Some(lib) = self.lib.as_mut() {
            lib.rescan();
        }
        self.opened = true;
        if let Some(l) = self.loaded.as_mut() {
            l.deleted = self.lib.as_ref().is_none_or(|lib| lib.get(&l.id).is_none());
        }
        self.sel = self.loaded.as_ref().filter(|l| !l.deleted).map(|l| l.id.clone()).or_else(|| self.list().first().cloned());
        self.edit = None;
        self.confirm_del = None;
        self.tag_buf = None;
        self.notes = None;
        self.summary = None;
        self.reveal_sel();
    }

    /// The sheet closed (§7.2): keeps what you hear; pending notes are saved.
    pub(super) fn closed(&mut self) {
        if self.trying.is_some() {
            self.keep();
        }
        self.commit_notes();
        self.edit = None;
        self.confirm_del = None;
        self.tag_buf = None;
    }

    /// Track the sheet opening / closing however it happened (its ×, the
    /// header, a test).
    pub(super) fn track(&mut self, open: bool) {
        if open && !self.was_open {
            self.opened();
        } else if !open && self.was_open {
            self.closed();
        }
        self.was_open = open;
    }

    // ---- actions (§7)

    fn selected(&self) -> Option<Preset> {
        let id = self.sel.as_ref()?;
        self.lib()?.get(id).cloned()
    }

    /// Why Save is greyed, or None when it isn't (§7.3: a user preset,
    /// edited).
    pub(super) fn save_blocked(&self) -> Option<String> {
        match &self.loaded {
            None => Some("Init isn't a preset — Save as… names this sound".into()),
            Some(l) if l.deleted => Some(format!("“{}” was deleted — Save as… keeps this sound", l.name)),
            Some(l) if l.kind == Kind::Factory => Some("Save writes over a user preset — factory presets are read-only, use Save as…".into()),
            Some(_) if self.trying.is_some() => Some("Keep or Back first — this sound is only being tried".into()),
            Some(l) if !self.edited => Some(format!("“{}” is unchanged — nothing to save", l.name)),
            Some(_) => None,
        }
    }

    pub(super) fn save(&mut self, params: &PetalParams) {
        if let Some(why) = self.save_blocked() {
            self.status = why;
            return;
        }
        let Some(l) = self.loaded.clone() else { return };
        let Some(lib) = self.lib.as_mut() else { return };
        match lib.save(&l.id, presets::capture(params)) {
            Ok(()) => {
                self.base = lib.get(&l.id).map(Base::of);
                self.edited_frame = None;
                self.summary = None;
                self.keep();
                self.status = format!("saved “{}”", l.name);
            }
            Err(e) => self.status = e,
        }
    }

    /// Save as… (§7.4): the search field becomes the name field.
    pub(super) fn begin_save_as(&mut self) {
        let buf = match (&self.loaded, self.lib()) {
            (Some(l), Some(lib)) if l.kind == Kind::User && !l.deleted => lib.free_name(&format!("{} copy", l.name)),
            _ => String::new(),
        };
        self.edit = Some(NameEdit::SaveAs);
        self.name_buf = buf;
        self.name_err.clear();
        self.confirm_del = None;
        self.tag_buf = None;
        self.focus = Some(NAME);
        self.select_all = false;
    }

    /// Rename (§7.5): user presets only; the field opens fully selected.
    pub(super) fn begin_rename(&mut self) {
        let Some(p) = self.selected() else { return };
        if p.kind() == Kind::Factory {
            self.status = presets::READ_ONLY.into();
            return;
        }
        self.edit = Some(NameEdit::Rename(p.id()));
        self.name_buf = p.name.clone();
        self.name_err.clear();
        self.confirm_del = None;
        self.tag_buf = None;
        self.focus = Some(NAME);
        self.select_all = true;
    }

    /// Enter in the name field: save as, or rename. A refusal stays in the
    /// field with its reason (§7.4).
    pub(super) fn commit_name(&mut self, params: &PetalParams) {
        let Some(edit) = self.edit.clone() else { return };
        let from = self.loaded.as_ref().filter(|l| !l.deleted).map(|l| l.id.clone());
        let Some(lib) = self.lib.as_mut() else { return };
        match edit {
            NameEdit::SaveAs => match lib.save_as(&self.name_buf, presets::capture(params), from.as_deref()) {
                Ok(id) => {
                    let p = lib.get(&id).cloned().expect("just saved");
                    self.loaded = Some(Loaded { id: id.clone(), name: p.name.clone(), kind: Kind::User, deleted: false });
                    self.base = Some(Base::of(&p));
                    self.edited_frame = None;
                    self.trying = None;
                    self.keep();
                    self.sel = Some(id);
                    self.reveal_sel();
                    self.edit = None;
                    self.status = format!("saved “{}” to User", p.name);
                }
                Err(e) => {
                    self.name_err = e;
                    self.focus = Some(NAME);
                }
            },
            NameEdit::Rename(old) => match lib.rename(&old, &self.name_buf) {
                Ok(new) => {
                    let name = lib.get(&new).map(|p| p.name.clone()).unwrap_or_default();
                    let follow = |l: &mut Option<Loaded>| {
                        if let Some(l) = l.as_mut().filter(|l| l.id == old) {
                            l.id = new.clone();
                            l.name = name.clone();
                        }
                    };
                    follow(&mut self.loaded);
                    if let Some(t) = self.trying.as_mut() {
                        follow(&mut t.loaded);
                    }
                    if let Some(n) = self.notes.as_mut().filter(|n| n.0 == old) {
                        n.0 = new.clone();
                    }
                    self.sel = Some(new);
                    self.summary = None;
                    self.reveal_sel();
                    self.edit = None;
                    self.status = format!("renamed to “{name}”");
                }
                Err(e) => {
                    self.name_err = e;
                    self.focus = Some(NAME);
                }
            },
        }
    }

    pub(super) fn cancel_edit(&mut self) {
        self.edit = None;
        self.name_err.clear();
    }

    /// Duplicate (§7.6): a user copy of the selection, then Rename on it.
    pub(super) fn duplicate(&mut self) {
        let Some(id) = self.sel.clone() else { return };
        let Some(lib) = self.lib.as_mut() else { return };
        match lib.duplicate(&id) {
            Ok(name) => {
                self.sel = Some(presets::identity(&name));
                self.reveal_sel();
                self.begin_rename();
                self.status = format!("made “{name}” in User — name it");
            }
            Err(e) => self.status = e,
        }
    }

    /// Delete (§7.7): the first ask only arms the confirm line.
    pub(super) fn ask_delete(&mut self) {
        let Some(p) = self.selected() else { return };
        if p.kind() == Kind::Factory {
            self.status = presets::READ_ONLY.into();
            return;
        }
        self.confirm_del = Some(p.id());
        self.edit = None;
        self.tag_buf = None;
    }

    /// The confirmed delete: the file goes, the current sound never changes.
    pub(super) fn delete(&mut self) {
        let Some(id) = self.confirm_del.take() else { return };
        let before = self.list();
        let name = self.lib().and_then(|l| l.get(&id)).map(|p| p.name.clone()).unwrap_or_default();
        let Some(lib) = self.lib.as_mut() else { return };
        match lib.delete(&id) {
            Ok(()) => {
                for l in [self.loaded.as_mut(), self.trying.as_mut().and_then(|t| t.loaded.as_mut())].into_iter().flatten() {
                    if l.id == id {
                        l.deleted = true;
                    }
                }
                // the row after it is selected, else the one before
                let at = before.iter().position(|x| *x == id).unwrap_or(0);
                let after = self.list();
                self.sel = after.get(at.min(after.len().saturating_sub(1))).cloned();
                self.notes = None;
                self.status = format!("deleted “{name}” — the current sound is unchanged");
            }
            Err(e) => self.status = e,
        }
    }

    pub(super) fn toggle_favourite(&mut self, id: &str) {
        let Some(lib) = self.lib.as_mut() else { return };
        let on = !lib.is_favourite(id);
        let r = lib.set_favourite(id, on);
        self.report(r);
    }

    /// Remember the browser's view in library.json (§4).
    fn remember(&mut self) {
        let coll = COLLS.iter().find(|c| c.0 == self.coll).map_or("all", |c| c.2).to_string();
        let time = self.time.map_or("any", Time::word).to_string();
        let cross = self.cross.map_or("any", Cross::word).to_string();
        let sort = if self.sort == Sort::FavFirst { "favourites" } else { "az" }.to_string();
        let try_on = !self.try_off;
        let Some(lib) = self.lib.as_mut() else { return };
        let r = lib.update_state(|s| {
            s.collection = coll;
            s.time = time;
            s.cross = cross;
            s.sort = sort;
            s.try_on = try_on;
        });
        self.report(r);
    }

    pub(super) fn set_coll(&mut self, c: Coll) {
        self.coll = c;
        self.scroll = 0;
        self.remember();
    }

    pub(super) fn set_time(&mut self, t: Option<Time>) {
        self.time = t;
        self.scroll = 0;
        self.remember();
    }

    pub(super) fn set_cross(&mut self, c: Option<Cross>) {
        self.cross = c;
        self.scroll = 0;
        self.remember();
    }

    pub(super) fn set_sort(&mut self, s: Sort) {
        self.sort = s;
        self.remember();
    }

    pub(super) fn set_try(&mut self, on: bool) {
        self.try_off = !on;
        self.remember();
    }

    pub(super) fn toggle_tag(&mut self, t: &str) {
        if let Some(i) = self.tags.iter().position(|x| x == t) {
            self.tags.remove(i);
        } else {
            self.tags.push(t.to_string());
        }
        self.scroll = 0;
    }

    /// Select row `id`, hearing it when Try is on (unless it's the loaded
    /// one already).
    pub(super) fn select(&mut self, params: &PetalParams, setter: &ParamSetter, id: &str, hear: bool) {
        if self.sel.as_deref() != Some(id) {
            self.commit_notes();
            self.confirm_del = None;
            self.tag_buf = None;
        }
        self.sel = Some(id.to_string());
        let loaded = self.loaded.as_ref().is_some_and(|l| l.id == id && !l.deleted);
        if hear && !self.try_off && !loaded {
            self.try_preset(params, setter, id);
        }
    }

    /// Enter / double-click (§7.1): load the selection, or keep it when
    /// it's the one being tried.
    pub(super) fn load_selected(&mut self, params: &PetalParams, setter: &ParamSetter) {
        let Some(id) = self.sel.clone() else { return };
        if self.trying.is_some() && self.loaded.as_ref().is_some_and(|l| l.id == id) {
            self.keep();
            self.status = format!("kept “{}”", self.loaded.as_ref().map_or("", |l| l.name.as_str()));
        } else {
            self.load(params, setter, &id, true);
        }
    }

    fn add_tag(&mut self) {
        let (Some(id), Some(t)) = (self.sel.clone(), self.tag_buf.clone()) else { return };
        if t.trim().is_empty() {
            return;
        }
        let Some(lib) = self.lib.as_mut() else { return };
        let r = lib.add_tag(&id, &t);
        if r.is_ok() {
            self.tag_buf = Some(String::new());
            self.focus = Some(TAG);
        }
        self.report(r);
    }

    fn remove_tag(&mut self, t: &str) {
        let Some(id) = self.sel.clone() else { return };
        let Some(lib) = self.lib.as_mut() else { return };
        let r = lib.remove_tag(&id, t);
        // a filter on a tag no preset has any more goes
        let left: std::collections::HashSet<String> = lib.presets().flat_map(|p| p.tags.iter().cloned()).collect();
        self.tags.retain(|x| left.contains(x));
        self.report(r);
    }

    /// Save the notes field when it changed.
    fn commit_notes(&mut self) {
        let Some((id, text)) = self.notes.take() else { return };
        let Some(lib) = self.lib.as_mut() else { return };
        if lib.get(&id).is_some_and(|p| p.notes != text && p.kind() == Kind::User) {
            let r = lib.set_notes(&id, &text);
            self.report(r);
        }
    }

    /// Import these files as user presets (§13.6: drag and drop).
    pub(super) fn import(&mut self, paths: &[PathBuf]) {
        self.ensure();
        let Some(lib) = self.lib.as_mut() else { return };
        let mut done = Vec::new();
        let mut err = None;
        for p in paths.iter().filter(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("json"))) {
            match lib.import(p) {
                Ok(name) => done.push(name),
                Err(e) => err = Some(e),
            }
        }
        if let Some(last) = done.last() {
            self.sel = Some(presets::identity(last));
            self.reveal_sel();
        }
        self.status = match (done.len(), err) {
            (_, Some(e)) => e,
            (0, None) => "Only .json preset files can be imported".into(),
            (1, None) => format!("imported “{}” to User", done[0]),
            (n, None) => format!("imported {n} presets to User"),
        };
    }

    /// Export… (§13.6): the selection into the Exports folder beside the
    /// presets ("<name>.json", then "<name> 2.json"…); the status line says
    /// where. No system file dialog (Vortex: the rfd crate broke the plug-in
    /// binary, DECISIONS 2026-10-30).
    pub(super) fn export(&mut self) {
        let Some(p) = self.selected() else { return };
        let Some(lib) = self.lib.as_ref() else { return };
        let dir = lib.dir.parent().map_or_else(|| lib.dir.join("Exports"), |d| d.join("Exports"));
        let _ = std::fs::create_dir_all(&dir);
        let stem = presets::sanitize(&p.name);
        let mut path = dir.join(format!("{stem}.json"));
        let mut k = 2;
        while path.exists() {
            path = dir.join(format!("{stem} {k}.json"));
            k += 1;
        }
        self.status = match lib.export(&p.id(), &path) {
            Ok(()) => format!("exported “{}” to {}", p.name, home_short(&path)),
            Err(e) => e,
        };
    }

    /// "In this preset" for the selection: what it sets, in the page's words.
    fn summary_of(&mut self, p: &Preset) -> Vec<String> {
        if let Some((id, lines)) = &self.summary {
            if *id == p.id() {
                return lines.clone();
            }
        }
        let probe = self.probe.get_or_insert_with(|| Box::new(PetalParams::default()));
        presets::load_into(probe, &p.values, p.values_format);
        let lines = summary_lines(probe);
        self.summary = Some((p.id(), lines.clone()));
        lines
    }
}

/// Six short lines: taps, time, the loop, its colour, Motion, the output.
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

// ─────────────────────────────────────────────────────────────────────────────
// The header's preset box (§6.1)
// ─────────────────────────────────────────────────────────────────────────────

/// The preset box, centred: ‹ name › with its state under the name. The
/// name opens the sheet (the box is outlined in ink while it's open); ‹ ›
/// load the previous / next preset of the browser's list.
pub(super) fn preset_box(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let (y, h) = (g::HEAD_Y, g::HEAD_H);
    let pb = r(o, s, (l.w - l.preset_w) * 0.5, y, l.preset_w, h);
    let open = st.preset_open;
    if open {
        ui.painter().rect_stroke(pb, g::R_CTL * s, Stroke::new(1.5 * s, INK()), egui::StrokeKind::Inside);
    } else {
        outline(ui.painter(), pb, false, s);
    }
    let prev = Rect::from_min_size(pb.min, Vec2::new(38.0 * s, pb.height()));
    let next = Rect::from_min_size(Pos2::new(pb.right() - 38.0 * s, pb.top()), Vec2::new(38.0 * s, pb.height()));
    let mid = Rect::from_min_max(Pos2::new(prev.right(), pb.top()), Pos2::new(next.left(), pb.bottom()));
    let rp = ui.interact(prev, ui.id().with("sc_prev"), Sense::click());
    let rn = ui.interact(next, ui.id().with("sc_next"), Sense::click());
    let rm = ui.interact(mid, ui.id().with("sc_preset"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    for (rect, glyph, resp) in [(prev, "‹", &rp), (next, "›", &rn)] {
        text(ui.painter(), rect.center(), Align2::CENTER_CENTER, glyph, 18.0, s, if resp.hovered() { INK() } else { INK2() });
    }
    let pu = &mut st.scope.presets;
    pu.sync_session(params, ui.ctx().cumulative_frame_nr());
    let (name, sub, sub_col) = pu.header();
    let name = elide(ui.painter(), &name, FontId::proportional(fs(14.0, s)), mid.width() - 8.0 * s);
    text(ui.painter(), mid.center() - Vec2::new(0.0, 7.0 * s), Align2::CENTER_CENTER, &name, 14.0, s, INK());
    let sub = elide(ui.painter(), &sub, FontId::proportional(fs(SMALL, s)), mid.width() - 8.0 * s);
    text(ui.painter(), mid.center() + Vec2::new(0.0, 9.0 * s), Align2::CENTER_CENTER, &sub, SMALL, s, sub_col);
    if epo::tap_or_click(ui, &rp) {
        pu.step(params, setter, -1);
    }
    if epo::tap_or_click(ui, &rn) {
        pu.step(params, setter, 1);
    }
    if epo::tap_or_click(ui, &rm) {
        st.preset_open = !open;
    }
}

/// `t` cut with "…" to fit `max_w` (px).
fn elide(p: &egui::Painter, t: &str, font: FontId, max_w: f32) -> String {
    let w = |s: &str| p.layout_no_wrap(s.to_string(), font.clone(), INK()).size().x;
    if w(t) <= max_w {
        return t.to_string();
    }
    let mut c: Vec<char> = t.chars().collect();
    while !c.is_empty() {
        c.pop();
        let s: String = c.iter().collect::<String>().trim_end().to_string() + "…";
        if w(&s) <= max_w {
            return s;
        }
    }
    "…".into()
}

// ─────────────────────────────────────────────────────────────────────────────
// The sheet (§6.2–6.3)
// ─────────────────────────────────────────────────────────────────────────────

/// The inspector band's height on Tall: name, "In this preset" (two
/// columns), tags, notes, then Init / Save / Import… / Export….
const INSP_TALL: f32 = 232.0;

/// Where everything sits (logical px): the sheet covers the field and the
/// lane. Wide: the left column, the list, the inspector to its right. Tall:
/// the inspector under the list, and Init / Save move into it (§6.2).
struct Geo {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tall: bool,
    lx: f32,
    right: f32,
    col_w: f32,
    top: f32,
    list_x: f32,
    list_w: f32,
    list_top: f32,
    row_h: f32,
    /// Rows fit above this (the "more" line under them).
    rows_bot: f32,
    /// The inspector: its column on Wide, its band under the list on Tall.
    ix: f32,
    iy: f32,
    iw: f32,
    foot_y: f32,
}

impl Geo {
    fn new(l: &Layout, tall: bool, finger: bool) -> Geo {
        let (x, y, w) = (l.picture.0, l.picture.1, l.picture.2);
        let h = l.lane.1 + l.lane.3 - y;
        let (lx, right) = (x + 12.0, x + w - 12.0);
        let col_w = if tall { 150.0 } else { 180.0 };
        let top = y + 64.0;
        let list_x = lx + col_w + 22.0;
        let foot_y = y + h - 46.0;
        let (ix, iy, iw, list_w, rows_bot) = if tall {
            let iy = foot_y - 16.0 - INSP_TALL;
            (list_x, iy, right - list_x, right - list_x, iy - 10.0)
        } else {
            let iw = 208.0;
            (right - iw, top, iw, right - iw - 14.0 - list_x, foot_y - 14.0)
        };
        Geo {
            x,
            y,
            w,
            h,
            tall,
            lx,
            right,
            col_w,
            top,
            list_x,
            list_w,
            list_top: top + 18.0,
            row_h: if finger { 44.0 } else { 30.0 },
            rows_bot,
            ix,
            iy,
            iw,
            foot_y,
        }
    }

    /// Rows that fit with the "N more" line under them.
    fn max_rows(&self) -> usize {
        (((self.rows_bot - 16.0 - self.list_top) / self.row_h).floor() as usize).max(1)
    }
}

/// A button's look.
#[derive(Clone, Copy, PartialEq)]
enum Btn {
    Plain,
    /// Ink-filled: Keep, Save in the name field.
    Primary,
    /// Coral: the confirmed delete.
    Danger,
    /// Greyed; a tap still says why.
    Greyed,
}

/// A sheet button, the page's: a hairline outline, ink-filled when primary.
/// On iPad its hit area is at least 44 pt tall. True when tapped.
fn btn(ui: &mut egui::Ui, rect: Rect, label: &str, salt: impl std::hash::Hash, s: f32, look: Btn) -> bool {
    let hit = if theme::touch() { rect.expand2(Vec2::new(0.0, ((44.0 * s - rect.height()) * 0.5).max(0.0))) } else { rect };
    let resp = ui.interact(hit, ui.id().with(("ps_btn", salt)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let col = match look {
        Btn::Primary => {
            p.rect_filled(rect, g::R_CTL * s, INK());
            PAPER()
        }
        Btn::Danger => {
            p.rect_filled(rect, g::R_CTL * s, theme::accent(1));
            PAPER()
        }
        Btn::Plain | Btn::Greyed => {
            outline(p, rect, false, s);
            match look {
                Btn::Greyed => theme::with_alpha(SOFT(), 120),
                _ if resp.hovered() => INK(),
                _ => INK2(),
            }
        }
    };
    if label == "×" {
        paint_cross(p, rect.center(), 5.0 * s, col, s);
    } else {
        text(p, rect.center(), Align2::CENTER_CENTER, label, TXT, s, col);
    }
    epo::tap_or_click(ui, &resp)
}

/// A single-line text field in `rect` (no frame: the caller paints it).
#[allow(clippy::too_many_arguments)]
fn field(ui: &mut egui::Ui, rect: Rect, salt: &str, buf: &mut String, hint: &str, px: f32, s: f32, limit: usize) -> egui::Response {
    let te = egui::TextEdit::singleline(buf)
        .id(ui.id().with(salt))
        .frame(egui::Frame::NONE)
        .hint_text(RichText::new(hint).color(SOFT()))
        .font(FontId::proportional(fs(px, s)))
        .text_color(INK())
        .desired_width(rect.width())
        .char_limit(limit);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| ui.add(te)).inner
}

/// Give field `salt` focus now (and select all of its `len` characters).
fn focus_field(ui: &egui::Ui, salt: &str, select_all: Option<usize>) {
    let id = ui.id().with(salt);
    ui.memory_mut(|m| m.request_focus(id));
    if let Some(n) = select_all {
        let mut st = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
        st.store(ui.ctx(), id);
    }
}

/// A tag chip's width (logical px).
fn chip_w(ui: &egui::Ui, t: &str, s: f32) -> f32 {
    ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(fs(SMALL, s)), INK2()).size().x / s + 16.0
}

/// A chip: a hairline pill, ink-filled when on.
fn chip(p: &egui::Painter, r: Rect, t: &str, on: bool, hover: bool, s: f32) {
    if on {
        p.rect_filled(r, g::R_CTL * s, INK());
    } else {
        p.rect_stroke(r, g::R_CTL * s, Stroke::new(1.0 * s, theme::line2()), egui::StrokeKind::Inside);
    }
    let col = if on { PAPER() } else if hover { INK() } else { INK2() };
    text(p, r.center(), Align2::CENTER_CENTER, t, SMALL, s, col);
}

/// A 5-point star: filled amber for a favourite, a hairline otherwise.
fn paint_star(p: &egui::Painter, c: Pos2, rad: f32, filled: bool, col: Color32, s: f32) {
    use std::f32::consts::PI;
    let mut pts = [Pos2::ZERO; 10];
    for (k, pt) in pts.iter_mut().enumerate() {
        let rr = if k % 2 == 0 { rad } else { rad * 0.42 };
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

/// The keys this frame (§8), read before any text field takes them.
#[derive(Default)]
struct Keys {
    up: bool,
    down: bool,
    enter: bool,
    esc: bool,
    save: bool,
    save_as: bool,
    rename: bool,
    delete: bool,
    fav: bool,
    /// Printable text typed this frame (not F's "f").
    typed: bool,
}

impl Keys {
    fn read(ui: &egui::Ui) -> Keys {
        ui.input(|i| {
            let pressed = |k: egui::Key, want: fn(egui::Modifiers) -> bool| {
                i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == k && want(*modifiers)))
            };
            let bare = |m: egui::Modifiers| !m.command && !m.ctrl && !m.alt && !m.mac_cmd;
            let any = |_: egui::Modifiers| true;
            let fav = pressed(egui::Key::F, |m| !m.any());
            Keys {
                up: pressed(egui::Key::ArrowUp, bare),
                down: pressed(egui::Key::ArrowDown, bare),
                enter: pressed(egui::Key::Enter, any),
                esc: pressed(egui::Key::Escape, any),
                save: pressed(egui::Key::S, |m| m.command && !m.shift),
                save_as: pressed(egui::Key::S, |m| m.command && m.shift),
                rename: pressed(egui::Key::F2, any),
                delete: pressed(egui::Key::Backspace, |m| !m.any()) || pressed(egui::Key::Delete, |m| !m.any()),
                fav,
                typed: !fav && i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t.chars().any(|c| !c.is_whitespace() && !c.is_control()))),
            }
        })
    }
}

/// Take `key`'s presses out of this frame's input: handled here, a text
/// field must not act on them too.
fn consume(ui: &egui::Ui, key: egui::Key) {
    ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Key { key: k, .. } if *k == key)));
}

/// What a draw needs from the page.
struct Cx<'a> {
    o: Pos2,
    s: f32,
    params: &'a Arc<PetalParams>,
    setter: &'a ParamSetter<'a>,
}

impl Cx<'_> {
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        r(self.o, self.s, x, y, w, h)
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.o + Vec2::new(x, y) * self.s
    }
}

/// The preset sheet over the field and the lane. `open` turns false when
/// it closes itself (×, Esc).
pub(super) fn sheet(ui: &mut egui::Ui, o: Pos2, s: f32, l: &Layout, tall: bool, params: &Arc<PetalParams>, setter: &ParamSetter, st: &mut EditorState) {
    let mut pu = std::mem::take(&mut st.scope.presets);
    pu.ensure();
    match std::mem::take(&mut pu.preview) {
        1 => pu.begin_save_as(),
        k @ (2 | 3) => {
            let list = pu.list();
            if let Some(first) = list.first() {
                pu.load(params, setter, first, true);
            }
            if let Some(id) = list.get(k as usize - 1) {
                pu.select(params, setter, id, k == 2);
            }
            if k == 3 {
                pu.ask_delete();
            }
        }
        _ => {}
    }
    let cx = Cx { o, s, params, setter };
    let mut open = true;
    draw(ui, &cx, l, tall, &mut pu, &mut open);
    st.scope.presets = pu;
    if !open {
        st.preset_open = false;
    }
}

fn draw(ui: &mut egui::Ui, cx: &Cx, l: &Layout, tall: bool, pu: &mut PresetUi, open: &mut bool) {
    let s = cx.s;
    let (params, setter) = (cx.params.as_ref(), cx.setter);
    let geo = Geo::new(l, tall, theme::touch());
    let card_r = cx.r(geo.x, geo.y, geo.w, geo.h);
    let p = ui.painter().clone();
    p.rect_filled(card_r, g::R_CARD * s, PANEL());
    p.rect_stroke(card_r, g::R_CARD * s, Stroke::new(1.0 * s, theme::line2()), egui::StrokeKind::Inside);
    // the card swallows clicks that miss its controls
    ui.interact(card_r, ui.id().with("ps_card"), Sense::click());

    // ---- keys (§8)
    let focused = ui.memory(|m| m.focused());
    let is = |salt: &str| focused == Some(ui.id().with(salt));
    let in_search = is(SEARCH);
    let in_field = is(NAME) || is(TAG) || is(NOTES);
    let k = Keys::read(ui);
    if k.enter {
        consume(ui, egui::Key::Enter);
    }
    if k.up || k.down {
        consume(ui, egui::Key::ArrowUp);
        consume(ui, egui::Key::ArrowDown);
    }
    let list = pu.list();
    // the selection is a row of the list: when a filter drops it, the
    // first row takes its place (without trying it); none when empty
    if pu.sel.as_ref().is_none_or(|id| !list.contains(id)) && !matches!(pu.edit, Some(NameEdit::Rename(_))) {
        pu.sel = list.first().cloned();
        pu.confirm_del = None;
        pu.tag_buf = None;
    }
    if pu.edit.is_some() {
        if k.enter {
            pu.commit_name(params);
        } else if k.esc {
            pu.cancel_edit();
        }
    } else if is(TAG) {
        if k.enter {
            pu.add_tag();
        } else if k.esc {
            pu.tag_buf = None;
        }
    } else if is(NOTES) {
        if k.enter {
            pu.commit_notes();
            ui.memory_mut(|m| m.surrender_focus(ui.id().with(NOTES)));
        } else if k.esc {
            pu.notes = None; // back to what's saved
        }
    } else if pu.confirm_del.is_some() && k.esc {
        pu.confirm_del = None;
    } else if !in_field {
        let cur = pu.sel.as_ref().and_then(|id| list.iter().position(|x| x == id));
        let moved = match (k.up, k.down, cur) {
            (true, _, Some(i)) => Some(i.saturating_sub(1)),
            (true, _, None) | (_, true, None) => (!list.is_empty()).then_some(0),
            (_, true, Some(i)) => Some((i + 1).min(list.len().saturating_sub(1))),
            _ => None,
        };
        if let Some(i) = moved.filter(|&i| Some(i) != cur) {
            let id = list[i].clone();
            pu.select(params, setter, &id, true);
            pu.reveal_sel();
        }
        if k.enter {
            pu.load_selected(params, setter);
        }
        if k.esc {
            if pu.trying() {
                pu.back(params, setter);
            } else {
                *open = false;
            }
        }
        if k.save_as {
            pu.begin_save_as();
        } else if k.save {
            pu.save(params);
        }
        if k.rename {
            pu.begin_rename();
        }
        if !in_search {
            if k.delete {
                pu.ask_delete();
            }
            if k.fav {
                if let Some(id) = pu.sel.clone() {
                    pu.toggle_favourite(&id);
                }
            }
            if k.typed && pu.edit.is_none() {
                pu.focus = Some(SEARCH);
            }
        }
    }

    top_row(ui, cx, &geo, pu, open);
    left_column(ui, cx, &geo, pu);

    // ---- the list
    let (list_x, list_w, top) = (geo.list_x, geo.list_w, geo.top);
    p.line_segment([cx.at(list_x - 11.0, top - 4.0), cx.at(list_x - 11.0, geo.foot_y - 14.0)], Stroke::new(1.0 * s, theme::line()));
    let n = list.len();
    text(&p, cx.at(list_x, top + 6.0), Align2::LEFT_CENTER, &format!("{n} preset{}", if n == 1 { "" } else { "s" }), SMALL, s, INK2());
    if pu.coll != Coll::Recent {
        let cur = if pu.sort == Sort::Az { 0 } else { 1 };
        if let Some(i) = seg(ui, cx.r(list_x + list_w - 150.0, top - 4.0, 150.0, 20.0), &["A–Z", "Favourites first"], cur, "ps_sort", s, SMALL, true) {
            pu.set_sort(if i == 0 { Sort::Az } else { Sort::FavFirst });
        }
    }
    list_rows(ui, cx, &geo, pu, &list);
    inspector(ui, cx, &geo, pu);
    footer(ui, cx, &geo, pu);

    // ---- drag and drop (§13.6): .json files dropped on the sheet import
    let hovered = ui.input(|i| !i.raw.hovered_files.is_empty());
    let dropped: Vec<PathBuf> = ui.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
    if hovered {
        let r = cx.r(list_x, geo.list_top, list_w, geo.rows_bot - geo.list_top);
        p.rect_filled(r, g::R_CTL * s, PANEL());
        p.rect_stroke(r, g::R_CTL * s, Stroke::new(1.0 * s, INK2()), egui::StrokeKind::Inside);
        text(&p, r.center(), Align2::CENTER_CENTER, "Drop .json presets to import them to User", TXT, s, INK());
    }
    if !dropped.is_empty() {
        pu.import(&dropped);
    }

    // ---- the status line, under the sheet
    if !pu.status.is_empty() {
        let msg = elide(&p, &pu.status, FontId::proportional(fs(SMALL, s)), (geo.w - 24.0) * s);
        text(&p, cx.at(geo.x + geo.w - 12.0, geo.y + geo.h + 8.0), Align2::RIGHT_CENTER, &msg, SMALL, s, SOFT());
    }
}

/// The top row: the search field (or the name field) and Init, Save,
/// Save as…, × (Tall keeps Save as… and ×; Init and Save are in the
/// inspector).
fn top_row(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi, open: &mut bool) {
    let s = cx.s;
    let p = ui.painter().clone();
    let (lx, right, by) = (geo.lx, geo.right, geo.y + 12.0);
    let bh = 32.0;
    let close = cx.r(right - 32.0, by, 32.0, bh);
    if let Some(edit) = pu.edit.clone() {
        let cancel_x = right - 32.0 - 8.0 - 70.0;
        let save_x = cancel_x - 8.0 - 64.0;
        let fr = cx.r(lx, by, save_x - 10.0 - lx, bh);
        p.rect_filled(fr, g::R_CTL * s, INSET());
        let err = !pu.name_err.is_empty();
        p.rect_stroke(fr, g::R_CTL * s, Stroke::new(1.0 * s, if err { theme::accent(1) } else { INK2() }), egui::StrokeKind::Inside);
        let word = if edit == NameEdit::SaveAs { "Save as" } else { "Rename" };
        text(&p, cx.at(lx + 12.0, by + bh * 0.5), Align2::LEFT_CENTER, word, SMALL, s, SOFT());
        if pu.focus == Some(NAME) {
            focus_field(ui, NAME, pu.select_all.then(|| pu.name_buf.chars().count()));
            pu.focus = None;
            pu.select_all = false;
        }
        let before = pu.name_buf.clone();
        field(ui, cx.r(lx + 70.0, by, fr.width() / s - 78.0, bh), NAME, &mut pu.name_buf, "Name this preset", TITLE, s, presets::NAME_MAX);
        if pu.name_buf != before {
            pu.name_err.clear();
        }
        let what = if edit == NameEdit::SaveAs { "name_save" } else { "name_rename" };
        if btn(ui, cx.r(save_x, by, 64.0, bh), "Save", what, s, Btn::Primary) {
            pu.commit_name(cx.params);
        }
        if btn(ui, cx.r(cancel_x, by, 70.0, bh), "Cancel", "name_cancel", s, Btn::Plain) {
            pu.cancel_edit();
        }
        if err {
            text(&p, cx.at(lx, by + bh + 9.0), Align2::LEFT_CENTER, &pu.name_err, SMALL, s, theme::accent(1));
        }
    } else {
        let sa_x = right - 32.0 - 8.0 - 86.0;
        let fw = if geo.tall { sa_x - 10.0 - lx } else { sa_x - 8.0 - 56.0 - 8.0 - 56.0 - 12.0 - lx };
        let fr = cx.r(lx, by, fw, bh);
        p.rect_filled(fr, g::R_CTL * s, INSET());
        let c = cx.at(lx + 16.0, by + bh * 0.5 - 1.0);
        p.circle_stroke(c, 5.0 * s, Stroke::new(1.5 * s, SOFT()));
        p.line_segment([c + Vec2::splat(3.6 * s), c + Vec2::splat(7.6 * s)], Stroke::new(1.5 * s, SOFT()));
        if pu.focus == Some(SEARCH) {
            focus_field(ui, SEARCH, None);
            pu.focus = None;
        }
        let before = pu.query.clone();
        field(ui, cx.r(lx + 30.0, by, fw - 38.0, bh), SEARCH, &mut pu.query, "Search names, tags and notes", TXT, s, 64);
        if pu.query != before {
            pu.scroll = 0;
        }
        if btn(ui, cx.r(sa_x, by, 86.0, bh), "Save as…", "save_as", s, Btn::Plain) {
            pu.begin_save_as();
        }
        if !geo.tall {
            init_save(ui, cx, pu, sa_x - 8.0 - 56.0 - 8.0 - 56.0, by);
        }
    }
    if btn(ui, close, "×", "close", s, Btn::Plain) {
        *open = false;
    }
}

/// Init and Save: the top row on Wide, the inspector's last line on Tall.
fn init_save(ui: &mut egui::Ui, cx: &Cx, pu: &mut PresetUi, x: f32, y: f32) {
    let s = cx.s;
    if btn(ui, cx.r(x, y, 56.0, 32.0), "Init", "init", s, Btn::Plain) {
        pu.init(cx.params, cx.setter);
    }
    let blocked = pu.save_blocked().is_some();
    if btn(ui, cx.r(x + 64.0, y, 56.0, 32.0), "Save", "save", s, if blocked { Btn::Greyed } else { Btn::Plain }) {
        pu.save(cx.params);
    }
}

/// The left column: Collections with counts, the Time and Cross facets, and
/// the tags that some preset has.
fn left_column(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi) {
    let s = cx.s;
    let p = ui.painter().clone();
    let (lx, col_w, top) = (geo.lx, geo.col_w, geo.top);
    group_head(&p, cx.at(lx, top + 6.0), Align2::LEFT_CENTER, "Collections", s);
    let rh = if theme::touch() { 30.0 } else { 25.0 };
    for (k, &(c, label, _)) in COLLS.iter().enumerate() {
        let rr = cx.r(lx, top + 16.0 + k as f32 * rh, col_w, rh);
        let resp = ui.interact(rr, ui.id().with(("ps_coll", k)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = pu.coll == c;
        if on {
            p.rect_filled(Rect::from_min_max(Pos2::new(rr.left(), rr.top() + 4.0 * s), Pos2::new(rr.left() + 2.0 * s, rr.bottom() - 4.0 * s)), 0.0, INK());
        }
        let col = if on { INK() } else if resp.hovered() { INK2() } else { SOFT() };
        text(&p, Pos2::new(rr.left() + 12.0 * s, rr.center().y), Align2::LEFT_CENTER, label, TXT, s, col);
        mono(&p, rr.right_center(), Align2::RIGHT_CENTER, &pu.count(c).to_string(), SMALL, s, SOFT());
        if epo::tap_or_click(ui, &resp) && !on {
            pu.set_coll(c);
        }
    }
    p.line_segment([cx.at(lx, top + 16.0), cx.at(lx, top + 16.0 + 5.0 * rh)], Stroke::new(1.0 * s, theme::line()));

    let ty = top + 16.0 + 5.0 * rh + 18.0;
    group_head(&p, cx.at(lx, ty), Align2::LEFT_CENTER, "Time", s);
    let cur = match pu.time {
        None => 0,
        Some(Time::Sync) => 1,
        Some(Time::Free) => 2,
    };
    if let Some(i) = seg(ui, cx.r(lx, ty + 8.0, col_w, 28.0), &["Any", "Sync", "Free"], cur, "ps_time", s, 12.0, true) {
        pu.set_time([None, Some(Time::Sync), Some(Time::Free)][i]);
    }
    let cy = ty + 52.0;
    group_head(&p, cx.at(lx, cy), Align2::LEFT_CENTER, "Cross", s);
    let cur = match pu.cross {
        None => 0,
        Some(Cross::Mix) => 1,
        Some(Cross::Ping) => 2,
        Some(Cross::Diffuse) => 3,
    };
    let px = if geo.tall { 11.0 } else { 12.0 };
    if let Some(i) = seg(ui, cx.r(lx, cy + 8.0, col_w, 28.0), &["Any", "Mix", "Ping", "Diffuse"], cur, "ps_cross", s, px, true) {
        pu.set_cross([None, Some(Cross::Mix), Some(Cross::Ping), Some(Cross::Diffuse)][i]);
    }
    let tags_y = cy + 52.0;
    group_head(&p, cx.at(lx, tags_y), Align2::LEFT_CENTER, "Tags", s);
    if !pu.tags.is_empty() {
        let cr = cx.r(lx + col_w - 44.0, tags_y - 9.0, 44.0, 18.0);
        let resp = ui.interact(cr, ui.id().with("ps_tags_clear"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        text(&p, cr.right_center(), Align2::RIGHT_CENTER, "Clear", SMALL, s, if resp.hovered() { INK() } else { INK2() });
        if epo::tap_or_click(ui, &resp) {
            pu.tags.clear();
            pu.scroll = 0;
        }
    }
    tag_filter(ui, cx, geo, pu, tags_y + 12.0);
}

/// The tag chips (only tags some preset has, A–Z), lines of 28 px down to
/// the column's end. Several narrow the list (AND). When they don't all fit,
/// the last place says "+N", which shows the next line.
fn tag_filter(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi, y0: f32) {
    let s = cx.s;
    let p = ui.painter().clone();
    let mut all: Vec<String> = pu.lib().map(|l| l.presets().flat_map(|p| p.tags.iter().cloned()).collect()).unwrap_or_default();
    all.sort();
    all.dedup();
    if all.is_empty() {
        text(&p, cx.at(geo.lx, y0 + 11.0), Align2::LEFT_CENTER, "No tags yet", SMALL, s, FAINT());
        return;
    }
    let (lx, col_w) = (geo.lx, geo.col_w);
    let mut lines: Vec<Vec<(String, f32, f32)>> = vec![Vec::new()];
    let mut x = lx;
    for t in &all {
        let w = chip_w(ui, t, s);
        if x + w > lx + col_w && x > lx {
            lines.push(Vec::new());
            x = lx;
        }
        lines.last_mut().unwrap().push((t.clone(), x, w));
        x += w + 6.0;
    }
    let fit = (((geo.foot_y - 14.0 - y0 + 6.0) / 28.0).floor() as usize).max(1);
    if pu.tag_line >= lines.len() {
        pu.tag_line = 0;
    }
    let more = lines.len() > fit;
    let shown = &lines[pu.tag_line..(pu.tag_line + fit).min(lines.len())];
    let hidden = all.len() - shown.iter().map(Vec::len).sum::<usize>();
    let mut toggle = None;
    for (li, line) in shown.iter().enumerate() {
        let y = y0 + li as f32 * 28.0;
        let last = li + 1 == shown.len();
        for (t, x, w) in line {
            if last && more && x + w > lx + col_w - 34.0 {
                break;
            }
            let rr = cx.r(*x, y, *w, 22.0);
            let resp = ui.interact(rr, ui.id().with(("ps_tagf", t)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            chip(&p, rr, t, pu.tags.contains(t), resp.hovered(), s);
            if epo::tap_or_click(ui, &resp) {
                toggle = Some(t.clone());
            }
        }
    }
    if more {
        let y = y0 + (shown.len() - 1) as f32 * 28.0;
        let rr = cx.r(lx + col_w - 30.0, y, 30.0, 22.0);
        let resp = ui.interact(rr, ui.id().with("ps_tags_more"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        text(&p, rr.center(), Align2::CENTER_CENTER, &format!("+{}", hidden.max(1)), SMALL, s, if resp.hovered() { INK() } else { INK2() });
        if epo::tap_or_click(ui, &resp) {
            pu.tag_line = if pu.tag_line + fit >= lines.len() { 0 } else { pu.tag_line + 1 };
        }
    }
    if let Some(t) = toggle {
        pu.toggle_tag(&t);
    }
}

/// The rows: a star, the name, Time · Cross (· User), the tags
/// right-aligned; the loaded one with a 3 px accent bar, the selected one in
/// a well.
fn list_rows(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi, list: &[String]) {
    let s = cx.s;
    let (params, setter) = (cx.params.as_ref(), cx.setter);
    let p = ui.painter().clone();
    let (lx, lw, rh) = (geo.list_x, geo.list_w, geo.row_h);
    let max_rows = geo.max_rows();
    // the unreadable files close the All list (§11): all of it, unfiltered
    let all = pu.coll == Coll::All && pu.time.is_none() && pu.cross.is_none() && pu.tags.is_empty() && pu.query.trim().is_empty();
    let bad: Vec<presets::Unreadable> = if all { pu.lib().map(|l| l.unreadable.clone()).unwrap_or_default() } else { Vec::new() };
    let total = list.len() + bad.len();
    // scrolling: the wheel over the list, a finger dragging a row, and the
    // selection kept in view
    let area = cx.r(lx, geo.list_top, lw, geo.rows_bot - geo.list_top);
    if ui.rect_contains_pointer(area) {
        pu.scroll_px -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    let max_scroll = total.saturating_sub(max_rows);
    if pu.scroll == usize::MAX {
        let at = pu.sel.as_ref().and_then(|id| list.iter().position(|x| x == id));
        pu.scroll = match at {
            Some(i) => i.saturating_sub(max_rows / 2),
            None => 0,
        };
    }
    let step = rh * s;
    while pu.scroll_px >= step {
        pu.scroll += 1;
        pu.scroll_px -= step;
    }
    while pu.scroll_px <= -step {
        pu.scroll = pu.scroll.saturating_sub(1);
        pu.scroll_px += step;
    }
    if let Some(i) = pu.sel.as_ref().and_then(|id| list.iter().position(|x| x == id)) {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowDown)) {
            if i < pu.scroll {
                pu.scroll = i;
            } else if i >= pu.scroll + max_rows {
                pu.scroll = i + 1 - max_rows;
            }
        }
    }
    pu.scroll = pu.scroll.min(max_scroll);
    // empty states (§6.2)
    let mid = lx + lw * 0.5;
    if pu.coll == Coll::User && pu.count(Coll::User) == 0 {
        text(&p, cx.at(mid, geo.list_top + 38.0), Align2::CENTER_CENTER, "No user presets yet.", TITLE, s, INK2());
        text(&p, cx.at(mid, geo.list_top + 60.0), Align2::CENTER_CENTER, "Save as… stores the current sound here.", TXT, s, SOFT());
    } else if pu.coll == Coll::Factory && pu.count(Coll::Factory) == 0 {
        text(&p, cx.at(mid, geo.list_top + 38.0), Align2::CENTER_CENTER, "No factory presets in this build yet.", TXT, s, SOFT());
    } else if list.is_empty() && pu.lib().is_some_and(|l| l.len() == 0) {
        text(&p, cx.at(mid, geo.list_top + 38.0), Align2::CENTER_CENTER, "No presets yet.", TITLE, s, INK2());
        text(&p, cx.at(mid, geo.list_top + 60.0), Align2::CENTER_CENTER, "Save as… stores the current sound.", TXT, s, SOFT());
    } else if list.is_empty() {
        text(&p, cx.at(mid, geo.list_top + 38.0), Align2::CENTER_CENTER, "Nothing matches — clear a tag or the search.", TXT, s, SOFT());
    }
    let Some(lib) = pu.lib.as_ref() else { return };
    let loaded = pu.loaded.as_ref().filter(|l| !l.deleted).map(|l| l.id.clone());
    let name_w = if geo.tall { lw - 210.0 } else { lw - 216.0 };
    let tags_w = 80.0;
    let mut act: Option<(String, u8)> = None; // 0 select+try, 2 load, 3 star
    let mut drag = 0.0;
    for (k, idx) in (pu.scroll..total.min(pu.scroll + max_rows)).enumerate() {
        let y = geo.list_top + k as f32 * rh;
        let row = cx.r(lx, y, lw, rh - 2.0);
        let cy = row.center().y;
        if idx >= list.len() {
            let u = &bad[idx - list.len()];
            let what = match &u.name {
                Some(n) if !n.trim().is_empty() => format!("Couldn't read “{}” ({})", n.trim(), u.file),
                _ => format!("Couldn't read {}", u.file),
            };
            let what = elide(&p, &what, FontId::proportional(fs(TXT, s)), (lw - 40.0 - tags_w) * s);
            text(&p, Pos2::new(row.left() + 30.0 * s, cy), Align2::LEFT_CENTER, &what, TXT, s, SOFT());
            let why = elide(&p, &u.reason, FontId::proportional(fs(SMALL, s)), tags_w * s);
            text(&p, Pos2::new(row.right() - 6.0 * s, cy), Align2::RIGHT_CENTER, &why, SMALL, s, FAINT());
            continue;
        }
        let id = &list[idx];
        let Some(pr) = lib.get(id) else { continue };
        let resp = ui.interact(row, ui.id().with(("ps_row", id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = pu.sel.as_deref() == Some(id.as_str());
        let is_loaded = loaded.as_deref() == Some(id.as_str());
        if on {
            p.rect_filled(row, g::R_CTL * s, INSET());
        }
        let fav = lib.is_favourite(id);
        let sc = Pos2::new(row.left() + 16.0 * s, cy);
        let star_hit = if theme::touch() {
            Rect::from_center_size(Pos2::new(row.left() + 22.0 * s, cy), Vec2::new(44.0, rh - 2.0) * s)
        } else {
            Rect::from_center_size(sc, Vec2::splat(22.0 * s))
        };
        let sresp = ui.interact(star_hit, ui.id().with(("ps_star", id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let star_col = if fav { theme::accent(3) } else if sresp.hovered() { INK2() } else { FAINT() };
        paint_star(&p, sc, 6.0 * s, fav, star_col, s);
        if is_loaded {
            p.rect_filled(Rect::from_min_size(Pos2::new(row.left(), row.top() + 6.0 * s), Vec2::new(3.0, rh - 16.0) * s), 1.5 * s, G());
        }
        let name = elide(&p, &pr.name, FontId::proportional(fs(TXT, s)), name_w * s);
        let ink = if on || is_loaded || resp.hovered() { INK() } else { INK2() };
        text(&p, Pos2::new(row.left() + 32.0 * s, cy), Align2::LEFT_CENTER, &name, TXT, s, ink);
        let user = pr.kind() == Kind::User;
        let word = format!("{} · {}{}", pr.time().label(), pr.cross().label(), if user { " · User" } else { "" });
        let wr = text(&p, Pos2::new(row.left() + (38.0 + name_w) * s, cy), Align2::LEFT_CENTER, &word, SMALL, s, SOFT());
        // the tags take what the words leave
        let room = row.right() - 8.0 * s - (wr.right() + 12.0 * s);
        if room > 24.0 * s {
            let tags = elide(&p, &pr.tags.join(", "), FontId::proportional(fs(SMALL, s)), room);
            text(&p, Pos2::new(row.right() - 8.0 * s, cy), Align2::RIGHT_CENTER, &tags, SMALL, s, SOFT());
        }
        if epo::tap_or_click(ui, &sresp) {
            act = Some((id.clone(), 3));
        } else if theme::touch() {
            let t = epo::touch_interact(ui, resp.id, row);
            drag += t.delta.y;
            if resp.double_clicked() {
                act = Some((id.clone(), 2));
            } else if t.tapped {
                act = Some((id.clone(), 0));
            }
        } else if resp.double_clicked() {
            act = Some((id.clone(), 2));
        } else if resp.clicked() {
            act = Some((id.clone(), 0));
        }
    }
    if drag != 0.0 {
        pu.scroll_px -= drag;
    }
    if total > max_rows {
        let left = total - (pu.scroll + max_rows).min(total);
        let msg = if left > 0 { format!("{left} more — scroll") } else { format!("{} above — scroll", pu.scroll) };
        text(&p, cx.at(lx + lw * 0.5, geo.rows_bot - 8.0), Align2::CENTER_CENTER, &msg, SMALL, s, FAINT());
    }
    match act {
        Some((id, 3)) => pu.toggle_favourite(&id),
        Some((id, 2)) => {
            pu.select(params, setter, &id, false);
            pu.load_selected(params, setter);
        }
        Some((id, _)) => pu.select(params, setter, &id, true),
        None => {}
    }
}

/// The inspector for the selection: its name and kind, "In this preset",
/// its tags (a user preset's with × and "+ tag"), its notes, Import… and
/// Export… (and Init and Save on Tall). Wide: a column right of the list.
/// Tall: a band under it.
fn inspector(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi) {
    let s = cx.s;
    let p = ui.painter().clone();
    let (ix, iw) = (geo.ix, geo.iw);
    let mut y = geo.iy;
    if geo.tall {
        p.line_segment([cx.at(ix, y - 6.0), cx.at(ix + iw, y - 6.0)], Stroke::new(1.0 * s, theme::line()));
    }
    // Import… / Export… at the column's foot (Wide), the band's last line
    // with Init and Save (Tall)
    let has = pu.selected().is_some();
    let (io_x, io_y) = if geo.tall { (ix + 128.0 + 16.0, geo.iy + INSP_TALL - 32.0) } else { (ix, geo.foot_y - 14.0 - 8.0 - 32.0) };
    if geo.tall {
        init_save(ui, cx, pu, ix, io_y);
    }
    if btn(ui, cx.r(io_x, io_y, 96.0, 32.0), "Import…", "import", s, Btn::Plain) {
        pu.status = "To import, drop .json presets onto this browser".into();
    }
    if btn(ui, cx.r(io_x + 104.0, io_y, 96.0, 32.0), "Export…", "export", s, if has { Btn::Plain } else { Btn::Greyed }) && has {
        pu.export();
    }
    let Some(sel) = pu.selected() else {
        text(&p, cx.at(ix, y + 8.0), Align2::LEFT_CENTER, "Nothing selected", TXT, s, SOFT());
        return;
    };
    let user = sel.kind() == Kind::User;
    let id = sel.id();
    // name and kind
    let name = elide(&p, &sel.name, FontId::proportional(fs(15.0, s)), iw * s);
    text(&p, cx.at(ix, y + 8.0), Align2::LEFT_CENTER, &name, 15.0, s, INK());
    let is_loaded = pu.loaded.as_ref().is_some_and(|l| l.id == id && !l.deleted);
    let kind = match (user, is_loaded && pu.edited) {
        (false, _) => "Factory · read-only".to_string(),
        (true, true) => "User · edited".to_string(),
        (true, false) => "User".to_string(),
    };
    let fav = if pu.lib().is_some_and(|l| l.is_favourite(&id)) { " · favourite" } else { "" };
    text(&p, cx.at(ix, y + 27.0), Align2::LEFT_CENTER, &format!("{kind}{fav}"), SMALL, s, if user { INK2() } else { SOFT() });
    y += 48.0;
    // In this preset
    group_head(&p, cx.at(ix, y), Align2::LEFT_CENTER, "In this preset", s);
    let lines = pu.summary_of(&sel);
    if geo.tall {
        let half = (iw - 12.0) * 0.5;
        for (i, t) in lines.iter().enumerate() {
            let (col, row) = (i / 3, i % 3);
            let t = elide(&p, t, FontId::proportional(fs(12.0, s)), half * s);
            text(&p, cx.at(ix + col as f32 * (half + 12.0), y + 18.0 + row as f32 * 18.0), Align2::LEFT_CENTER, &t, 12.0, s, INK2());
        }
        y += 18.0 + 3.0 * 18.0 + 6.0;
    } else {
        for (i, t) in lines.iter().enumerate() {
            let t = elide(&p, t, FontId::proportional(fs(12.0, s)), iw * s);
            text(&p, cx.at(ix, y + 18.0 + i as f32 * 19.0), Align2::LEFT_CENTER, &t, 12.0, s, INK2());
        }
        y += 18.0 + lines.len() as f32 * 19.0 + 12.0;
    }
    // tags: chips (with × on a user preset), then "+ tag"
    let (tags_x, tags_w) = if geo.tall { (ix + 44.0, iw - 44.0) } else { (ix, iw) };
    if geo.tall {
        group_head(&p, cx.at(ix, y + 11.0), Align2::LEFT_CENTER, "Tags", s);
    } else {
        group_head(&p, cx.at(ix, y), Align2::LEFT_CENTER, "Tags", s);
        y += 10.0;
    }
    let tags_end = tags_x + tags_w;
    let mut x = tags_x;
    let add_w = 56.0;
    let mut remove = None;
    let room = tags_end - if user { add_w + 6.0 } else { 0.0 };
    for (i, t) in sel.tags.iter().enumerate() {
        let w = chip_w(ui, t, s) + if user { 14.0 } else { 0.0 };
        if x + w > room {
            text(&p, cx.at(x, y + 11.0), Align2::LEFT_CENTER, &format!("+{}", sel.tags.len() - i), SMALL, s, INK2());
            x += 26.0;
            break;
        }
        let rr = cx.r(x, y, w, 22.0);
        p.rect_stroke(rr, g::R_CTL * s, Stroke::new(1.0 * s, theme::line2()), egui::StrokeKind::Inside);
        let tx = if user { rr.center().x - 7.0 * s } else { rr.center().x };
        text(&p, Pos2::new(tx, rr.center().y), Align2::CENTER_CENTER, t, SMALL, s, INK2());
        if user {
            let xc = Pos2::new(rr.right() - 11.0 * s, rr.center().y);
            let xr = Rect::from_center_size(xc, Vec2::splat(if theme::touch() { 30.0 } else { 18.0 } * s));
            let resp = ui.interact(xr, ui.id().with(("ps_untag", t)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            paint_cross(&p, xc, 3.0 * s, if resp.hovered() { INK() } else { SOFT() }, s);
            if epo::tap_or_click(ui, &resp) {
                remove = Some(t.clone());
            }
        }
        x += w + 6.0;
    }
    if user {
        if let Some(buf) = pu.tag_buf.as_mut() {
            let rr = cx.r(x, y, (tags_end - x).clamp(60.0, 110.0), 22.0);
            p.rect_filled(rr, g::R_CTL * s, INSET());
            p.rect_stroke(rr, g::R_CTL * s, Stroke::new(1.0 * s, INK2()), egui::StrokeKind::Inside);
            if pu.focus == Some(TAG) {
                focus_field(ui, TAG, None);
                pu.focus = None;
            }
            field(ui, rr.shrink2(Vec2::new(8.0 * s, 0.0)), TAG, buf, "tag", SMALL, s, presets::TAG_LEN_MAX);
        } else if sel.tags.len() < presets::TAGS_MAX && x + add_w <= tags_end + 0.5 {
            let rr = cx.r(x, y, add_w, 22.0);
            let resp = ui.interact(rr, ui.id().with("ps_addtag"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            p.rect_stroke(rr, g::R_CTL * s, Stroke::new(1.0 * s, theme::line()), egui::StrokeKind::Inside);
            text(&p, rr.center(), Align2::CENTER_CENTER, "+ tag", SMALL, s, if resp.hovered() { INK() } else { SOFT() });
            if epo::tap_or_click(ui, &resp) {
                pu.tag_buf = Some(String::new());
                pu.focus = Some(TAG);
            }
        }
    } else if sel.tags.is_empty() {
        text(&p, cx.at(tags_x, y + 11.0), Align2::LEFT_CENTER, "No tags", SMALL, s, FAINT());
    } else if !geo.tall {
        text(&p, cx.at(ix, y + 34.0), Align2::LEFT_CENTER, "Factory tags are fixed", SMALL, s, FAINT());
    }
    if let Some(t) = remove {
        pu.remove_tag(&t);
    }
    y += if geo.tall { 30.0 } else { 52.0 };
    // notes: one line, saved on Enter or when the field loses focus
    let (nx, nw) = if geo.tall { (ix + 44.0, iw - 44.0) } else { (ix, iw) };
    if geo.tall {
        group_head(&p, cx.at(ix, y + 14.0), Align2::LEFT_CENTER, "Notes", s);
    } else {
        group_head(&p, cx.at(ix, y), Align2::LEFT_CENTER, "Notes", s);
        y += 10.0;
    }
    let nr = cx.r(nx, y, nw, 28.0);
    if user {
        p.rect_filled(nr, g::R_CTL * s, INSET());
        if pu.notes.as_ref().is_none_or(|n| n.0 != id) {
            pu.commit_notes();
            pu.notes = Some((id.clone(), sel.notes.clone()));
        }
        let buf = &mut pu.notes.as_mut().expect("set above").1;
        let resp = field(ui, nr.shrink2(Vec2::new(10.0 * s, 0.0)), NOTES, buf, "Notes", 12.0, s, 200);
        if resp.lost_focus() {
            pu.commit_notes();
        }
    } else {
        let t = if sel.notes.is_empty() { "No notes".to_string() } else { sel.notes.clone() };
        let t = elide(&p, &t, FontId::proportional(fs(12.0, s)), nr.width() - 4.0 * s);
        text(&p, nr.left_center(), Align2::LEFT_CENTER, &t, 12.0, s, if sel.notes.is_empty() { FAINT() } else { INK2() });
    }
}

/// The footer: Try on/off; while trying Keep and Back ("back returns to
/// <name>"); then the selection's Duplicate, Rename, Delete — greyed on a
/// factory preset (a tap says why). Delete turns it into the confirm line.
fn footer(ui: &mut egui::Ui, cx: &Cx, geo: &Geo, pu: &mut PresetUi) {
    let s = cx.s;
    let (params, setter) = (cx.params.as_ref(), cx.setter);
    let p = ui.painter().clone();
    let (lx, right, fy) = (geo.lx, geo.right, geo.foot_y);
    p.line_segment([cx.at(lx, fy - 10.0), cx.at(right, fy - 10.0)], Stroke::new(1.0 * s, theme::line()));
    if let Some(id) = pu.confirm_del.clone() {
        let name = pu.lib().and_then(|l| l.get(&id)).map(|p| p.name.clone()).unwrap_or_default();
        let msg = elide(&p, &format!("Delete “{name}”? This removes its file."), FontId::proportional(fs(TXT, s)), (right - 170.0 - lx) * s);
        text(&p, cx.at(lx, fy + 16.0), Align2::LEFT_CENTER, &msg, TXT, s, INK());
        if btn(ui, cx.r(right - 76.0, fy, 76.0, 32.0), "Delete", "delete_yes", s, Btn::Danger) {
            pu.delete();
        }
        if btn(ui, cx.r(right - 76.0 - 8.0 - 74.0, fy, 74.0, 32.0), "Cancel", "delete_no", s, Btn::Plain) {
            pu.confirm_del = None;
        }
        return;
    }
    // Try: on / off
    text(&p, cx.at(lx, fy + 16.0), Align2::LEFT_CENTER, "Try", SMALL, s, SOFT());
    if let Some(i) = seg(ui, cx.r(lx + 26.0, fy + 2.0, 64.0, 28.0), &["on", "off"], if pu.try_off { 1 } else { 0 }, "ps_try", s, 12.0, true) {
        pu.set_try(i == 0);
    }
    let after = lx + 104.0;
    if pu.trying() {
        if btn(ui, cx.r(after, fy, 64.0, 32.0), "Keep", "keep", s, Btn::Primary) {
            pu.keep();
            pu.status = format!("kept “{}”", pu.loaded.as_ref().map_or("Init", |l| l.name.as_str()));
        }
        if btn(ui, cx.r(after + 72.0, fy, 64.0, 32.0), "Back", "back", s, Btn::Plain) {
            pu.back(params, setter);
        }
        if !geo.tall {
            let to = pu.trying.as_ref().and_then(|t| t.loaded.as_ref()).map_or("Init".to_string(), |l| l.name.clone());
            let msg = elide(&p, &format!("back returns to {to}"), FontId::proportional(fs(SMALL, s)), 200.0 * s);
            text(&p, cx.at(after + 148.0, fy + 16.0), Align2::LEFT_CENTER, &msg, SMALL, s, SOFT());
        }
    } else if !geo.tall {
        let hint = if pu.try_off { "Double-click a preset to load it" } else { "Select to hear · double-click keeps" };
        text(&p, cx.at(after, fy + 16.0), Align2::LEFT_CENTER, hint, SMALL, s, SOFT());
    }
    // the selection's actions, from the right
    let Some(sel) = pu.selected() else { return };
    let ro = sel.kind() == Kind::Factory;
    let look = if ro { Btn::Greyed } else { Btn::Plain };
    let mut x = right - 68.0;
    if btn(ui, cx.r(x, fy, 68.0, 32.0), "Delete", "delete", s, look) {
        pu.ask_delete();
    }
    x -= 8.0 + 72.0;
    if btn(ui, cx.r(x, fy, 72.0, 32.0), "Rename", "rename", s, look) {
        pu.begin_rename();
    }
    x -= 8.0 + 86.0;
    if btn(ui, cx.r(x, fy, 86.0, 32.0), "Duplicate", "duplicate", s, Btn::Plain) {
        pu.duplicate();
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
    use crate::presets::tests::{test_factory, TempDir, TestGui};

    fn ui_in(t: &TempDir) -> PresetUi {
        PresetUi { dir: Some((t.0.clone(), None)), factory: Some(test_factory()), ..Default::default() }
    }

    /// Try snapshots the whole state, edits included; Back restores it
    /// exactly, the header too.
    #[test]
    fn try_then_back_restores_the_exact_state() {
        let t = TempDir::new("ui-try");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.track(true);
        pu.load(&p, &setter, "dark repeats", true);
        setter.set_parameter(&p.dry_wet, 0.123);
        let before = presets::capture(&p);
        assert!(pu.update(&p, 1), "edited");
        pu.select(&p, &setter, "ping garden", true);
        assert!(pu.trying());
        assert_eq!(pu.header().1, "trying · not kept yet");
        assert_ne!(presets::capture(&p), before);
        pu.back(&p, &setter);
        assert_eq!(presets::capture(&p), before);
        assert_eq!(pu.loaded.as_ref().unwrap().id, "dark repeats");
        assert!(pu.update(&p, 2));
        assert_eq!(pu.header().1, "Factory · edited");
    }

    /// Closing the sheet while trying keeps what you hear (§13.3).
    #[test]
    fn closing_while_trying_keeps_it() {
        let t = TempDir::new("ui-close");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.track(true);
        pu.select(&p, &setter, "ping garden", true);
        pu.track(false);
        assert!(!pu.trying());
        assert_eq!(pu.loaded.as_ref().unwrap().id, "ping garden");
        assert_eq!(pu.lib.as_ref().unwrap().state.recent, vec!["ping garden".to_string()]);
    }

    /// Deleting the loaded preset never changes the sound; the header says
    /// it's gone until something else is loaded or saved.
    #[test]
    fn delete_keeps_the_sound() {
        let t = TempDir::new("ui-delete");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.track(true);
        setter.set_parameter(&p.feedback, 0.66);
        pu.begin_save_as();
        pu.name_buf = "Mine".into();
        pu.commit_name(&p);
        assert_eq!(pu.header(), ("Mine".to_string(), "User".to_string(), INK2()));
        let sound = presets::capture(&p);
        pu.sel = Some("mine".into());
        pu.ask_delete();
        assert!(t.join("Mine.json").exists(), "nothing removed on the first ask");
        pu.delete();
        assert!(!t.join("Mine.json").exists());
        assert_eq!(presets::capture(&p), sound);
        assert_eq!(pu.header().1, "deleted · Save as… to keep");
        assert!(pu.save_blocked().is_some());
    }

    /// Save is only for an edited user preset; otherwise it says why.
    #[test]
    fn save_is_greyed_with_its_reason() {
        let t = TempDir::new("ui-save");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.track(true);
        assert!(pu.save_blocked().unwrap().contains("Init"));
        pu.load(&p, &setter, "ping garden", true);
        assert!(pu.save_blocked().unwrap().contains("read-only"));
        pu.begin_save_as();
        assert_eq!(pu.name_buf, "", "a factory preset's Save as starts empty");
        pu.name_buf = "Mine".into();
        pu.commit_name(&p);
        pu.update(&p, 1);
        assert!(pu.save_blocked().unwrap().contains("unchanged"));
        setter.set_parameter(&p.feedback, 0.31);
        pu.update(&p, 2);
        assert_eq!(pu.save_blocked(), None);
        pu.save(&p);
        assert!(!pu.update(&p, 3));
        pu.begin_save_as();
        assert_eq!(pu.name_buf, "Mine copy");
    }

    /// ‹ › follow the browser's list once it was opened, filters included.
    #[test]
    fn step_follows_the_filtered_list() {
        let t = TempDir::new("ui-step");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.step(&p, &setter, 1);
        assert_eq!(pu.loaded.as_ref().unwrap().id, "dark repeats", "A–Z first");
        pu.step(&p, &setter, 1);
        assert_eq!(pu.loaded.as_ref().unwrap().id, "ping garden");
        pu.track(true);
        pu.tags = vec!["rhythmic".into()];
        pu.step(&p, &setter, 1);
        assert_eq!(pu.loaded.as_ref().unwrap().id, "ping garden", "only one in the filtered list");
    }

    /// The host session names the loaded preset, and a reopened editor
    /// adopts it.
    #[test]
    fn the_session_names_the_preset() {
        let t = TempDir::new("ui-session");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut pu = ui_in(&t);
        pu.load(&p, &setter, "ping garden", true);
        pu.sync_session(&p, 1);
        let s = p.preset_session.lock().unwrap().clone();
        assert_eq!((s.id.as_str(), s.kind.as_str(), s.edited), ("ping garden", "factory", false));
        let mut again = ui_in(&t);
        again.sync_session(&p, 1);
        assert_eq!(again.header().0, "Ping Garden");
    }

    #[test]
    fn the_summary_reads_the_preset() {
        let t = TempDir::new("ui-summary");
        let mut pu = ui_in(&t);
        pu.ensure();
        let pg = pu.lib().unwrap().get("ping garden").unwrap().clone();
        let lines = pu.summary_of(&pg);
        assert_eq!(lines.len(), 6);
        assert!(lines[0].starts_with("5 taps"), "{lines:?}");
        assert!(lines[2].starts_with("Ping"), "{lines:?}");
    }
}
