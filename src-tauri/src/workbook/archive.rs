use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::images::{
    JPEG_MIME, PNG_MIME, extension_for_mime, supported_image_mime, validate_image_bytes,
};
use super::limits::{CURRENT_FORMAT_VERSION, DEFAULT_LIMITS, Limits};
use super::model::{ContentBlock, Workbook, validate_workbook};

const WORKBOOK_ENTRY: &str = "workbook.json";

struct ArchiveIndex {
    workbook_index: usize,
    images: HashMap<String, usize>,
}

struct ReadBudget {
    remaining: u64,
}

impl ReadBudget {
    fn new(remaining: u64) -> Self {
        Self { remaining }
    }

    fn account(&mut self, amount: u64) -> Result<(), String> {
        if amount > self.remaining {
            return Err(
                "Ապասեղմված արխիվի ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։".to_string(),
            );
        }
        self.remaining -= amount;
        Ok(())
    }
}

struct LimitedJsonWriter {
    buffer: Vec<u8>,
    limit: u64,
    exceeded: bool,
}

impl LimitedJsonWriter {
    fn new(limit: u64) -> Self {
        Self {
            buffer: Vec::new(),
            limit,
            exceeded: false,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buffer
    }
}

impl Write for LimitedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next_length = (self.buffer.len() as u64).checked_add(buffer.len() as u64);
        match next_length {
            Some(next_length) if next_length <= self.limit => {
                self.buffer.extend_from_slice(buffer);
                Ok(buffer.len())
            }
            _ => {
                self.exceeded = true;
                Err(io::Error::other("JSON size limit exceeded"))
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn read_workbook_archive(path: String) -> Result<Value, String> {
    read_workbook_archive_with_limits(path, DEFAULT_LIMITS)
}

fn read_workbook_archive_with_limits(path: String, limits: Limits) -> Result<Value, String> {
    let file = File::open(&path).map_err(|error| format!("Չհաջողվեց կարդալ ֆայլը։ {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Չհաջողվեց ստանալ ֆայլի տվյալները։ {error}"))?;
    if !metadata.is_file() {
        return Err("Տետրի ուղին ֆայլ չէ։".to_string());
    }
    if metadata.len() > limits.archive_bytes {
        return Err("Տետրի ֆայլը գերազանցում է թույլատրելի սահմանը։".to_string());
    }

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Ընտրված ֆայլը վավեր ZIP արխիվ չէ։ {error}"))?;
    let index = inspect_archive(&mut archive, &limits)?;
    let mut budget = ReadBudget::new(limits.total_uncompressed_bytes);

    let workbook_json = {
        let mut entry = archive
            .by_index(index.workbook_index)
            .map_err(|error| format!("Արխիվում workbook.json ֆայլը չկա։ {error}"))?;
        let bytes = read_entry_limited(
            &mut entry,
            limits.workbook_json_bytes,
            &mut budget,
            WORKBOOK_ENTRY,
        )?;
        String::from_utf8(bytes)
            .map_err(|error| format!("workbook.json-ը վավեր UTF-8 տեքստ չէ։ {error}"))?
    };
    let mut workbook: Workbook = serde_json::from_str(&workbook_json)
        .map_err(|error| format!("workbook.json-ը վավեր տետրի JSON չէ։ {error}"))?;
    validate_workbook(&workbook, &limits)?;

    restore_images(&mut workbook, &mut archive, &index, &limits, &mut budget)?;
    serde_json::to_value(workbook)
        .map_err(|error| format!("Չհաջողվեց պատրաստել տետրի տվյալները։ {error}"))
}

fn inspect_archive(
    archive: &mut zip::ZipArchive<File>,
    limits: &Limits,
) -> Result<ArchiveIndex, String> {
    if archive.has_duplicate_names() {
        return Err("Արխիվում կրկնվող ֆայլ կա։".to_string());
    }
    if archive.len() > limits.zip_entries {
        return Err("Արխիվում ֆայլերի քանակը գերազանցում է թույլատրելի սահմանը։".to_string());
    }

    let mut workbook_index = None;
    let mut names = HashSet::new();
    let mut images = HashMap::new();
    let mut declared_total = 0u64;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("Չհաջողվեց կարդալ արխիվի գրառումը։ {error}"))?;
        if entry.is_dir() {
            return Err("Արխիվը չպետք է պարունակի պանակային գրառումներ։".to_string());
        }

        let name = entry
            .name()
            .map_err(|error| format!("Արխիվում ֆայլի անունը վավեր չէ։ {error}"))?
            .into_owned();
        if name.len() > 255 || name.chars().any(char::is_control) {
            return Err("Արխիվում ֆայլի անունը սխալ ձևաչափ ունի։".to_string());
        }
        if !names.insert(name.clone()) {
            return Err(format!(
                "Արխիվում կրկնվող ֆայլ կա՝ {}։",
                display_entry_name(&name)
            ));
        }

        if name == WORKBOOK_ENTRY {
            workbook_index = Some(index);
        } else if is_image_entry_name(&name) {
            images.insert(name, index);
        } else {
            return Err(format!(
                "Արխիվում անթույլատրելի ֆայլ կա՝ {}։",
                display_entry_name(&name)
            ));
        }

        declared_total = declared_total.checked_add(entry.size()).ok_or_else(|| {
            "Ապասեղմված արխիվի չափը գերազանցում է թույլատրելի սահմանը։".to_string()
        })?;
        if declared_total > limits.total_uncompressed_bytes {
            return Err(
                "Ապասեղմված արխիվի ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։".to_string(),
            );
        }
    }

    let workbook_index =
        workbook_index.ok_or_else(|| "Արխիվում workbook.json ֆայլը չկա։".to_string())?;
    Ok(ArchiveIndex {
        workbook_index,
        images,
    })
}

fn read_entry_limited<R: Read>(
    reader: &mut R,
    max_bytes: u64,
    budget: &mut ReadBudget,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Չհաջողվեց կարդալ {}։ {error}", display_entry_name(label)))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{} չափը գերազանցում է թույլատրելի սահմանը։",
            display_entry_name(label)
        ));
    }
    budget.account(bytes.len() as u64)?;
    Ok(bytes)
}

