use crate::Error;
use std::fs;
use std::path::Path;
use rocket::serde::{Serialize, Deserialize};

pub const FILENAME: &'static str = "niobium.config";


#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default="config_default_title")]
    pub TITLE: String,
    
    #[serde(default)]
    pub INSTAGRAM: String,

    #[serde(default="config_default_photos_dir")]
    pub PHOTOS_DIR: String,

    #[serde(default="config_default_cache_dir")]
    pub CACHE_DIR: String,
    
    #[serde(default="config_default_true")]
    pub INDEX_SUBDIRS: bool,
    
    #[serde(default="config_default_true")]
    pub SHOW_PHOTOS_FROM_SUBDIRS: bool,
    
    #[serde(default="config_default_true")]
    pub SHOW_NAVIGATION_PANEL: bool,
    
    #[serde(default="config_default_true")]
    pub OPEN_NAVIGATION_PANEL_BY_DEFAULT: bool,
    
    #[serde(default="config_default_database_path")]
    pub DATABASE_PATH: String,
    
    #[serde(default="config_default_sort_order")]
    pub SORT_ORDER: String,
    
    #[serde(default)]
    pub REVERSE_SORT_ORDER: bool,
    
    #[serde(default="config_default_row_height")]
    pub DEFAULT_ROW_HEIGHT: u32,
    
    #[serde(default="config_default_max_zoom")]
    pub MAX_ZOOM: u32,
    
    #[serde(default="config_default_row_height_step")]
    pub ROW_HEIGHT_STEP: u32,
    
    #[serde(default="config_default_true")]
    pub SHOW_DOWNLOAD_BUTTON: bool,
    
    #[serde(default="config_default_slideshow_delay")]
    pub SLIDESHOW_DELAY: u32,
    
    #[serde(default="config_default_thumbnail_max_size")]
    pub THUMBNAIL_MAX_SIZE: u32,
    
    #[serde(default="config_default_thumbnail_quality")]
    pub THUMBNAIL_QUALITY: u32,
    
    #[serde(default="config_default_large_view_max_size")]
    pub LARGE_VIEW_MAX_SIZE: u32,
    
    #[serde(default="config_default_large_view_quality")]
    pub LARGE_VIEW_QUALITY: u32,
    
    #[serde(default="config_default_true")]
    pub READ_EXIF: bool,
    
    #[serde(default="config_default_true")]
    pub SHOW_METADATA: bool,
    
    #[serde(default="config_default_true")]
    pub METADATA_VISIBLE_BY_DEFAULT: bool,
    
    #[serde(default="config_default_dowload_prefix")]
    pub DOWNLOAD_PREFIX: String,
    
    #[serde(default="config_default_true")]
    pub BEHIND_REVERSE_PROXY: bool,
    
    #[serde(default="config_default_uid_length")]
    pub UID_LENGTH: u32,
    
    #[serde(default)]
    pub PASSWORD: String,
}

impl Config {

    pub fn read() -> Result<Self, Error> {
        Ok(toml::from_str(Self::read_path_as_string(FILENAME)?.as_str())
            .map_err(|e| Error::ParseError(e))?)
    }

    pub fn read_as_value() -> Result<toml::Value, Error> {
        Self::read_path_as_value(FILENAME)
    }

    pub fn read_path_as_string<P>(path: P) -> Result<String, Error>
        where P: AsRef<Path>
    {
        fs::read_to_string(path)
            .map_err(|e| Error::FileError(e))
    }

    pub fn read_path_as_value<P>(path: P) -> Result<toml::Value, Error>
        where P: AsRef<Path>
    {
        Ok(Self::read_path_as_string(path)?.parse::<toml::Value>()
            .map_err(|e| Error::ParseError(e))?)
    }

    pub fn update_with<'a>(value: &'a mut toml::Value, other: &toml::Value) -> &'a toml::Value {
        if let Some(table) = value.as_table_mut() {
            if let Some(other_table) = other.as_table() {
                for entry in other_table.iter() {
                    table.insert(entry.0.clone(), entry.1.clone());
                }
            }
        }
        value
    }

    pub fn from(value: toml::Value) -> Result<Self, toml::de::Error> {
        value.try_into::<Self>()
    }

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

// /// List of chars used when building an UID (set)
// pub fn uid_chars_set() -> &'static str {
//     "0123456789abcdefghijklmnopqrstuvwxyz"
// }
