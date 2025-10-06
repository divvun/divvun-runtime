mod commands;
mod state;
mod syntax;

use state::PlaygroundState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PlaygroundState::new())
        .invoke_handler(tauri::generate_handler![
            commands::load_bundle,
            commands::run_pipeline,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
