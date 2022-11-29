use crate::config::Config;
use crate::{Error, db};
use std::future::Future;
use std::io::{self, Write};
use std::path::PathBuf;
use std::pin::Pin;
use md5::{Md5, Digest};
use rand::seq::SliceRandom;
use rand::thread_rng;
use rocket::serde::Serialize;
use rocket::tokio::time::Instant;
use rocket::tokio::{fs, sync::Mutex};
use rocket::futures::StreamExt;
use tokio_stream::wrappers::ReadDirStream;
use rusqlite::Connection;
use toml::value::Table;

#[derive(Default, Serialize, Clone, Debug)]
pub struct Photo {
    pub id: u32,
    pub filename: String,
    pub path: PathBuf,
    pub uid: String,
    pub md5: String,
    pub sort_order: u32,
    pub hidden: bool,
    pub metadata_parsed: bool,
    pub width: u32,
    pub height: u32,
    pub color: String,
    pub title: String,
    pub place: String,
    pub date_taken: String,
    pub camera_model: String,
    pub lens_mode: String,
    pub focal_length: String,
    pub aperture: String,
    pub exposure_time: String,
    pub sensitivity: String,
    pub index: u32,
    pub get_grid_item_url: String,
}


pub async fn load(path: &PathBuf, config: &Config, db_conn: &Mutex<Connection>) -> Result<Vec<Photo>, Error> {

    // Inner function used to load photos recursively
    fn _load<'a>(full_path: &'a PathBuf, rel_path: &'a PathBuf, db_conn: &'a Mutex<Connection>, main_config: &'a Config, subdir_config: &'a toml::value::Table, displayed_photos: &'a mut Vec<Photo>, photos_to_insert: &'a mut Option<&mut Vec<Photo>>,
            photos_to_remove: &'a mut Option<&mut Vec<Photo>>, paths_found: &'a mut Option<&mut Vec<PathBuf>>, is_subdir: bool) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(async move {
            let is_requested_root = !is_subdir;

            // Append this path to the list of paths found
            if let Some(paths_found) = paths_found {
                paths_found.push(rel_path.clone());
            }

            // Try to find a config file in this directory and append it to a copy of the current one (so it won't propagate to sibling directories)
            let parent_config = subdir_config.clone();
            let mut subdir_config = subdir_config.clone();
            merge_subdir_config(&full_path, &mut subdir_config);

            // HIDDEN only applies to subdirectories, and a HIDDEN=false doesn't override a parent HIDDEN=true
            if (is_requested_root && subdir_config.contains_key("HIDDEN")) || parent_config.get("HIDDEN") == Some(&toml::value::Value::Boolean(true)) {
                subdir_config.remove("HIDDEN");
                println!("    update : subdir_config={:?}", subdir_config);
            }

            // List the files inside this path in the photos directory
            let mut filenames_in_fs: Vec<String> = Vec::new();
            if photos_to_insert.is_some() || photos_to_remove.is_some() {
                let dir = fs::read_dir(full_path).await
                    .map_err(|e| Error::FileError(e, full_path.clone()))?;
                let mut dir_stream = ReadDirStream::new(dir);
                while let Some(entry) = dir_stream.next().await {
                    let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
                    if let Ok(file_type) = entry.file_type().await {
                        if let Ok(filename) = entry.file_name().into_string() {
                            let filename_lowercase = filename.to_lowercase();
                            if file_type.is_file() && (filename_lowercase.ends_with(".jpg") || filename_lowercase.ends_with(".jpeg")) {
                                filenames_in_fs.push(filename);
                            }
                        }
                    }
                }
                filenames_in_fs.sort();
            }

            // Get the list of photos saved in the database for this path exactly
            let sort_columns = String::from(subdir_config.get("SORT_ORDER").and_then(|v| v.as_str()).unwrap_or("filename"))
                .split(",").map(|s| String::from(s.trim())).collect::<Vec<String>>();
            let reverse_sort_order = subdir_config.get("REVERSE_SORT_ORDER").and_then(|v| v.as_bool()).unwrap_or(false);
            let photos_in_db = db::get_photos_in_path(db_conn, &rel_path, &sort_columns, reverse_sort_order).await?;

            // Find photos in the filesystem that are not in the database yet
            if let Some(ref mut photos_to_insert) = photos_to_insert {
                let filenames_in_db = photos_in_db.iter().map(|photo| &photo.filename).collect::<Vec<&String>>();
                for filename in &filenames_in_fs {
                    if !filenames_in_db.contains(&filename) {
                        photos_to_insert.push(Photo {
                            path: rel_path.clone(),
                            filename: filename.clone(),
                            ..Default::default()
                        });
                    }
                }
            }

            // Find photos in the database that are not in the filesystem anymore
            if let Some(ref mut photos_to_remove) = photos_to_remove {
                for photo in &photos_in_db {
                    if !filenames_in_fs.contains(&photo.filename) {
                        photos_to_remove.push(photo.clone());
                    }
                }
            }

            // Delete old resized photos from cache
            let mut resized_photos_to_delete: Vec<String> = Vec::new();
            let all_uids_in_path = photos_in_db.iter().map(|photo| &photo.uid).collect::<Vec<&String>>();
            let suffix = ".jpg";
            let mut cache_path = PathBuf::from(&main_config.CACHE_DIR);
            cache_path.push(rel_path);
            match fs::read_dir(&cache_path).await {
                Ok(dir) => {
                    // Iterate over the list of resized photos in the cache directory for this path
                    let mut dir_stream = ReadDirStream::new(dir);
                    while let Some(entry) = dir_stream.next().await {
                        let entry = entry.map_err(|e| Error::FileError(e, cache_path.clone()))?;
                        if let Ok(file_type) = entry.file_type().await {
                            if let Ok(filename) = entry.file_name().into_string() {
                                let filename_lowercase = filename.to_lowercase();
                                for prefix in ["thumbnail_", "large_"] {
                                    // Check if this is a jpeg file with a known prefix
                                    if file_type.is_file() && filename_lowercase.starts_with(prefix) && filename_lowercase.ends_with(suffix) {
                                        // Extract the UID from the filename
                                        let file_uid: String = filename.chars().skip(prefix.len()).take(filename.len() - prefix.len() - suffix.len()).collect();
                                        if !all_uids_in_path.contains(&&file_uid) {
                                            // This UID is not in the database anymore for this path, add it to the 'to remove' list
                                            resized_photos_to_delete.push(filename);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("Warning : unable to open cache directory \"{}\" : {}", &cache_path.display(), error);
                    }
                }
            }
            if !resized_photos_to_delete.is_empty() {
                // Log the list of files to delete
                println!("Deleting {} obsolete resized photos in \"{}\" from cache : {}", 
                        resized_photos_to_delete.len(),
                        &cache_path.display(),
                        resized_photos_to_delete.iter()
                            .map(|filename| format!("\"{}\"", filename))
                            .collect::<Vec<String>>().join(", ")
                );

                // Delete the files
                for filename in resized_photos_to_delete {
                    let mut path = cache_path.clone();
                    path.push(filename);
                    fs::remove_file(&path).await.map_err(|e| Error::FileError(e, path.clone()))?;
                }
            }

            // Check if a password is required for this path, and if so, if it has been provided
            let is_password_ok = match subdir_config.get("PASSWORD") {
                Some(value) => match value.as_str() {
                    Some(_password) => {
                        // TODO : check password in session
                        true
                    }
                    None => {
                        eprintln!("Invalid value for config parameter \"PASSWORD\" in path {}", rel_path.display());
                        false // Forbid access by default
                    }
                }
                None => true, // Password not needed
            };

            // If this is a subdirectory, add these photos only if :
            //   - the SHOW_PHOTOS_FROM_SUBDIRS config is enabled
            //   - this directory is not hidden
            //   - the password has been provided, if required
            let show_photos_from_subdir = parent_config.get("SHOW_PHOTOS_FROM_SUBDIRS").and_then(|v| v.as_bool()).unwrap_or(true);
            let hidden = subdir_config.get("HIDDEN").and_then(|v| v.as_bool()).unwrap_or(false);
            if is_requested_root || (show_photos_from_subdir && !hidden && is_password_ok) {
                for photo in photos_in_db {
                    if !photo.hidden {
                        displayed_photos.push(photo);
                    }
                }
            }

            // If the INDEX_SUBDIRS config is enabled, recursively load photos from subdirectories
            if main_config.INDEX_SUBDIRS {
                // Find the list of subdirectories in the path, in the filesystem
                let mut subdirs = list_subdirs(&rel_path, &main_config, true).await?;

                // Clean obsolete subdirectories (that do not correspond to a subdirectory in the photos folder) from the cache folder
                //let subdirs_in_cache: Vec<String> = Vec::new();
                // TODO

                // Load subdirs recursively
                if !subdirs.is_empty() {
                    subdirs.sort();
                    //println!("    subdirs({}):{:?}", subdirs.len(), subdirs);
                    for subdir in subdirs {
                        let mut subdir_rel_path = rel_path.clone();
                        subdir_rel_path.push(&subdir);
                        let mut subdir_full_path = full_path.clone();
                        subdir_full_path.push(&subdir);
                        _load(&subdir_full_path, &subdir_rel_path, db_conn, main_config, &subdir_config, displayed_photos, photos_to_insert, photos_to_remove, paths_found, true).await?;
                    }
                }
            }

            Ok(())
        })
    }

    // Make sure the main directories (photos and cache) exist, and if not, try to create them
    check_config_dir(&PathBuf::from(&config.PHOTOS_DIR)).await
        .or_else(|e| {
            if let Error::FileError(error, path) = &e {
                println!("There is an issue with the PHOTOS_DIR setting in the config file (\"{}\") : {} : {}", path.display(), error.kind(), error.to_string());
            }
            Err(e)
        })?;
    check_config_dir(&PathBuf::from(&config.CACHE_DIR)).await
        .or_else(|error| {
            if let Error::FileError(error, path) = &error {
                eprintln!("There is an issue with the CACHE_DIR setting in the config file (\"{}\") : {} : {}", path.display(), error.kind(), error.to_string());
            }
            Err(error)
        })?;

    // Make sure the requested path is valid and if so, convert it to the full path on the file system
    let full_path = check_path(&path, &config)?;

    // Find and parse all the local config files parent to this path
    let mut subdir_config = get_subdir_config(&PathBuf::from(&config.PHOTOS_DIR), &path)
        .unwrap_or(toml::value::Table::new());
    subdir_config.remove("HIDDEN"); // This setting is not passed on from the parent to the currently open path

    // Get all existing UIDs from the database
    let mut existing_uids = db::get_existing_uids(db_conn).await?;

    // Load the photos in this path
    let mut displayed_photos: Vec<Photo> = Vec::new();
    let mut photos_to_insert: Vec<Photo> = Vec::new();
    let mut photos_to_remove: Vec<Photo> = Vec::new();
    let mut paths_found: Vec<PathBuf> = Vec::new();
    _load(&full_path, &path, db_conn, &config, &subdir_config, &mut displayed_photos, &mut Some(&mut photos_to_insert), 
    &mut Some(&mut photos_to_remove), &mut Some(&mut paths_found), false).await?;

    // Get the list of all known subdirs of the current path in the database, check if some have been removed,
    // and if so add their photos to the 'to_remove' list
    if config.INDEX_SUBDIRS {
        let mut deleted_paths:Vec<PathBuf> = Vec::new();
        let known_paths_in_db = db::get_paths_starting_with(db_conn, &path).await?;
        for known_path in known_paths_in_db {
            if !paths_found.contains(&known_path) {
                deleted_paths.push(known_path);
            }
        }
        if !deleted_paths.is_empty() {
            let photos_in_deleted_paths = db::get_photos_in_paths(db_conn, &deleted_paths).await?;
            for photo in photos_in_deleted_paths {
                photos_to_remove.push(photo);
            }
        }
    }

    // Calculate the MD5 hashes of the new files
    if !photos_to_insert.is_empty() {
        let now = Instant::now();
        let n = photos_to_insert.len();
        let mut last_percent: usize = 0;
        for (i, photo) in photos_to_insert.iter_mut().enumerate() {
            let mut photo_full_path = PathBuf::from(&config.PHOTOS_DIR);
            photo_full_path.push(&photo.path);
            photo_full_path.push(&photo.filename);
            photo.md5 = calculate_file_md5(&photo_full_path).await?;
            let percent: usize = (i + 1) * 100 / n;
            if percent > last_percent {
                print!("\rCalculating MD5 hashes of {} new files... {}%", n, percent);
                std::io::stdout().flush().ok();
                last_percent = percent;
            }
        }
        println!("\nDone in {}ms", now.elapsed().as_millis());
    }

    // Detect if some of the insert/remove are actually the same file that has been moved or renamed
    let mut photos_to_move: Vec<(Photo, Photo)> = Vec::new();
    if !&photos_to_insert.is_empty() && !photos_to_remove.is_empty() {
        let mut duplicate_hashes: Vec<String> = Vec::new();
        for new_photo in &photos_to_insert {
            for old_photo in &photos_to_remove {
                if old_photo.md5 == new_photo.md5 {
                    duplicate_hashes.push(old_photo.md5.clone());
                    photos_to_move.push((old_photo.clone(), new_photo.clone()));
                }
            }
        }
        photos_to_insert.retain(|photo| !duplicate_hashes.contains(&photo.md5));
        photos_to_remove.retain(|photo| !duplicate_hashes.contains(&photo.md5));
    }

    // Apply detected modifications (photos added, moved, or deleted) to the database
    if !photos_to_insert.is_empty() {
        // Generate a new UID for these photos
        for photo in photos_to_insert.iter_mut() {
            photo.uid = generate_uid(&existing_uids, config.UID_LENGTH);
            existing_uids.push(photo.uid.clone());
        }

        // Log the list of photos to insert
        println!("Inserting {} photo(s) into the database : {}",
                photos_to_insert.len(),
                photos_to_insert.iter()
                    .map(|photo| format!("\"{}/{}\"", photo.path.to_str().unwrap(), photo.filename))
                    .collect::<Vec<String>>().join(", ")
        );

        // Insert them into the database
        db::insert_photos(db_conn, &photos_to_insert).await?;
    }
    if !photos_to_remove.is_empty() {
        // Log the list of photos to remove
        println!("Removing {} photo(s) into the database : {}",
                photos_to_remove.len(),
                photos_to_remove.iter()
                    .map(|photo| format!("\"{}/{}\"", photo.path.to_str().unwrap(), photo.filename))
                    .collect::<Vec<String>>().join(", ")
        );

        // Remove them from the database
        db::remove_photos(db_conn, &photos_to_remove).await?;
    }
    if !photos_to_move.is_empty() {
        // Log the list of photos to rename/move
        println!("Renaming/moving {} photo(s) into the database : {}",
                photos_to_move.len(),
                photos_to_move.iter()
                    .map(|pair| format!("\"{}/{}\" -> \"{}/{}\"", pair.0.path.to_str().unwrap(), pair.0.filename, pair.1.path.to_str().unwrap(), pair.1.filename))
                    .collect::<Vec<String>>().join(", ")
        );

        // Update the database
        db::move_photos(db_conn, &photos_to_move).await?;
    }

    // If there were some modifications to the photos, reload the database after updating it
    if !photos_to_insert.is_empty() || !photos_to_remove.is_empty() || !photos_to_move.is_empty() {
        displayed_photos.clear();
        _load(&full_path, &path, db_conn, &config, &subdir_config, &mut displayed_photos, &mut None,
            &mut None, &mut None, false).await?;
    }

    for (index, photo) in displayed_photos.iter_mut().enumerate() {
        photo.index = index as u32;
    }

    Ok(displayed_photos)
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
                ), path.clone()))
            }
        },

        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                // The directory doesn't exist, try to create it and return the result
                // of that operation directly
                println!("Creating empty directory \"{}\"", path.display());
                fs::create_dir_all(path).await.map_err(|e| Error::FileError(e, path.clone()))

            } else {
                // Another error happened, return it directly
                Err(Error::FileError(error, path.clone()))
            }
        },
    }
}


