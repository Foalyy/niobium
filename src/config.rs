use crate::photos::ImageFormat;
use crate::Error;
use rand::RngCore;
use rocket::serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;
use std::{fs, path::PathBuf};
use toml::value::Table;

/// Name of the main config file in the app's folder
pub const FILENAME: &str = "niobium.config";

/// The app's config
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config {
    /// IP address to serve on. Set to "0.0.0.0" to serve on all interfaces.
    /// Default : 127.0.0.1 (only accessible locally)
    #[serde(default = "config_default_address")]
    pub ADDRESS: String,

    /// Port to serve on.
    /// Default : 8000
    #[serde(default = "config_default_port")]
    pub PORT: u16,

    /// Title displayed in the page title and the top of the navigation panel.
    /// Default : "Niobium"
    #[serde(default = "config_default_title")]
    pub TITLE: String,

    /// Instagram handle to link to in the dedicated button at the upper right,
    /// leave empty to remove the button.
    /// Default : (empty)
    #[serde(default)]
    pub INSTAGRAM: String,

    /// Path to the photos folder containing the photos to display.
    /// Write access is not required.
    /// Default : "photos" (in the app's folder)
    #[serde(default = "config_default_photos_dir")]
    pub PHOTOS_DIR: String,

    /// Path to the cache folder that will be used by the app to store thumbnails.
    /// Write access is required.
    /// Default : "cache" (in the app's folder)
    #[serde(default = "config_default_cache_dir")]
    pub CACHE_DIR: String,

    /// Path to the SQLite database file used by the app to store the photos index. It will
    /// be automatically created during the first launch, but write access to the containing
    /// folder is required.
    /// Default : "niobium.sqlite" (in the app's folder)
    #[serde(default = "config_default_database_path")]
    pub DATABASE_PATH: String,

    /// If enabled, the app will index subdirectories recursively in the photos folder.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub INDEX_SUBDIRS: bool,

    /// Number of parallel worker tasks that will be spawned when loading new photos into the
    /// database.
    /// Default : 16
    #[serde(default = "config_default_loading_workers")]
    pub LOADING_WORKERS: usize,

    /// If enable, the app will try to read EXIF metadata of photos and save them in the
    /// database.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub READ_EXIF: bool,

    /// If enabled, thumbnails will be generated immediately when the photos are loaded into
    /// the database; otherwise they will be generated on demand when requested by a browser
    /// for the first time.
    /// Default : false
    #[serde(default)]
    pub PRE_GENERATE_THUMBNAILS: bool,

    /// Configure a password needed to access this gallery. Leave empty to disable.
    /// Default : empty (no password needed)
    /// This setting is overridable.
    #[serde(default)]
    pub PASSWORD: String,

    /// If enabled, the grid display for a requested path will show every photo available in
    /// its subdirectories (therefore the root directory will show every photo in the database).
    /// Otherwise, only the photos actually inside the requested path will be shown, most like
    /// a classic file browser.
    /// Default : true
    /// This setting is overridable.
    #[serde(default = "config_default_true")]
    pub SHOW_PHOTOS_FROM_SUBDIRS: bool,

    /// If enabled, a navigation panel will be displayed when there are subdirectories in the
    /// photos folder. Otherwise, only direct links will allow users to access subdirectories.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub SHOW_NAVIGATION_PANEL: bool,

    /// If enabled, the navigation will be open by default when there are subdirectories in
    /// the requested path.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub OPEN_NAVIGATION_PANEL_BY_DEFAULT: bool,

    /// Fields(s) to use to sort the photos being displayed. This can be a single field or a
    /// comma-separated list of fields for multi-ordering. Available fields : `id`,
    /// `filename`, `title`, `date_taken`, `sort_order`.
    /// Default : "id" (the order in which they have been added to the database, which
    /// is a natural sort on the filename)
    /// This setting is overridable.
    #[serde(default = "config_default_sort_order")]
    pub SORT_ORDER: String,

    /// If enabled, the sort order of the photos will be reversed.
    /// Default : false
    /// This setting is overridable.
    #[serde(default)]
    pub REVERSE_SORT_ORDER: bool,

    /// Height of a single row displayed in grid view, as a percent of the browser's viewport
    /// height. For example, `20` will show up to 5 rows at a time. The user can change it
    /// using Zoom+ and Zoom- buttons in the interface.
    /// Default : 23 (show 4 rows with a hint of more at the bottom)
    #[serde(default = "config_default_row_height")]
    pub DEFAULT_ROW_HEIGHT: usize,

    /// Percentage by which the grid's row height is modified every time the user presses the
    /// Zoom+ / Zoom- buttons.
    /// Default : 10
    #[serde(default = "config_default_row_height_step")]
    pub ROW_HEIGHT_STEP: usize,

    /// In order to display a neat grid with photos of arbitrary ratios, the grid needs to
    /// crop some photos. This setting defines the maximum amount of crop that can be applied
    /// before giving up and leaving holes in the grid.
    /// For example, 1 means no crop is allowed, and 2 means that photos can be cropped to as
    /// much as half of their original height.
    /// Default : 2
    #[serde(default = "config_default_max_zoom")]
    pub MAX_CROP: usize,

    /// If enabled, show a button allowing the user to view metadata of photos (such as camera
    /// model and aperture) in Loupe mode.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub SHOW_METADATA: bool,

    /// If enabled, shows the photo's filename inside the metadata section in Loupe mode.
    /// Default : false
    #[serde(default = "config_default_false")]
    pub SHOW_FILENAME_IN_METADATA: bool,

    /// If enabled, the metadata will be visible by default (but can still be hidden by the
    /// user). Requires `SHOW_METADATA` to be enabled.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub METADATA_VISIBLE_BY_DEFAULT: bool,

    /// If enabled, the Loupe view will show a button allowing the user to download the photo
    /// in original quality.
    /// Default : true
    #[serde(default = "config_default_true")]
    pub SHOW_DOWNLOAD_BUTTON: bool,

    /// Prefix used for the name of downloaded photos. The UID of the photo will be appended
    /// to it.
    /// Default : "niobium_"
    #[serde(default = "config_default_dowload_prefix")]
    pub DOWNLOAD_PREFIX: String,

    /// Delay (in milliseconds) to wait before switching to the next photo in Slideshow mode.
    /// Default : 5000 (5s)
    #[serde(default = "config_default_slideshow_delay")]
    pub SLIDESHOW_DELAY: usize,

    /// Max size of thumbnails on any side, in pixels.
    /// Default : 600
    #[serde(default = "config_default_thumbnail_max_size")]
    pub THUMBNAIL_MAX_SIZE: usize,

    /// Quality used to reencode thumbnails images, in percent.
    /// Default : 75
    #[serde(default = "config_default_thumbnail_quality")]
    pub THUMBNAIL_QUALITY: usize,

    /// Max size of large-size images in Loupe view on any side, in pixels.
    /// Default : 1920
    #[serde(default = "config_default_large_view_max_size")]
    pub LARGE_VIEW_MAX_SIZE: usize,

    /// Quality used to reencode large-size images in Loupe view, in percent.
    /// Default : 85
    #[serde(default = "config_default_large_view_quality")]
    pub LARGE_VIEW_QUALITY: usize,

    /// Image format used for resized photos in cache : JPEG or WEBP.
    /// Default : WEBP
    #[serde(default = "config_default_resized_image_format")]
    pub RESIZED_IMAGE_FORMAT: ImageFormat,

    // Only for subdirs :
    /// If disabled, this directory will be ignored and no file inside it will be indexed.
    /// Default : true
    /// This setting is only allowed in subdirectories' config files.
    #[serde(default = "config_default_true")]
    pub INDEX: bool,

    /// If enabled, this folder will not be shown in the navigation panel, and a direct link
    /// will be required to access it.
    /// Default : false
    /// This setting is only allowed in subdirectories' config files.
    #[serde(default)]
    pub HIDDEN: bool,
}