fn restore_images(
    workbook: &mut Workbook,
    archive: &mut zip::ZipArchive<File>,
    index: &ArchiveIndex,
    limits: &Limits,
    budget: &mut ReadBudget,
) -> Result<(), String> {
    let mut image_count = 0usize;
    let mut total_image_bytes = 0u64;

    for assignment in &mut workbook.assignments {
        for blocks in [&mut assignment.problem, &mut assignment.solution] {
            for block in blocks {
                let ContentBlock::Image { url } = block else {
                    continue;
                };

                let entry_path = url.clone();
                validate_image_entry_path(&entry_path)?;
                let entry_index = *index.images.get(&entry_path).ok_or_else(|| {
                    format!(
                        "Արխիվում նկարը չի գտնվել՝ {}։",
                        display_entry_name(&entry_path)
                    )
                })?;
                if image_count >= limits.images {
                    return Err(
                        "Արխիվում պատկերների քանակը գերազանցում է թույլատրելի սահմանը։".to_string(),
                    );
                }
                let expected_mime = supported_image_mime(&entry_path).ok_or_else(|| {
                    format!(
                        "Չաջակցվող նկարի ձևաչափ՝ {}։",
                        display_entry_name(&entry_path)
                    )
                })?;
                let mut image_entry = archive.by_index(entry_index).map_err(|error| {
                    format!(
                        "Չհաջողվեց կարդալ նկարը՝ {}։ {error}",
                        display_entry_name(&entry_path)
                    )
                })?;
                if image_entry.is_dir() {
                    return Err(format!(
                        "Արխիվի նկարի ուղին ֆայլ չէ՝ {}։",
                        display_entry_name(&entry_path)
                    ));
                }
                let image_bytes =
                    read_entry_limited(&mut image_entry, limits.image_bytes, budget, &entry_path)?;
                drop(image_entry);
                let mime_type = validate_image_bytes(&image_bytes, Some(expected_mime), limits)?;

                total_image_bytes = total_image_bytes
                    .checked_add(image_bytes.len() as u64)
                    .ok_or_else(|| {
                        "Արխիվում պատկերների ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։"
                            .to_string()
                    })?;
                if total_image_bytes > limits.total_image_bytes {
                    return Err(
                        "Արխիվում պատկերների ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։"
                            .to_string(),
                    );
                }
                image_count += 1;

                *url = format!("data:{mime_type};base64,{}", STANDARD.encode(image_bytes));
            }
        }
    }

    Ok(())
}

