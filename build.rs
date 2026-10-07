fn main() {
    println!("cargo:rerun-if-changed=src/macos_session.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new().file("src/macos_session.c")
            .flag_if_supported("-Wno-deprecated-declarations")
            .compile("hgs_macos_session");
        for library in ["framework=Security", "framework=CoreFoundation", "bsm"] {
            println!("cargo:rustc-link-lib={library}");
        }
    }
}
