//! Named preset system — GUI/editor thread only, independent of the host's own
//! state save (gives named presets the DAW can't). Ported from cumulus/Inertia.
//!
//! INVARIANT: never touched from the audio `process()` callback.
//!
//! Key rule: discrete params (`step_count().is_some()`) are stored by PLAIN
//! value, floats by NORMALIZED value — a normalized enum index silently remaps
//! when a variant is appended. `values_format` tags the format.

use crate::params::PetalParams;
use nice_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const VALUES_FORMAT_PLAIN: u32 = 1;

/// Snapshot every param: discrete by plain value, floats normalized.
pub fn capture(params: &PetalParams) -> BTreeMap<String, f32> {
    let mut map = BTreeMap::new();
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: ptr comes from this same params and outlives the call.
        let v = unsafe {
            if ptr.step_count().is_some() {
                ptr.unmodulated_plain_value()
            } else {
                ptr.unmodulated_normalized_value()
            }
        };
        map.insert(id, v);
    }
    map
}

unsafe fn to_normalized(ptr: ParamPtr, stored: f32, fmt: u32) -> f32 {
    if fmt >= VALUES_FORMAT_PLAIN && unsafe { ptr.step_count() }.is_some() {
        unsafe { ptr.preview_normalized(stored) }
    } else {
        stored
    }
}

/// Restore params by driving each like a knob gesture so the host records
/// automation/undo. Unknown ids skipped; missing ids keep their current value.
pub fn apply(params: &PetalParams, setter: &ParamSetter, snap: &BTreeMap<String, f32>, fmt: u32) {
    for (id, ptr, _group) in params.param_map() {
        if let Some(&stored) = snap.get(&id) {
            // SAFETY: ptr belongs to params; begin/set/end like a knob drag.
            unsafe {
                let v = to_normalized(ptr, stored, fmt);
                setter.raw_context.raw_begin_set_parameter(ptr);
                setter.raw_context.raw_set_parameter_normalized(ptr, v);
                setter.raw_context.raw_end_set_parameter(ptr);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub values_format: u32,
    pub values: BTreeMap<String, f32>,
}

pub struct Library {
    pub dir: PathBuf,
    pub presets: Vec<Preset>,
}

fn data_dir() -> PathBuf {
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

    /// Capture the live params under `name`, writing a JSON file. Overwrites an
    /// existing preset of the same name. Only mutates memory on a successful write.
    pub fn save_as(&mut self, name: &str, params: &PetalParams) -> bool {
        let preset = Preset {
            name: name.to_string(),
            values_format: VALUES_FORMAT_PLAIN,
            values: capture(params),
        };
        let path = self.dir.join(format!("{}.json", sanitize(name)));
        let Ok(txt) = serde_json::to_string_pretty(&preset) else { return false };
        if std::fs::write(&path, txt).is_err() {
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
}
