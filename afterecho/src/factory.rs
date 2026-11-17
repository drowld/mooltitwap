//! The factory presets (the preset spec §5): shipped inside the binary,
//! listed under Factory, read-only. A new `FACTORY_VERSION` can add, fix or
//! remove them without touching the user's files.
//!
//! Each preset is written as its changes from Init, in PLAIN values (what the
//! page shows: taps, ms, %, enum index), and stored the way `presets::capture`
//! stores a sound.

use crate::params::AfterechoParams;
use crate::presets::{sound, Kind, Preset, Store};
use nice_plug::prelude::*;
use softsystems_mod::{Modulator, Rate, Shape, Trigger};

/// Raised whenever the factory set changes (kept in library.json as
/// `factory_version`).
pub const FACTORY_VERSION: u32 = 2;

/// One factory preset: `changes` are (param id, plain value) from Init.
struct Recipe {
    name: &'static str,
    tags: &'static [&'static str],
    notes: &'static str,
    changes: &'static [(&'static str, f32)],
}

// Plain values the recipes use, worked out once (see each formula).
// Sync time: `base_time_l` = the division's index in SYNC_DIVS / 17.
const T_1_16: f32 = 2.0 / 17.0;
const T_1_8: f32 = 5.0 / 17.0;
const T_1_8D: f32 = 6.0 / 17.0;
const T_1_4: f32 = 8.0 / 17.0;
const T_1_4D: f32 = 9.0 / 17.0;
// Free time: `base_time_l` = ln(ms) / ln(2000).
const T_11MS: f32 = 0.3155;
const T_80MS: f32 = 0.5765;
const T_120MS: f32 = 0.6298;
const T_150MS: f32 = 0.6592;
const T_300MS: f32 = 0.7504;
const T_600MS: f32 = 0.8416;
// Loop tone: LP at f = 20 kHz · 0.03^−t (t < 0), HP at f = 20 Hz · 50^t.
const LP_1K5: f32 = -0.7387;
const LP_3K: f32 = -0.5410;
const LP_5K: f32 = -0.3954;
const LP_6K: f32 = -0.3433;
const HP_200: f32 = 0.5886;
const HP_300: f32 = 0.6923;
const HP_600: f32 = 0.8694;
// Enum indices.
const SYNC: f32 = 1.0;
const PING: f32 = 1.0;
const DIFFUSE: f32 = 2.0;
const EXP: f32 = 1.0;
const LOG: f32 = 2.0;
const EUCLID: f32 = 3.0;
const BLOOM: f32 = 1.0;
const ON_FB: f32 = 1.0;
const ON_OUT: f32 = 2.0;
const BP: f32 = 1.0;

