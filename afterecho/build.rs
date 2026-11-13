// The plug-in dylib's install name is its build path (/Volumes/…/libafterecho.dylib)
// unless set, which leaks the folder layout into the shipped plug-in (nice-plug
// tools/check-release-binary.py). Hosts load a bundle by path, so a neutral @rpath name
// changes nothing for them. Set at link time, the linker's ad-hoc signature stays valid
// (Tide's way). The iOS bundler sets its own name.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/Afterecho");
    }
}
