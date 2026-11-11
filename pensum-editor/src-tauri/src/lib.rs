use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let create_item = MenuItem::with_id(app, "create", "Ստեղծել", true, None::<&str>)?;
            let save_item = MenuItem::with_id(app, "save", "Պահպանել", true, None::<&str>)?;
            let send_item = MenuItem::with_id(app, "send", "Ուղարկել", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Կարգավորումներ", true, None::<&str>)?;
            let exit_item = MenuItem::with_id(app, "exit", "Ելք", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let workbook_menu = Submenu::with_items(
                app,
                "Տետր",
                true,
                &[
                    &create_item,
                    &save_item,
                    &send_item,
                    &separator,
                    &settings_item,
                    &exit_item,
                ],
            )?;
            let menu = Menu::with_items(app, &[&workbook_menu])?;

            app.set_menu(menu)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
