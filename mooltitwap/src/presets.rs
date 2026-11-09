//! The preset system (Vortex's `docs/PRESET_SYSTEM.md`, the Soft Systems
//! spec) on the shared store, `softsystems-presets` in nice-plug: the preset
//! file, the library (factory presets served from the binary, user presets on
//! disk, `library.json`) and the rules every operation keeps live there,
//! written once for every Soft Systems plug-in. This module is Afterecho's
//! half: which params a preset holds, its facets, its folder and its
//! factory bank.
//!
//! Afterecho has no blobs: everything a preset holds is a parameter (§3.3).
//! Its facets are Time (Sync | Free) and Cross (Mix | Ping | Diffuse), both
//! derived from the values when saving (§3.4).
//!
//! INVARIANT: every function here runs on the GUI (editor) thread only. The
//! audio `process()` callback must never touch this module.
//!
//! Key rule: discrete params (`step_count().is_some()`) are stored by PLAIN
//! value, floats by NORMALIZED value — a normalized enum index silently remaps
//! when a variant is appended. `values_format` tags the format. The floats in
//! `PLAIN_FLOATS` are stored by plain value too, because their range grew.

use crate::params::PetalParams;
use nice_plug::prelude::*;
use softsystems_presets::{FacetSpec, Facets, Plugin, Sound, Values};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub use softsystems_presets::{Kind, PresetRef as PresetSession, Sent, VALUES_FORMAT_PLAIN};

/// The CLAP id (`lib.rs`; kept from the Mooltitwap days so sessions recall
/// the plug-in): a file from another plug-in is refused.
pub const PLUGIN_ID: &str = "com.bussien.mooltitwap";

/// Floats whose range grew past 0..1 after presets were already saved
/// (`feedback_time`, v2.30): stored by PLAIN value. Before the change their
/// normalized value equalled the plain one, so every preset, old or new, reads
/// back exactly.
const PLAIN_FLOATS: &[&str] = &["feedback_time"];

/// Performance switches a preset or A/B leaves as they are (v2.35): loading
/// one mustn't let go of a held loop, nor flipping A/B start one.
const NOT_STORED: &[&str] = &["freeze", "ear_guard"];

/// Afterecho, as the shared store sees it.
pub struct Store;

pub type Preset = softsystems_presets::Preset<()>;
pub type Library = softsystems_presets::Library<Store>;

impl Plugin for Store {
    type Params = PetalParams;
    type Blobs = ();
    const ID: &'static str = PLUGIN_ID;
    const FOLDER: &'static str = "Afterecho";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const FACTORY_VERSION: u32 = crate::factory::FACTORY_VERSION;
    const FACETS: &'static [FacetSpec] = &[
        FacetSpec { name: "time", label: "Time", values: &[("sync", "Sync"), ("free", "Free")] },
        FacetSpec { name: "cross", label: "Cross", values: &[("mix", "Mix"), ("ping", "Ping"), ("diffuse", "Diffuse")] },
    ];

    fn stored(id: &str) -> bool {
        !NOT_STORED.contains(&id)
    }

    fn plain_float(id: &str) -> bool {
        PLAIN_FLOATS.contains(&id)
    }

    /// Both facets are derived from the sound when saving (§3.4).
    fn facets(values: &Values) -> Facets {
        Facets::from([
            ("time".to_string(), Time::of(values).word().to_string()),
            ("cross".to_string(), Cross::of(values).word().to_string()),
        ])
    }

    fn factory() -> Vec<Preset> {
        crate::factory::factory_presets()
    }

    fn capture_blobs(_: &PetalParams) {}

    fn apply_blobs(_: &PetalParams, _: &()) {}
}

// ---------------------------------------------------------------- capture / apply

/// Snapshot every stored param: discrete by plain value, floats normalized.
pub fn capture(params: &PetalParams) -> BTreeMap<String, f32> {
    softsystems_presets::capture_values::<Store>(params)
}

/// The whole sound as it is now (Afterecho has no blobs).
pub fn sound(params: &PetalParams) -> Sound<()> {
    Sound { values: capture(params), blobs: () }
}

/// Restore params by driving each like a knob gesture so the host records
/// automation/undo. A preset defines the whole sound: a param it doesn't
/// mention (added after it was saved, like Tape or Saturate) loads at its
/// default instead of keeping whatever was set. Unknown ids are skipped. An
/// empty map is Init. Only the params that change are sent; `sent` is the
/// editor's record of what it last sent (CLAP sets land a block later).
pub fn apply(params: &PetalParams, setter: &ParamSetter, snap: &BTreeMap<String, f32>, fmt: u32, sent: &mut Sent) {
    softsystems_presets::apply_values::<Store>(params, setter, snap, fmt, sent);
}

