// Embeds the program icon, version info and manifest into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") && cfg!(not(windows)) {
        // Cross-compiling from Linux with MinGW.
        res.set_windres_path("x86_64-w64-mingw32-windres");
        res.set_ar_path("x86_64-w64-mingw32-ar");
    }
    res.set_icon("assets/pico-mixpad.ico");
    res.set_manifest_file("assets/app.manifest");
    res.set("ProductName", "Pico Mixpad");
    res.set("FileDescription", "Pico Mixpad host app");
    res.set("LegalCopyright", "Copyright (c) 2026 Jack Vermaak, MIT licence");
    if let Err(e) = res.compile() {
        println!("cargo:warning=building without the Windows icon and manifest: {e}");
    }
}
