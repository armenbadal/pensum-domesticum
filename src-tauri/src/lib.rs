use base64::{Engine as _, engine::general_purpose::STANDARD};
use docx_rs::{
    BorderType, Docx, Paragraph, Pic, Run, Style, StyleType, Table, TableCell, TableCellBorder,
    TableCellBorderPosition, TableRow,
};
use image::{GenericImageView, ImageFormat};
use serde::Deserialize;
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs::File,
    io::{Cursor, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, Wry};
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
    let create_item = MenuItem::with_id(manager, "create", "Ստեղծել", true, None::<&str>)?;
    let open_item = MenuItem::with_id(manager, "open", "Բացել", true, None::<&str>)?;
    let save_item = MenuItem::with_id(manager, "save", "Պահպանել", true, None::<&str>)?;
    let export_word_item =
        MenuItem::with_id(manager, "export-word", "Արտահանել Word", true, None::<&str>)?;
    let send_item = MenuItem::with_id(manager, "send", "Ուղարկել", true, None::<&str>)?;
    let settings_item =
        MenuItem::with_id(manager, "settings", "Կարգավորումներ", true, None::<&str>)?;
    let exit_item = MenuItem::with_id(manager, "exit", "Ելք", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(manager)?;
    let items: Vec<&dyn IsMenuItem<Wry>> = if is_teacher {
        vec![
            &create_item,
            &open_item,
            &save_item,
            &export_word_item,
            &send_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    } else if workbook_verified {
        vec![
            &open_item,
            &export_word_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    } else {
        vec![
            &open_item,
            &save_item,
            &export_word_item,
            &separator,
            &settings_item,
            &exit_item,
        ]
    };
    let workbook_menu = Submenu::with_items(manager, "Տետր", true, &items)?;
    Menu::with_items(manager, &[&workbook_menu])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocxExportMode {
    Teacher,
    Student,
}

impl DocxExportMode {
    fn parse(mode: &str) -> Result<Self, String> {
        match mode {
            "teacher" => Ok(Self::Teacher),
            "student" => Ok(Self::Student),
            _ => Err("DOCX արտահանման օգտվողի ռեժիմը սխալ է։".to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DocxWorkbook {
    title: String,
    #[serde(default)]
    description: String,
    assignments: Vec<DocxAssignment>,
}

#[derive(Debug, Deserialize)]
struct DocxAssignment {
    problem: Vec<DocxBlock>,
    #[serde(default)]
    solution: Vec<DocxBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum DocxBlock {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "image")]
    Image { url: String },
    #[serde(rename = "squared")]
    Squared { content: Value },
}

#[derive(Debug)]
struct DocxGrid {
    cells: Vec<Vec<String>>,
    borders: Vec<(usize, TableCellBorderPosition)>,
}

#[tauri::command]
async fn export_workbook_docx(path: String, mode: String, workbook: Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode = DocxExportMode::parse(&mode)?;
        let workbook: DocxWorkbook = serde_json::from_value(workbook)
            .map_err(|error| format!("Տետրի տվյալները չեն կարող արտահանվել։ {error}"))?;
        write_workbook_docx(path, mode, workbook)
    })
    .await
    .map_err(|error| format!("Չհաջողվեց ստեղծել Word փաստաթուղթը։ {error}"))?
}

fn write_workbook_docx(
    path: String,
    mode: DocxExportMode,
    workbook: DocxWorkbook,
) -> Result<(), String> {
    let target_path = docx_path(PathBuf::from(path))?;
    let document = build_docx_document(mode, &workbook)?;
    let parent = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("Չհաջողվեց ստեղծել ժամանակավոր Word ֆայլը։ {error}"))?;

    document
        .build()
        .pack(temporary.as_file_mut())
        .map_err(|error| format!("Չհաջողվեց գեներացնել DOCX փաստաթուղթը։ {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Չհաջողվեց պահպանել Word փաստաթուղթը։ {error}"))?;
    temporary
        .persist(&target_path)
        .map_err(|error| format!("Չհաջողվեց պահպանել Word ֆայլը։ {}", error.error))?;
    Ok(())
}

fn docx_path(mut path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Պահպանման ուղին դատարկ է։".to_string());
    }

    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("docx") => Ok(path),
        None => {
            path.set_extension("docx");
            Ok(path)
        }
        Some(_) => Err("Word փաստաթուղթը պետք է պահպանվի .docx ընդլայնմամբ։".to_string()),
    }
}

fn build_docx_document(mode: DocxExportMode, workbook: &DocxWorkbook) -> Result<Docx, String> {
    let mut document = Docx::new()
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("Heading 1")
                .outline_lvl(0)
                .size(30)
                .bold(),
        )
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("Heading 2")
                .outline_lvl(1)
                .size(26)
                .bold(),
        )
        .add_style(
            Style::new("Heading3", StyleType::Paragraph)
                .name("Heading 3")
                .outline_lvl(2)
                .size(22)
                .bold(),
        )
        .add_paragraph(
            Paragraph::new()
                .style("Title")
                .add_run(Run::new().add_text(workbook.title.clone())),
        );

    if mode == DocxExportMode::Teacher && !workbook.description.trim().is_empty() {
        document = document.add_paragraph(
            Paragraph::new()
                .style("Heading1")
                .add_run(Run::new().add_text("Նկարագրություն")),
        );
        document = add_text_paragraphs(document, &workbook.description);
    }

    for (index, assignment) in workbook.assignments.iter().enumerate() {
        document = document.add_paragraph(
            Paragraph::new()
                .style("Heading1")
                .add_run(Run::new().add_text(format!("Առաջադրանք {}", index + 1))),
        );
        document = document.add_paragraph(
            Paragraph::new()
                .style("Heading2")
                .add_run(Run::new().add_text("Խնդիր")),
        );
        document = add_docx_blocks(document, &assignment.problem)?;

        if mode == DocxExportMode::Student {
            document = document.add_paragraph(
                Paragraph::new()
                    .style("Heading2")
                    .add_run(Run::new().add_text("Լուծում")),
            );
            document = add_docx_blocks(document, &assignment.solution)?;
        }
    }

    Ok(document)
}

