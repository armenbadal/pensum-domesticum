use serde::Serialize;
use serde_json::Value;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_updater::UpdaterExt;

mod workbook;

use workbook::{read_workbook_archive, write_workbook_archive};

#[derive(Default)]
struct PendingWorkbookPaths(Mutex<Vec<PathBuf>>);

impl PendingWorkbookPaths {
    fn new(paths: Vec<PathBuf>) -> Self {
        Self(Mutex::new(paths))
    }

    fn enqueue(&self, paths: impl IntoIterator<Item = PathBuf>) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|error| format!("Չհաջողվեց հերթագրել բացվող տետրը։ {error}"))?
            .extend(paths);
        Ok(())
    }
}

fn collect_workbook_paths<I, S>(args: I, working_directory: &Path) -> Vec<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .filter_map(|arg| {
            let path = PathBuf::from(arg.as_ref());
            let is_workbook = path
                .extension()
                .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("pdw"));
            if !is_workbook {
                return None;
            }

            Some(if path.is_absolute() {
                path
            } else {
                working_directory.join(path)
            })
        })
        .collect()
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    downloaded_bytes: u64,
    content_length: Option<u64>,
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<Option<String>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }

    #[cfg(target_os = "windows")]
    {
        let update = app
            .updater()
            .map_err(|error| format!("Չհաջողվեց պատրաստել թարմացման ստուգումը։ {error}"))?
            .check()
            .await
            .map_err(|error| format!("Չհաջողվեց ստուգել թարմացումները։ {error}"))?;
        Ok(update.map(|update| update.version))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
async fn install_update(app: AppHandle, expected_version: String) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("Թարմացումները հասանելի են միայն տեղադրված տարբերակում։".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let update = app
            .updater()
            .map_err(|error| format!("Չհաջողվեց պատրաստել թարմացումը։ {error}"))?
            .check()
            .await
            .map_err(|error| format!("Չհաջողվեց ստուգել թարմացումը։ {error}"))?
            .ok_or_else(|| "Թարմացումն այլևս հասանելի չէ։".to_string())?;
        if update.version != expected_version {
            return Err(
                "Հասանելի տարբերակը փոխվել է։ Փակիր և հաջորդ գործարկման ժամանակ նորից ստուգիր։"
                    .to_string(),
            );
        }

        let mut downloaded_bytes = 0u64;
        let progress_app = app.clone();
        update
            .download_and_install(
                move |chunk_length, content_length| {
                    downloaded_bytes = downloaded_bytes.saturating_add(chunk_length as u64);
                    if let Err(error) = progress_app.emit(
                        "update-progress",
                        UpdateProgress {
                            downloaded_bytes,
                            content_length,
                        },
                    ) {
                        eprintln!("Չհաջողվեց ցուցադրել թարմացման ընթացքը։ {error}");
                    }
                },
                || {},
            )
            .await
            .map_err(|error| format!("Չհաջողվեց ներբեռնել կամ տեղադրել թարմացումը։ {error}"))?;
        app.restart()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, expected_version);
        Err("Թարմացումները ներկայումս աջակցվում են միայն Windows-ում։".to_string())
    }
}

#[tauri::command]
fn set_user_mode(app: AppHandle, mode: String, verified: bool) -> Result<(), String> {
    let is_teacher = match mode.as_str() {
        "teacher" => true,
        "student" => false,
        _ => return Err("Օգտվողի ռեժիմը սխալ է։".to_string()),
    };
    let title = if is_teacher {
        "Տնային աշխատանք (դասատու)"
    } else {
        "Տնային աշխատանք (սովորող)"
    };
    app.get_webview_window("main")
        .ok_or_else(|| "Ծրագրի հիմնական պատուհանը չի գտնվել։".to_string())?
        .set_title(title)
        .map_err(|error| format!("Չհաջողվեց փոխել պատուհանի վերնագիրը։ {error}"))?;

    let menu = build_app_menu(&app, is_teacher, verified)
        .map_err(|error| format!("Չհաջողվեց փոխել մենյուն։ {error}"))?;
    app.set_menu(menu)
        .map_err(|error| format!("Չհաջողվեց կիրառել մենյուն։ {error}"))?;
    Ok(())
}

