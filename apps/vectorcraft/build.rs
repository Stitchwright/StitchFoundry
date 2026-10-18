//! Windows only: embed the app icon and version info (VERSIONINFO) into `stitchfoundry.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `VECTORCRAFT_REQUIRE_WINRES=1` turns it into
//! an error (for release builds).

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/stitchfoundry.ico");
    println!("cargo:rerun-if-env-changed=VECTORCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/stitchfoundry.ico")
        .set("ProductName", "StitchFoundry")
        .set("FileDescription", "StitchFoundry machine embroidery digitizer")
        .set("CompanyName", "SiteSentrix")
        .set("LegalCopyright", "Copyright (c) Bruce Wattendorf (SiteSentrix) and the VectorCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "stitchfoundry.exe")
        .set("InternalName", "stitchfoundry");
    if let Err(e) = res.compile() {
        if std::env::var_os("VECTORCRAFT_REQUIRE_WINRES").is_some() {
            return Err(format!("embedding Windows resources failed: {e}"));
        }
        println!("cargo:warning=stitchfoundry.exe built without icon/version resources: {e}");
    }
    Ok(())
}
