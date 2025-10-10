mod commands;
mod state;
mod syntax;

use state::PlaygroundState;
use tauri::menu::{
    AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::{Manager, WebviewWindowBuilder};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub initial_path: Option<PathBuf>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let cli_args = parse_args();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PlaygroundState::new())
        .manage(cli_args)
        .invoke_handler(tauri::generate_handler![
            commands::init_window,
            commands::get_window_state,
            commands::create_tab,
            commands::close_tab,
            commands::switch_tab,
            commands::duplicate_tab,
            commands::get_tab_data,
            commands::update_tab_input,
            commands::update_tab_view,
            commands::load_bundle,
            commands::list_pipelines,
            commands::run_pipeline,
            commands::list_ftl_files,
            commands::get_ftl_messages,
            commands::test_ftl_message,
            commands::get_cli_args,
        ])
        .setup(|app| {
            let handle = app.handle();

            // Build custom menu items
            let new_window = MenuItemBuilder::with_id("new_window", "New Window")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?;
            let new_tab = MenuItemBuilder::with_id("new_tab", "New Tab")
                .accelerator("CmdOrCtrl+T")
                .build(app)?;
            let close_tab = MenuItemBuilder::with_id("close_tab", "Close Tab")
                .accelerator("CmdOrCtrl+W")
                .build(app)?;
            let duplicate_tab = MenuItemBuilder::with_id("duplicate_tab", "Duplicate Tab")
                .accelerator("CmdOrCtrl+Shift+D")
                .build(app)?;

            // On macOS, the first submenu becomes the app menu with standard items
            // Create app menu (first submenu on macOS)
            let app_name = app.package_info().name.clone();
            let product_name = app.config().product_name.clone().unwrap_or_else(|| "Divvun Runtime Playground".to_string());
            let app_menu = SubmenuBuilder::new(app, &app_name)
                .item(&PredefinedMenuItem::about(app, Some("About Divvun Runtime Playground".into()), None)?)
                .separator()
                .item(&PredefinedMenuItem::hide(app, Some("Hide Divvun Runtime Playground".into()))?)
                .item(&PredefinedMenuItem::hide_others(app, None)?)
                .item(&PredefinedMenuItem::show_all(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::quit(app, None)?)
                .build()?;

            // Create File menu with our custom items
            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&new_window)
                .item(&new_tab)
                .separator()
                .item(&close_tab)
                .separator()
                .item(&duplicate_tab)
                .build()?;

            // Create Edit menu with standard items
            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .item(&PredefinedMenuItem::undo(app, None)?)
                .item(&PredefinedMenuItem::redo(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::cut(app, None)?)
                .item(&PredefinedMenuItem::copy(app, None)?)
                .item(&PredefinedMenuItem::paste(app, None)?)
                .item(&PredefinedMenuItem::select_all(app, None)?)
                .build()?;

            // let window_menu = SubmenuBuilder::new(app, "Window")
            //     .item(&PredefinedMenuItem::minimize(app, None)?)
            //     .item(&PredefinedMenuItem::zoom(app, None)?)
            //     .separator()
            //     .item(&PredefinedMenuItem::bring_all_to_front(app, None)?)
            //     .build()?;

            // Build complete menu (app menu must be first on macOS)
            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&file_menu)
                .item(&edit_menu)
                // .item(&window_menu)
                .build()?;

            app.set_menu(menu)?;

            // Handle menu events
            let handle_clone = handle.clone();
            app.on_menu_event(move |app, event| {
                let window = app.get_webview_window("main");

                match event.id().as_ref() {
                    "new_window" => {
                        let window_id = uuid::Uuid::new_v4().to_string();
                        let _ = WebviewWindowBuilder::new(
                            &handle_clone,
                            window_id.clone(),
                            tauri::WebviewUrl::default(),
                        )
                        .title("Divvun Runtime Playground")
                        .inner_size(1280.0, 800.0)
                        .build();
                    }
                    "new_tab" => {
                        if let Some(window) = &window {
                            let _ = window.eval(&format!(
                                "window.__TAURI_INTERNALS__.invoke('create_tab', {{ windowId: '{}' }})",
                                window.label()
                            ));
                        }
                    }
                    "close_tab" => {
                        if let Some(window) = &window {
                            let _ = window.eval(&format!(
                                "window.dispatchEvent(new CustomEvent('menu-close-tab'))"
                            ));
                        }
                    }
                    "duplicate_tab" => {
                        if let Some(window) = &window {
                            let _ = window.eval(&format!(
                                "window.dispatchEvent(new CustomEvent('menu-duplicate-tab'))"
                            ));
                        }
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();

    let initial_path = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    CliArgs { initial_path }
}
