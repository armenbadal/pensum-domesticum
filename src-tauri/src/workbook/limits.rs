#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) archive_bytes: u64,
    pub(crate) zip_entries: usize,
    pub(crate) workbook_json_bytes: u64,
    pub(crate) image_bytes: u64,
    pub(crate) total_image_bytes: u64,
    pub(crate) images: usize,
    pub(crate) total_uncompressed_bytes: u64,
    pub(crate) grid_rows: usize,
    pub(crate) grid_columns: usize,
    pub(crate) image_pixels: u64,
}

pub(crate) const CURRENT_FORMAT_VERSION: u8 = 2;

pub(crate) const DEFAULT_LIMITS: Limits = Limits {
    archive_bytes: 50 * 1024 * 1024,
    zip_entries: 110,
    workbook_json_bytes: 2 * 1024 * 1024,
    image_bytes: 5 * 1024 * 1024,
    total_image_bytes: 40 * 1024 * 1024,
    images: 100,
    total_uncompressed_bytes: 50 * 1024 * 1024,
    grid_rows: 200,
    grid_columns: 200,
    image_pixels: 50_000_000,
};