impl Config {
    /// Return a Config struct with default values
    pub fn default() -> Config {
        let empty_value = toml::value::Value::Table(Table::new());
        empty_value.try_into::<Self>().unwrap()
    }

    /// Deserialize a TOML Table into a Config struct
    pub fn from_table(table: Table) -> Result<Self, toml::de::Error> {
        Self::from_value(toml::Value::Table(table))
    }

    /// Deserialize a TOML Value into a Config struct
    pub fn from_value(value: toml::Value) -> Result<Self, toml::de::Error> {
        value.try_into::<Self>()
    }

    /// Read the main config file and deserialize it into a Config struct
    pub fn read() -> Result<Self, Error> {
        Ok(toml::from_str(
            Self::read_path_as_string(FILENAME)?.as_str(),
        )?)
    }

    /// Try to read and parse the config file
    /// In case of error, print it to stderr and exit with a status code of -1
    pub fn read_or_exit() -> Self {
        // Read the config file and parse it into a Config struct
        Self::read().unwrap_or_else(|e| match e {
            Error::FileError(error, path) => {
                eprintln!(
                    "Error, unable to open the config file \"{}\" : {}",
                    path.display(),
                    error
                );
                std::process::exit(-1);
            }
            Error::TomlParserError(error) => {
                eprintln!(
                    "Error, unable to parse the config file \"{}\" : {}",
                    FILENAME, error
                );
                std::process::exit(-1);
            }
            _ => std::process::exit(-1),
        })
    }

    // /// Read the main config file and return it as a TOML Value
    // pub fn read_as_value() -> Result<toml::Value, Error> {
    //     Self::read_path_as_value(FILENAME)
    // }

    /// Read the main config file and return it as a TOML Table
    pub fn read_as_table() -> Result<Table, Error> {
        Self::read_path_as_table(FILENAME)
    }