fn is_image_entry_name(path: &str) -> bool {
    let Some(name) = path.strip_prefix("images/") else {
        return false;
    };
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && supported_image_mime(path).is_some()
}

fn validate_image_entry_path(path: &str) -> Result<(), String> {
    if is_image_entry_name(path) {
        Ok(())
    } else {
        Err(format!("Նկարի ուղին սխալ է՝ {}։", display_entry_name(path)))
    }
}

fn display_entry_name(name: &str) -> String {
    let mut displayed: String = name
        .chars()
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .take(120)
        .collect();
    if name.chars().count() > 120 {
        displayed.push('…');
    }
    displayed
}

pub(crate) fn write_workbook_archive(path: String, workbook: Value) -> Result<(), String> {
    write_workbook_archive_with_limits(path, workbook, DEFAULT_LIMITS)
}

fn write_workbook_archive_with_limits(
    path: String,
    mut workbook: Value,
    limits: Limits,
) -> Result<(), String> {
    let target_path = zip_path(PathBuf::from(path))?;
    let document = workbook
        .as_object_mut()
        .ok_or_else(|| "Տետրի տվյալները պետք է լինեն JSON օբյեկտ։".to_string())?;
    document.insert(
        "formatVersion".to_string(),
        Value::from(CURRENT_FORMAT_VERSION),
    );
    let mut workbook: Workbook = serde_json::from_value(workbook)
        .map_err(|error| format!("Տետրի տվյալները սխալ ձևաչափ ունեն։ {error}"))?;
    validate_workbook(&workbook, &limits)?;
    let parent = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("Չհաջողվեց ստեղծել ժամանակավոր ֆայլը։ {error}"))?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut image_count = 0usize;
    let mut total_image_bytes = 0u64;
    {
        let mut archive = ZipWriter::new(temporary.as_file_mut());
        package_images(
            &mut workbook,
            &mut archive,
            options,
            &limits,
            &mut image_count,
            &mut total_image_bytes,
        )?;
        let mut json_writer = LimitedJsonWriter::new(limits.workbook_json_bytes);
        if let Err(error) = serde_json::to_writer_pretty(&mut json_writer, &workbook) {
            return Err(if json_writer.exceeded {
                "workbook.json-ի չափը գերազանցում է թույլատրելի սահմանը։".to_string()
            } else {
                format!("Չհաջողվեց ձևավորել տետրի JSON-ը։ {error}")
            });
        }
        let json_bytes = json_writer.into_inner();
        if (json_bytes.len() as u64)
            .checked_add(total_image_bytes)
            .is_none_or(|total| total > limits.total_uncompressed_bytes)
        {
            return Err(
                "Պահպանվող տետրի ապասեղմված չափը գերազանցում է թույլատրելի սահմանը։".to_string(),
            );
        }
        if 1 + image_count > limits.zip_entries {
            return Err("Պահպանվող տետրում ֆայլերի քանակը գերազանցում է սահմանը։".to_string());
        }

        archive
            .start_file(WORKBOOK_ENTRY, options)
            .map_err(|error| format!("Չհաջողվեց ավելացնել workbook.json-ը։ {error}"))?;
        archive
            .write_all(&json_bytes)
            .map_err(|error| format!("Չհաջողվեց գրել workbook.json-ը։ {error}"))?;
        archive
            .finish()
            .map_err(|error| format!("Չհաջողվեց ավարտել ZIP արխիվը։ {error}"))?;
    }

    let archive_size = temporary
        .as_file()
        .metadata()
        .map_err(|error| format!("Չհաջողվեց ստուգել ZIP արխիվի չափը։ {error}"))?
        .len();
    if archive_size > limits.archive_bytes {
        return Err("Պահպանվող տետրի ֆայլը գերազանցում է թույլատրելի սահմանը։".to_string());
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
    workbook: &mut Workbook,
    archive: &mut ZipWriter<W>,
    options: SimpleFileOptions,
    limits: &Limits,
    image_count: &mut usize,
    total_image_bytes: &mut u64,
) -> Result<(), String> {
    let mut image_number = 0;

    for assignment in &mut workbook.assignments {
        for blocks in [&mut assignment.problem, &mut assignment.solution] {
            for block in blocks {
                let ContentBlock::Image { url } = block else {
                    continue;
                };
                if *image_count >= limits.images {
                    return Err(
                        "Տետրում պատկերների քանակը գերազանցում է թույլատրելի սահմանը։".to_string(),
                    );
                }

                let (mime_type, bytes) = decode_image_data_url(url, limits)?;
                let extension = extension_for_mime(mime_type)
                    .ok_or_else(|| format!("Չաջակցվող նկարի ձևաչափ՝ {mime_type}։"))?;

                *image_count += 1;
                *total_image_bytes = total_image_bytes
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| {
                        "Տետրում պատկերների ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։"
                            .to_string()
                    })?;
                if *total_image_bytes > limits.total_image_bytes {
                    return Err(
                        "Տետրում պատկերների ընդհանուր չափը գերազանցում է թույլատրելի սահմանը։"
                            .to_string(),
                    );
                }

                image_number += 1;
                let entry_path = format!("images/image-{image_number:04}.{extension}");
                archive
                    .start_file(entry_path.clone(), options)
                    .map_err(|error| format!("Չհաջողվեց ավելացնել նկարը ZIP-ում։ {error}"))?;
                archive
                    .write_all(&bytes)
                    .map_err(|error| format!("Չհաջողվեց գրել նկարը ZIP-ում։ {error}"))?;

                *url = entry_path;
            }
        }
    }

    Ok(())
}