/// The set, A–Z (approved 2026-10-31; every one still to be tuned by ear).
const RECIPES: &[Recipe] = &[
    Recipe {
        name: "Bloom Hall",
        tags: &["ambient", "wide"],
        notes: "Six taps diffused in the loop; try Freeze on a held chord.",
        changes: &[
            ("num_taps_active", 6.0), ("spacing_mode", EXP), ("base_time_l", T_600MS), ("feedback", 0.70),
            ("crossfeed_mode", DIFFUSE), ("crossfeed", 0.6), ("diffuse_amount", 0.6), ("diffuse_routing", ON_FB),
            ("fb_curve", BLOOM), ("dry_wet", 0.30),
        ],
    },
    Recipe {
        name: "Dark Repeats",
        tags: &["dub", "dark"],
        notes: "Each repeat darker than the last.",
        changes: &[
            ("num_taps_active", 2.0), ("sync_mode", SYNC), ("base_time_l", T_1_4), ("feedback", 0.70),
            ("loop_tone", LP_1K5), ("fb_curve", BLOOM),
        ],
    },
    Recipe {
        name: "Dotted Eighth",
        tags: &["rhythmic", "wide"],
        notes: "The dotted-eighth bounce, side to side.",
        changes: &[
            ("num_taps_active", 2.0), ("sync_mode", SYNC), ("base_time_l", T_1_8D), ("feedback", 0.50),
            ("crossfeed_mode", PING), ("crossfeed", 1.0), ("dry_wet", 0.35),
        ],
    },
    Recipe {
        name: "Dub Station",
        tags: &["dub", "tape"],
        notes: "Snare throws. Ride Tone for the dark tail.",
        changes: &[
            ("num_taps_active", 3.0), ("sync_mode", SYNC), ("base_time_l", T_1_4D), ("feedback", 0.78),
            ("loop_tone", LP_3K), ("degrade_amount", 0.40), ("degrade_routing", ON_FB),
        ],
    },
    Recipe {
        name: "Filter Drift",
        tags: &["motion", "filter"],
        notes: "Band-passed taps whose filters drift over two bars.",
        changes: &[
            ("num_taps_active", 6.0), ("sync_mode", SYNC), ("base_time_l", T_1_8), ("feedback", 0.45),
            ("tap_filter_type_1", BP), ("tap_filter_type_2", BP), ("tap_filter_type_3", BP),
            ("tap_filter_type_4", BP), ("tap_filter_type_5", BP), ("tap_filter_type_6", BP),
            ("tap_filter_cutoff_1", 1200.0), ("tap_filter_cutoff_2", 1200.0), ("tap_filter_cutoff_3", 1200.0),
            ("tap_filter_cutoff_4", 1200.0), ("tap_filter_cutoff_5", 1200.0), ("tap_filter_cutoff_6", 1200.0),
            ("tap_filter_q_1", 2.0), ("tap_filter_q_2", 2.0), ("tap_filter_q_3", 2.0),
            ("tap_filter_q_4", 2.0), ("tap_filter_q_5", 2.0), ("tap_filter_q_6", 2.0),
        ],
    },
    Recipe {
        name: "Five over Sixteen",
        tags: &["euclid", "rhythmic"],
        notes: "Five taps spread over sixteen steps, each a little quieter.",
        changes: &[
            ("num_taps_active", 5.0), ("spacing_mode", EUCLID), ("euclid_steps", 16.0), ("euclid_rotate", 2.0),
            ("sync_mode", SYNC), ("base_time_l", T_1_16), ("feedback", 0.35),
            ("gain_db_2", -3.0), ("gain_db_3", -6.0), ("gain_db_4", -9.0), ("gain_db_5", -12.0),
        ],
    },
    Recipe {
        name: "Haas Wide",
        tags: &["wide", "short"],
        notes: "Two quick taps, hard left and right: width without echoes.",
        changes: &[
            ("num_taps_active", 2.0), ("base_time_l", T_11MS), ("feedback", 0.0),
            ("pan_1", -1.0), ("pan_2", 1.0), ("dry_wet", 0.40),
        ],
    },
    Recipe {
        name: "Ping Garden",
        tags: &["rhythmic", "motion"],
        notes: "Five taps bounce side to side and dip while you play.",
        changes: &[
            ("num_taps_active", 5.0), ("spacing_mode", EUCLID), ("euclid_steps", 16.0),
            ("sync_mode", SYNC), ("base_time_l", T_1_16), ("feedback", 0.60),
            ("crossfeed_mode", PING), ("crossfeed", 1.0), ("duck", 6.0), ("dry_wet", 0.35),
        ],
    },
    Recipe {
        name: "Quarter Repeats",
        tags: &["clean", "rhythmic"],
        notes: "Plain quarter-note repeats.",
        changes: &[("num_taps_active", 1.0), ("sync_mode", SYNC), ("base_time_l", T_1_4), ("feedback", 0.45), ("dry_wet", 0.35)],
    },
    Recipe {
        name: "Runaway",
        tags: &["self-osc", "noise"],
        notes: "Feedback past unity: it grows until the saturation holds it. Pull Feedback down to stop it.",
        changes: &[
            ("num_taps_active", 3.0), ("base_time_l", T_120MS), ("feedback", 1.05),
            ("sat_amount", 0.50), ("sat_routing", ON_FB), ("loop_tone", HP_200),
        ],
    },
    Recipe {
        name: "Slapback 80",
        tags: &["slapback", "short"],
        notes: "One short slap, a touch dark.",
        changes: &[("num_taps_active", 1.0), ("base_time_l", T_80MS), ("feedback", 0.10), ("loop_tone", LP_6K), ("dry_wet", 0.30)],
    },
    Recipe {
        name: "Swell Duck",
        tags: &["ambient", "duck"],
        notes: "The echoes wait for you to stop, then swell. Freeze holds them.",
        changes: &[
            ("num_taps_active", 4.0), ("sync_mode", SYNC), ("base_time_l", T_1_4), ("feedback", 0.65),
            ("duck", 12.0), ("duck_release", 600.0), ("dry_wet", 0.45),
        ],
    },
    Recipe {
        name: "Swing Shuffle",
        tags: &["swing", "groove"],
        notes: "Sixteenths pushed into a shuffle.",
        changes: &[
            ("num_taps_active", 4.0), ("sync_mode", SYNC), ("base_time_l", T_1_16), ("swing", 0.66), ("feedback", 0.30),
        ],
    },
    Recipe {
        name: "Tape Ghosts",
        tags: &["tape", "lo-fi"],
        notes: "Worn tape on every pass.",
        changes: &[
            ("num_taps_active", 4.0), ("sync_mode", SYNC), ("base_time_l", T_1_8), ("feedback", 0.60),
            ("degrade_amount", 0.70), ("degrade_routing", ON_FB), ("degrade_wear", 0.70), ("loop_tone", LP_5K),
        ],
    },
    Recipe {
        name: "Thin Air",
        tags: &["ambient", "bright"],
        notes: "Many taps, thinned and diffused.",
        changes: &[
            ("num_taps_active", 8.0), ("spacing_mode", LOG), ("base_time_l", T_300MS), ("feedback", 0.55),
            ("crossfeed_mode", DIFFUSE), ("crossfeed", 0.6), ("loop_tone", HP_600),
            ("diffuse_amount", 0.40), ("diffuse_routing", ON_OUT),
        ],
    },
    Recipe {
        name: "Tresillo",
        tags: &["euclid", "rhythmic"],
        notes: "Three over eight: the tresillo.",
        changes: &[
            ("num_taps_active", 3.0), ("spacing_mode", EUCLID), ("euclid_steps", 8.0),
            ("sync_mode", SYNC), ("base_time_l", T_1_16), ("feedback", 0.40), ("crossfeed_mode", PING), ("crossfeed", 1.0),
        ],
    },
    Recipe {
        name: "Vocal Throw",
        tags: &["dub", "vocal", "duck"],
        notes: "For vocal ends. Duck keeps the words clear.",
        changes: &[
            ("num_taps_active", 3.0), ("sync_mode", SYNC), ("base_time_l", T_1_8), ("feedback", 0.55),
            ("crossfeed_mode", PING), ("crossfeed", 1.0), ("loop_tone", HP_300), ("duck", 9.0), ("dry_wet", 0.30),
        ],
    },
    Recipe {
        name: "Wobble Pan",
        tags: &["motion", "wide"],
        notes: "A slow wave of pan through the taps.",
        changes: &[
            ("num_taps_active", 6.0), ("base_time_l", T_150MS), ("feedback", 0.40),
            ("mx_scatter", 1.0),
        ],
    },
];

