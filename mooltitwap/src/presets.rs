//! Named preset system — GUI/editor thread only, independent of the host's own
//! state save (gives named presets the DAW can't). Ported from cumulus/Inertia.
//!
//! INVARIANT: never touched from the audio `process()` callback.
//!
//! Key rule: discrete params (`step_count().is_some()`) are stored by PLAIN
//! value, floats by NORMALIZED value — a normalized enum index silently remaps
//! when a variant is appended. `values_format` tags the format. The floats in
//! `PLAIN_FLOATS` are stored by plain value too, because their range grew.

use crate::params::PetalParams;
use nice_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const VALUES_FORMAT_PLAIN: u32 = 1;

/// Floats whose range grew past 0..1 after presets were already saved
/// (`feedback_time`, v2.30): stored by PLAIN value. Before the change their
/// normalized value equalled the plain one, so every preset, old or new, reads
/// back exactly.
const PLAIN_FLOATS: &[&str] = &["feedback_time"];

/// Whether `id` is stored by plain value (discrete, or in PLAIN_FLOATS).
unsafe fn stored_plain(id: &str, ptr: ParamPtr) -> bool {
    unsafe { ptr.step_count() }.is_some() || PLAIN_FLOATS.contains(&id)
}

/// Snapshot every param: discrete by plain value, floats normalized.
pub fn capture(params: &PetalParams) -> BTreeMap<String, f32> {
    let mut map = BTreeMap::new();
    for (id, ptr, _group) in params.param_map() {
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
/// default instead of keeping whatever was set. Unknown ids are skipped.
pub fn apply(params: &PetalParams, setter: &ParamSetter, snap: &BTreeMap<String, f32>, fmt: u32) {
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: ptr belongs to params; begin/set/end like a knob drag.
        unsafe {
            let v = match snap.get(&id) {
                Some(&stored) => to_normalized(&id, ptr, stored, fmt),
                None => ptr.default_normalized_value(),
            };
            setter.raw_context.raw_begin_set_parameter(ptr);
            setter.raw_context.raw_set_parameter_normalized(ptr, v);
            setter.raw_context.raw_end_set_parameter(ptr);
        }
    }
}

/// True if the live params still match `snap` (within float tolerance) — drives
/// the preset-pill "dirty" asterisk. Compares in the stored domain (plain for
/// discrete, normalized for floats), same as `capture`; a param the snapshot
/// doesn't mention is compared with its default, as `apply` loads it.
pub fn matches(params: &PetalParams, snap: &BTreeMap<String, f32>) -> bool {
    params.param_map().into_iter().all(|(id, ptr, _group)| {
        // SAFETY: ptr comes from these params and outlives the call.
        let (live, default) = unsafe {
            if stored_plain(&id, ptr) {
                (ptr.unmodulated_plain_value(), ptr.preview_plain(ptr.default_normalized_value()))
            } else {
                (ptr.unmodulated_normalized_value(), ptr.default_normalized_value())
            }
        };
        (live - snap.get(&id).copied().unwrap_or(default)).abs() <= 1e-4
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub favorite: bool,
    /// Build/version stamp of the plugin that saved this preset (shown per row).
    #[serde(default)]
    pub build: String,
    /// Free-form descriptor tags, shown + searchable in the palette.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub values_format: u32,
    pub values: BTreeMap<String, f32>,
}

pub struct Library {
    pub dir: PathBuf,
    pub presets: Vec<Preset>,
}

pub(crate) fn data_dir() -> PathBuf {
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h).join("Library/Application Support/Mooltitwap/presets");
    }
    std::env::temp_dir().join("mooltitwap-presets")
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

impl Library {
    pub fn load() -> Self {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut presets = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(txt) = std::fs::read_to_string(&p) {
                        if let Ok(pr) = serde_json::from_str::<Preset>(&txt) {
                            presets.push(pr);
                        }
                    }
                }
            }
        }
        presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Self { dir, presets }
    }

    pub fn names(&self) -> Vec<String> {
        self.presets.iter().map(|p| p.name.clone()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.name == name)
    }

    /// Write a single preset to its JSON file. Returns success.
    fn write(&self, p: &Preset) -> bool {
        let path = self.dir.join(format!("{}.json", sanitize(&p.name)));
        let Ok(txt) = serde_json::to_string_pretty(p) else { return false };
        std::fs::write(&path, txt).is_ok()
    }

    /// Capture the live params under `name`, writing a JSON file. Overwrites an
    /// existing preset of the same name (preserving its favorite + tags). Only
    /// mutates memory on a successful write. `build` stamps the plugin version.
    pub fn save_as(&mut self, name: &str, params: &PetalParams, build: &str) -> bool {
        let (favorite, tags) = self
            .get(name)
            .map(|p| (p.favorite, p.tags.clone()))
            .unwrap_or((false, Vec::new()));
        let preset = Preset {
            name: name.to_string(),
            favorite,
            build: build.to_string(),
            tags,
            values_format: VALUES_FORMAT_PLAIN,
            values: capture(params),
        };
        if !self.write(&preset) {
            return false;
        }
        if let Some(slot) = self.presets.iter_mut().find(|p| p.name == name) {
            *slot = preset;
        } else {
            self.presets.push(preset);
            self.presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        true
    }

    /// Delete a preset's JSON file and drop it from memory. No-op if absent.
    pub fn delete(&mut self, name: &str) -> bool {
        let path = self.dir.join(format!("{}.json", sanitize(name)));
        let _ = std::fs::remove_file(&path);
        let before = self.presets.len();
        self.presets.retain(|p| p.name != name);
        self.presets.len() != before
    }

    /// Rename a preset (file + memory), preserving favorite/tags/values. Fails on
    /// empty new name, collision with a DIFFERENT preset, missing `old`, or write
    /// failure. A case-only change keeps the same underlying file.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || self.get(old).is_none() {
            return false;
        }
        if !old.eq_ignore_ascii_case(new) && self.get(new).is_some() {
            return false;
        }
        let idx = match self.index_of(old) {
            Some(i) => i,
            None => return false,
        };
        let mut preset = self.presets[idx].clone();
        preset.name = new.to_string();
        if !self.write(&preset) {
            return false;
        }
        // remove the old file if the sanitized path actually differs
        let old_path = self.dir.join(format!("{}.json", sanitize(old)));
        let new_path = self.dir.join(format!("{}.json", sanitize(new)));
        if old_path != new_path {
            let _ = std::fs::remove_file(&old_path);
        }
        self.presets[idx] = preset;
        self.presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        true
    }

    pub fn toggle_favorite(&mut self, name: &str) {
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == name) {
            p.favorite = !p.favorite;
            let p = p.clone();
            self.write(&p);
        }
    }

    pub fn add_tag(&mut self, name: &str, tag: &str) {
        let tag = tag.trim();
        if tag.is_empty() {
            return;
        }
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == name) {
            if !p.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                p.tags.push(tag.to_string());
                let p = p.clone();
                self.write(&p);
            }
        }
    }

    pub fn remove_tag(&mut self, name: &str, tag: &str) {
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == name) {
            let before = p.tags.len();
            p.tags.retain(|t| t != tag);
            if p.tags.len() != before {
                let p = p.clone();
                self.write(&p);
            }
        }
    }

    /// Index of `name` in the sorted list, if present (for palette selection).
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.presets.iter().position(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches(&params, &snap));
        // An older preset that predates these params: their defaults stand in.
        snap.remove("feedback_time");
        snap.remove("dry_wet");
        assert!(matches(&params, &snap));
        // A real difference still reads as edited.
        *snap.get_mut("feedback").unwrap() += 0.1;
        assert!(!matches(&params, &snap));
    }
}
