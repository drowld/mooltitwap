//! The preset system (Vortex's `docs/PRESET_SYSTEM.md`, the Soft Systems
//! spec): the preset file, the library (factory presets served from the
//! binary, user presets on disk, the library state in `library.json`) and the
//! rules every operation keeps. Independent of the host's own state save;
//! gives named presets the DAW can't. Ported from Vortex `presets.rs`.
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
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const VALUES_FORMAT_PLAIN: u32 = 1;
/// The file layout version (§3.1). Readers accept every older one (an absent
/// `format` is 1); a file from a newer one is refused, not half-loaded.
pub const FORMAT: u32 = 2;
/// The CLAP id (`lib.rs`; kept from the Mooltitwap days so sessions recall
/// the plug-in): a file from another plug-in is refused.
pub const PLUGIN_ID: &str = "com.bussien.mooltitwap";
/// A name is trimmed and 1–64 characters (§3.1).
pub const NAME_MAX: usize = 64;
/// At most 8 tags of up to 24 characters each (§3.1).
pub const TAGS_MAX: usize = 8;
pub const TAG_LEN_MAX: usize = 24;
/// library.json keeps the last 12 kept presets (§4).
pub const RECENT_MAX: usize = 12;
/// Why Save, Rename, Delete and tag / notes edits are greyed on a factory
/// preset (§5).
pub const READ_ONLY: &str = "factory presets are read-only — Duplicate makes an editable copy";
/// The library state file, beside the presets.
const LIBRARY_FILE: &str = "library.json";

/// Floats whose range grew past 0..1 after presets were already saved
/// (`feedback_time`, v2.30): stored by PLAIN value. Before the change their
/// normalized value equalled the plain one, so every preset, old or new, reads
/// back exactly.
const PLAIN_FLOATS: &[&str] = &["feedback_time"];

/// Performance switches a preset or A/B leaves as they are (v2.35): loading
/// one mustn't let go of a held loop, nor flipping A/B start one.
const NOT_STORED: &[&str] = &["freeze"];

// ---------------------------------------------------------------- capture / apply

/// Whether `id` is stored by plain value (discrete, or in PLAIN_FLOATS).
unsafe fn stored_plain(id: &str, ptr: ParamPtr) -> bool {
    unsafe { ptr.step_count() }.is_some() || PLAIN_FLOATS.contains(&id)
}

/// Snapshot every param: discrete by plain value, floats normalized.
pub fn capture(params: &PetalParams) -> BTreeMap<String, f32> {
    let mut map = BTreeMap::new();
    for (id, ptr, _group) in params.param_map() {
        if NOT_STORED.contains(&id.as_str()) {
            continue;
        }
        // SAFETY: ptr comes from this same params and outlives the call.
        let v = unsafe {
            if stored_plain(&id, ptr) {
                ptr.unmodulated_plain_value()
            } else {
                ptr.unmodulated_normalized_value()
            }
        };
        map.insert(id, v);
    }
    map
}

/// Turn a stored value into the NORMALIZED value to apply, honoring the format.
unsafe fn to_normalized(id: &str, ptr: ParamPtr, stored: f32, fmt: u32) -> f32 {
    // Format 0 stored discrete params normalized; PLAIN_FLOATS are plain in every format.
    let discrete = unsafe { ptr.step_count() }.is_some();
    if (fmt >= VALUES_FORMAT_PLAIN && discrete) || PLAIN_FLOATS.contains(&id) {
        unsafe { ptr.preview_normalized(stored) }
    } else {
        stored
    }
}

/// Restore params by driving each like a knob gesture so the host records
/// automation/undo. A preset defines the whole sound: a param it doesn't
/// mention (added after it was saved, like Tape or Saturate) loads at its
/// default instead of keeping whatever was set. Unknown ids are skipped. An
/// empty map is Init.
pub fn apply(params: &PetalParams, setter: &ParamSetter, snap: &BTreeMap<String, f32>, fmt: u32) {
    let snap = &*crate::migrate::preset(snap, fmt);
    for (id, ptr, _group) in params.param_map() {
        if NOT_STORED.contains(&id.as_str()) {
            continue;
        }
        // SAFETY: ptr belongs to params; begin/set/end like a knob drag.
        unsafe {
            let v = match snap.get(&id) {
                Some(&stored) if stored.is_finite() => to_normalized(&id, ptr, stored, fmt),
                _ => ptr.default_normalized_value(),
            };
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter.raw_context.raw_set_parameter_normalized(ptr, v);
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
}

/// Set a stand-alone params instance (the browser's "In this preset" probe,
/// the factory builder) to a stored sound, without gestures: no host sees it.
pub(crate) fn load_into(params: &PetalParams, snap: &BTreeMap<String, f32>, fmt: u32) {
    let snap = &*crate::migrate::preset(snap, fmt);
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: ptr belongs to params, which no host or audio thread reads.
        unsafe {
            let v = match snap.get(&id) {
                Some(&stored) if stored.is_finite() => to_normalized(&id, ptr, stored, fmt),
                _ => ptr.default_normalized_value(),
            };
            ptr._internal_set_normalized_value(v);
        }
    }
}

/// True if the live params still match `snap` within 1e-4 (§7.11), compared
/// in the stored domain. A param the snapshot doesn't list is expected at its
/// default, as `apply` left it. Floats compare on their step grid: a stepped
/// param whose default is off the grid (the taps' Q, 0.707) lands on the
/// grid once set, and that's still the same sound.
pub fn matches(params: &PetalParams, snap: &BTreeMap<String, f32>, fmt: u32) -> bool {
    let snap = &*crate::migrate::preset(snap, fmt);
    params.param_map().into_iter().filter(|(id, _, _)| !NOT_STORED.contains(&id.as_str())).all(|(id, ptr, _group)| {
        // SAFETY: ptr comes from these params and outlives the call.
        let (live, want) = unsafe {
            let plain = stored_plain(&id, ptr);
            let want_n = match snap.get(&id) {
                Some(&v) if v.is_finite() => to_normalized(&id, ptr, v, fmt),
                _ => ptr.default_normalized_value(),
            };
            if plain {
                (ptr.unmodulated_plain_value(), ptr.preview_plain(want_n))
            } else {
                let grid = |n: f32| ptr.preview_normalized(ptr.preview_plain(n));
                (grid(ptr.unmodulated_normalized_value()), grid(want_n))
            }
        };
        (live - want).abs() <= 1e-4
    })
}

/// Every param's default in the stored form: what a non-finite value is
/// replaced with before a write, and what a missing id compares as.
fn stored_defaults() -> &'static BTreeMap<String, f32> {
    static D: std::sync::OnceLock<BTreeMap<String, f32>> = std::sync::OnceLock::new();
    D.get_or_init(|| capture(&PetalParams::default()))
}

