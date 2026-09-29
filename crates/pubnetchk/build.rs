// Embeds Info.plist into the `pubnetchk` binary's Mach-O __TEXT,__info_plist
// section on macOS — the established way for a loose executable (not an
// .app bundle) to carry the usage-description metadata CoreLocation's
// authorization prompt needs. See macos_location.rs and
// docs/decisions/2026-09-04-macos-location-authorization-request.md.
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let plist_path = std::path::Path::new(&manifest_dir).join("Info.plist");
    println!("cargo:rerun-if-changed={}", plist_path.display());
    println!(
        "cargo:rustc-link-arg-bin=pubnetchk=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist_path.display()
    );
}