/// A stored value as the normalized value to set (`values_format` ≥ 1, as
/// every preset is once read).
unsafe fn to_normalized(id: &str, ptr: ParamPtr, stored: f32) -> f32 {
    unsafe {
        if ptr.step_count().is_some() || PLAIN_FLOATS.contains(&id) {
            ptr.preview_normalized(stored)
        } else {
            stored
        }
    }
}

/// Set a stand-alone params instance (the browser's "In this preset" probe,
/// the factory builder's tests) to a stored sound, without gestures: no host
/// sees it.
pub(crate) fn load_into(params: &PetalParams, snap: &BTreeMap<String, f32>) {
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: ptr belongs to params, which no host or audio thread reads.
        unsafe {
            let v = match snap.get(&id) {
                Some(&stored) if stored.is_finite() => to_normalized(&id, ptr, stored),
                _ => ptr.default_normalized_value(),
            };
            ptr._internal_set_normalized_value(v);
        }
    }
}

/// Every param's default in the stored form: what a missing id stands for.
fn stored_defaults() -> &'static BTreeMap<String, f32> {
    static D: std::sync::OnceLock<BTreeMap<String, f32>> = std::sync::OnceLock::new();
    D.get_or_init(softsystems_presets::stored_defaults::<Store>)
}

// ------------------------------------------------------------------ the facets

/// The Time facet (§3.4): the delay's Sync or Free, from `sync_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    Sync,
    Free,
}

impl Time {
    pub fn word(self) -> &'static str {
        match self {
            Time::Sync => "sync",
            Time::Free => "free",
        }
    }


    /// From stored values: `sync_mode` is an enum, stored plain (0 Free, 1 Sync).
    pub(crate) fn of(values: &BTreeMap<String, f32>) -> Time {
        let v = values.get("sync_mode").or_else(|| stored_defaults().get("sync_mode")).copied().unwrap_or(0.0);
        if v >= 0.5 { Time::Sync } else { Time::Free }
    }
}

/// The Cross facet (§3.4): how the two sides' repeats meet, from
/// `crossfeed_mode` (0 Mix, 1 Ping, 2 Diffuse).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cross {
    Mix,
    Ping,
    Diffuse,
}

impl Cross {
    pub fn word(self) -> &'static str {
        match self {
            Cross::Mix => "mix",
            Cross::Ping => "ping",
            Cross::Diffuse => "diffuse",
        }
    }


    pub(crate) fn of(values: &BTreeMap<String, f32>) -> Cross {
        let v = values.get("crossfeed_mode").or_else(|| stored_defaults().get("crossfeed_mode")).copied().unwrap_or(0.0);
        match v.round() as i32 {
            1 => Cross::Ping,
            2 => Cross::Diffuse,
            _ => Cross::Mix,
        }
    }
}

// ------------------------------------------------------------------ folders

/// The presets folder (§4): `Soft Systems/Afterecho/Presets` in the data
/// folder (`soft-systems/afterecho/presets` on Linux). iOS: the extension's
/// own container until an App Group id is configured (as Vortex).
pub fn default_dir() -> PathBuf {
    if cfg!(test) {
        panic!("tests must give the library a temp folder, never the user's");
    }
    softsystems_presets::presets_dir(Store::FOLDER)
}

