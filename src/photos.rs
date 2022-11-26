use crate::config::Config;
use crate::Error;
use std::path::PathBuf;
use std::io;
use rocket::serde::Serialize;
use rocket::tokio::{fs, sync::Mutex};
use rusqlite::Connection;

#[derive(Default, Serialize)]
pub struct Photo {
    id: u32,
    filename: u32,
    path: PathBuf,
    uid: String,
    md5: String,
    sort_order: u32,
    hidden: bool,
    metadata_parsed: bool,
    width: u32,
    height: u32,
    color: String,
    title: String,
    place: String,
    date_taken: String,
    camera_model: String,
    lens_mode: String,
    focal_length: String,
    aperture: String,
    exposure_time: String,
    sensitivity: String,
    index: u32,
    get_grid_item_url: String,
}

// impl Photo {
//     pub fn new() -> Self {
//         Default::default()
//     }
// }


pub async fn load(path: &PathBuf, config: &Config, db: &Mutex<Connection>) -> Result<Vec<Photo>, Error> {
    let photos: Vec<Photo> = Vec::new();

    // Make sure the main directories (photos and cache) exist, and if not, try to create them
    check_config_dir(&PathBuf::from(&config.PHOTOS_DIR)).await
        .or_else(|e| {
            if let Error::FileError(e) = &e {
                println!("There is an issue with the PHOTOS_DIR setting in the config file (\"{}\") : {} : {}", &config.PHOTOS_DIR, e.kind(), e.to_string());
            }
            Err(e)
        })?;
    check_config_dir(&PathBuf::from(&config.CACHE_DIR)).await
        .or_else(|e| {
            if let Error::FileError(e) = &e {
                eprintln!("There is an issue with the CACHE_DIR setting in the config file (\"{}\") : {} : {}", &config.CACHE_DIR, e.kind(), e.to_string());
            }
            Err(e)
        })?;

    // Make sure the requested path is valid and if so, convert it to the full path on the file system
    let full_path = check_path(&path, &config)?;

    // Find and parse all the local config files parent to this path
    let mut subdir_config = get_subdir_config(&PathBuf::from(&config.PHOTOS_DIR), &path)
        .unwrap_or(toml::value::Table::new());
    subdir_config.remove("HIDDEN"); // This setting is not passed on from the parent to the currently open path

    // Get all existing UIDs from the database
    let db_guard = db.lock().await;
    let uids = match db_guard.prepare("SELECT uid FROM photo;") {
        Ok(mut stmt) => stmt.query_map([], |row| row.get(0))
            .map_err(|e| Error::DatabaseError(e))?
            .map(|x| x.unwrap())
            .collect::<Vec<i32>>(),
        Err(e) => return Err(Error::DatabaseError(e)),
    };

    Ok(photos)
}


/// Check that the given path from the config (either PHOTOS_DIR or CACHE_DIR) exists,
/// and if not, try to create it
async fn check_config_dir(path: &PathBuf) -> Result<(), Error> {
    // Check the given path
    match path.metadata() {

        Ok(metadata) => {
            // The entity exists, check its type
            if metadata.is_dir() {
                // The given path is a valid directory, accept it
                Ok(())

            } else {
                // The given path exists but is not a valid directory so we can't create
                // it (probably a file?), return an AlreadyExists error
                Err(Error::FileError(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("\"{}\" is not a valid directory", path.display())
                )))
            }
        },

        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                // The directory doesn't exist, try to create it and return the result
                // of that operation directly
                println!("Creating empty directory \"{}\"", path.display());
                fs::create_dir_all(path).await.map_err(|e| Error::FileError(e))

            } else {
                // Another error happened, return it directly
                Err(Error::FileError(error))
            }
        },
    }
}


/// Check that the given path exists and is a valid photos folder
pub fn check_path(path: &PathBuf, config: &Config) -> Result<PathBuf, Error> {
    // The given path must be relative because it will appended to the PHOTOS_DIR path
    if path.is_absolute() {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound)));
    }

    // Forbid opening subdirectories if INDEX_SUBDIRS is disabled
    if !config.INDEX_SUBDIRS && path.to_str() != Some("") {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound)));
    }

    // Append the given relative path to the PHOTOS_DIR path, and make sure the resulting full_path exists
    let mut full_path = PathBuf::from(&config.PHOTOS_DIR);
    full_path.push(path);
    if !full_path.is_dir() {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound)));
    }

    // Return the full path to the caller
    Ok(full_path)
}


/// Find and parse all the subdir config files parent to the given path and return the compiled config
fn get_subdir_config(photos_path: &PathBuf, path: &PathBuf) -> Result<toml::value::Table, Error> {

    /// Merge the subdir config file in the given path (if this file exists) into the given config
    fn merge(path: &PathBuf, into_value: &mut toml::Value) {
        // Check if the config file exists
        let mut subdir_config_path = PathBuf::from(&path);
        subdir_config_path.push(".niobium.config");
        if subdir_config_path.is_file() {
            // Try to read it as a TOML value
            match Config::read_path_as_value(&subdir_config_path) {
                Ok(value) => {
                    // Update the current config with the content of this one
                    Config::update_with(into_value, &value);
                }
                Err(error) => {
                    // Log the error and continue
                    eprintln!("Error reading local config file \"{}\" : {}", subdir_config_path.display(), error);
                },
            };
        }
    }

    // Read the main config as the base config to start with
    let mut subdir_config_value = Config::read_as_value()
        .map_err(|error| {
            eprintln!("Error reading config file \"{}\" : {}", crate::config::FILENAME, error);
            io::Error::new(io::ErrorKind::Other, error.to_string())
        }).map_err(|e| Error::FileError(e))?;

    // From the photos directory, explore every subdir in the given path
    let mut current_path = PathBuf::from(photos_path);
    merge(&current_path, &mut subdir_config_value);
    for component in path.components() {
        current_path.push(&component);
        merge(&current_path, &mut subdir_config_value);
    }

    // Try to deserialize the TOML value into a Config struct and return it
    Ok(subdir_config_value.try_into::<toml::value::Table>().unwrap_or_else(|e| {
        eprintln!("Error : unable to parse the subdirectory's config : {}", e);
        toml::value::Table::new()
    }))
}