// ------------------------------------------------------------------ the preset file

/// Factory (shipped in the binary, read-only) or user (§2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Factory,
    User,
}

/// The Time facet (§3.4): the delay's Sync or Free, from `sync_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    Sync,
    Free,
}

impl Time {
    pub const ALL: [Time; 2] = [Time::Sync, Time::Free];

    pub fn word(self) -> &'static str {
        match self {
            Time::Sync => "sync",
            Time::Free => "free",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Time::Sync => "Sync",
            Time::Free => "Free",
        }
    }

    fn from_word(w: &str) -> Option<Time> {
        Time::ALL.into_iter().find(|t| t.word() == w)
    }

    /// From stored values: `sync_mode` is an enum, stored plain (0 Free, 1 Sync).
    fn of(values: &BTreeMap<String, f32>) -> Time {
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
    pub const ALL: [Cross; 3] = [Cross::Mix, Cross::Ping, Cross::Diffuse];

    pub fn word(self) -> &'static str {
        match self {
            Cross::Mix => "mix",
            Cross::Ping => "ping",
            Cross::Diffuse => "diffuse",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Cross::Mix => "Mix",
            Cross::Ping => "Ping",
            Cross::Diffuse => "Diffuse",
        }
    }

    fn from_word(w: &str) -> Option<Cross> {
        Cross::ALL.into_iter().find(|c| c.word() == w)
    }

    fn of(values: &BTreeMap<String, f32>) -> Cross {
        let v = values.get("crossfeed_mode").or_else(|| stored_defaults().get("crossfeed_mode")).copied().unwrap_or(0.0);
        match v.round() as i32 {
            1 => Cross::Ping,
            2 => Cross::Diffuse,
            _ => Cross::Mix,
        }
    }
}

/// One preset file (§3.1). Every field but `name` and `values` is optional
/// when read, so files written by older versions always load; fields this
/// build doesn't know are kept in `extra` and written back.
#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    /// The layout version; absent = 1 (the files before this one).
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub plugin: String,
    #[serde(default)]
    pub plugin_version: String,
    pub name: String,
    /// Absent in format 1, where every file was the user's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Facet name → value ("time" → "sync" | "free", "cross" → "mix" |
    /// "ping" | "diffuse").
    #[serde(default)]
    pub facets: BTreeMap<String, String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub values_format: u32,
    #[serde(deserialize_with = "lenient_values")]
    pub values: BTreeMap<String, f32>,
    /// Format 1: the build stamp (e.g. "v2.30.0"). Read so it isn't kept in
    /// `extra`, never written.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pub build: String,
    /// Format 1: the favourite flag, now in library.json (moved there once).
    /// Read, never written.
    #[serde(default, skip_serializing)]
    pub favorite: bool,
    /// Fields a newer plug-in wrote (§3.1: they survive a rewrite).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A value that isn't a finite number (a `null` from a NaN) is dropped, so
/// it loads its default instead of failing the whole file.
fn lenient_values<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, f32>, D::Error> {
    let raw = BTreeMap::<String, Value>::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|(k, v)| v.as_f64().map(|x| (k, x as f32)))
        .filter(|(_, x)| x.is_finite())
        .collect())
}

impl Preset {
    /// A new preset of this sound.
    pub fn new(name: &str, values: BTreeMap<String, f32>, kind: Kind) -> Preset {
        let mut p = Preset {
            format: FORMAT,
            plugin: PLUGIN_ID.into(),
            plugin_version: env!("CARGO_PKG_VERSION").into(),
            name: name.to_string(),
            kind: Some(kind),
            tags: Vec::new(),
            facets: BTreeMap::new(),
            notes: String::new(),
            values_format: VALUES_FORMAT_PLAIN,
            values,
            build: String::new(),
            favorite: false,
            extra: Map::new(),
        };
        p.derive_facets();
        p
    }

    /// Every format-1 file was a user preset (Afterecho had no factory set).
    pub fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::User)
    }

    pub fn id(&self) -> String {
        identity(&self.name)
    }

    /// The Time facet: as written, else from the values (format-1 files).
    pub fn time(&self) -> Time {
        self.facets.get("time").and_then(|w| Time::from_word(w)).unwrap_or_else(|| Time::of(&self.values))
    }

    /// The Cross facet: as written, else from the values.
    pub fn cross(&self) -> Cross {
        self.facets.get("cross").and_then(|w| Cross::from_word(w)).unwrap_or_else(|| Cross::of(&self.values))
    }

    /// Both facets are derived from the sound when saving (§3.4).
    fn derive_facets(&mut self) {
        self.facets.insert("time".into(), Time::of(&self.values).word().into());
        self.facets.insert("cross".into(), Cross::of(&self.values).word().into());
    }

    /// Same sound as `o`: params within 1e-4, a missing id at its default.
    pub fn same_sound(&self, o: &Preset) -> bool {
        let d = stored_defaults();
        let val = |p: &Preset, k: &str| p.values.get(k).or_else(|| d.get(k)).copied().unwrap_or(0.0);
        d.keys().all(|k| (val(self, k) - val(o, k)).abs() <= 1.0e-4)
    }

    /// The form every write takes: this format, this plug-in, normalized
    /// tags, and no non-finite number anywhere (§3.2).
    fn for_write(&self) -> Preset {
        let mut p = self.clone();
        p.format = FORMAT;
        p.plugin = PLUGIN_ID.into();
        p.plugin_version = env!("CARGO_PKG_VERSION").into();
        p.kind = Some(self.kind());
        p.values_format = if p.values_format == 0 { 0 } else { VALUES_FORMAT_PLAIN };
        p.tags = normalize_tags(&p.tags);
        let d = stored_defaults();
        p.values.retain(|k, v| v.is_finite() || d.contains_key(k));
        for (k, v) in p.values.iter_mut() {
            if !v.is_finite() {
                *v = d[k];
            }
        }
        p
    }
}

// ------------------------------------------------------------------ names, tags

/// Filesystem-safe stem from a preset name (§4): letters, digits, space,
/// `-` and `_` stay, anything else becomes `_`.
pub fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "untitled".into() } else { s }
}

