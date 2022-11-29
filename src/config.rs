use crate::Error;
use std::{fs, path::PathBuf};
use std::path::Path;
use rocket::serde::{Serialize, Deserialize};
use toml::value::Table;

pub const FILENAME: &'static str = "niobium.config";


#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
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

    // Only for subdirs :
    
    #[serde(default="config_default_true")]
    pub INDEX: bool,
    
    #[serde(default="config_default_true")]
    pub HIDDEN: bool,
}

impl Config {

    pub fn read() -> Result<Self, Error> {
        Ok(toml::from_str(Self::read_path_as_string(FILENAME)?.as_str())
            .map_err(|e| Error::ParseError(e))?)
    }

    /// Try to read and parse the config file
    /// In case of error, print it to stderr and exit with a status code of -1
    pub fn read_or_exit() -> Self {
        // Read the config file and parse it into a Config struct
        Self::read()
            .unwrap_or_else(|e| match e {
                Error::FileError(error, path) => {
                    eprintln!("Error, unable to open the config file \"{}\" : {}", path.display(), error);
                    std::process::exit(-1);
                }
                Error::ParseError(error) => {
                    eprintln!("Error, unable to parse the config file \"{}\" : {}", FILENAME, error);
                    std::process::exit(-1);
                }
                _ => std::process::exit(-1),
            })
    }

    // pub fn read_as_value() -> Result<toml::Value, Error> {
    //     Self::read_path_as_value(FILENAME)
    // }

    pub fn read_as_table() -> Result<Table, Error> {
        Self::read_path_as_table(FILENAME)
    }

    // pub fn read_path<P>(path: P) -> Result<Self, Error>
    //     where P: AsRef<Path>
    // {
    //     Ok(toml::from_str(Self::read_path_as_string(path)?.as_str())
    //         .map_err(|e| Error::ParseError(e))?)
    // }

    pub fn read_path_as_string<P>(path: P) -> Result<String, Error>
        where P: AsRef<Path>
    {
        fs::read_to_string(&path)
            .map_err(|e| Error::FileError(e, PathBuf::from(path.as_ref())))
    }

    pub fn read_path_as_value<P>(path: P) -> Result<toml::Value, Error>
        where P: AsRef<Path>
    {
        Ok(Self::read_path_as_string(path)?.parse::<toml::Value>()
            .map_err(|e| Error::ParseError(e))?)
    }

    pub fn read_path_as_table<P>(path: P) -> Result<Table, Error>
        where P: AsRef<Path>
    {
        Ok(Self::read_path_as_value(path)?.try_into::<Table>()
            .map_err(|e| Error::ParseError(e))?)
    }

    // pub fn update_with_value<'a>(value: &'a mut toml::Value, other: &toml::Value) -> &'a toml::Value {
    //     if let Some(table) = value.as_table_mut() {
    //         if let Some(other_table) = other.as_table() {
    //             Self::update_with(table, other_table);
    //         }
    //     }
    //     value
    // }

    pub fn update_with<'a>(table: &'a mut Table, other_table: &Table) -> &'a Table {
        for entry in other_table.iter() {
            table.insert(entry.0.clone(), entry.1.clone());
        }
        table
    }

    pub fn from_table(table: Table) -> Result<Self, toml::de::Error> {
        Self::from_value(toml::Value::Table(table))
    }

    pub fn from_value(value: toml::Value) -> Result<Self, toml::de::Error> {
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
