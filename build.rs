fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => println!(
            "cargo::rustc-link-arg-cdylib=-Wl,-install_name,@rpath/libclaviron_orbit.dylib"
        ),
        Ok("linux" | "freebsd") => println!(
            "cargo::rustc-link-arg-cdylib=-Wl,-soname,libclaviron_orbit.so"
        ),
        _ => {}
    }
}