/// A preset's identity: its sanitized name without regard to case ("Pad" and
/// "pad" are one preset — macOS volumes are case-insensitive).
pub fn identity(name: &str) -> String {
    sanitize(name.trim()).to_lowercase()
}

/// A name as typed, checked (§3.1): trimmed, 1–64 characters.
pub fn check_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("A preset needs a name".into());
    }
    if n.chars().count() > NAME_MAX {
        return Err(format!("A name has at most {NAME_MAX} characters"));
    }
    Ok(n.to_string())
}

/// One tag normalized (§3.1): trimmed, lower case, at most 24 characters.
pub fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase().chars().take(TAG_LEN_MAX).collect::<String>().trim().to_string()
}

/// Tags normalized and de-duplicated without regard to case, at most 8.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags.iter().map(|t| normalize_tag(t)) {
        if !t.is_empty() && !out.contains(&t) && out.len() < TAGS_MAX {
            out.push(t);
        }
    }
    out
}

// ------------------------------------------------------------------ disk

/// Crash-safe write: write + fsync a temp file, rename over the target, fsync
/// the dir. On failure the temp file is removed and the target is as it was.
fn write_atomic(path: &Path, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let res = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    match res {
        Ok(()) => {
            if let Some(dir) = path.parent() {
                if let Ok(d) = std::fs::File::open(dir) {
                    let _ = d.sync_all();
                }
            }
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Write a preset file in its write form; the reason in words on failure.
fn write_preset(path: &Path, p: &Preset) -> Result<(), String> {
    let s = serde_json::to_string_pretty(&p.for_write()).map_err(|e| e.to_string())?;
    write_atomic(path, &s).map_err(|e| e.to_string())
}

/// Read one preset file. The reason in words when it can't be used (§11):
/// not JSON, another plug-in's, a newer format, or not a preset.
pub fn read_preset(path: &Path) -> Result<Preset, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("can't be read ({e})"))?;
    let raw: Value = serde_json::from_str(&s).map_err(|_| "isn't valid JSON".to_string())?;
    if let Some(p) = raw.get("plugin").and_then(Value::as_str) {
        if !p.is_empty() && p != PLUGIN_ID {
            return Err(format!("is a preset for another plug-in ({p})"));
        }
    }
    if let Some(f) = raw.get("format").and_then(Value::as_u64) {
        if f > FORMAT as u64 {
            return Err(format!("needs a newer Afterecho (format {f})"));
        }
    }
    let p: Preset = serde_json::from_value(raw).map_err(|_| "isn't an Afterecho preset".to_string())?;
    if p.name.trim().is_empty() {
        return Err("has no name".into());
    }
    Ok(p)
}

/// The `.json` files in `dir` that may be presets, sorted by file name.
fn preset_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("json"))
                .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some(LIBRARY_FILE))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The user's data folder for this platform (`~/Library/Application Support`
/// on macOS and in the iOS container, `%APPDATA%` on Windows,
/// `$XDG_DATA_HOME` or `~/.local/share` elsewhere).
fn data_root() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA").map(PathBuf::from);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        return home.map(|h| h.join("Library").join("Application Support"));
    }
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local").join("share")))
}

/// The presets folder (§4): `Soft Systems/Afterecho/Presets` in the data
/// folder (`soft-systems/afterecho/presets` on Linux). Never relative to the
/// current directory (it belongs to the host). iOS: the extension's own
/// container until an App Group id is configured (as Vortex).
pub fn default_dir() -> PathBuf {
    if cfg!(test) {
        panic!("tests must give the library a temp folder, never the user's");
    }
    match data_root() {
        Some(base) if cfg!(any(target_os = "macos", target_os = "ios", target_os = "windows")) => {
            base.join("Soft Systems").join("Afterecho").join("Presets")
        }
        Some(base) => base.join("soft-systems").join("afterecho").join("presets"),
        None => std::env::temp_dir().join("soft-systems-afterecho-presets"),
    }
}

/// The folder of the Mooltitwap days, which still holds the look
/// (`appearance.json`) beside the old `presets` folder.
pub(crate) fn legacy_root() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join("Library/Application Support/Mooltitwap"),
        None => std::env::temp_dir().join("mooltitwap"),
    }
}

/// The presets folder before v2.40, copied from once (§4).
pub fn old_dir() -> Option<PathBuf> {
    Some(legacy_root().join("presets"))
}

// ------------------------------------------------------------------ library.json

/// The library state (§4), shared by every instance through `library.json`.
#[derive(Clone, Serialize, Deserialize)]
pub struct LibState {
    /// Favourite presets, by identity.
    #[serde(default)]
    pub favourites: Vec<String>,
    /// The last 12 kept presets, by identity, newest first.
    #[serde(default)]
    pub recent: Vec<String>,
    /// The browser's last collection, facets and sort (words).
    #[serde(default)]
    pub collection: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub cross: String,
    #[serde(default)]
    pub sort: String,
    /// Try (audition) on or off (§13.4: on by default).
    #[serde(default = "yes")]
    pub try_on: bool,
    /// Files hidden by a factory migration: unchanged copies of a factory
    /// preset. They stay on disk (§5).
    #[serde(default)]
    pub hidden: Vec<String>,
    /// The format-1 `favorite` flags were moved in here.
    #[serde(default)]
    pub favourites_migrated: bool,
    /// The old folder's presets were copied across.
    #[serde(default)]
    pub old_folder_copied: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn yes() -> bool {
    true
}

impl Default for LibState {
    fn default() -> Self {
        Self {
            favourites: Vec::new(),
            recent: Vec::new(),
            collection: String::new(),
            time: String::new(),
            cross: String::new(),
            sort: String::new(),
            try_on: true,
            hidden: Vec::new(),
            favourites_migrated: false,
            old_folder_copied: false,
            extra: Map::new(),
        }
    }
}

fn read_state(dir: &Path) -> Option<LibState> {
    let s = std::fs::read_to_string(dir.join(LIBRARY_FILE)).ok()?;
    serde_json::from_str(&s).ok()
}

// ------------------------------------------------------------------ the library

/// One preset in the library, and the file it lives in (None: served from
/// the binary).
#[derive(Clone)]
pub struct Entry {
    pub preset: Preset,
    pub file: Option<PathBuf>,
}

/// A file in the folder that couldn't be used (§11): listed, never deleted.
#[derive(Clone, Debug)]
pub struct Unreadable {
    pub file: String,
    /// The name inside it, when there is one.
    pub name: Option<String>,
    pub reason: String,
}

pub struct Library {
    pub dir: PathBuf,
    /// The factory set this library serves (the binary's, or a test's).
    factory: Vec<Preset>,
    /// Factory and user presets, A–Z by name.
    entries: Vec<Entry>,
    pub unreadable: Vec<Unreadable>,
    pub state: LibState,
}

impl Library {
    /// The user's library: the real folder, copied once from the old one.
    pub fn open_default() -> Library {
        Library::open(default_dir(), old_dir(), crate::factory::factory_presets())
    }

