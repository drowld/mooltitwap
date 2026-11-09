//! The UI layer the scope-first page is built on: Tide's own copy of the Epoché
//! widgets, theme and tokens (y-flanger `src/{epo,theme,tokens}.rs`, copied
//! 2026-10-27). Colours, fonts and the shared controls now come from
//! `softsystems-ui` (v2.49); what's left here are Afterecho's own widgets. Keep these files close to Tide's so
//! fixes can move between the two plugins.
#![allow(dead_code)]

pub mod epo;
pub mod theme;
pub mod tokens;
