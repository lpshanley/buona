fn main() {
    // Expose the cargo build target as a compile-time environment variable.
    // Used by self_update::platform::current_target() to select the correct
    // prebuilt binary from GitHub Releases.
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-env=TARGET={target}");
}