fn decode_image_data_url(
    data_url: &str,
    limits: &Limits,
) -> Result<(&'static str, Vec<u8>), String> {
    let (metadata, encoded_data) = data_url
        .split_once(',')
        .ok_or_else(|| "Նկարի տվյալների URL-ը սխալ ձևաչափ ունի։".to_string())?;
    let metadata = metadata
        .strip_prefix("data:")
        .ok_or_else(|| "Նկարի տվյալները պետք է լինեն Data URL ձևաչափով։".to_string())?;
    let mut parts = metadata.split(';');
    let mime_type = parts.next().unwrap_or_default().to_ascii_lowercase();

    if !matches!(mime_type.as_str(), PNG_MIME | JPEG_MIME) || !parts.any(|part| part == "base64") {
        return Err("Նկարը պետք է լինի base64 կոդավորված PNG կամ JPEG Data URL։".to_string());
    }

    let max_encoded_bytes = limits.image_bytes.div_ceil(3).saturating_mul(4);
    if encoded_data.len() as u64 > max_encoded_bytes {
        return Err("Նկարի չափը գերազանցում է թույլատրելի սահմանը։".to_string());
    }

    let bytes = STANDARD
        .decode(encoded_data)
        .map_err(|error| format!("Չհաջողվեց ապակոդավորել նկարը։ {error}"))?;
    if bytes.len() as u64 > limits.image_bytes {
        return Err("Նկարի չափը գերազանցում է թույլատրելի սահմանը։".to_string());
    }
    let mime_type = validate_image_bytes(&bytes, Some(&mime_type), limits)?;
    Ok((mime_type, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::File;
    use zip::ZipArchive;

    const TEST_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    const TEST_PNG_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    fn write_test_archive(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, contents) in entries {
            archive.start_file(*name, options).unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
    }

    fn minimal_workbook() -> Value {
        json!({
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "solved": false,
            "assignments": []
        })
    }

    #[test]
    fn zip_contains_versioned_json_and_extracted_images() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "title": "Դաս",
            "description": "",
            "author": "անհայտ դաշտ",
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
                    "url": TEST_PNG_DATA_URL,
                    "unknown": true
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
        assert!(
            document["assignments"][0]["problem"][0]
                .get("unknown")
                .is_none()
        );

        let mut image = archive.by_name("images/image-0001.png").unwrap();
        let mut bytes = Vec::new();
        image.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, STANDARD.decode(TEST_PNG_BASE64).unwrap());
    }

    #[test]
    fn rejects_invalid_image_data_urls() {
        assert!(decode_image_data_url("data:text/plain;base64,SGVsbG8=", &DEFAULT_LIMITS).is_err());
        assert!(decode_image_data_url("data:image/png,SGVsbG8=", &DEFAULT_LIMITS).is_err());
        assert!(decode_image_data_url("data:image/png;base64,SGVsbG8=", &DEFAULT_LIMITS).is_err());
        assert!(decode_image_data_url(TEST_PNG_DATA_URL, &DEFAULT_LIMITS).is_ok());
    }

    #[test]
    fn rejects_image_mime_mismatch_on_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let mut workbook = minimal_workbook();
        workbook["assignments"] = json!([{
            "problem": [{
                "type": "image",
                "url": format!("data:image/jpeg;base64,{TEST_PNG_BASE64}")
            }],
            "solution": []
        }]);

        let error = write_workbook_archive_with_limits(
            path.to_string_lossy().into_owned(),
            workbook,
            DEFAULT_LIMITS,
        )
        .unwrap_err();

        assert!(error.contains("համապատասխանում"), "{error}");
        assert!(!path.exists());
    }

    #[test]
    fn rejects_fake_archive_image() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [{
                    "type": "image",
                    "url": "images/image-0001.png"
                }],
                "solution": []
            }]
        });
        let workbook_json = serde_json::to_vec(&workbook).unwrap();
        write_test_archive(
            &path,
            &[
                ("workbook.json", &workbook_json),
                ("images/image-0001.png", b"Hello"),
            ],
        );

        let error = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap_err();

        assert!(error.contains("PNG"), "{error}");
    }

    #[test]
    fn rejects_image_with_too_many_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let mut workbook = minimal_workbook();
        workbook["assignments"] = json!([{
            "problem": [{
                "type": "image",
                "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGP4z8AARAwQCgAf7gP9i18U1AAAAABJRU5ErkJggg=="
            }],
            "solution": []
        }]);
        let limits = Limits {
            image_pixels: 3,
            ..DEFAULT_LIMITS
        };

        let error = write_workbook_archive_with_limits(
            path.to_string_lossy().into_owned(),
            workbook,
            limits,
        )
        .unwrap_err();

        assert!(error.contains("չափերը"), "{error}");
        assert!(!path.exists());
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
                    "url": TEST_PNG_DATA_URL
                }]
            }]
        });
        write_workbook_archive(path.to_string_lossy().into_owned(), workbook).unwrap();

        let opened = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap();

        assert_eq!(
            opened["assignments"][0]["problem"][0]["url"],
            TEST_PNG_DATA_URL
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
                "email": "student@example.com",
                "class": "9-րդ դասարան"
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
                    "url": TEST_PNG_DATA_URL
                }]
            }]
        });
        write_workbook_archive(path.to_string_lossy().into_owned(), workbook).unwrap();

        let opened = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap();

        assert_eq!(
            opened["assignments"][0]["solution"][0]["url"],
            TEST_PNG_DATA_URL
        );
        assert_eq!(opened["teacher"]["name"], "Ուսուցիչ");
        assert_eq!(opened["student"]["name"], "Սովորող");
        assert_eq!(opened["student"]["class"], "9-րդ դասարան");
        assert_eq!(opened["verified"]["grade"], 18);
        assert_eq!(opened["assignments"][0]["teacher-comment"], "Լավ աշխատանք։");
        assert_eq!(opened["solved"], true);
    }

    #[test]
    fn requires_current_workbook_fields() {
        assert!(serde_json::from_value::<Workbook>(json!({"assignments": []})).is_err());
        assert!(
            serde_json::from_value::<Workbook>(json!({
                "formatVersion": 1,
                "solved": "true",
                "assignments": []
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<Workbook>(json!({
                "formatVersion": 1,
                "solved": false,
                "assignments": [{
                    "problem": []
                }]
            }))
            .is_err()
        );

        let valid: Workbook = serde_json::from_value(json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [],
                "solution": []
            }]
        }))
        .unwrap();
        assert!(validate_workbook(&valid, &DEFAULT_LIMITS).is_ok());
    }

    #[test]
    fn rejects_verified_workbook_without_solved_flag() {
        let workbook: Workbook = serde_json::from_value(json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "verified": {
                "grade": 18
            },
            "created-at": "2026-11-11",
            "assignments": []
        }))
        .unwrap();

        assert!(validate_workbook(&workbook, &DEFAULT_LIMITS).is_err());
    }

    #[test]
    fn rejects_invalid_grid_state() {
        let workbook: Workbook = serde_json::from_value(json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [{
                    "type": "squared",
                    "content": {
                        "version": 2,
                        "rows": 1,
                        "columns": 1,
                        "cells": "a",
                        "borders": []
                    }
                }],
                "solution": []
            }]
        }))
        .unwrap();

        assert!(validate_workbook(&workbook, &DEFAULT_LIMITS).is_err());
    }

    #[test]
    fn rejects_grid_larger_than_limit() {
        let workbook: Workbook = serde_json::from_value(json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [{
                    "type": "squared",
                    "content": {
                        "version": 2,
                        "rows": 2,
                        "columns": 1,
                        "cells": " \n ",
                        "borders": []
                    }
                }],
                "solution": []
            }]
        }))
        .unwrap();
        let limits = Limits {
            grid_rows: 1,
            ..DEFAULT_LIMITS
        };

        assert!(validate_workbook(&workbook, &limits).is_err());
    }

    #[test]
    fn rejects_unsupported_format_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "formatVersion": 2,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": []
        });
        write_test_archive(
            &path,
            &[(
                "workbook.json",
                serde_json::to_vec(&workbook).unwrap().as_slice(),
            )],
        );

        let error = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap_err();

        assert!(error.contains("տարբերակը"), "{error}");
    }

    #[test]
    fn rejects_unsupported_archive_image_extension() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [{
                    "type": "image",
                    "url": "images/image-0001.gif"
                }],
                "solution": []
            }]
        });
        let workbook_json = serde_json::to_vec(&workbook).unwrap();
        write_test_archive(
            &path,
            &[
                ("workbook.json", &workbook_json),
                ("images/image-0001.gif", b"GIF89a"),
            ],
        );

        let error = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap_err();

        assert!(error.contains("անթույլատրելի"), "{error}");
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

    #[test]
    fn rejects_archive_larger_than_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.pdw");
        let file = File::create(&path).unwrap();
        file.set_len(64).unwrap();
        drop(file);
        let limits = Limits {
            archive_bytes: 32,
            ..DEFAULT_LIMITS
        };

        let error = read_workbook_archive_with_limits(path.to_string_lossy().into_owned(), limits)
            .unwrap_err();

        assert!(error.contains("գերազանցում է"));
    }

    #[test]
    fn rejects_too_many_archive_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        write_test_archive(
            &path,
            &[("workbook.json", b"{}"), ("images/image-0001.png", b"")],
        );
        let limits = Limits {
            zip_entries: 1,
            ..DEFAULT_LIMITS
        };

        let error = read_workbook_archive_with_limits(path.to_string_lossy().into_owned(), limits)
            .unwrap_err();

        assert!(error.contains("ֆայլերի քանակը"));
    }

    #[test]
    fn rejects_unexpected_archive_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        write_test_archive(&path, &[("workbook.json", b"{}"), ("evil.exe", b"x")]);

        let error = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap_err();

        assert!(error.contains("անթույլատրելի"));
    }

    #[test]
    fn rejects_duplicate_archive_entries() {
        const DUPLICATE_ARCHIVE: &str = "UEsDBBQAAAAAADhpb11Dv6ajAgAAAAIAAAANAAAAd29ya2Jvb2suanNvbnt9UEsDBBQAAAAAADhpb11Dv6ajAgAAAAIAAAANAAAAd29ya2Jvb2suanNvbnt9UEsBAhQDFAAAAAAAOGlvXUO/pqMCAAAAAgAAAA0AAAAAAAAAAAAAAIABAAAAAHdvcmtib29rLmpzb25QSwECFAMUAAAAAAA4aW9dQ7+mowIAAAACAAAADQAAAAAAAAAAAAAAgAEtAAAAd29ya2Jvb2suanNvblBLBQYAAAAAAgACAHYAAABaAAAAAAA=";
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        std::fs::write(&path, STANDARD.decode(DUPLICATE_ARCHIVE).unwrap()).unwrap();

        let error = read_workbook_archive(path.to_string_lossy().into_owned()).unwrap_err();

        assert!(error.contains("կրկնվող"), "{error}");
    }

    #[test]
    fn rejects_oversized_workbook_json() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = format!("{{\"padding\":\"{}\"}}", "a".repeat(64));
        write_test_archive(&path, &[("workbook.json", workbook.as_bytes())]);
        let limits = Limits {
            workbook_json_bytes: 16,
            ..DEFAULT_LIMITS
        };

        let error = read_workbook_archive_with_limits(path.to_string_lossy().into_owned(), limits)
            .unwrap_err();

        assert!(error.contains("workbook.json"));
    }

    #[test]
    fn rejects_oversized_archive_image() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = json!({
            "formatVersion": 1,
            "solved": false,
            "title": "Դաս",
            "description": "",
            "teacher": {
                "name": "Ուսուցիչ",
                "email": "teacher@example.com"
            },
            "created-at": "2026-11-11",
            "assignments": [{
                "problem": [{
                    "type": "image",
                    "url": "images/image-0001.png"
                }],
                "solution": []
            }]
        });
        let workbook_json = serde_json::to_vec(&workbook).unwrap();
        write_test_archive(
            &path,
            &[
                ("workbook.json", &workbook_json),
                ("images/image-0001.png", b"Hello"),
            ],
        );
        let limits = Limits {
            image_bytes: 4,
            ..DEFAULT_LIMITS
        };

        let error = read_workbook_archive_with_limits(path.to_string_lossy().into_owned(), limits)
            .unwrap_err();

        assert!(error.contains("images/image-0001.png"), "{error}");
    }

    #[test]
    fn rejects_oversized_image_data_url_on_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let mut workbook = minimal_workbook();
        workbook["assignments"] = json!([{
            "problem": [{
                "type": "image",
                "url": "data:image/png;base64,SGVsbG8="
            }],
            "solution": []
        }]);
        let limits = Limits {
            image_bytes: 4,
            ..DEFAULT_LIMITS
        };

        let error = write_workbook_archive_with_limits(
            path.to_string_lossy().into_owned(),
            workbook,
            limits,
        )
        .unwrap_err();

        assert!(error.contains("Նկարի չափը"));
        assert!(!path.exists());
    }

    #[test]
    fn rejects_oversized_workbook_json_on_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lesson.pdw");
        let workbook = minimal_workbook();
        let limits = Limits {
            workbook_json_bytes: 16,
            ..DEFAULT_LIMITS
        };

        let error = write_workbook_archive_with_limits(
            path.to_string_lossy().into_owned(),
            workbook,
            limits,
        )
        .unwrap_err();

        assert!(error.contains("workbook.json"));
        assert!(!path.exists());
    }
}