    // /// Read the config file at the given location and deserialize it into a Config struct
    // pub fn read_path<P>(path: P) -> Result<Self, Error>
    //     where P: AsRef<Path>
    // {
    //     Ok(toml::from_str(Self::read_path_as_string(path)?.as_str())?)
    // }

    /// Read the config file at the given location and return it as a simple String
    pub fn read_path_as_string<P>(path: P) -> Result<String, Error>
    where
        P: AsRef<Path>,
    {
        fs::read_to_string(&path).map_err(|e| Error::FileError(e, PathBuf::from(path.as_ref())))
    }

    /// Read the config file at the given location and return it as a TOML Value
    pub fn read_path_as_value<P>(path: P) -> Result<toml::Value, Error>
    where
        P: AsRef<Path>,
    {
        Ok(Self::read_path_as_string(path)?.parse::<toml::Value>()?)
    }

    /// Read the config file at the given location and return it as a TOML Table
    pub fn read_path_as_table<P>(path: P) -> Result<Table, Error>
    where
        P: AsRef<Path>,
    {
        Ok(Self::read_path_as_value(path)?.try_into::<Table>()?)
    }

    // /// Update in place the given config (as a TOML Value) with the `other` config and return it
    // pub fn update_with_value<'a>(value: &'a mut toml::Value, other: &toml::Value) -> &'a toml::Value {
    //     if let Some(table) = value.as_table_mut() {
    //         if let Some(other_table) = other.as_table() {
    //             Self::update_with(table, other_table);
    //         }
    //     }
    //     value
    // }

    /// Update in place the given TOML Table by replacing its keys with the ones found in `other_table`
    pub fn update_with<'a>(table: &'a mut Table, other_table: &Table) -> &'a Table {
        for (key, value) in other_table.iter() {
            table.insert(key.clone(), value.clone());
        }
        table
    }

    /// Merge the subdir config file in the given path (if this file exists) into the given config, and
    /// return the local config found if any
    pub fn update_with_subdir(full_path: &PathBuf, into_value: &mut Table) -> Option<Table> {
        // Check if the config file exists
        let mut subdir_config_path = PathBuf::from(&full_path);
        subdir_config_path.push(".niobium.config");
        if subdir_config_path.is_file() {
            // Try to read it as a TOML value
            match Config::read_path_as_table(&subdir_config_path) {
                Ok(value) => {
                    // Update the current config with the content of this one
                    Config::update_with(into_value, &value);
                    Some(value)
                }
                Err(error) => {
                    // Log the error and continue
                    eprintln!(
                        "Warning: unable to read local config file \"{}\" : {}",
                        subdir_config_path.display(),
                        error
                    );
                    None
                }
            }
        } else {
            None
        }
    }
}

// Default values for config keys

fn config_default_address() -> String {
    "127.0.0.1".to_string()
}

fn config_default_port() -> u16 {
    8000
}

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

fn config_default_false() -> bool {
    false
}

fn config_default_database_path() -> String {
    "niobium.sqlite".to_string()
}

fn config_default_sort_order() -> String {
    "filename".to_string()
}

fn config_default_row_height() -> usize {
    23 // vh
}

fn config_default_max_zoom() -> usize {
    2
}

fn config_default_row_height_step() -> usize {
    10 // %
}

fn config_default_slideshow_delay() -> usize {
    5000 // ms
}

fn config_default_thumbnail_max_size() -> usize {
    600 // px, on any side
}

fn config_default_thumbnail_quality() -> usize {
    75 // %
}

fn config_default_large_view_max_size() -> usize {
    1920 // px, on any side
}

fn config_default_large_view_quality() -> usize {
    85 // %
}

fn config_default_resized_image_format() -> ImageFormat {
    ImageFormat::WEBP
}

fn config_default_dowload_prefix() -> String {
    "niobium_".to_string()
}

fn config_default_loading_workers() -> usize {
    16
}

/// Try to open the .secret file in the app's directory and return
/// the secret key inside. If this file doesn't exist, try to generate
/// a new one. In case of an error, print the error on stderr and exit.
pub fn get_secret_key_or_exit() -> String {
    let path = PathBuf::from(".secret");
    match fs::read_to_string(&path) {
        Ok(secret) => secret.trim().to_string(),

        // The secret file doesn't exist, try to generate one
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            print!("Secret file not found, generating a new one... ");
            let mut rand_buffer = [0; 32];
            rand::thread_rng().fill_bytes(&mut rand_buffer);
            let secret = base64::encode(rand_buffer);
            let mut file = File::create(&path).unwrap_or_else(|error| {
                eprintln!("\nError : unable to create the .secret file : {}", error);
                std::process::exit(-1);
            });
            writeln!(&mut file, "{}", secret).unwrap_or_else(|error| {
                eprintln!("\nError : unable to write to the .secret file : {}", error);
                std::process::exit(-1);
            });
            println!(" done");
            secret
        }

        // The secret file can't be read for some other reason
        Err(error) => {
            eprintln!("Error : unable to read the .secret file : {}", error);
            std::process::exit(-1);
        }
    }
}
