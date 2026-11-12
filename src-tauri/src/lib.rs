use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tempfile::NamedTempFile;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn set_user_mode(app: AppHandle, mode: String, solved: bool) -> Result<(), String> {
    let is_teacher = match mode.as_str() {
        "teacher" => true,
        "student" => false,
        _ => return Err("Օգտվողի ռեժիմը սխալ է։".to_string()),
    };
    let menu = build_app_menu(&app, is_teacher, solved)
        .map_err(|error| format!("Չհաջողվեց փոխել մենյուն։ {error}"))?;
    app.set_menu(menu)
        .map_err(|error| format!("Չհաջողվեց կիրառել մենյուն։ {error}"))?;
    Ok(())
}

fn build_app_menu<M: Manager<Wry>>(
    manager: &M,
    is_teacher: bool,
    workbook_solved: bool,
) -> tauri::Result<Menu<Wry>> {
    let create_item = MenuItem::with_id(manager, "create", "Ստեղծել", true, None::<&str>)?;
    let open_item = MenuItem::with_id(manager, "open", "Բացել", true, None::<&str>)?;
    let save_item = MenuItem::with_id(manager, "save", "Պահպանել", true, None::<&str>)?;
    let send_item = MenuItem::with_id(manager, "send", "Ուղարկել", true, None::<&str>)?;
    let settings_item =
        MenuItem::with_id(manager, "settings", "Կարգավորումներ", true, None::<&str>)?;
    let exit_item = MenuItem::with_id(manager, "exit", "Ելք", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(manager)?;
    let items: Vec<&dyn IsMenuItem<Wry>> = if is_teacher && workbook_solved {
        vec![
            &create_item,
            &open_item,
            &save_item,
            &send_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    } else if is_teacher {
        vec![
            &create_item,
            &open_item,
            &save_item,
            &send_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
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
    Menu::with_items(manager, &[&workbook_menu])
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
    if workbook
        .get("solved")
        .is_some_and(|solved| !solved.is_boolean())
    {
        return Err("Տետրի solved դրոշը պետք է լինի boolean։".to_string());
    }

    restore_images(&mut workbook, &mut archive)?;
    Ok(workbook)
}

fn restore_images(workbook: &mut Value, archive: &mut zip::ZipArchive<File>) -> Result<(), String> {
    let assignments = workbook
        .get_mut("assignments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Տետրում առաջադրանքների ցուցակ չկա։".to_string())?;

    for assignment in assignments {
        for section_name in ["problem", "solution"] {
            let Some(blocks) = assignment
                .get_mut(section_name)
                .and_then(Value::as_array_mut)
            else {
                continue;
            };

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
    ensure_solution_sections(&mut workbook)?;
    ensure_solved_flag(&mut workbook)?;
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

fn ensure_solution_sections(workbook: &mut Value) -> Result<(), String> {
    let assignments = workbook
        .get_mut("assignments")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Տետրում առաջադրանքների ցուցակ չկա։".to_string())?;
    for assignment in assignments {
        let assignment = assignment
            .as_object_mut()
            .ok_or_else(|| "Առաջադրանքի տվյալները սխալ ձևաչափ ունեն։".to_string())?;
        match assignment.get("solution") {
            Some(Value::Array(_)) => {}
            Some(_) => return Err("Առաջադրանքի լուծումները պետք է լինեն ցուցակ։".to_string()),
            None => {
                assignment.insert("solution".to_string(), Value::Array(Vec::new()));
            }
        }
    }
    Ok(())
}

fn ensure_solved_flag(workbook: &mut Value) -> Result<(), String> {
    let document = workbook
        .as_object_mut()
        .ok_or_else(|| "Տետրի տվյալները պետք է լինեն JSON օբյեկտ։".to_string())?;
    match document.get("solved") {
        Some(Value::Bool(_)) => Ok(()),
        Some(_) => Err("Տետրի solved դրոշը պետք է լինի boolean։".to_string()),
        None => {
            document.insert("solved".to_string(), Value::Bool(false));
            Ok(())
        }
    }
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
            let Some(blocks) = assignment
                .get_mut(section_name)
                .and_then(Value::as_array_mut)
            else {
                continue;
            };

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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(|app, event| {
            if event.id() == "settings" {
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
            greet,
            exit_app,
            open_workbook,
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
    fn zip_contains_versioned_json_and_extracted_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "title": "Դաս",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
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
            "solved": true,
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
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
            "solved": true,
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "student": {
                "name": "Աշակերտ",
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
        assert_eq!(opened["student"]["name"], "Աշակերտ");
        assert_eq!(opened["verified"]["grade"], 18);
        assert_eq!(opened["assignments"][0]["teacher-comment"], "Լավ աշխատանք։");
        assert_eq!(opened["solved"], true);
    }

    #[test]
    fn rejects_non_boolean_solved_flags() {
        let mut workbook = json!({ "solved": "true", "assignments": [] });
        assert!(ensure_solved_flag(&mut workbook).is_err());
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