/// Check that the given path exists and is a valid photos folder
pub fn check_path(path: &PathBuf, config: &Config) -> Result<PathBuf, Error> {
    // The given path must be relative because it will appended to the PHOTOS_DIR path
    if path.is_absolute() {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound), path.clone()));
    }

    // Forbid opening subdirectories if INDEX_SUBDIRS is disabled
    if !config.INDEX_SUBDIRS && path.to_str() != Some("") {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound), path.clone()));
    }

    // Append the given relative path to the PHOTOS_DIR path, and make sure the resulting full_path exists
    let mut full_path = PathBuf::from(&config.PHOTOS_DIR);
    full_path.push(path);
    if !full_path.is_dir() {
        return Err(Error::FileError(io::Error::from(io::ErrorKind::NotFound), path.clone()));
    }

    // Return the full path to the caller
    Ok(full_path)
}


/// Find and parse all the subdir config files parent to the given path and return the compiled config
fn get_subdir_config(photos_path: &PathBuf, path: &PathBuf) -> Result<toml::value::Table, Error> {

    // Read the main config as the base config to start with
    let mut subdir_config_table = Config::read_as_table()?;

    // From the photos directory, explore every subdir in the given path
    let mut current_path = PathBuf::from(photos_path);
    merge_subdir_config(&current_path, &mut subdir_config_table);
    for component in path.components() {
        current_path.push(&component);
        merge_subdir_config(&current_path, &mut subdir_config_table);
    }

    Ok(subdir_config_table)
}


