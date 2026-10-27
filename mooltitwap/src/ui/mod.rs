//! The UI layer the scope-first page is built on: Tide's own copy of the Epoché
//! widgets, theme and tokens (y-flanger `src/{epo,theme,tokens}.rs`, copied
//! 2026-10-27). Afterecho follows Tide and no longer builds new UI on the
//! shared `epoche-ui` crate; the v2/v3 editors keep using it until the new page
//! replaces them, then that dependency goes. Keep these files close to Tide's so
//! fixes can move between the two plugins.
#![allow(dead_code)]

pub mod epo;
pub mod theme;
pub mod tokens;
