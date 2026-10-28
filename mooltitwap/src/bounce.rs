//! Ping (v2.32): the repeats bounce between left and right.
//!
//! The loop runs in two mono lines beside the main ones: one for the odd
//! repeats (bound left), one for the even repeats (bound right). The mono sum
//! of what the main lines hold starts each bounce, and each repeat comes back
//! as the next one on the other side. The main lines keep only the dry input,
//! so every tap places the bounced repeats itself: at its own pan, squeezed
//! toward the side the repeat is bound for. A spread pattern swings as a whole
//! instead of losing the taps panned the other way.
//!
//! Crossfeed is the width. At 0 % every repeat sits at its tap's pan, which
//! is how the main lines play mono content. At 100 % odd repeats go hard left
//! and even ones hard right. The Echo lane draws from these same functions,
//! so the picture and the audio agree.

use crate::params::ChannelMode;

/// Where a repeat sits, −1 (left) … +1 (right): the tap's pan squeezed toward
/// the repeat's side by `width`.
pub fn swing_pos(pan: f32, width: f32, odd: bool) -> f32 {
    let side = if odd { -width } else { width };
    side + (1.0 - width) * pan
}

/// The taps' constant-power pan law: (left, right) gains for −1 … +1.
pub fn pan_gains(pos: f32) -> (f32, f32) {
    let a = (pos.clamp(-1.0, 1.0) + 1.0) * core::f32::consts::FRAC_PI_4;
    (a.cos(), a.sin())
}

/// The bounce width Crossfeed sets. Mono plays both sides as one, so there
/// the repeats stay where the taps put them.
pub fn width(ch: ChannelMode, crossfeed: f32) -> f32 {
    if ch == ChannelMode::Mono { 0.0 } else { crossfeed.clamp(0.0, 1.0) }
}

/// The (left, right) gains a mono source gets from a tap on pass `k`: 0 is
/// the taps' own pass, then the repeats. What the Echo lane draws.
pub fn mono_sides(ch: ChannelMode, pan: f32, width: f32, k: usize) -> (f32, f32) {
    let (a, b) = if k == 0 {
        let (pl, pr) = pan_gains(pan);
        let (m0, m1) = mono_to(ch, 1.0);
        (pl * m0, pr * m1)
    } else {
        let g = placement(ch, pan, width);
        if k % 2 == 1 { (g[0], g[1]) } else { (g[2], g[3]) }
    };
    match ch {
        ChannelMode::Stereo => (a, b),
        ChannelMode::MidSide => (a + b, a - b),
        ChannelMode::Mono => (0.5 * (a + b), 0.5 * (a + b)),
    }
}

/// A tap's gains for the bounced repeats into its two channels (L/R, or M/S
/// in Mid/Side), as `[odd→0, odd→1, even→0, even→1]`. Width 0 places them
/// exactly as the main lines play mono content.
pub fn placement(ch: ChannelMode, pan: f32, width: f32) -> [f32; 4] {
    match ch {
        ChannelMode::MidSide => {
            // Pan is the tap's mid/side balance here, so the bounce swings
            // around the centre, goes in as its mid and side parts, and the
            // tap's balance applies on top (√2: centred = unity).
            let (m, s) = pan_gains(pan);
            let (m, s) = (m * core::f32::consts::SQRT_2, s * core::f32::consts::SQRT_2);
            let (ol, or) = pan_gains(-width);
            let (el, er) = pan_gains(width);
            [(ol + or) * 0.5 * m, (ol - or) * 0.5 * s, (el + er) * 0.5 * m, (el - er) * 0.5 * s]
        }
        ChannelMode::Stereo | ChannelMode::Mono => {
            let (ol, or) = pan_gains(swing_pos(pan, width, true));
            let (el, er) = pan_gains(swing_pos(pan, width, false));
            [ol, or, el, er]
        }
    }
}

/// The two channels' mono sum, which starts a bounce (Mid/Side: the mid).
pub fn mono_of(ch: ChannelMode, a: f32, b: f32) -> f32 {
    match ch {
        ChannelMode::MidSide => a,
        ChannelMode::Stereo | ChannelMode::Mono => 0.5 * (a + b),
    }
}

/// Mono content as the two channels carry it.
pub fn mono_to(ch: ChannelMode, v: f32) -> (f32, f32) {
    match ch {
        ChannelMode::MidSide => (v, 0.0),
        ChannelMode::Stereo | ChannelMode::Mono => (v, v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_zero_places_repeats_like_mono_content() {
        for ch in [ChannelMode::Stereo, ChannelMode::MidSide, ChannelMode::Mono] {
            for pan in [-1.0, -0.4, 0.0, 0.7, 1.0] {
                let (pl, pr) = pan_gains(pan);
                // the main lines play mono content v as (v·pl, v·pr) — M/S: (v·pl, 0)
                let (a, b) = mono_to(ch, 1.0);
                let g = placement(ch, pan, 0.0);
                for (got, want) in [(g[0], a * pl), (g[1], b * pr), (g[2], a * pl), (g[3], b * pr)] {
                    assert!((got - want).abs() < 1e-6, "{ch:?} pan {pan}: {g:?}");
                }
            }
        }
    }

    #[test]
    fn full_width_bounces_hard_left_then_right_at_full_power() {
        for pan in [-1.0, 0.0, 0.5, 1.0] {
            assert_eq!(placement(ChannelMode::Stereo, pan, 1.0).map(|v| (v * 1e4).round() / 1e4), [1.0, 0.0, 0.0, 1.0]);
        }
        // Mid/Side: decoded l = M + S, r = M − S
        let g = placement(ChannelMode::MidSide, 0.0, 1.0);
        assert!((g[0] + g[1] - 1.0).abs() < 1e-6 && (g[0] - g[1]).abs() < 1e-6);
        assert!((g[2] + g[3]).abs() < 1e-6 && (g[2] - g[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_spread_pattern_keeps_its_order_as_it_swings() {
        // half width: a left tap lands hard left, a right tap at the centre
        assert!((swing_pos(-1.0, 0.5, true) + 1.0).abs() < 1e-6);
        assert!(swing_pos(1.0, 0.5, true).abs() < 1e-6);
        assert!(swing_pos(-1.0, 0.5, false).abs() < 1e-6);
        assert!((swing_pos(1.0, 0.5, false) - 1.0).abs() < 1e-6);
    }
}