fn add_text_paragraphs(mut document: Docx, text: &str) -> Docx {
    for line in text.split('\n') {
        document = document.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(line.trim_end_matches('\r'))),
        );
    }
    document
}

fn add_docx_blocks(mut document: Docx, blocks: &[DocxBlock]) -> Result<Docx, String> {
    for block in blocks {
        match block {
            DocxBlock::Text { content } => {
                document = add_text_paragraphs(document, content);
            }
            DocxBlock::Image { url } => {
                let (mime_type, bytes) = decode_image_data_url(url)?;
                let image = image::load_from_memory(&bytes)
                    .map_err(|error| format!("Չհաջողվեց բացել նկարը ({mime_type})՝ {error}"))?;
                let (width, height) = image.dimensions();
                if width == 0 || height == 0 {
                    return Err("Նկարի չափերը սխալ են։".to_string());
                }
                let mut png = Cursor::new(Vec::new());
                image
                    .write_to(&mut png, ImageFormat::Png)
                    .map_err(|error| format!("Չհաջողվեց փոխարկել նկարը DOCX-ի համար։ {error}"))?;
                let (width_emu, height_emu) = image_dimensions_emu(width, height);
                let picture = Pic::new_with_dimensions(png.into_inner(), width, height)
                    .size(width_emu, height_emu);
                document =
                    document.add_paragraph(Paragraph::new().add_run(Run::new().add_image(picture)));
            }
            DocxBlock::Squared { content } => {
                if let Some(grid) = parse_docx_grid(content)? {
                    document = document.add_table(build_docx_grid_table(grid));
                }
            }
        }
    }
    Ok(document)
}

fn image_dimensions_emu(width: u32, height: u32) -> (u32, u32) {
    const EMU_PER_INCH: f64 = 914_400.0;
    const MAX_WIDTH_EMU: f64 = 6.3 * EMU_PER_INCH;
    const MAX_HEIGHT_EMU: f64 = 8.5 * EMU_PER_INCH;
    const EMU_PER_PIXEL: f64 = EMU_PER_INCH / 96.0;

    let width_emu = f64::from(width) * EMU_PER_PIXEL;
    let height_emu = f64::from(height) * EMU_PER_PIXEL;
    let scale = (MAX_WIDTH_EMU / width_emu)
        .min(MAX_HEIGHT_EMU / height_emu)
        .min(1.0);
    (
        (width_emu * scale).round().max(1.0) as u32,
        (height_emu * scale).round().max(1.0) as u32,
    )
}

