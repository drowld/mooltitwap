// - The plug-in dylib's install name is its build path (/Volumes/…/libafterecho.dylib)
//   unless set, which leaks the folder layout into the shipped plug-in (nice-plug
//   tools/check-release-binary.py). Hosts load a bundle by path, so a neutral @rpath name
//   changes nothing for them. Set at link time, the linker's ad-hoc signature stays valid
//   (Tide's way). The iOS bundler sets its own name.
// - The build the ☰ panel's footer, the beta pop-up and its bug reports name (Strata's way):
//   AFTERECHO_BUILD "1.0.0-beta.1 · a1b2c3d" and AFTERECHO_BUILD_ID
//   "afterecho 1.0.0-beta.1 (a1b2c3d)", "+" after the hash when tracked files have
//   uncommitted changes. Without git, the plain version.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/Afterecho");
    }
    let version = env!("CARGO_PKG_VERSION");
    let hash = git(&["rev-parse", "--short", "HEAD"]).filter(|h| !h.is_empty());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    let (shown, id) = match hash {
        Some(h) => {
            let h = format!("{h}{}", if dirty { "+" } else { "" });
            (format!("{version} · {h}"), format!("afterecho {version} ({h})"))
        }
        None => (version.to_string(), format!("afterecho {version}")),
    };
    println!("cargo:rustc-env=AFTERECHO_BUILD={shown}");
    println!("cargo:rustc-env=AFTERECHO_BUILD_ID={id}");
    // a new commit, a checkout, or an edit to the sources changes it
    for p in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", p]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(head) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &head]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