/// Merge the subdir config file in the given path (if this file exists) into the given config
fn merge_subdir_config(full_path: &PathBuf, into_value: &mut toml::value::Table) -> bool {
    // Check if the config file exists
    let mut subdir_config_path = PathBuf::from(&full_path);
    subdir_config_path.push(".niobium.config");
    if subdir_config_path.is_file() {
        // Try to read it as a TOML value
        match Config::read_path_as_table(&subdir_config_path) {
            Ok(value) => {
                // Update the current config with the content of this one
                Config::update_with(into_value, &value);
            }
            Err(error) => {
                // Log the error and continue
                eprintln!("Warning: unable to read local config file \"{}\" : {}", subdir_config_path.display(), error);
            },
        };
        true
    } else {
        false
    }
}


/// Return the list of valid subdirectories in the given path in the photos folder
async fn list_subdirs(path: &PathBuf, config: &Config, include_hidden: bool) -> Result<Vec<String>, Error> {
    let mut subdirs: Vec<String> = Vec::new();
    let mut full_path = PathBuf::from(&config.PHOTOS_DIR);
    full_path.push(path);
    let a = fs::read_dir(&full_path).await;
    let dir = a.map_err(|e| Error::FileError(e, full_path.clone()))?;
    let mut dir_stream = ReadDirStream::new(dir);
    while let Some(entry) = dir_stream.next().await {
        let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
        if let Ok(file_type) = entry.file_type().await {
            if let Ok(dir_name) = entry.file_name().into_string() {
                if file_type.is_dir() && !dir_name.starts_with(".") {
                    let mut subdir_path = full_path.clone();
                    subdir_path.push(&dir_name);
                    let mut subdir_config_table: Table = Table::new();
                    merge_subdir_config(&subdir_path, &mut subdir_config_table);
                    let subdir_config = Config::from_table(subdir_config_table).unwrap_or_default();
                    if subdir_config.INDEX && (include_hidden || !subdir_config.HIDDEN) {
                        subdirs.push(dir_name);
                    }
                }
            }
        }
    }
    Ok(subdirs)
}


