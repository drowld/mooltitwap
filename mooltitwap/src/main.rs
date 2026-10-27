//! Standalone host (desktop only) — for local visual testing.
//! On iOS the plugin ships as an AUv3 (no standalone host); this bin is a stub
//! so `cargo build --target aarch64-apple-ios*` doesn't try to link baseview.

#[cfg(not(target_os = "ios"))]
fn main() {
    nice_plug::prelude::nice_export_standalone::<mooltitwap::Mooltitwap>();
}

#[cfg(target_os = "ios")]
fn main() {}
