//! `VERSION`: the workspace version plus `BANDCONTROL_VERSION_SUFFIX`, which builds of main set (e.g. `-dev+abc1234`).

fn main() {
    println!("cargo:rerun-if-env-changed=BANDCONTROL_VERSION_SUFFIX");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let suffix = std::env::var("BANDCONTROL_VERSION_SUFFIX").unwrap_or_default();
    println!("cargo:rustc-env=BANDCONTROL_VERSION={version}{suffix}");
}
