use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_updater::UpdaterExt;
use tempfile::NamedTempFile;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

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

fn read_workbook_archive(path: String) -> Result<Value, String> {
    let file = File::open(&path).map_err(|error| format!("Չհաջողվեց կարդալ ֆայլը։ {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Ընտրված ֆայլը վավեր ZIP արխիվ չէ։ {error}"))?;
    let workbook_json = {
        let mut entry = archive
            .by_name("workbook.json")
            .map_err(|error| format!("Արխիվում workbook.json ֆայլը չկա։ {error}"))?;
        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .map_err(|error| format!("Չհաջողվեց կարդալ workbook.json-ը։ {error}"))?;
        contents
    };
    let mut workbook: Value = serde_json::from_str(&workbook_json)
        .map_err(|error| format!("workbook.json-ը վավեր JSON չէ։ {error}"))?;
    if workbook.get("formatVersion").and_then(Value::as_u64) != Some(1) {
        return Err("Տետրի ZIP ձևաչափի տարբերակը չի աջակցվում։".to_string());
    }
    validate_current_workbook(&workbook)?;

    restore_images(&mut workbook, &mut archive)?;
    Ok(workbook)
}

fn validate_current_workbook(workbook: &Value) -> Result<(), String> {
    if !workbook.get("solved").is_some_and(Value::is_boolean) {
        return Err("Տետրի solved դրոշը պետք է լինի boolean։".to_string());
    }

    let assignments = workbook
        .get("assignments")
        .and_then(Value::as_array)
        .ok_or_else(|| "Տետրում առաջադրանքների ցուցակ չկա։".to_string())?;
    for assignment in assignments {
        let assignment = assignment
            .as_object()
            .ok_or_else(|| "Առաջադրանքի տվյալները սխալ ձևաչափ ունեն։".to_string())?;
        for section_name in ["problem", "solution"] {
            if !assignment.get(section_name).is_some_and(Value::is_array) {
                return Err(format!(
                    "Առաջադրանքի {section_name} բաժինը պետք է լինի ցուցակ։"
                ));
            }
        }
    }

    Ok(())
}

fn restore_images(workbook: &mut Value, archive: &mut zip::ZipArchive<File>) -> Result<(), String> {
    let assignments = workbook
        .get_mut("assignments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Տետրում առաջադրանքների ցուցակ չկա։".to_string())?;

    for assignment in assignments {
        for section_name in ["problem", "solution"] {
            let blocks = assignment
                .get_mut(section_name)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| format!("Առաջադրանքի {section_name} բաժինը պետք է լինի ցուցակ։"))?;

            for block in blocks {
                if block.get("type").and_then(Value::as_str) != Some("image") {
                    continue;
                }

                let entry_path = block
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Նկարի բլոկում պատկերի ուղին բացակայում է։".to_string())?
                    .to_string();
                validate_image_entry_path(&entry_path)?;
                let mime_type = mime_guess::from_path(&entry_path)
                    .first()
                    .map(|mime| mime.to_string())
                    .filter(|mime| mime.starts_with("image/"))
                    .ok_or_else(|| format!("Չաջակցվող նկարի ձևաչափ՝ {entry_path}։"))?;
                let mut image_entry = archive
                    .by_name(&entry_path)
                    .map_err(|error| format!("Արխիվում նկարը չի գտնվել՝ {entry_path}։ {error}"))?;
                if image_entry.is_dir() {
                    return Err(format!("Արխիվի նկարի ուղին ֆայլ չէ՝ {entry_path}։"));
                }
                let mut image_bytes = Vec::new();
                image_entry
                    .read_to_end(&mut image_bytes)
                    .map_err(|error| format!("Չհաջողվեց կարդալ նկարը՝ {entry_path}։ {error}"))?;
                drop(image_entry);

                let image_url = block
                    .get_mut("url")
                    .ok_or_else(|| "Նկարի URL-ը բացակայում է։".to_string())?;
                *image_url = Value::String(format!(
                    "data:{mime_type};base64,{}",
                    STANDARD.encode(image_bytes)
                ));
            }
        }
    }

    Ok(())
}

fn validate_image_entry_path(path: &str) -> Result<(), String> {
    let components: Vec<_> = path.split('/').collect();
    if path.starts_with("images/")
        && !path.contains('\\')
        && components.len() > 1
        && components
            .iter()
            .all(|component| !component.is_empty() && *component != "." && *component != "..")
    {
        Ok(())
    } else {
        Err(format!("Նկարի ուղին սխալ է՝ {path}։"))
    }
}

