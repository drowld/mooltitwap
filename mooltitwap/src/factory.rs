//! The factory presets (the preset spec §5): shipped inside the binary,
//! listed under Factory, read-only. A new `FACTORY_VERSION` can add, fix or
//! remove them without touching the user's files.
//!
//! Each preset is written as its changes from Init, in PLAIN values (what the
//! page shows: taps, ms, %, enum index), and stored the way `presets::capture`
//! stores a sound.

use crate::params::PetalParams;
use crate::presets::{capture, Kind, Preset};
use nice_plug::prelude::*;

/// Raised whenever the factory set changes (nothing reads it yet).
#[allow(dead_code)]
pub const FACTORY_VERSION: u32 = 1;

/// One factory preset: `changes` are (param id, plain value) from Init.
struct Recipe {
    name: &'static str,
    tags: &'static [&'static str],
    notes: &'static str,
    changes: &'static [(&'static str, f32)],
}

/// The set, A–Z. Empty until the owner signs off the list.
const RECIPES: &[Recipe] = &[];

/// The factory presets, built from Init plus each recipe's changes.
pub fn factory_presets() -> Vec<Preset> {
    RECIPES.iter().map(build).collect()
}

fn build(r: &Recipe) -> Preset {
    let p = PetalParams::default();
    let map = p.param_map();
    for (id, plain) in r.changes {
        if let Some((_, ptr, _)) = map.iter().find(|(pid, _, _)| pid == id) {
            // SAFETY: ptr belongs to `p`, alive for this call; nothing else
            // reads these params.
            unsafe { ptr._internal_set_normalized_value(ptr.preview_normalized(*plain)) };
        } else {
            debug_assert!(false, "factory preset {} names an unknown param {id}", r.name);
        }
    }
    let mut pr = Preset::new(r.name, capture(&p), Kind::Factory);
    pr.tags = r.tags.iter().map(|t| t.to_string()).collect();
    pr.notes = r.notes.to_string();
    pr
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every recipe names real params and a unique name.
    #[test]
    fn recipes_are_sound() {
        let p = PetalParams::default();
        let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
        let mut names = std::collections::HashSet::new();
        for r in RECIPES {
            assert!(names.insert(crate::presets::identity(r.name)), "{} twice", r.name);
            for (id, _) in r.changes {
                assert!(ids.iter().any(|x| x == id), "{}: unknown param {id}", r.name);
            }
        }
        assert_eq!(factory_presets().len(), RECIPES.len());
    }
}
