fn main() {
    tauri_build::build();
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "macos") {
        cc::Build::new()
            .file("src/macos_drop.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("sticker_macos_drop");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rerun-if-changed=src/macos_drop.m");
    }
}