    /// The library in `dir` (created if missing), serving `factory`. `old`:
    /// a folder whose presets are copied in once (never deleted, never
    /// copied twice).
    pub fn open(dir: PathBuf, old: Option<PathBuf>, factory: Vec<Preset>) -> Library {
        let _ = std::fs::create_dir_all(&dir);
        let state = read_state(&dir).unwrap_or_default();
        let mut lib = Library { dir, factory, entries: Vec::new(), unreadable: Vec::new(), state };
        if let Some(old) = old {
            lib.copy_old_folder(&old);
        }
        lib.rescan();
        lib
    }

    /// Re-read the folder and library.json (§11: another instance or Finder
    /// may have changed them). A file under a factory preset's name that
    /// sounds the same is hidden (it stays on disk); one that doesn't becomes
    /// the user preset "<name> (edited)", its file rewritten in place (§5).
    /// Nothing is deleted; a second run changes nothing.
    pub fn rescan(&mut self) {
        if let Some(s) = read_state(&self.dir) {
            self.state = s;
        }
        let mut entries: Vec<Entry> = self.factory.iter().cloned().map(|preset| Entry { preset, file: None }).collect();
        let mut taken: std::collections::HashSet<String> = entries.iter().map(|e| e.preset.id()).collect();
        let mut unreadable = Vec::new();
        let mut hide = Vec::new();
        let mut legacy_favs = Vec::new();
        for path in preset_files(&self.dir) {
            let fname = file_name(&path);
            if self.state.hidden.iter().any(|h| h.eq_ignore_ascii_case(&fname)) {
                continue;
            }
            let mut pr = match read_preset(&path) {
                Ok(p) => p,
                Err(reason) => {
                    let name = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string));
                    unreadable.push(Unreadable { file: fname, name, reason });
                    continue;
                }
            };
            let fav = pr.favorite;
            if let Some(f) = self.factory.iter().find(|f| f.id() == pr.id()) {
                if pr.same_sound(f) {
                    hide.push(fname);
                    if fav {
                        legacy_favs.push(f.id());
                    }
                    continue;
                }
                // a user preset under a factory preset's name
                pr.name = unique_name(&format!("{} (edited)", pr.name.trim()), &taken);
                pr.kind = Some(Kind::User);
                let _ = write_preset(&path, &pr); // in place; retried next scan if it fails
            }
            let id = pr.id();
            if !taken.insert(id.clone()) {
                unreadable.push(Unreadable {
                    file: fname,
                    name: Some(pr.name.clone()),
                    reason: "has the same name as another preset".into(),
                });
                continue;
            }
            if fav {
                legacy_favs.push(id);
            }
            if pr.kind.is_none() {
                pr.kind = Some(pr.kind());
            }
            entries.push(Entry { preset: pr, file: Some(path) });
        }
        entries.sort_by_key(|e| e.preset.name.to_lowercase());
        self.entries = entries;
        self.unreadable = unreadable;
        let migrate_favs = !self.state.favourites_migrated;
        if !hide.is_empty() || migrate_favs {
            let _ = self.update_state(|s| {
                for h in &hide {
                    if !s.hidden.contains(h) {
                        s.hidden.push(h.clone());
                    }
                }
                if migrate_favs {
                    for id in &legacy_favs {
                        if !s.favourites.contains(id) {
                            s.favourites.push(id.clone());
                        }
                    }
                    s.favourites_migrated = true;
                }
            });
        }
    }

    /// Copy the old folder's presets across once (§4). Unreadable files stay
    /// where they are; the old folder is never touched.
    fn copy_old_folder(&mut self, old: &Path) {
        if self.state.old_folder_copied || !old.is_dir() || old == self.dir {
            return;
        }
        let mut all_ok = true;
        for path in preset_files(old) {
            if read_preset(&path).is_err() {
                continue;
            }
            let dest = self.dir.join(path.file_name().unwrap_or_default());
            if dest.exists() {
                continue;
            }
            if std::fs::copy(&path, &dest).is_err() {
                all_ok = false;
            }
        }
        if all_ok {
            let _ = self.update_state(|s| s.old_folder_copied = true);
        }
    }

    /// Change the library state the way `f` says, merged with what is on
    /// disk now (another instance may have written it): re-read, apply,
    /// write atomically. On a failed write nothing changes (memory included).
    pub fn update_state(&mut self, f: impl FnOnce(&mut LibState)) -> Result<(), String> {
        let mut s = read_state(&self.dir).unwrap_or_else(|| self.state.clone());
        f(&mut s);
        let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
        write_atomic(&self.dir.join(LIBRARY_FILE), &json).map_err(|e| format!("Couldn't save — {e}"))?;
        self.state = s;
        Ok(())
    }

    // ---- reading

    /// Every preset, A–Z.
    pub fn presets(&self) -> impl Iterator<Item = &Preset> {
        self.entries.iter().map(|e| &e.preset)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, id: &str) -> Option<&Preset> {
        self.entry(id).map(|e| &e.preset)
    }

    /// The preset `id` names (an identity, or a name: both resolve).
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        let id = identity(id);
        self.entries.iter().find(|e| e.preset.id() == id)
    }

    pub fn is_favourite(&self, id: &str) -> bool {
        self.state.favourites.iter().any(|f| f == id)
    }

    /// The identity is taken by a preset, or by a file in the folder this
    /// library doesn't list (an unreadable or hidden one): writing there
    /// would overwrite it.
    fn taken(&self, id: &str, except: Option<&str>) -> bool {
        if except == Some(id) {
            return false;
        }
        self.get(id).is_some()
            || preset_files(&self.dir)
                .iter()
                .any(|p| p.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == id))
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", sanitize(name)))
    }

    /// A user preset to change, or why it can't be.
    fn user(&self, id: &str) -> Result<&Entry, String> {
        let e = self.entry(id).ok_or("That preset is gone — the folder changed")?;
        if e.preset.kind() == Kind::Factory || e.file.is_none() {
            return Err(READ_ONLY.into());
        }
        Ok(e)
    }

    /// Write `p` as the user preset `old_id` now is, in its own file.
    fn rewrite(&mut self, old_id: &str, p: Preset) -> Result<(), String> {
        let path = self.user(old_id)?.file.clone().unwrap_or_else(|| self.path_for(&p.name));
        write_preset(&path, &p).map_err(|e| format!("Couldn't save — {e}"))?;
        if let Some(e) = self.entries.iter_mut().find(|e| e.preset.id() == old_id) {
            e.preset = p.for_write();
        }
        Ok(())
    }

    fn insert(&mut self, p: Preset, file: PathBuf) {
        self.entries.push(Entry { preset: p.for_write(), file: Some(file) });
        self.entries.sort_by_key(|e| e.preset.name.to_lowercase());
    }

    // ---- operations (§7). Each returns the reason in words when refused;
    // memory changes only when the disk did.

    /// Save (§7.3): overwrite user preset `id` with this sound. Keeps its
    /// name, tags, notes and favourite: those are metadata, not sound.
    pub fn save(&mut self, id: &str, values: BTreeMap<String, f32>) -> Result<(), String> {
        let mut p = self.user(id)?.preset.clone();
        p.values = values;
        p.values_format = VALUES_FORMAT_PLAIN;
        p.derive_facets();
        self.rewrite(id, p)
    }

    /// Save as… (§7.4): a new user preset of this sound, never over another
    /// one. It copies the tags of `from` (the loaded preset); the facets come
    /// from the sound. Returns its identity.
    pub fn save_as(&mut self, name: &str, values: BTreeMap<String, f32>, from: Option<&str>) -> Result<String, String> {
        let name = check_name(name)?;
        let id = identity(&name);
        if self.taken(&id, None) {
            return Err(format!("“{name}” already exists — pick another name"));
        }
        let mut p = Preset::new(&name, values, Kind::User);
        if let Some(src) = from.and_then(|f| self.get(f)) {
            p.tags = src.tags.clone();
        }
        let path = self.path_for(&name);
        write_preset(&path, &p).map_err(|e| format!("Couldn't save — {e}"))?;
        self.insert(p, path);
        Ok(id)
    }

    /// Rename (§7.5), user presets only. The collision check is against the
    /// other presets; a case-only rename keeps the file. The new file is
    /// written first, then the old one removed when it is a different file.
    /// Favourites and recent follow. Returns the new identity.
    pub fn rename(&mut self, id: &str, new: &str) -> Result<String, String> {
        let new = check_name(new)?;
        let new_id = identity(&new);
        let e = self.user(id)?.clone();
        if new_id != id && self.taken(&new_id, None) {
            return Err(format!("“{new}” already exists — pick another name"));
        }
        let old_path = e.file.clone().unwrap_or_else(|| self.path_for(&e.preset.name));
        let new_path = self.path_for(&new);
        let mut p = e.preset.clone();
        p.name = new.clone();
        write_preset(&new_path, &p).map_err(|e| format!("Couldn't rename — {e}"))?;
        if old_path != new_path {
            let same = old_path.to_string_lossy().to_lowercase() == new_path.to_string_lossy().to_lowercase();
            // a case-only change is the same file on a case-insensitive
            // volume: removing it would remove what was just written. On a
            // case-sensitive one both names are listed; drop the old.
            let both = same && {
                let names: Vec<String> =
                    std::fs::read_dir(&self.dir).map(|rd| rd.flatten().map(|d| file_name(&d.path())).collect()).unwrap_or_default();
                names.contains(&file_name(&old_path)) && names.contains(&file_name(&new_path))
            };
            if !same || both {
                let _ = std::fs::remove_file(&old_path);
            }
        }
        self.entries.retain(|x| x.preset.id() != id);
        self.insert(p, new_path);
        if new_id != id {
            let (a, b) = (id.to_string(), new_id.clone());
            let swap = |v: &mut Vec<String>| v.iter_mut().filter(|x| **x == a).for_each(|x| *x = b.clone());
            let _ = self.update_state(|s| {
                swap(&mut s.favourites);
                swap(&mut s.recent);
            });
        }
        Ok(new_id)
    }

    /// Duplicate (§7.6): a user copy of any preset, "<name> copy", then
    /// "copy 2", "copy 3"… Returns its name.
    pub fn duplicate(&mut self, id: &str) -> Result<String, String> {
        let src = self.get(id).ok_or("That preset is gone — the folder changed")?.clone();
        let name = self.free_name(&format!("{} copy", src.name.trim()));
        let mut p = src;
        p.name = name.clone();
        p.kind = Some(Kind::User);
        let path = self.path_for(&name);
        write_preset(&path, &p).map_err(|e| format!("Couldn't save — {e}"))?;
        self.insert(p, path);
        Ok(name)
    }

    /// Delete (§7.7), user presets only: removes its file, and its name
    /// from favourites and recent.
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let path = self.user(id)?.file.clone().unwrap_or_default();
        if let Err(e) = std::fs::remove_file(&path) {
            if path.exists() {
                return Err(format!("Couldn't delete — {e}"));
            }
        }
        self.entries.retain(|e| e.preset.id() != id);
        let _ = self.update_state(|s| {
            s.favourites.retain(|x| x != id);
            s.recent.retain(|x| x != id);
        });
        Ok(())
    }

    /// Add a tag (§7.9), normalized; at most 8.
    pub fn add_tag(&mut self, id: &str, tag: &str) -> Result<(), String> {
        let t = normalize_tag(tag);
        let mut p = self.user(id)?.preset.clone();
        if t.is_empty() || p.tags.contains(&t) {
            return Ok(());
        }
        if p.tags.len() >= TAGS_MAX {
            return Err(format!("A preset holds at most {TAGS_MAX} tags"));
        }
        p.tags.push(t);
        self.rewrite(id, p)
    }

    pub fn remove_tag(&mut self, id: &str, tag: &str) -> Result<(), String> {
        let mut p = self.user(id)?.preset.clone();
        let n = p.tags.len();
        p.tags.retain(|x| x != tag);
        if p.tags.len() == n {
            return Ok(());
        }
        self.rewrite(id, p)
    }

    /// The notes (§13.7): one line of free text.
    pub fn set_notes(&mut self, id: &str, notes: &str) -> Result<(), String> {
        let mut p = self.user(id)?.preset.clone();
        let n = notes.replace(['\n', '\r'], " ");
        if p.notes == n {
            return Ok(());
        }
        p.notes = n;
        self.rewrite(id, p)
    }

    /// Favourite (§7.8), any preset: kept in library.json.
    pub fn set_favourite(&mut self, id: &str, on: bool) -> Result<(), String> {
        self.update_state(|s| {
            s.favourites.retain(|x| x != id);
            if on {
                s.favourites.push(id.to_string());
            }
        })
    }

    /// A kept preset goes to the top of Recent (§7.1).
    pub fn push_recent(&mut self, id: &str) -> Result<(), String> {
        self.update_state(|s| {
            s.recent.retain(|x| x != id);
            s.recent.insert(0, id.to_string());
            s.recent.truncate(RECENT_MAX);
        })
    }

    /// Export (§13.6): write preset `id` as one `.json` file at `path`.
    pub fn export(&self, id: &str, path: &Path) -> Result<(), String> {
        let p = self.get(id).ok_or("That preset is gone — the folder changed")?;
        write_preset(path, p).map_err(|e| format!("Couldn't export — {e}"))
    }

    /// Import (§13.6): read a `.json` preset and add it as a user preset,
    /// under "<name> 2", "<name> 3"… if its name is taken. Another plug-in's
    /// file, or a newer format, is refused. Returns its name.
    pub fn import(&mut self, path: &Path) -> Result<String, String> {
        let file = file_name(path);
        let mut p = read_preset(path).map_err(|why| format!("Couldn't import “{file}” — it {why}"))?;
        let name = check_name(&p.name).map_err(|why| format!("Couldn't import “{file}” — {why}"))?;
        let name = self.free_name(&name);
        p.name = name.clone();
        p.kind = Some(Kind::User);
        if !p.facets.contains_key("time") || !p.facets.contains_key("cross") {
            p.derive_facets();
        }
        let dest = self.path_for(&name);
        write_preset(&dest, &p).map_err(|e| format!("Couldn't import — {e}"))?;
        self.insert(p, dest);
        Ok(name)
    }

    /// `base` if free, else `base 2`, `base 3`… (cut to fit 64 characters).
    /// A file nobody lists (unreadable, hidden) holds its name too.
    pub fn free_name(&self, base: &str) -> String {
        let base = base.trim();
        let mut n = fit(base);
        let mut k = 2;
        while self.taken(&identity(&n), None) {
            n = fit(&format!("{base} {k}"));
            k += 1;
        }
        n
    }
}