fn parse_docx_grid(content: &Value) -> Result<Option<DocxGrid>, String> {
    let (cells, border_values) = if let Some(rows) = content.as_array() {
        let mut cells = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row
                .as_array()
                .ok_or_else(|| "Վանդակավոր դաշտի տողը սխալ ձևաչափ ունի։".to_string())?;
            cells.push(
                row.iter()
                    .map(|cell| {
                        let value = cell.as_str().ok_or_else(|| {
                            "Վանդակավոր դաշտի արժեքը պետք է լինի տեքստ։".to_string()
                        })?;
                        Ok(if value == " " {
                            String::new()
                        } else {
                            value.to_string()
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
        }
        let columns = if cells.is_empty() {
            20
        } else {
            cells.iter().map(Vec::len).max().unwrap_or(1).max(1)
        };
        if cells.is_empty() {
            cells = vec![vec![String::new(); columns]; 10];
        } else {
            for row in &mut cells {
                row.resize(columns, String::new());
            }
        }
        (cells, Vec::new())
    } else {
        let version = content
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "Վանդակավոր դաշտի տարբերակը բացակայում է։".to_string())?;
        let rows = if version == 1 {
            20
        } else {
            content
                .get("rows")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| "Վանդակավոր դաշտի տողերի քանակը սխալ է։".to_string())?
        };
        let columns = if version == 1 {
            20
        } else {
            content
                .get("columns")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| "Վանդակավոր դաշտի սյուների քանակը սխալ է։".to_string())?
        };
        if !matches!(version, 1 | 2) || rows == 0 || columns == 0 {
            return Err("Վանդակավոր դաշտի չափերը չեն աջակցվում։".to_string());
        }
        let cell_text = content
            .get("cells")
            .and_then(Value::as_str)
            .ok_or_else(|| "Վանդակավոր դաշտի վանդակների տվյալները բացակայում են։".to_string())?;
        let source_rows: Vec<_> = cell_text.split('\n').collect();
        if source_rows.len() != rows {
            return Err("Վանդակավոր դաշտի տողերի քանակը սխալ է։".to_string());
        }
        let cells = source_rows
            .into_iter()
            .map(|row| {
                let characters: Vec<_> = row.chars().collect();
                if characters.len() != columns {
                    return Err("Վանդակավոր դաշտի սյուների քանակը սխալ է։".to_string());
                }
                Ok(characters
                    .into_iter()
                    .map(|character| {
                        if character == ' ' {
                            String::new()
                        } else {
                            character.to_string()
                        }
                    })
                    .collect())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let borders = content
            .get("borders")
            .and_then(Value::as_array)
            .ok_or_else(|| "Վանդակավոր դաշտի եզրագծերը սխալ ձևաչափ ունեն։".to_string())?;
        (cells, borders.clone())
    };

    let rows = cells.len();
    let columns = cells.first().map_or(0, Vec::len);
    if rows == 0 || columns == 0 {
        return Ok(None);
    }
    if cells.iter().any(|row| row.len() != columns) {
        return Err("Վանդակավոր դաշտի տողերի չափերը համահունչ չեն։".to_string());
    }
    let cell_count = rows
        .checked_mul(columns)
        .ok_or_else(|| "Վանդակավոր դաշտի չափերը չափազանց մեծ են։".to_string())?;
    let borders = border_values
        .iter()
        .map(|border| {
            let cell = border
                .get("cell")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|cell| *cell < cell_count)
                .ok_or_else(|| "Վանդակավոր դաշտի եզրագծի վանդակը սխալ է։".to_string())?;
            let edge = match border.get("edge").and_then(Value::as_str) {
                Some("bottom") => TableCellBorderPosition::Bottom,
                Some("left") => TableCellBorderPosition::Left,
                _ => return Err("Վանդակավոր դաշտի եզրագծի տեսակը սխալ է։".to_string()),
            };
            Ok((cell, edge))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(Some(DocxGrid { cells, borders }))
}

fn build_docx_grid_table(grid: DocxGrid) -> Table {
    let column_count = grid.cells[0].len();
    let rows = grid
        .cells
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| {
            TableRow::new(
                row.into_iter()
                    .enumerate()
                    .map(|(column_index, value)| {
                        let cell_index = row_index * column_count + column_index;
                        let mut cell = TableCell::new().add_paragraph(Paragraph::new().add_run(
                            Run::new().add_text(if value.is_empty() { " " } else { &value }),
                        ));
                        for (border_cell, edge) in &grid.borders {
                            if *border_cell == cell_index {
                                cell = cell.set_border(
                                    TableCellBorder::new(edge.clone())
                                        .border_type(BorderType::Single)
                                        .size(16)
                                        .color("000000"),
                                );
                            }
                        }
                        cell
                    })
                    .collect(),
            )
        })
        .collect();
    let column_width = (9_360 / column_count).max(1);
    Table::new(rows).set_grid(vec![column_width; column_count])
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
            } else if event.id() == "export-word" {
                if let Err(error) = app.emit("export-word", ()) {
                    eprintln!("Failed to start Word export: {error}");
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
            open_workbook,
            take_pending_workbook_paths,
            export_workbook_docx,
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

    fn docx_document_xml(mode: DocxExportMode, workbook: Value) -> String {
        let workbook: DocxWorkbook = serde_json::from_value(workbook).unwrap();
        let mut output = Cursor::new(Vec::new());
        build_docx_document(mode, &workbook)
            .unwrap()
            .build()
            .pack(&mut output)
            .unwrap();
        let mut archive = ZipArchive::new(Cursor::new(output.into_inner())).unwrap();
        let mut document = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document)
            .unwrap();
        document
    }

    #[test]
    fn docx_export_filters_content_by_mode_and_preserves_editable_grids() {
        let workbook = json!({
            "title": "Տետրի վերնագիր",
            "description": "Նկարագրություն գաղտնիք",
            "student": {
                "name": "Աշակերտի անուն գաղտնիք",
                "email": "student-secret@example.com"
            },
            "verified": { "grade": 19 },
            "assignments": [{
                "problem": [
                    { "type": "text", "content": "Խնդրի պայման" },
                    {
                        "type": "squared",
                        "content": {
                            "version": 2,
                            "rows": 1,
                            "columns": 2,
                            "cells": "1 ",
                            "borders": [{ "cell": 0, "edge": "bottom" }]
                        }
                    }
                ],
                "solution": [
                    { "type": "text", "content": "Աշակերտի լուծում" },
                    {
                        "type": "squared",
                        "content": [["2", "+", "÷"], ["", "3"]]
                    }
                ],
                "teacher-comment": "Դասատուի մեկնաբանություն գաղտնիք"
            }]
        });

        let teacher_document = docx_document_xml(DocxExportMode::Teacher, workbook.clone());
        assert!(teacher_document.contains("Տետրի վերնագիր"));
        assert!(teacher_document.contains("Նկարագրություն գաղտնիք"));
        assert!(teacher_document.contains("Խնդրի պայման"));
        assert!(teacher_document.contains("<w:tbl>"));
        assert!(teacher_document.contains("<w:tcBorders>"));
        assert!(teacher_document.contains("w:sz=\"16\""));
        assert!(!teacher_document.contains("Աշակերտի լուծում"));
        assert!(!teacher_document.contains("student-secret@example.com"));
        assert!(!teacher_document.contains("Դասատուի մեկնաբանություն գաղտնիք"));
        assert!(!teacher_document.contains(">19<"));

        let student_document = docx_document_xml(DocxExportMode::Student, workbook);
        assert!(student_document.contains("Տետրի վերնագիր"));
        assert!(student_document.contains("Խնդրի պայման"));
        assert!(student_document.contains("Աշակերտի լուծում"));
        assert!(student_document.contains("<w:tbl>"));
        assert!(!student_document.contains("Նկարագրություն գաղտնիք"));
        assert!(!student_document.contains("Աշակերտի անուն գաղտնիք"));
        assert!(!student_document.contains("student-secret@example.com"));
        assert!(!student_document.contains("Դասատուի մեկնաբանություն գաղտնիք"));
        assert!(!student_document.contains(">19<"));
    }

    #[test]
    fn docx_export_embeds_images_and_rejects_invalid_image_data() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let data_url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        );
        let workbook: DocxWorkbook = serde_json::from_value(json!({
            "title": "Նկարով տետր",
            "assignments": [{
                "problem": [{ "type": "image", "url": data_url }]
            }]
        }))
        .unwrap();

        let mut output = Cursor::new(Vec::new());
        build_docx_document(DocxExportMode::Teacher, &workbook)
            .unwrap()
            .build()
            .pack(&mut output)
            .unwrap();
        let archive = ZipArchive::new(Cursor::new(output.into_inner())).unwrap();
        assert!(archive.file_names().any(|name| {
            name.is_ok_and(|name| name.starts_with("word/media/") && name.ends_with(".png"))
        }));

        let invalid_image: DocxBlock = serde_json::from_value(json!({
            "type": "image",
            "url": "data:image/png;base64,SGVsbG8="
        }))
        .unwrap();
        assert!(add_docx_blocks(Docx::new(), &[invalid_image]).is_err());
    }

    #[test]
    fn docx_export_paths_require_the_docx_extension() {
        let path = PathBuf::from("lesson");
        assert_eq!(docx_path(path).unwrap().extension().unwrap(), "docx");
        assert!(docx_path(PathBuf::from("lesson.pdf")).is_err());
        assert_eq!(
            docx_path(PathBuf::from("lesson.DOCX"))
                .unwrap()
                .extension()
                .unwrap(),
            "DOCX"
        );
        assert!(DocxExportMode::parse("unknown").is_err());
    }
}
