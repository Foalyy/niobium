use rocket::serde::{Serialize, Deserialize};

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(default="config_default_title")]
    TITLE: String,
    
    #[serde(default)]
    INSTAGRAM: String,

    #[serde(default="config_default_photos_dir")]
    PHOTOS_DIR: String,

    #[serde(default="config_default_cache_dir")]
    CACHE_DIR: String,
    
    #[serde(default="config_default_true")]
    INDEX_SUBDIRS: bool,
    
    #[serde(default="config_default_true")]
    SHOW_PHOTOS_FROM_SUBDIRS: bool,
    
    #[serde(default="config_default_true")]
    SHOW_NAVIGATION_PANEL: bool,
    
    #[serde(default="config_default_true")]
    OPEN_NAVIGATION_PANEL_BY_DEFAULT: bool,
    
    #[serde(default="config_default_database_path")]
    DATABASE_PATH: String,
    
    #[serde(default="config_default_sort_order")]
    SORT_ORDER: String,
    
    #[serde(default)]
    REVERSE_SORT_ORDER: bool,
    
    #[serde(default="config_default_row_height")]
    DEFAULT_ROW_HEIGHT: u32,
    
    #[serde(default="config_default_max_zoom")]
    MAX_ZOOM: u32,
    
    #[serde(default="config_default_row_height_step")]
    ROW_HEIGHT_STEP: u32,
    
    #[serde(default="config_default_true")]
    SHOW_DOWNLOAD_BUTTON: bool,
    
    #[serde(default="config_default_slideshow_delay")]
    SLIDESHOW_DELAY: u32,
    
    #[serde(default="config_default_thumbnail_max_size")]
    THUMBNAIL_MAX_SIZE: u32,
    
    #[serde(default="config_default_thumbnail_quality")]
    THUMBNAIL_QUALITY: u32,
    
    #[serde(default="config_default_large_view_max_size")]
    LARGE_VIEW_MAX_SIZE: u32,
    
    #[serde(default="config_default_large_view_quality")]
    LARGE_VIEW_QUALITY: u32,
    
    #[serde(default="config_default_true")]
    READ_EXIF: bool,
    
    #[serde(default="config_default_true")]
    SHOW_METADATA: bool,
    
    #[serde(default="config_default_true")]
    METADATA_VISIBLE_BY_DEFAULT: bool,
    
    #[serde(default="config_default_dowload_prefix")]
    DOWNLOAD_PREFIX: String,
    
    #[serde(default="config_default_true")]
    BEHIND_REVERSE_PROXY: bool,
    
    #[serde(default="config_default_uid_length")]
    UID_LENGTH: u32,
    
    #[serde(default)]
    PASSWORD: String,
}


// Default values for config keys

fn config_default_title() -> String {
    "Niobium".to_string()
}

fn config_default_photos_dir() -> String {
    "photos".to_string()
}

fn config_default_cache_dir() -> String {
    "cache".to_string()
}

fn config_default_true() -> bool {
    true
}

fn config_default_database_path() -> String {
    "niobium.sqlite".to_string()
}

fn config_default_sort_order() -> String {
    "filename".to_string()
}

fn config_default_row_height() -> u32 {
    23 // vh
}

fn config_default_max_zoom() -> u32 {
    2
}

fn config_default_row_height_step() -> u32 {
    10 // %
}

fn config_default_slideshow_delay() -> u32 {
    5000 // ms
}

fn config_default_thumbnail_max_size() -> u32 {
    400 // px, on any side
}

fn config_default_thumbnail_quality() -> u32 {
    70 // %
}

fn config_default_large_view_max_size() -> u32 {
    1920 // px, on any side
}

fn config_default_large_view_quality() -> u32 {
    85 // %
}

fn config_default_dowload_prefix() -> String {
    "niobium_".to_string()
}

fn config_default_uid_length() -> u32 {
    10 // Do not modify after the database has been generated
}


/// List of chars used when building an UID (biased)
pub fn uid_chars() -> &'static str {
    "012345678901234567890123456789abcdefghijklmnopqrstuvwxyz" // Intentionally biased toward numbers
}

/// List of chars used when building an UID (set)
pub fn uid_chars_set() -> &'static str {
    "0123456789abcdefghijklmnopqrstuvwxyz"
}