/// Calculate and return the MD5 hash of the file located at the given path
async fn calculate_file_md5(path: &PathBuf) -> Result<String, Error> {
    let file_content = fs::read(path).await.map_err(|e| Error::FileError(e, path.clone()))?;
    let hash = Md5::digest(file_content);
    Ok(format!("{:x}", hash))
}


// List of chars used when building an UID (biased)
pub const UID_CHARS_BIASED: [(char, u32); 36] = [
    ('0', 4), ('1', 4), ('2', 4), ('3', 4), ('4', 4), ('5', 4), ('6', 4), ('7', 4), ('8', 4), ('9', 4),
    ('a', 1), ('b', 1), ('c', 1), ('d', 1), ('e', 1), ('f', 1), ('g', 1), ('h', 1), ('i', 1), ('j', 1), ('k', 1), ('l', 1), ('m', 1),
    ('n', 1), ('o', 1), ('p', 1), ('q', 1), ('r', 1), ('s', 1), ('t', 1), ('u', 1), ('v', 1), ('w', 1), ('x', 1), ('y', 1), ('z', 1)
];

// List of chars used when building an UID (set)
pub const UID_CHARS: &str = "0123456789abcdefghijklmnopqrstuvwxyz";

/// Generate an UID of the given length that doesn't already exist in the given list
fn generate_uid(existing_uids: &Vec<String>, length: u32) -> String {
    let mut rng = thread_rng();
    loop {
        let uid = UID_CHARS_BIASED.choose_multiple_weighted(&mut rng, length as usize, |item| item.1).unwrap()
            .map(|item| String::from(item.0))
            .collect::<Vec<String>>()
            .join("");
        if !existing_uids.contains(&uid) {
            break uid;
        }
    }
}