/// Motion in the recipes that move (beta pass 8, on the shared crate): LFO 1's shape and
/// rate, and its routes as (slot, target id, amount: a share of the knob's travel).
/// v2.42's depths carried over by reach: Pan's knob is two wide, so v2.42's ±0.6 is 0.3 of
/// its travel; Cutoff's ±2.4 octaves round 1.2 kHz is about 0.2 of its skewed travel
/// (170 Hz to 4.6 kHz).
type MotionRecipe = (&'static str, Shape, Rate, &'static [(u16, &'static str, f32)]);
const MOTION: &[MotionRecipe] = &[
    ("Filter Drift", Shape::Glide, Rate::bars(2.0), &[(0, "cutoff", 0.20)]),
    ("Ping Garden", Shape::Sine, Rate::bars(1.0), &[(0, "pan", 0.30)]),
    ("Wobble Pan", Shape::Triangle, Rate::hz(0.7), &[(0, "pan", 0.40)]),
];

/// The factory presets, built from Init plus each recipe's changes.
pub fn factory_presets() -> Vec<Preset> {
    RECIPES.iter().map(build).collect()
}

fn build(r: &Recipe) -> Preset {
    let p = AfterechoParams::default();
    let map = p.param_map();
    // the set was made on the feedback loop (Repeats ∞), before counts (v2.42)
    let base = [("repeats", crate::params::REPEATS_LOOP as f32)];
    for (id, plain) in base.iter().chain(r.changes) {
        if let Some((_, ptr, _)) = map.iter().find(|(pid, _, _)| pid == id) {
            // SAFETY: ptr belongs to `p`, alive for this call; nothing else
            // reads these params.
            unsafe { ptr._internal_set_normalized_value(ptr.preview_normalized(*plain)) };
        } else {
            debug_assert!(false, "factory preset {} names an unknown param {id}", r.name);
        }
    }
    if let Some((_, shape, rate, routes)) = MOTION.iter().find(|m| m.0 == r.name) {
        let mut g = p.motion.patch.lock().unwrap_or_else(|e| e.into_inner());
        if let Modulator::Curve(c) = &mut g.modulators[0] {
            c.shape = *shape;
            c.clock.rate = *rate;
            c.clock.trigger = if rate.unit.is_tempo() { Trigger::Sync } else { Trigger::Free };
        }
        for &(slot, target, amount) in routes.iter() {
            g.routes.set(slot, target, amount, crate::modulation::ROUTE_CAP);
        }
        // the rate's value is a host param (its position in the unit)
        if let Some((_, ptr, _)) = map.iter().find(|(pid, _, _)| pid == "mod1_rate") {
            // SAFETY: as above
            unsafe { ptr._internal_set_normalized_value(ptr.preview_normalized(rate.position())) };
        }
    }
    let mut pr = Preset::new::<Store>(r.name, sound(&p), Kind::Factory);
    pr.author = "Soft Systems".into();
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
        let p = AfterechoParams::default();
        let ids: Vec<String> = p.param_map().into_iter().map(|(id, _, _)| id).collect();
        let mut names = std::collections::HashSet::new();
        for r in RECIPES {
            assert!(names.insert(softsystems_presets::identity(r.name)), "{} twice", r.name);
            for (id, _) in r.changes {
                assert!(ids.iter().any(|x| x == id), "{}: unknown param {id}", r.name);
            }
        }
        assert_eq!(factory_presets().len(), RECIPES.len());
    }

    /// Each preset is what its description says: its facets, and the page's
    /// own reading of it ("In this preset" uses the same params).
    #[test]
    fn each_preset_is_what_it_says() {
        use crate::presets::{Cross, Time};
        let all = factory_presets();
        let get = |n: &str| all.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("{n} missing"));
        let read = |n: &str| {
            let q = AfterechoParams::default();
            let p = get(n);
            crate::presets::load_into(&q, &p.values);
            q
        };
        let want = [
            ("Bloom Hall", Time::Free, Cross::Diffuse),
            ("Dark Repeats", Time::Sync, Cross::Mix),
            ("Dotted Eighth", Time::Sync, Cross::Ping),
            ("Dub Station", Time::Sync, Cross::Mix),
            ("Filter Drift", Time::Sync, Cross::Mix),
            ("Five over Sixteen", Time::Sync, Cross::Mix),
            ("Haas Wide", Time::Free, Cross::Mix),
            ("Ping Garden", Time::Sync, Cross::Ping),
            ("Quarter Repeats", Time::Sync, Cross::Mix),
            ("Runaway", Time::Free, Cross::Mix),
            ("Slapback 80", Time::Free, Cross::Mix),
            ("Swell Duck", Time::Sync, Cross::Mix),
            ("Swing Shuffle", Time::Sync, Cross::Mix),
            ("Tape Ghosts", Time::Sync, Cross::Mix),
            ("Thin Air", Time::Free, Cross::Diffuse),
            ("Tresillo", Time::Sync, Cross::Ping),
            ("Vocal Throw", Time::Sync, Cross::Ping),
            ("Wobble Pan", Time::Free, Cross::Mix),
        ];
        assert_eq!(all.len(), want.len());
        for (n, t, c) in want {
            assert_eq!((Time::of(&get(n).values), Cross::of(&get(n).values)), (t, c), "{n}");
        }
        // the times the descriptions name
        let time = |n: &str| {
            let q = read(n);
            AfterechoParams::format_base_time_mode(q.sync_mode.value(), q.base_time_l.value(), 120.0)
        };
        for (n, t) in [
            ("Dub Station", "1/4."), ("Dotted Eighth", "1/8."), ("Quarter Repeats", "1/4"), ("Tresillo", "1/16"),
            ("Slapback 80", "80.0 ms"), ("Haas Wide", "11.0 ms"), ("Bloom Hall", "600 ms"), ("Thin Air", "300 ms"),
        ] {
            assert_eq!(time(n), t, "{n}");
        }
        // the loop tones they name
        for (n, t) in [
            ("Dark Repeats", "LP 1.5 kHz"), ("Dub Station", "LP 3.0 kHz"), ("Tape Ghosts", "LP 5.0 kHz"),
            ("Slapback 80", "LP 6.0 kHz"), ("Runaway", "HP 200 Hz"), ("Vocal Throw", "HP 300 Hz"), ("Thin Air", "HP 600 Hz"),
        ] {
            assert_eq!(crate::params::tone_text(read(n).loop_tone.value()), t, "{n}");
        }
        // Runaway is allowed past unity; everything else is a plain loop
        assert!(read("Runaway").feedback.value() > 1.0);
        let euclid = read("Tresillo");
        assert_eq!((euclid.num_taps_active.value(), euclid.euclid_steps.value()), (3, 8));
        assert_eq!(read("Five over Sixteen").taps[4].gain_db.value(), -12.0);
        assert_eq!((read("Haas Wide").taps[0].pan.value(), read("Haas Wide").taps[1].pan.value()), (-1.0, 1.0));
        assert!((read("Swing Shuffle").swing.value() - 0.66).abs() < 1e-4);
        assert_eq!(read("Swell Duck").duck.value(), 12.0);
        // Motion's routes and LFO 1 travel in the preset's blob
        let pan = |n: &str| get(n).blobs.mods.as_ref().and_then(|m| m.routes.iter().find(|r| r.target == "pan").map(|r| r.amount));
        assert_eq!(pan("Wobble Pan"), Some(0.40));
        assert_eq!(pan("Ping Garden"), Some(0.30));
        assert!((read("Wobble Pan").motion.scatter.value() - 1.0).abs() < 1e-6);
        let drift = read("Filter Drift");
        assert_eq!(drift.taps[5].filter_type.value(), crate::params::FilterType::BandPass);
        assert!((drift.taps[5].filter_q.value() - 2.0).abs() < 0.01);
        // every one is its own sound, and none is Init
        let same = crate::presets::tests::same_sound;
        let init = Preset::new::<Store>("Init", sound(&AfterechoParams::default()), Kind::User);
        for (i, a) in all.iter().enumerate() {
            assert!(!same(a, &init), "{} is Init", a.name);
            for b in &all[i + 1..] {
                assert!(!same(a, b), "{} = {}", a.name, b.name);
            }
        }
    }
}