fn write_workbook_archive(path: String, mut workbook: Value) -> Result<(), String> {
    let target_path = zip_path(PathBuf::from(path))?;
    validate_current_workbook(&workbook)?;
    let parent = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("Չհաջողվեց ստեղծել ժամանակավոր ֆայլը։ {error}"))?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    {
        let mut archive = ZipWriter::new(temporary.as_file_mut());
        package_images(&mut workbook, &mut archive, options)?;
        let document = workbook
            .as_object_mut()
            .ok_or_else(|| "Տետրի տվյալները պետք է լինեն JSON օբյեկտ։".to_string())?;
        document.insert("formatVersion".to_string(), Value::from(1));

        archive
            .start_file("workbook.json", options)
            .map_err(|error| format!("Չհաջողվեց ավելացնել workbook.json-ը։ {error}"))?;
        serde_json::to_writer_pretty(&mut archive, &workbook)
            .map_err(|error| format!("Չհաջողվեց ձևավորել տետրի JSON-ը։ {error}"))?;
        archive
            .finish()
            .map_err(|error| format!("Չհաջողվեց ավարտել ZIP արխիվը։ {error}"))?;
    }

    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Չհաջողվեց պահպանել ZIP արխիվը սկավառակի վրա։ {error}"))?;
    temporary
        .persist(&target_path)
        .map_err(|error| format!("Չհաջողվեց վերջնականացնել ZIP ֆայլը։ {}", error.error))?;

    Ok(())
}

fn zip_path(mut path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Պահպանման ուղին դատարկ է։".to_string());
    }

    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("pdw") => Ok(path),
        None => {
            path.set_extension("pdw");
            Ok(path)
        }
        Some(_) => Err("Տետրը պետք է պահպանվի .pdw ընդլայնմամբ։".to_string()),
    }
}

fn package_images<W: Write + Seek>(
    workbook: &mut Value,
    archive: &mut ZipWriter<W>,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let assignments = workbook
        .get_mut("assignments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Տետրում առաջադրանքների ցուցակ չկա։".to_string())?;
    let mut image_number = 0;

    for assignment in assignments {
        for section_name in ["problem", "solution"] {
            let blocks = assignment
                .get_mut(section_name)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| format!("Առաջադրանքի {section_name} բաժինը պետք է լինի ցուցակ։"))?;

            for block in blocks {
                if block.get("type").and_then(Value::as_str) != Some("image") {
                    continue;
                }

                let data_url = block
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Նկարի բլոկում պատկերի տվյալները բացակայում են։".to_string())?;
                let (mime_type, bytes) = decode_image_data_url(data_url)?;
                let extension = mime_guess::get_mime_extensions_str(&mime_type)
                    .and_then(|extensions| extensions.first())
                    .filter(|extension| {
                        extension
                            .chars()
                            .all(|character| character.is_ascii_alphanumeric())
                    })
                    .ok_or_else(|| format!("Չաջակցվող նկարի ձևաչափ՝ {mime_type}։"))?;
                image_number += 1;
                let entry_path = format!("images/image-{image_number:04}.{extension}");
                archive
                    .start_file(entry_path.clone(), options)
                    .map_err(|error| format!("Չհաջողվեց ավելացնել նկարը ZIP-ում։ {error}"))?;
                archive
                    .write_all(&bytes)
                    .map_err(|error| format!("Չհաջողվեց գրել նկարը ZIP-ում։ {error}"))?;

                let image_url = block
                    .get_mut("url")
                    .ok_or_else(|| "Նկարի URL-ը բացակայում է։".to_string())?;
                *image_url = Value::String(entry_path);
            }
        }
    }

    Ok(())
}