/// The user's library: the real folder, copied once from the old one.
pub fn open_default() -> Library {
    if cfg!(test) {
        panic!("tests must give the library a temp folder, never the user's");
    }
    Library::open_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::Path;

    /// The live params still match `snap` (the crate's check, on the step grid).
    fn matches(params: &PetalParams, snap: &BTreeMap<String, f32>) -> bool {
        softsystems_presets::matches::<Store>(params, snap, VALUES_FORMAT_PLAIN)
    }

    /// Stand-in for the host's GUI context: a parameter gesture just sets the
    /// value, as the host would by the end of the gesture.
    pub(crate) struct TestGui;
    impl GuiContext for TestGui {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            false
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            unsafe { param._internal_set_normalized_value(normalized) };
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            unimplemented!("not used by presets")
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// A fresh folder of its own for one test, removed when dropped. No
    /// test ever opens the user's real preset folder (`default_dir` asserts).
    pub(crate) struct TempDir(pub(crate) PathBuf);

    impl TempDir {
        pub(crate) fn new(tag: &str) -> Self {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let d = std::env::temp_dir().join(format!("afterecho-presets-{tag}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            TempDir(d)
        }

        pub(crate) fn join(&self, p: &str) -> PathBuf {
            self.0.join(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Same sound: params within 1e-4, a missing id at its default.
    pub(crate) fn same_sound(a: &Preset, b: &Preset) -> bool {
        let d = stored_defaults();
        let val = |p: &Preset, k: &str| p.values.get(k).or_else(|| d.get(k)).copied().unwrap_or(0.0);
        d.keys().all(|k| (val(a, k) - val(b, k)).abs() <= 1.0e-4)
    }

    fn open(t: &TempDir) -> Library {
        Library::open(t.0.clone(), &[])
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".json"))
            .collect();
        v.sort();
        v
    }

    fn raw(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn read(path: &Path) -> Preset {
        softsystems_presets::read_preset::<Store>(path).unwrap()
    }

    #[test]
    fn plugin_id_is_the_clap_id() {
        assert_eq!(PLUGIN_ID, <crate::Mooltitwap as ClapPlugin>::CLAP_ID);
    }

    #[test]
    fn saved_cycles_read_back_after_the_range_grew() {
        let params = PetalParams::default();
        // Captured by plain value, whatever the range.
        assert_eq!(capture(&params)["feedback_time"], params.feedback_time.value());
        // A preset saved before v2.30 holds 0.4706 (1/4 on a 0..1 range): it
        // must come back as that plain value, in either values format.
        let t = TempDir::new("cycles");
        let old = 8.0 / 17.0;
        for fmt in [0, VALUES_FORMAT_PLAIN] {
            let f = t.join(&format!("c{fmt}.json"));
            std::fs::write(&f, format!(r#"{{"name":"c","values_format":{fmt},"values":{{"feedback_time":{old}}}}}"#)).unwrap();
            let q = PetalParams::default();
            load_into(&q, &read(&f).values);
            assert!((q.feedback_time.value() - old).abs() < 1e-5, "format {fmt}: {}", q.feedback_time.value());
        }
    }

    #[test]
    fn a_preset_missing_a_param_matches_it_at_its_default() {
        let params = PetalParams::default();
        let mut snap = capture(&params);
        assert!(matches(&params, &snap));
        // An older preset that predates these params: their defaults stand in.
        snap.remove("feedback_time");
        snap.remove("dry_wet");
        assert!(matches(&params, &snap));
        // A real difference still reads as edited.
        *snap.get_mut("feedback").unwrap() += 0.1;
        assert!(!matches(&params, &snap));
    }

    #[test]
    fn freeze_stays_out_of_presets_and_a_b() {
        let params = PetalParams::default();
        let snap = capture(&params);
        assert!(!snap.contains_key("freeze"));
        // Holding a loop doesn't mark the preset edited.
        let (_, ptr, _) = params.param_map().into_iter().find(|(id, _, _)| id == "freeze").unwrap();
        // SAFETY: ptr belongs to `params`, alive for the whole test.
        unsafe { ptr._internal_set_normalized_value(1.0) };
        assert!(params.freeze.value() && matches(&params, &snap));
        // and a load leaves it held
        apply(&params, &ParamSetter::new(&TestGui), &BTreeMap::new(), VALUES_FORMAT_PLAIN, &mut Sent::default());
        assert!(params.freeze.value());
    }

    /// A preset defines the whole sound: params it doesn't list load at their
    /// default, not at whatever was set last.
    #[test]
    fn apply_loads_defaults_for_unlisted_params() {
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        setter.set_parameter(&p.feedback, 0.9);
        setter.set_parameter(&p.num_taps_active, 3);
        let mut snap = BTreeMap::new();
        snap.insert("dry_wet".to_string(), 0.8);
        apply(&p, &setter, &snap, VALUES_FORMAT_PLAIN, &mut Sent::default());
        assert_eq!(p.feedback.value(), p.feedback.default_plain_value());
        assert_eq!(p.num_taps_active.value(), p.num_taps_active.default_plain_value());
        assert!((p.dry_wet.unmodulated_normalized_value() - 0.8).abs() < 1e-6);
        assert!(matches(&p, &snap));
    }

    /// A fresh instance is Init, and a loaded preset isn't edited: the taps'
    /// Q default sits off its step grid and moves onto it when set.
    #[test]
    fn a_load_is_not_an_edit() {
        let fresh = PetalParams::default();
        assert!(matches(&fresh, &BTreeMap::new()));
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        let mut sent = Sent::default();
        let f = Store::factory();
        let pg = f.iter().find(|x| x.name == "Ping Garden").unwrap();
        setter.set_parameter(&p.taps[2].filter_q, 3.0);
        apply(&p, &setter, &BTreeMap::new(), VALUES_FORMAT_PLAIN, &mut sent);
        assert!(matches(&p, &BTreeMap::new()), "Init after a Q edit");
        apply(&p, &setter, &pg.values, pg.values_format, &mut sent);
        assert!(matches(&p, &pg.values));
        setter.set_parameter(&p.taps[2].filter_q, 3.0);
        assert!(!matches(&p, &pg.values));
    }

    /// Enums are stored by plain index, so appending a variant can't remap
    /// what a preset holds.
    #[test]
    fn discrete_plain_survives_enum_append() {
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        setter.set_parameter(&p.spacing_mode, crate::params::SpacingMode::Euclidean);
        let snap = capture(&p);
        assert_eq!(snap["spacing_mode"], 3.0, "stored by index, not normalized");
        setter.set_parameter(&p.spacing_mode, crate::params::SpacingMode::Linear);
        apply(&p, &setter, &snap, VALUES_FORMAT_PLAIN, &mut Sent::default());
        assert_eq!(p.spacing_mode.value(), crate::params::SpacingMode::Euclidean);
    }

    #[test]
    fn no_duplicate_param_ids() {
        let p = PetalParams::default();
        let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
        let set: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(set.len(), ids.len());
    }

    #[test]
    fn facets_come_from_the_sound() {
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        setter.set_parameter(&p.crossfeed_mode, crate::params::CrossfeedMode::PingPong);
        let ping = Preset::new::<Store>("x", sound(&p), Kind::User);
        assert_eq!((Cross::of(&ping.values), ping.facets["cross"].as_str()), (Cross::Ping, "ping"));
        setter.set_parameter(&p.sync_mode, crate::params::SyncMode::Sync);
        assert_eq!(Preset::new::<Store>("x", sound(&p), Kind::User).facets["time"], "sync");
        setter.set_parameter(&p.sync_mode, crate::params::SyncMode::Free);
        assert_eq!(Preset::new::<Store>("x", sound(&p), Kind::User).facets["time"], "free");
        // the store's facet list names what the sheet filters by
        let names: Vec<&str> = Store::FACETS.iter().map(|f| f.name).collect();
        assert_eq!(names, ["time", "cross"]);
    }

    #[test]
    fn save_as_writes_named_preset_to_disk() {
        let t = TempDir::new("saveas");
        let mut lib = open(&t);
        let p = PetalParams::default();
        let id = lib.save_as("Vocal Throw 2", sound(&p), Some("ping garden")).unwrap();
        assert_eq!(id, "vocal throw 2");
        let v = raw(&t.join("Vocal Throw 2.json"));
        assert_eq!(v["format"], 2);
        assert_eq!(v["plugin"], PLUGIN_ID);
        assert_eq!(v["kind"], "user");
        assert_eq!(v["tags"][0], "rhythmic", "tags copied from the loaded preset");
        assert_eq!(v["facets"]["cross"], "mix", "facets from this sound, not the source");
        assert_eq!(v["based_on"]["name"], "Ping Garden");
        assert!(open(&t).get("vocal throw 2").is_some_and(|p| p.kind() == Kind::User));
    }

    #[test]
    fn factory_is_read_only() {
        let t = TempDir::new("factory");
        let mut lib = open(&t);
        let s = sound(&PetalParams::default());
        assert_eq!(lib.save("ping garden", s).unwrap_err().to_string(), softsystems_presets::READ_ONLY);
        assert_eq!(lib.delete("ping garden").unwrap_err().to_string(), softsystems_presets::READ_ONLY);
        assert!(files(&t.0).iter().all(|f| f == "library.json"), "nothing written but the library state");
        assert_eq!(lib.duplicate("ping garden").unwrap(), "Ping Garden copy");
        assert_eq!(lib.get("ping garden copy").unwrap().kind(), Kind::User);
    }

    #[test]
    fn preset_coverage_audit() {
        let a = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        // move every stored param somewhere off its default
        for (id, ptr, _) in a.param_map() {
            if NOT_STORED.contains(&id.as_str()) {
                continue;
            }
            // SAFETY: ptr belongs to `a`.
            unsafe {
                let d = ptr.default_normalized_value();
                ptr._internal_set_normalized_value(if d > 0.5 { 0.2 } else { 0.8 });
            }
        }
        let snap = capture(&a);
        let n_stored = a.param_map().len() - NOT_STORED.len();
        assert_eq!(snap.len(), n_stored);
        let b = PetalParams::default();
        apply(&b, &setter, &snap, VALUES_FORMAT_PLAIN, &mut Sent::default());
        assert!(matches(&b, &snap));
        assert_eq!(capture(&b), snap);
    }
}
