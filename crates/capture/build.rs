fn main() {
    // The ScreenCaptureKit bindings are Swift: examples need the system's Swift
    // runtime on their rpath, like the app (see crates/app/build.rs).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-examples=-Wl,-rpath,/usr/lib/swift");
    }
}