fn decode_image_data_url(data_url: &str) -> Result<(String, Vec<u8>), String> {
    let (metadata, encoded_data) = data_url
        .split_once(',')
        .ok_or_else(|| "Նկարի տվյալների URL-ը սխալ ձևաչափ ունի։".to_string())?;
    let metadata = metadata
        .strip_prefix("data:")
        .ok_or_else(|| "Նկարի տվյալները պետք է լինեն Data URL ձևաչափով։".to_string())?;
    let mut parts = metadata.split(';');
    let mime_type = parts.next().unwrap_or_default().to_ascii_lowercase();

    if !mime_type.starts_with("image/") || !parts.any(|part| part == "base64") {
        return Err("Նկարը պետք է լինի base64 կոդավորված image Data URL։".to_string());
    }

    let bytes = STANDARD
        .decode(encoded_data)
        .map_err(|error| format!("Չհաջողվեց ապակոդավորել նկարը։ {error}"))?;
    Ok((mime_type, bytes))
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
        .plugin(tauri_plugin_opener::init())
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
    use serde_json::json;
    use std::{fs::File, io::Read};
    use zip::ZipArchive;

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

    #[test]
    fn zip_contains_versioned_json_and_extracted_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "solved": false,
            "assignments": [{
                "solution": [],
                "problem": [{
                    "type": "image",
                    "url": "data:image/png;base64,SGVsbG8="
                }]
            }]
        });

        write_workbook_archive(path.to_string_lossy().into_owned(), workbook).unwrap();

        let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let document: Value = {
            let mut entry = archive.by_name("workbook.json").unwrap();
            serde_json::from_reader(&mut entry).unwrap()
        };
        assert_eq!(document["formatVersion"], 1);
        assert_eq!(document["solved"], false);
        assert_eq!(document["teacher"]["name"], "Ուսուցիչ");
        assert!(document.get("author").is_none());
        assert_eq!(document["assignments"][0]["solution"], json!([]));
        assert_eq!(
            document["assignments"][0]["problem"][0]["url"],
            "images/image-0001.png"
        );

        let mut image = archive.by_name("images/image-0001.png").unwrap();
        let mut bytes = Vec::new();
        image.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"Hello");
    }

    #[test]
    fn rejects_invalid_image_data_urls() {
        assert!(decode_image_data_url("data:text/plain;base64,SGVsbG8=").is_err());
        assert!(decode_image_data_url("data:image/png,SGVsbG8=").is_err());
    }

    #[test]
    fn opening_archive_restores_embedded_image_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "title": "Դաս",
            "description": "",
            "solved": true,
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "solution": [],
                "problem": [{
                    "type": "image",
                    "url": "data:image/png;base64,SGVsbG8="
                }]
            }]
        });
        write_workbook_archive(path.to_string_lossy().into_owned(), workbook).unwrap();

        let opened = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap();

        assert_eq!(
            opened["assignments"][0]["problem"][0]["url"],
            "data:image/png;base64,SGVsbG8="
        );
        assert_eq!(opened["solved"], true);
    }

    #[test]
    fn student_solution_images_round_trip_in_archive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "title": "Դաս",
            "description": "",
            "solved": true,
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "student": {
                "name": "Սովորող",
                "email": "student@example.com"
            },
            "verified": {
                "grade": 18
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [],
                "teacher-comment": "Լավ աշխատանք։",
                "solution": [{
                    "type": "image",
                    "url": "data:image/png;base64,SGVsbG8="
                }]
            }]
        });
        write_workbook_archive(path.to_string_lossy().into_owned(), workbook).unwrap();

        let opened = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap();

        assert_eq!(
            opened["assignments"][0]["solution"][0]["url"],
            "data:image/png;base64,SGVsbG8="
        );
        assert_eq!(opened["teacher"]["name"], "Ուսուցիչ");
        assert_eq!(opened["student"]["name"], "Սովորող");
        assert_eq!(opened["verified"]["grade"], 18);
        assert_eq!(opened["assignments"][0]["teacher-comment"], "Լավ աշխատանք։");
        assert_eq!(opened["solved"], true);
    }

    #[test]
    fn requires_current_workbook_fields() {
        assert!(
            validate_current_workbook(&json!({
                "assignments": []
            }))
            .is_err()
        );
        assert!(
            validate_current_workbook(&json!({
                "solved": "true",
                "assignments": []
            }))
            .is_err()
        );
        assert!(
            validate_current_workbook(&json!({
                "solved": false,
                "assignments": [{
                    "problem": []
                }]
            }))
            .is_err()
        );
        assert!(
            validate_current_workbook(&json!({
                "solved": false,
                "assignments": [{
                    "problem": [],
                    "solution": []
                }]
            }))
            .is_ok()
        );
    }

    #[test]
    fn rejects_image_paths_outside_archive_image_directory() {
        assert!(validate_image_entry_path("../outside.png").is_err());
        assert!(validate_image_entry_path("images/../../outside.png").is_err());
        assert!(validate_image_entry_path("images\\outside.png").is_err());
        assert!(validate_image_entry_path("images/image-0001.png").is_ok());
    }

    #[test]
    fn workbook_files_use_the_pdw_extension() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson");
        assert_eq!(zip_path(path).unwrap().extension().unwrap(), "pdw");
        assert!(zip_path(directory.path().join("lesson.zip")).is_err());
        assert_eq!(
            zip_path(directory.path().join("lesson.PDW"))
                .unwrap()
                .extension()
                .unwrap(),
            "PDW"
        );
    }
}