/// For the retiring v2 / v3 editors (SOF-89), which key presets by name.
/// Remove with them.
impl Library {
    pub fn load() -> Library {
        Library::open_default()
    }

    pub fn names(&self) -> Vec<String> {
        self.presets().map(|p| p.name.clone()).collect()
    }

    /// Their Save: over the user preset of that name, else a new one.
    pub fn save_named(&mut self, name: &str, values: BTreeMap<String, f32>) -> bool {
        match self.entry(name) {
            Some(e) if e.preset.kind() == Kind::User => self.save(&identity(name), values).is_ok(),
            Some(_) => false,
            None => self.save_as(name, values, None).is_ok(),
        }
    }

    pub fn toggle_favorite(&mut self, name: &str) {
        let id = identity(name);
        let on = !self.is_favourite(&id);
        let _ = self.set_favourite(&id, on);
    }
}

/// `base`, else `base 2`, `base 3`… whose identity isn't in `taken`; each
/// cut to 64 characters.
fn unique_name(base: &str, taken: &std::collections::HashSet<String>) -> String {
    let mut n = fit(base);
    let mut k = 2;
    while taken.contains(&identity(&n)) {
        n = fit(&format!("{base} {k}"));
        k += 1;
    }
    n
}

/// A name cut to 64 characters, its end kept (the suffix that makes it unique).
fn fit(name: &str) -> String {
    let c: Vec<char> = name.chars().collect();
    if c.len() <= NAME_MAX {
        return name.to_string();
    }
    c[c.len() - NAME_MAX..].iter().collect::<String>().trim().to_string()
}

