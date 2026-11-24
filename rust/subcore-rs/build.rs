fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some()
    {
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
