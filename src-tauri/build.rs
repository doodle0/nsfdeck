fn main() {
    // generate_context!() embeds the icons, but tauri-build doesn't watch them,
    // so without this an icon change never reaches the binary.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
