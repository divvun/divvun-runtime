fn main() {
    let target = std::env::var("TARGET").unwrap();
    tauri_build::build()
}
