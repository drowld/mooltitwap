//! Old sessions and presets into v2.42's params, so they sound as they did.
//!
//! - **Repeats** (v2.42) starts new instances on a count; anything saved
//!   before it had the feedback loop, so it loads as ∞.
//! - **Motion** (v2.39) had one source with four depths. Its source is LFO 1
//!   (same ids) and its depths become LFO 1's row of the matrix, or Follow's
//!   row when the source was Env (the input's level). Its rate followed
//!   Time's Sync; LFO 1 now has its own Sync, set from Time's.

use nice_plug::plugin::{ParamValue, PluginState};
use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::params::{MotionShape, REPEATS_LOOP};
use crate::presets::VALUES_FORMAT_PLAIN;

const OLD_DEPTHS: [(&str, &str); 4] =
    [("motion_pan", "pan"), ("motion_gain", "gain"), ("motion_cutoff", "cutoff"), ("motion_reso", "reso")];
const ENV_INDEX: usize = MotionShape::Env as usize;

/// A host session (plain values).
pub fn session(state: &mut PluginState) {
    let p = &mut state.params;
    if !p.contains_key("repeats") {
        p.insert("repeats".into(), ParamValue::I32(REPEATS_LOOP));
    }
    let env = matches!(p.get("motion_shape"), Some(ParamValue::String(s)) if s == "env");
    let row = if env { "mx_follow" } else { "mx_lfo1" };
    let mut moved = false;
    for (old, t) in OLD_DEPTHS {
        if let Some(ParamValue::F32(d)) = p.remove(old) {
            p.entry(format!("{row}_{t}")).or_insert(ParamValue::F32(d));
            moved = true;
        }
    }
    if moved && env {
        p.insert("motion_shape".into(), ParamValue::String("sine".into()));
    }
    if !p.contains_key("motion_sync") {
        let synced = matches!(p.get("sync_mode"), Some(ParamValue::String(s)) if s == "sync");
        p.insert("motion_sync".into(), ParamValue::Bool(synced));
    }
}

/// A preset's values (`fmt`: floats normalized; discrete params plain from
/// format 1 on, normalized before). An empty map is Init and stays empty.
pub fn preset(snap: &BTreeMap<String, f32>, fmt: u32) -> Cow<'_, BTreeMap<String, f32>> {
    let old_motion = OLD_DEPTHS.iter().any(|(old, _)| snap.contains_key(*old));
    let needs = !snap.is_empty() && (!snap.contains_key("repeats") || !snap.contains_key("motion_sync") || old_motion);
    if !needs {
        return Cow::Borrowed(snap);
    }
    let plain = fmt >= VALUES_FORMAT_PLAIN;
    let mut m = snap.clone();
    m.entry("repeats".into()).or_insert(if plain { REPEATS_LOOP as f32 } else { 1.0 });
    // an enum's stored value → its index (6 variants after Sine … Env: 7)
    let index = |v: f32, n: usize| if plain { v.round() as usize } else { (v * (n - 1) as f32).round() as usize };
    let env = m.get("motion_shape").is_some_and(|&v| index(v, ENV_INDEX + 1) == ENV_INDEX);
    let row = if env { "mx_follow" } else { "mx_lfo1" };
    for (old, t) in OLD_DEPTHS {
        // depths ran 0…1 (normalized = plain); cells run −1…1
        if let Some(d) = m.remove(old) {
            m.entry(format!("{row}_{t}")).or_insert((d + 1.0) * 0.5);
        }
    }
    if env && old_motion {
        m.insert("motion_shape".into(), 0.0);
    }
    if !m.contains_key("motion_sync") {
        let synced = m.get("sync_mode").is_some_and(|&v| index(v, 2) == 1);
        m.insert("motion_sync".into(), if synced { 1.0 } else { 0.0 });
    }
    Cow::Owned(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::PetalParams;
    use nice_plug::prelude::Params;

    #[test]
    fn an_old_session_keeps_its_loop_and_its_motion() {
        let mut st = PluginState { version: String::new(), params: BTreeMap::new(), fields: BTreeMap::new() };
        st.params.insert("motion_shape".into(), ParamValue::String("env".into()));
        st.params.insert("motion_cutoff".into(), ParamValue::F32(0.6));
        st.params.insert("sync_mode".into(), ParamValue::String("sync".into()));
        session(&mut st);
        let p = &st.params;
        assert!(matches!(p.get("repeats"), Some(ParamValue::I32(REPEATS_LOOP))));
        assert!(matches!(p.get("mx_follow_cutoff"), Some(ParamValue::F32(v)) if (*v - 0.6).abs() < 1e-6));
        assert!(!p.contains_key("motion_cutoff"));
        assert!(matches!(p.get("motion_shape"), Some(ParamValue::String(s)) if s == "sine"));
        assert!(matches!(p.get("motion_sync"), Some(ParamValue::Bool(true))));
        // every id it writes is a real param
        let ids: Vec<String> = PetalParams::default().param_map().into_iter().map(|(id, _, _)| id).collect();
        for k in p.keys().filter(|k| *k != "sync_mode") {
            assert!(ids.contains(k), "{k}");
        }
    }

    #[test]
    fn an_old_preset_keeps_its_loop_and_its_motion() {
        let mut snap = BTreeMap::new();
        snap.insert("motion_pan".to_string(), 0.8);
        snap.insert("motion_shape".to_string(), 1.0);
        let m = preset(&snap, VALUES_FORMAT_PLAIN);
        assert_eq!(m["repeats"], REPEATS_LOOP as f32);
        assert!((m["mx_lfo1_pan"] - 0.9).abs() < 1e-6);
        assert_eq!(m["motion_shape"], 1.0);
        assert_eq!(m["motion_sync"], 0.0);
        // Init stays Init, and a v2.42 preset is left alone
        assert!(preset(&BTreeMap::new(), VALUES_FORMAT_PLAIN).is_empty());
        let mut new = BTreeMap::new();
        new.insert("repeats".to_string(), 3.0);
        new.insert("motion_sync".to_string(), 1.0);
        assert!(matches!(preset(&new, VALUES_FORMAT_PLAIN), Cow::Borrowed(_)));
    }
}