fn build_app_menu<M: Manager<Wry>>(
    manager: &M,
    is_teacher: bool,
    workbook_verified: bool,
) -> tauri::Result<Menu<Wry>> {
    let create_item = icon_menu_item(
        manager,
        "create",
        "Ստեղծել",
        include_bytes!("../icons/menu/create.png"),
    )?;
    let open_item = icon_menu_item(
        manager,
        "open",
        "Բացել",
        include_bytes!("../icons/menu/open.png"),
    )?;
    let save_item = icon_menu_item(
        manager,
        "save",
        "Պահպանել",
        include_bytes!("../icons/menu/save.png"),
    )?;
    let save_as_item = icon_menu_item(
        manager,
        "save-as",
        "Պահպանել որպես…",
        include_bytes!("../icons/menu/save.png"),
    )?;
    let settings_item = icon_menu_item(
        manager,
        "settings",
        "Կարգավորումներ",
        include_bytes!("../icons/menu/settings.png"),
    )?;
    let exit_item = icon_menu_item(
        manager,
        "exit",
        "Ելք",
        include_bytes!("../icons/menu/exit.png"),
    )?;
    let about_item = icon_menu_item(
        manager,
        "about",
        "Ծրագրի մասին",
        include_bytes!("../icons/menu/about.png"),
    )?;
    let separator = PredefinedMenuItem::separator(manager)?;
    let items: Vec<&dyn IsMenuItem<Wry>> = if is_teacher {
        vec![
            &create_item,
            &open_item,
            &save_item,
            &save_as_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    } else if workbook_verified {
        vec![&open_item, &separator, &settings_item, &exit_item]
    } else {
        vec![
            &open_item,
            &save_item,
            &save_as_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    };
    let workbook_menu = Submenu::with_items(manager, "Տետր", true, &items)?;
    let help_menu = Submenu::with_items(manager, "Հուշում", true, &[&about_item])?;
    Menu::with_items(manager, &[&workbook_menu, &help_menu])
}

fn icon_menu_item<M: Manager<Wry>>(
    manager: &M,
    id: &str,
    text: &str,
    icon_bytes: &[u8],
) -> tauri::Result<IconMenuItem<Wry>> {
    let icon = Image::from_bytes(icon_bytes)?;
    IconMenuItem::with_id(manager, id, text, true, Some(icon), None::<&str>)
}

#[tauri::command]
async fn save_workbook(path: String, workbook: Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_workbook_archive(path, workbook))
        .await
        .map_err(|error| format!("Չհաջողվեց ստեղծել ZIP արխիվը։ {error}"))?
}

#[tauri::command]
async fn open_workbook(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || read_workbook_archive(path))
        .await
        .map_err(|error| format!("Չհաջողվեց բացել ZIP արխիվը։ {error}"))?
}

#[tauri::command]
fn take_pending_workbook_paths(
    state: State<'_, PendingWorkbookPaths>,
) -> Result<Vec<String>, String> {
    let mut pending = state
        .0
        .lock()
        .map_err(|error| format!("Չհաջողվեց ստանալ բացվող տետրի ուղին։ {error}"))?;
    let paths = pending
        .iter()
        .map(|path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| "Տետրի ֆայլի ուղին չի կարող ներկայացվել որպես տեքստ։".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    pending.clear();
    Ok(paths)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let working_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Չհաջողվեց որոշել մեկնարկային պանակը։ {error}");
            PathBuf::new()
        }
    };
    let startup_paths = collect_workbook_paths(std::env::args_os().skip(1), &working_directory);
    let mut builder = tauri::Builder::default().manage(PendingWorkbookPaths::new(startup_paths));

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let paths = collect_workbook_paths(args, Path::new(&cwd));
            if !paths.is_empty() {
                match app.state::<PendingWorkbookPaths>().enqueue(paths) {
                    Ok(()) => {
                        if let Err(error) = app.emit("open-workbook-files", ()) {
                            eprintln!("Չհաջողվեց փոխանցել բացվող տետրը։ {error}");
                        }
                    }
                    Err(error) => eprintln!("{error}"),
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = window.set_focus() {
                    eprintln!("Չհաջողվեց ակտիվացնել ծրագրի պատուհանը։ {error}");
                }
            }
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_menu_event(|app, event| {
            if event.id() == "about" {
                if let Err(error) = app.emit("open-about", env!("CARGO_PKG_VERSION")) {
                    eprintln!("Չհաջողվեց բացել «Ծրագրի մասին» պատուհանը։ {error}");
                }
            } else if event.id() == "settings" {
                if let Err(error) = app.emit("open-settings", ()) {
                    eprintln!("Failed to open settings: {error}");
                }
            } else if event.id() == "create" {
                if let Err(error) = app.emit("create-workbook", ()) {
                    eprintln!("Failed to start new workbook: {error}");
                }
            } else if event.id() == "save" {
                if let Err(error) = app.emit("save-workbook", ()) {
                    eprintln!("Failed to start workbook save: {error}");
                }
            } else if event.id() == "save-as" {
                if let Err(error) = app.emit("save-workbook-as", ()) {
                    eprintln!("Failed to start workbook save as: {error}");
                }
            } else if event.id() == "open" {
                if let Err(error) = app.emit("open-workbook", ()) {
                    eprintln!("Failed to start workbook open: {error}");
                }
            } else if event.id() == "exit" {
                if let Err(error) = app.emit("exit-workbook", ()) {
                    eprintln!("Failed to request application exit: {error}");
                }
            }
        })
        .setup(|app| {
            let menu = build_app_menu(app, true, false)?;
            app.set_menu(menu)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            exit_app,
            check_for_updates,
            install_update,
            open_workbook,
            take_pending_workbook_paths,
            save_workbook,
            set_user_mode
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_workbook_paths_are_filtered_and_resolved() {
        let working_directory = Path::new("working");
        let paths = collect_workbook_paths(
            ["pensum.exe", "lesson.pdw", "response.PDW", "notes.txt"],
            working_directory,
        );

        assert_eq!(
            paths,
            vec![
                working_directory.join("lesson.pdw"),
                working_directory.join("response.PDW")
            ]
        );
    }
}
