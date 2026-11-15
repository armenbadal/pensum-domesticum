use std::io::Cursor;

use super::limits::Limits;

pub(crate) const PNG_MIME: &str = "image/png";
pub(crate) const JPEG_MIME: &str = "image/jpeg";

pub(crate) fn supported_image_mime(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(PNG_MIME),
        "jpg" | "jpeg" => Some(JPEG_MIME),
        _ => None,
    }
}

pub(crate) fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        PNG_MIME => Some("png"),
        JPEG_MIME => Some("jpg"),
        _ => None,
    }
}

pub(crate) fn validate_image_bytes(
    bytes: &[u8],
    expected_mime: Option<&str>,
    limits: &Limits,
) -> Result<&'static str, String> {
    let format = image::guess_format(bytes)
        .map_err(|_| "Նկարը պետք է լինի վավեր PNG կամ JPEG ֆայլ։".to_string())?;
    let mime = match format {
        image::ImageFormat::Png => PNG_MIME,
        image::ImageFormat::Jpeg => JPEG_MIME,
        _ => {
            return Err(
                "Նկարի ձևաչափը չի աջակցվում․ թույլատրվում են միայն PNG և JPEG։".to_string(),
            );
        }
    };
    if let Some(expected_mime) = expected_mime {
        if expected_mime != mime {
            return Err("Նկարի բովանդակությունը չի համապատասխանում նշված ձևաչափին։".to_string());
        }
    }

    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|error| format!("Չհաջողվեց կարդալ նկարի չափերը։ {error}"))?;
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or_else(|| "Նկարի չափերը չափազանց մեծ են։".to_string())?;
    if width == 0 || height == 0 || pixels > limits.image_pixels {
        return Err("Նկարի չափերը գերազանցում են թույլատրելի սահմանը։".to_string());
    }

    Ok(mime)
}
