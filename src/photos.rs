use std::path::PathBuf;

use std::{io, fs};
use rocket::serde::Serialize;
use crate::config::Config;

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


pub fn load(path: &PathBuf, config: &Config) -> io::Result<Vec<Photo>> {
    let photos: Vec<Photo> = Vec::new();

    // Make sure the main directories (photos and cache) exist, and if not, try to create them
    check_config_dir(&PathBuf::from(&config.PHOTOS_DIR))
        .or_else(|e| {
            eprintln!("There is an issue with the PHOTOS_DIR setting in the config file (\"{}\") : {} : {}", &config.PHOTOS_DIR, e.kind(), e.to_string());
            Err(e)
        })?;

    check_config_dir(&PathBuf::from(&config.CACHE_DIR))
        .or_else(|e| {
            eprintln!("There is an issue with the CACHE_DIR setting in the config file (\"{}\") : {} : {}", &config.CACHE_DIR, e.kind(), e.to_string());
            Err(e)
        })?;
    
    // Make sure the requested path is valid and if so, convert it to the full path on the file system
    let full_path = check_path(&path, &config)?;

    Ok(photos)
}


fn check_config_dir(path: &PathBuf) -> io::Result<()> {
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
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("\"{}\" is not a valid directory", path.display())
                ))
            }
        },

        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                // The directory doesn't exist, try to create it and return the result
                // of that operation directly
                println!("Creating empty directory \"{}\"", path.display());
                fs::create_dir_all(path)

            } else {
                // Another error happened, return it directly
                Err(error)
            }
        },
    }
}


pub fn check_path(path: &PathBuf, config: &Config) -> io::Result<PathBuf> {
    // The given path must be relative because it will appended to the PHOTOS_DIR path
    if path.is_absolute() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }

    // Forbid opening subdirectories if INDEX_SUBDIRS is disabled
    if !config.INDEX_SUBDIRS && path.to_str() != Some("") {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }

    // Append the given relative path to the PHOTOS_DIR path, and make sure the resulting full_path exists
    let mut full_path = PathBuf::from(&config.PHOTOS_DIR);
    full_path.push(path);
    if !full_path.is_dir() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }

    // Return the full path to the caller
    Ok(full_path)
}