// ------------------------------------------------------------------ host session

/// The loaded preset, saved with the host's session (§10) so a reopened
/// project shows "Dub Station · edited", not "Init". Empty `id` = Init.
/// Append-only, every field defaulted.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// "factory" | "user" | "" (Init).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub edited: bool,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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

    /// A factory set for the tests: two presets that differ from Init.
    pub(crate) fn test_factory() -> Vec<Preset> {
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        setter.set_parameter(&p.num_taps_active, 5);
        setter.set_parameter(&p.crossfeed_mode, crate::params::CrossfeedMode::PingPong);
        let mut a = Preset::new("Ping Garden", capture(&p), Kind::Factory);
        a.tags = vec!["rhythmic".into()];
        setter.set_parameter(&p.feedback, 0.8);
        let b = Preset::new("Dark Repeats", capture(&p), Kind::Factory);
        vec![a, b]
    }

    fn open(t: &TempDir) -> Library {
        Library::open(t.0.clone(), None, test_factory())
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> =
            std::fs::read_dir(dir).unwrap().flatten().map(|e| file_name(&e.path())).filter(|n| n.ends_with(".json")).collect();
        v.sort();
        v
    }

    fn raw(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn saved_cycles_read_back_after_the_range_grew() {
        let params = PetalParams::default();
        let (id, ptr, _) = params.param_map().into_iter().find(|(id, _, _)| id == "feedback_time").unwrap();
        // Captured by plain value, whatever the range.
        assert_eq!(capture(&params)[&id], params.feedback_time.value());
        // A preset saved before v2.30 holds 0.4706 (1/4 on a 0..1 range): it
        // must come back as that plain value, in either values format.
        let old = 8.0 / 17.0;
        for fmt in [0, VALUES_FORMAT_PLAIN] {
            // SAFETY: ptr belongs to `params`, alive for the whole test.
            let plain = unsafe { ptr.preview_plain(to_normalized(&id, ptr, old, fmt)) };
            assert!((plain - old).abs() < 1e-5, "format {fmt}: {plain}");
        }
    }

    #[test]
    fn a_preset_missing_a_param_matches_it_at_its_default() {
        let params = PetalParams::default();
        let mut snap = capture(&params);
        assert!(matches(&params, &snap, VALUES_FORMAT_PLAIN));
        // An older preset that predates these params: their defaults stand in.
        snap.remove("feedback_time");
        snap.remove("dry_wet");
        assert!(matches(&params, &snap, VALUES_FORMAT_PLAIN));
        // A real difference still reads as edited.
        *snap.get_mut("feedback").unwrap() += 0.1;
        assert!(!matches(&params, &snap, VALUES_FORMAT_PLAIN));
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
        assert!(params.freeze.value() && matches(&params, &snap, VALUES_FORMAT_PLAIN));
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
        apply(&p, &setter, &snap, VALUES_FORMAT_PLAIN);
        assert_eq!(p.feedback.value(), p.feedback.default_plain_value());
        assert_eq!(p.num_taps_active.value(), p.num_taps_active.default_plain_value());
        assert!((p.dry_wet.unmodulated_normalized_value() - 0.8).abs() < 1e-6);
        assert!(matches(&p, &snap, VALUES_FORMAT_PLAIN));
    }

    /// A fresh instance is Init, and a loaded preset isn't edited: the taps'
    /// Q default sits off its step grid and moves onto it when set.
    #[test]
    fn a_load_is_not_an_edit() {
        let fresh = PetalParams::default();
        assert!(matches(&fresh, &BTreeMap::new(), VALUES_FORMAT_PLAIN));
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        apply(&p, &setter, &BTreeMap::new(), VALUES_FORMAT_PLAIN);
        assert!(matches(&p, &BTreeMap::new(), VALUES_FORMAT_PLAIN));
        let f = test_factory();
        apply(&p, &setter, &f[0].values, f[0].values_format);
        assert!(matches(&p, &f[0].values, f[0].values_format));
        setter.set_parameter(&p.taps[2].filter_q, 3.0);
        assert!(!matches(&p, &f[0].values, f[0].values_format));
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
        apply(&p, &setter, &snap, VALUES_FORMAT_PLAIN);
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
    fn sanitize_is_filesystem_safe() {
        assert_eq!(sanitize("Dub / Station: 2"), "Dub _ Station_ 2");
        assert_eq!(sanitize("   "), "untitled");
        assert_eq!(identity(" Pad "), identity("pad"));
    }

    #[test]
    fn facets_come_from_the_sound() {
        let f = test_factory();
        assert_eq!(f[0].cross(), Cross::Ping);
        assert_eq!(f[0].facets["cross"], "ping");
        assert_eq!(f[0].time(), Time::of(&BTreeMap::new()), "time at its default");
        let p = PetalParams::default();
        let setter = ParamSetter::new(&TestGui);
        setter.set_parameter(&p.sync_mode, crate::params::SyncMode::Sync);
        assert_eq!(Preset::new("x", capture(&p), Kind::User).time(), Time::Sync);
        setter.set_parameter(&p.sync_mode, crate::params::SyncMode::Free);
        assert_eq!(Preset::new("x", capture(&p), Kind::User).time(), Time::Free);
    }

    #[test]
    fn save_as_writes_named_preset_to_disk() {
        let t = TempDir::new("saveas");
        let mut lib = open(&t);
        let p = PetalParams::default();
        let id = lib.save_as("Vocal Throw", capture(&p), Some("ping garden")).unwrap();
        assert_eq!(id, "vocal throw");
        let v = raw(&t.join("Vocal Throw.json"));
        assert_eq!(v["format"], 2);
        assert_eq!(v["plugin"], PLUGIN_ID);
        assert_eq!(v["kind"], "user");
        assert_eq!(v["tags"][0], "rhythmic", "tags copied from the loaded preset");
        assert_eq!(v["facets"]["cross"], "mix", "facets from this sound, not the source");
        // a second library reads it back
        let lib2 = open(&t);
        assert!(lib2.get("vocal throw").is_some_and(|p| p.kind() == Kind::User));
    }

    #[test]
    fn factory_is_read_only() {
        let t = TempDir::new("factory");
        let mut lib = open(&t);
        let v = capture(&PetalParams::default());
        assert_eq!(lib.save("ping garden", v).unwrap_err(), READ_ONLY);
        assert_eq!(lib.rename("ping garden", "x").unwrap_err(), READ_ONLY);
        assert_eq!(lib.delete("ping garden").unwrap_err(), READ_ONLY);
        assert_eq!(lib.add_tag("ping garden", "x").unwrap_err(), READ_ONLY);
        assert!(files(&t.0).iter().all(|f| f == LIBRARY_FILE), "nothing written but the library state");
        // favourites work on it, and Duplicate makes an editable copy
        lib.set_favourite("ping garden", true).unwrap();
        assert!(lib.is_favourite("ping garden"));
        let name = lib.duplicate("ping garden").unwrap();
        assert_eq!(name, "Ping Garden copy");
        assert_eq!(lib.get("ping garden copy").unwrap().kind(), Kind::User);
        assert_eq!(lib.duplicate("ping garden").unwrap(), "Ping Garden copy 2");
    }

    #[test]
    fn rename_and_tags_round_trip() {
        let t = TempDir::new("rename");
        let mut lib = open(&t);
        lib.save_as("Pad", capture(&PetalParams::default()), None).unwrap();
        lib.set_favourite("pad", true).unwrap();
        lib.push_recent("pad").unwrap();
        lib.add_tag("pad", "  Dub ").unwrap();
        lib.add_tag("pad", "dub").unwrap();
        assert_eq!(lib.get("pad").unwrap().tags, vec!["dub"]);
        lib.set_notes("pad", "for\nsnares").unwrap();
        let id = lib.rename("pad", "Slow Pad").unwrap();
        assert_eq!(id, "slow pad");
        assert!(!t.join("Pad.json").exists() && t.join("Slow Pad.json").exists());
        let lib2 = open(&t);
        let p = lib2.get("slow pad").unwrap();
        assert_eq!((p.tags.clone(), p.notes.clone()), (vec!["dub".to_string()], "for snares".to_string()));
        assert!(lib2.is_favourite("slow pad") && lib2.state.recent == vec!["slow pad".to_string()]);
        lib.remove_tag("slow pad", "dub").unwrap();
        assert!(lib.get("slow pad").unwrap().tags.is_empty());
    }

    #[test]
    fn collisions_are_refused() {
        let t = TempDir::new("collide");
        let mut lib = open(&t);
        let v = capture(&PetalParams::default());
        lib.save_as("Pad", v.clone(), None).unwrap();
        assert!(lib.save_as("pad", v.clone(), None).unwrap_err().contains("already exists"));
        assert!(lib.save_as("Ping Garden", v.clone(), None).unwrap_err().contains("already exists"), "a factory name");
        assert_eq!(lib.save_as("  ", v.clone(), None).unwrap_err(), "A preset needs a name");
        lib.save_as("Lead", v, None).unwrap();
        assert!(lib.rename("lead", "PAD").unwrap_err().contains("already exists"));
    }

    #[test]
    fn case_only_rename_keeps_the_file() {
        let t = TempDir::new("case");
        let mut lib = open(&t);
        lib.save_as("pad", capture(&PetalParams::default()), None).unwrap();
        lib.rename("pad", "Pad").unwrap();
        let f: Vec<String> = files(&t.0).into_iter().filter(|f| f != LIBRARY_FILE).collect();
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(raw(&t.0.join(&f[0]))["name"], "Pad");
    }

    #[test]
    fn delete_removes_the_file_and_its_references() {
        let t = TempDir::new("delete");
        let mut lib = open(&t);
        lib.save_as("Pad", capture(&PetalParams::default()), None).unwrap();
        lib.set_favourite("pad", true).unwrap();
        lib.push_recent("pad").unwrap();
        lib.delete("pad").unwrap();
        assert!(!t.join("Pad.json").exists() && lib.get("pad").is_none());
        assert!(lib.state.favourites.is_empty() && lib.state.recent.is_empty());
    }

    #[test]
    fn non_finite_values_never_written() {
        let t = TempDir::new("nan");
        let mut lib = open(&t);
        let mut v = capture(&PetalParams::default());
        v.insert("feedback".into(), f32::NAN);
        v.insert("dry_wet".into(), f32::INFINITY);
        lib.save_as("Odd", v, None).unwrap();
        let s = std::fs::read_to_string(t.join("Odd.json")).unwrap();
        assert!(!s.contains("null"), "{s}");
        let p = read_preset(&t.join("Odd.json")).unwrap();
        assert_eq!(p.values["feedback"], stored_defaults()["feedback"]);
    }

    #[test]
    fn unknown_fields_survive_a_rewrite() {
        let t = TempDir::new("unknown");
        let mut lib = open(&t);
        lib.save_as("Pad", capture(&PetalParams::default()), None).unwrap();
        let mut v = raw(&t.join("Pad.json"));
        v["from_the_future"] = serde_json::json!({ "x": 1 });
        std::fs::write(t.join("Pad.json"), serde_json::to_string(&v).unwrap()).unwrap();
        let mut lib = open(&t);
        lib.add_tag("pad", "dub").unwrap();
        assert_eq!(raw(&t.join("Pad.json"))["from_the_future"]["x"], 1);
    }

    #[test]
    fn an_unreadable_file_is_listed_not_lost() {
        let t = TempDir::new("bad");
        std::fs::write(t.join("Broken.json"), "{ not json").unwrap();
        std::fs::write(t.join("Other.json"), r#"{"plugin":"com.softsystems.vortex","name":"X","values":{}}"#).unwrap();
        let lib = open(&t);
        assert_eq!(lib.unreadable.len(), 2);
        assert!(t.join("Broken.json").exists() && t.join("Other.json").exists());
        assert!(lib.unreadable.iter().any(|u| u.reason.contains("another plug-in")));
    }

    /// The files Afterecho wrote before v2.40 (no format, plugin or kind; a
    /// build stamp and a favourite flag) load as user presets, and their
    /// favourites move into library.json once.
    #[test]
    fn format_1_files_still_load() {
        let t = TempDir::new("fmt1");
        let old = r#"{"name":"Old One","favorite":true,"build":"v2.30.0","tags":["dub"],"values_format":1,"values":{"feedback":0.7,"num_taps_active":3.0}}"#;
        std::fs::write(t.join("Old One.json"), old).unwrap();
        let lib = open(&t);
        let p = lib.get("old one").unwrap();
        assert_eq!(p.kind(), Kind::User);
        assert_eq!(p.values["num_taps_active"], 3.0);
        assert!(lib.is_favourite("old one"));
        assert!(lib.state.favourites_migrated);
    }

    #[test]
    fn old_folder_is_copied_once() {
        let new = TempDir::new("new");
        let old = TempDir::new("old");
        std::fs::write(old.join("Mine.json"), r#"{"name":"Mine","values":{}}"#).unwrap();
        std::fs::write(old.join("Junk.json"), "nope").unwrap();
        let lib = Library::open(new.0.clone(), Some(old.0.clone()), test_factory());
        assert!(lib.get("mine").is_some());
        assert!(!new.join("Junk.json").exists(), "an unreadable file stays behind");
        assert!(old.join("Mine.json").exists(), "the old folder is never touched");
        // deleted here, it doesn't come back from the old folder
        let mut lib = lib;
        lib.delete("mine").unwrap();
        let lib = Library::open(new.0.clone(), Some(old.0.clone()), test_factory());
        assert!(lib.get("mine").is_none());
    }

    #[test]
    fn a_user_file_under_a_factory_name_is_kept_as_edited() {
        let t = TempDir::new("shadow");
        // the same sound as the factory preset: hidden, left on disk
        let same = serde_json::to_string(&test_factory()[0].for_write()).unwrap();
        std::fs::write(t.join("Ping Garden.json"), same.replace("\"factory\"", "\"user\"")).unwrap();
        // a different sound under the other factory name: kept, renamed
        std::fs::write(t.join("Dark Repeats.json"), r#"{"name":"Dark Repeats","values":{"feedback":0.1}}"#).unwrap();
        let lib = open(&t);
        assert!(t.join("Ping Garden.json").exists());
        assert_eq!(lib.presets().filter(|p| p.id() == "ping garden").count(), 1);
        assert!(lib.get("dark repeats (edited)").is_some_and(|p| p.kind() == Kind::User));
        assert_eq!(raw(&t.join("Dark Repeats.json"))["name"], "Dark Repeats (edited)");
    }

    #[test]
    fn import_and_export_round_trip() {
        let t = TempDir::new("io");
        let mut lib = open(&t);
        let out = TempDir::new("io-out");
        lib.export("ping garden", &out.join("pg.json")).unwrap();
        let name = lib.import(&out.join("pg.json")).unwrap();
        assert_eq!(name, "Ping Garden 2");
        assert_eq!(lib.get("ping garden 2").unwrap().kind(), Kind::User);
        assert!(lib.get("ping garden 2").unwrap().same_sound(&test_factory()[0]));
    }

    /// Every param a preset can hold is captured, and a capture → apply round
    /// trip is exact on a fresh instance.
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
        apply(&b, &setter, &snap, VALUES_FORMAT_PLAIN);
        assert!(matches(&b, &snap, VALUES_FORMAT_PLAIN));
        assert_eq!(capture(&b), snap);
    }
}

