use crate::config::Config;
use crate::uid::UID;
use crate::{Error, db};
use std::cmp::min;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use md5::{Md5, Digest};
use rocket::tokio::sync::{RwLock, RwLockReadGuard};
use rocket::tokio::task::JoinSet;
use rocket::{Rocket, fairing, tokio};
use rocket::serde::Serialize;
use rocket::tokio::fs::create_dir_all;
use rocket::tokio::time::Instant;
use rocket::tokio::fs;
use rocket::futures::StreamExt;
use rocket_db_pools::Database;
use rocket_db_pools::sqlx::Sqlite;
use rocket_db_pools::sqlx::pool::PoolConnection;
use serde::Deserialize;
use tokio_stream::wrappers::ReadDirStream;
use toml::value::Table;



#[derive(Default, Serialize, Clone, Debug)]
pub struct Photo {
    pub id: u32,
    pub filename: String,
    pub path: PathBuf,
    pub full_path: PathBuf,
    pub uid: UID,
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
    pub lens_model: String,
    pub focal_length: String,
    pub aperture: String,
    pub exposure_time: String,
    pub sensitivity: String,
}

impl Photo {
    /// Try to open the photo file to extract its metadata and store them in the database.
    /// If this has already been done according to the `metadata_parsed` field, this is a no-op.
    pub async fn parse_metadata(&mut self, read_exif: bool) -> Result<(), Error> {
        if self.metadata_parsed {
            // Metadata already parsed, nothing to do
            return Ok(());
        }

        // Load the image
        println!("Parsing metadata for photo {}...", self.full_path.display());
        let img = image::io::Reader::open(&self.full_path)
            .map_err(|e| Error::FileError(e, self.full_path.clone()))?
            .decode()
            .map_err(|e| Error::ImageError(e, self.full_path.clone()))?;

        // Image dimensions
        self.width = img.width();
        self.height = img.height();

        // Compute the photo's average color
        let img_rgb8 = match img {
            image::DynamicImage::ImageRgb8(pixels) => pixels,
            _ => {
                eprintln!("Warning : converting \"{}\" from {:?} to RGB8, this is not efficient", self.full_path.display(), img.color());
                img.into_rgb8()
            }
        };
        let pixels = img_rgb8.as_flat_samples().samples;
        let mut average_r: u64 = 0;
        let mut average_g: u64 = 0;
        let mut average_b: u64 = 0;
        let n_pixels = pixels.len() / 3;
        for i in 0..n_pixels {
            let offset = i * 3;
            average_r += pixels[offset + 0] as u64;
            average_g += pixels[offset + 1] as u64;
            average_b += pixels[offset + 2] as u64;
        }
        average_r /= n_pixels as u64;
        average_g /= n_pixels as u64;
        average_b /= n_pixels as u64;
        let darken_factor = 6;
        self.color = format!("{:02x}{:02x}{:02x}", average_r / darken_factor, average_g / darken_factor, average_b / darken_factor);

        // Parse EXIF metadata
        if read_exif {
            if let Err(Error::EXIFParserError(error, _)) = self.parse_exif() {
                match error {
                    exif::Error::NotFound(_) => (), // Ignore
                    _ => eprintln!("Warning : unable to parse EXIF data from \"{}\" : {}", &self.full_path.display(), error),
                }
            }
        }

        self.metadata_parsed = true;
        Ok(())
    }

    /// Try to parse exif metadata
    pub fn parse_exif(&mut self) -> Result<(), Error> {
        fn remove_quotes(value: String) -> String {
            let mut value = value.clone();
            if value.starts_with("\"") {
                value.remove(0);
            }
            if value.ends_with("\"") {
                value.pop();
            }
            value
        }

        // Read the EXIF data from the file
        let exif_file = std::fs::File::open(&self.full_path)
            .map_err(|e| Error::FileError(e, self.full_path.clone()))?;
        let mut buf_reader = std::io::BufReader::new(&exif_file);
        let exif_reader = exif::Reader::new();
        let exif = exif_reader.read_from_container(&mut buf_reader)
            .map_err(|e| Error::EXIFParserError(e, self.full_path.clone()))?;
        
        // Add every relevant available fields to the photo object
        if let Some(field) = exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY) {
            self.date_taken = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
            self.date_taken = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
            self.camera_model = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
            self.lens_model = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
            self.focal_length = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
            self.aperture = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
            self.exposure_time = remove_quotes(format!("{}", field.display_value()));
        }
        if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
            self.sensitivity = remove_quotes(format!("{}", field.display_value()));
        }

        Ok(())
    }

    /// Create a resized version of this photo in the cache folder
    async fn create_resized(&self, resized_type: ResizedType, image_format: ImageFormat, config: &Config) -> Result<PathBuf, Error> {
        let max_size = resized_type.max_size(config);
        let quality = resized_type.quality(config);
        self.create_resized_from_params(resized_type, image_format, config.CACHE_DIR.clone(), max_size, quality).await
    }

    async fn create_resized_from_params(&self, resized_type: ResizedType, image_format: ImageFormat, cache_dir: String, max_size: usize, quality: usize) -> Result<PathBuf, Error> {
        // Extention according to the configure image format
        let file_extension = match image_format {
            ImageFormat::JPEG => "jpg",
            ImageFormat::WEBP => "webp",
        };

        // Path of the resized version of this photo in the cache folder
        let mut resized_file_path = PathBuf::from(&cache_dir);
        resized_file_path.push(&self.path);
        resized_file_path.push(format!("{}_{}.{}", resized_type.prefix(), &self.uid, file_extension));

        // Check if the file already exists
        if resized_file_path.exists() {
            return Ok(resized_file_path);
        }

        // Extract parameter from the config
        let file_path = &self.full_path;
        println!("Generating resized version ({}, max {}x{}, quality {}%) of \"{}\" in the cache directory... ",
            resized_type.prefix(),
            max_size, max_size, quality,
            file_path.display()
        );
    
        // Make sure the directory exists in the cache folder
        let cache_dir = PathBuf::from(&cache_dir);
        let dir_path = PathBuf::from(resized_file_path.parent().unwrap_or_else(|| &cache_dir));
        if !dir_path.is_dir() {
            create_dir_all(&dir_path).await
                .map_err(|e| {
                    eprintln!("Error : unable to create a directory in the cache folder : {}", dir_path.display());
                    Error::FileError(e, dir_path.clone())
                })?;
        }
    
        // Load the image
        let img = image::io::Reader::open(&file_path)
            .map_err(|e| Error::FileError(e, file_path.clone()))?
            .decode()
            .map_err(|e| {
                eprintln!("Error : unable to decode photo at \"{}\" : {}", file_path.display(), e);
                Error::ImageError(e, file_path.clone())
            })?;
        
        // Resize this image
        let img_resized = img.resize(max_size as u32, max_size as u32, FilterType::CatmullRom);

        // Check quality setting
        let quality = quality.try_into().unwrap_or_else(|_| {
            eprintln!("Warning : invalid 'quality' value for image encoder (must be between 10 and 100), falling back to 80");
            80
        });
    
        // Create an encoder and write the image to the output file.
        // Note that this uses the standard fs API, as opposed to tokio's async API, because the encoder is not compatible
        // with the async equivalent of Writer.
        let file = std::fs::File::create(&resized_file_path)
            .map_err(|e| Error::FileError(e, resized_file_path.clone()))?;
        let mut writer = std::io::BufWriter::new(file);
        match image_format {
            ImageFormat::JPEG => {
                // Create the JPEG encoder with the configured quality
                let mut encoder = JpegEncoder::new_with_quality(writer, quality);
        
                // Encode the image
                encoder.encode_image(&img_resized)
                    .map_err(|e| Error::ImageError(e, file_path.clone()))?;
            }
            ImageFormat::WEBP => {
                // Create the WEPB encoder
                let encoder = webp::Encoder::from_image(&img_resized).or_else(|error| {
                    eprintln!("Error : failed to create a WEBP encoder for \"{}\" : {}", resized_file_path.display(), error);
                    Err(Error::WebpEncoderError(error.to_string(), resized_file_path.clone()))
                })?;

                // Encode the image to a memory buffer
                let data = encoder.encode(quality as f32);

                // Write the buffer to the output file
                writer.write(&*data).or_else(|error| {
                    eprintln!("Error : unable to write to \"{}\" : {}", resized_file_path.display(), error);
                    Err(Error::FileError(error, resized_file_path.clone()))
                })?;
            }
        }
        
        Ok(resized_file_path)
    }
}


pub type GalleryContent = HashMap<String, Vec<Arc<Photo>>>;

/// Thread-safe struct that holds a list of photos and allows them to be accessed efficiently once loaded
/// This is supposed to be managed by Rocket
pub struct Gallery {
    gallery: RwLock<GalleryContent>,
    photos: RwLock<HashMap<UID, Arc<Photo>>>,
}

impl Gallery {

    /// Create a new, empty gallery
    pub fn new() -> Gallery {
        Self {
            gallery: RwLock::new(HashMap::new()),
            photos: RwLock::new(HashMap::new()),
        }
    }


    /// Total number of photos in the gallery
    pub async fn len(&self) -> usize {
        self.photos.read().await.len()
    }


    /// Remove every photo from the gallery
    pub async fn clear(&self) {
        let mut gallery_lock = self.gallery.write().await;
        let mut photos_lock = self.photos.write().await;
        gallery_lock.clear();
        photos_lock.clear();
    }


    /// Reload the gallery. This will `clear()` then `load()`.
    pub async fn reload(&self, config: &Config, db_conn: &mut PoolConnection<Sqlite>) -> Result<(), Error> {
        self.clear().await;
        println!("Reloading photos...");
        let now = Instant::now();
        match self.load(config, db_conn).await {
            Ok(_) => {
                println!("Loaded {} photos successfully in {}ms", self.len().await, now.elapsed().as_millis());
                Ok(())
            }
            Err(error) => {
                eprintln!("Error : unable to load photos : {}", error);
                Err(error)
            }
        }
    }


    /// Insert an empty array at the given path if it doesn't already exist in the gallery
    pub async fn insert_path(&self, path: &PathBuf) {
        // If this path is not already in the hashmap, insert an empty vec at this key
        // This is used to make sure that 
        let mut gallery_lock = self.gallery.write().await;
        gallery_lock.entry(path.to_string_lossy().into_owned())
            .or_insert_with(|| Vec::new());
    }


    /// Check if the given path exists in the gallery
    pub async fn path_exists(&self, path: &PathBuf) -> bool {
        self.gallery.read().await.contains_key(&path.to_string_lossy().to_string())
    }


    /// Insert the given photo in the gallery at the given path. If this photo is already registered in the gallery for a given path,
    /// it will not get duplicated, instead the smart pointer that will be inserted will point to the same Photo internally
    async fn insert_photo(&self, path: &PathBuf, photo: &Photo) {
        // If this photo has already been inserted somewhere in the gallery, it also has an Arc pointer stored in the `photos`
        // hashmap that we can retreive efficiently; otherwise, create a new Arc and insert it into the hashmap
        let mut photos_lock = self.photos.write().await;
        let arc = photos_lock.entry(photo.uid.clone()).or_insert_with(|| Arc::new(photo.clone()));

        // Create a clone of this Arc pointer to share the underlying Photo object
        let photo_pointer = Arc::clone(arc);

        // If this path has already been inserted in the gallery, retreive its Vec of Arc pointers to Photo objects; otherwise,
        // create an empty Vec
        let mut gallery_lock = self.gallery.write().await;
        let vec = gallery_lock.entry(path.to_string_lossy().into_owned()).or_insert_with(|| Vec::new());

        // Add this Arc pointer to the list of photos for this path in the gallery if it hasn't already been inserted
        if !vec.iter().any(|p| p.uid == photo.uid) {
            vec.push(photo_pointer);
        }
    }


    /// Acquire a read lock on the gallery if the path exists, or return None otherwise. Use `as_slice()` on the return type to read the photos.
    /// If the metadata of some of the requested photos hasn't been parsed, this will acquire a temporary write lock on the gallery
    /// to perform this operation, which may take some time.
    pub async fn read<'a>(&'a self, path: &'a PathBuf, start: Option<usize>, count: Option<usize>, uid: Option<UID>) -> Option<GalleryReadLock<'a>> {
        let path = path.to_string_lossy().to_string();
        
        let gallery_read_lock = self.gallery.read().await;
        if gallery_read_lock.contains_key(&path) {
            let photos = gallery_read_lock.get(&path).unwrap();
            let n_photos = photos.len();
    
            // Compute pagination
            let mut start = start.unwrap_or(0);
            let mut count = count.unwrap_or(n_photos);
            if let Some(uid) = uid { // Only return a single UID if requested
                if let Some(idx) = photos.iter().position(|p| p.uid == uid) {
                    start = idx;
                    count = 1;
                }
                // If the requested UID hasn't been found, ignore this constraint and return a list based on `start` and `count` if provided,
                // or default values otherwise.
            }
            if start >= n_photos {
                start = 0;
            }
            if start + count > n_photos {
                count = n_photos - start;
            }
            if count > 100 { // Limit the maximum number of results to 100
                count = 100;
            }

            // Return a read lock on the gallery that can be used to access the photos
            Some(GalleryReadLock::new(gallery_read_lock, path, start, count, n_photos))

        } else {
            // This path is not found in the gallery
            None
        }
    }


    /// Return a copy of a single photo from the cache, based on its UID
    pub async fn get_from_uid(&self, uid: &UID) -> Option<Photo> {
        Some(Photo::clone(self.photos.read().await.get(uid)?))
    }


    /// Return a copy of a single photo from the cache based on its UID, with the path to its resized version,
    /// after generating it if necessary
    pub async fn get_resized_from_uid(&self, uid: &UID, resized_type: ResizedType, config: &Config) -> Result<Option<(Photo, PathBuf)>, Error> {
        match self.get_from_uid(uid).await {
            Some(photo) => {
                // Generate this file if it doesn't exist
                let resized_file_path = photo.create_resized(resized_type, config.RESIZED_IMAGE_FORMAT, config).await?;
    
                Ok(Some((photo, resized_file_path)))
            }
            None => Ok(None),
        }
    }


    // Private function used to load photos recursively
    fn load_rec<'a>(
            &'a self,
            full_path: &'a PathBuf,
            rel_path: &'a PathBuf,
            db_conn: &'a mut PoolConnection<Sqlite>,
            main_config: &'a Config,
            configs_stack: &'a mut Vec<(PathBuf, Table)>,
            default_config: &'a Config,
            photos_to_insert: &'a mut Option<&mut Vec<Photo>>,
            photos_to_remove: &'a mut Option<&mut Vec<Photo>>,
            paths_found: &'a mut Option<&mut Vec<PathBuf>>
        ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>
    {
        Box::pin(async move {
            // Append this path to the list of paths found
            if let Some(paths_found) = paths_found {
                paths_found.push(rel_path.clone());
            }
            self.insert_path(&rel_path).await;

            // Try to find a config file in this directory, append it to a copy of the current one (so it won't propagate to
            // sibling directories), and put it on the stack
            let mut cfg = configs_stack.last().unwrap().1.clone();
            cfg.remove("HIDDEN"); // This setting doesn't propagate from the parent
            Config::update_with_subdir(&full_path, &mut cfg);
            configs_stack.push((rel_path.clone(), cfg));
            let subdir_config = &configs_stack.last().unwrap().1;

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
                            if file_type.is_file() && !filename_lowercase.starts_with(".") && (filename_lowercase.ends_with(".jpg") || filename_lowercase.ends_with(".jpeg")) {
                                filenames_in_fs.push(filename);
                            }
                        }
                    }
                }
                filenames_in_fs.sort_by(|a, b| natord::compare_ignore_case(a, b));
            }

            // Get the list of photos saved in the database for this path exactly
            let sort_columns = String::from(subdir_config.get("SORT_ORDER").and_then(|v| v.as_str()).unwrap_or(&default_config.SORT_ORDER))
                .split(",").map(|s| String::from(s.trim())).collect::<Vec<String>>();
            let photos_in_db = db::get_photos_in_path(db_conn, &rel_path, &sort_columns, main_config).await?;

            // Find photos in the filesystem that are not in the database yet
            if let Some(ref mut photos_to_insert) = photos_to_insert {
                let filenames_in_db = photos_in_db.iter().map(|photo| &photo.filename).collect::<Vec<&String>>();
                for filename in &filenames_in_fs {
                    if !filenames_in_db.contains(&filename) {
                        let mut full_path = PathBuf::from(&main_config.PHOTOS_DIR);
                        full_path.push(&rel_path);
                        full_path.push(&filename);
                        photos_to_insert.push(Photo {
                            path: rel_path.clone(),
                            filename: filename.clone(),
                            full_path,
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
            let all_uids_in_path = photos_in_db.iter().map(|photo| &photo.uid).collect::<Vec<&UID>>();
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
                                        let file_uid = UID::try_from(&filename.chars().skip(prefix.len()).take(filename.len() - prefix.len() - suffix.len()).collect::<String>());
                                        if let Ok(file_uid) = file_uid {
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

            // Add these photos to the gallery
            // TODO : handle PASSWORD setting
            if !photos_in_db.is_empty() {
                // Add them to this path
                for photo in &photos_in_db {
                    self.insert_photo(rel_path, photo).await;
                }

                // Add them recursively to the parent paths as long as SHOW_PHOTOS_FROM_SUBDIRS is set and HIDDEN isn't
                for (path, entry_config) in configs_stack[1..configs_stack.len()-1].iter().rev() {
                    let show_photos_from_subdir = entry_config.get("SHOW_PHOTOS_FROM_SUBDIRS").and_then(|v| v.as_bool()).unwrap_or(default_config.SHOW_PHOTOS_FROM_SUBDIRS);
                    let hidden = entry_config.get("HIDDEN").and_then(|v| v.as_bool()).unwrap_or(default_config.HIDDEN);
                    if !show_photos_from_subdir || hidden {
                        break;
                    }
                    for photo in &photos_in_db {
                        self.insert_photo(path, photo).await;
                    }
                }
            }

            // If the INDEX_SUBDIRS config is enabled, recursively load photos from subdirectories
            if main_config.INDEX_SUBDIRS {
                // Find the list of valid subdirectories in the path, in the filesystem
                let subdirs = list_subdirs(&rel_path, &main_config.PHOTOS_DIR, true, true).await?;

                // Clean obsolete subdirectories (that do not correspond to a subdirectory in the photos folder) from the cache folder
                let subdirs_in_cache = list_subdirs(&rel_path, &main_config.CACHE_DIR, true, false).await?;
                if !subdirs_in_cache.is_empty() {
                    let mut subdirs_in_cache_to_remove: Vec<PathBuf> = Vec::new();
                    for subdir in subdirs_in_cache {
                        if !subdirs.contains(&subdir) {
                            let mut subdir_path = PathBuf::from(&main_config.CACHE_DIR);
                            subdir_path.push(&rel_path);
                            subdir_path.push(&subdir);
                            subdirs_in_cache_to_remove.push(subdir_path);
                        }
                    }
                    if !subdirs_in_cache_to_remove.is_empty() {
                        println!("Removing {} obsolete directory(ies) in cache : {}",
                                subdirs_in_cache_to_remove.len(),
                                subdirs_in_cache_to_remove.iter()
                                    .map(|subdir| format!("\"{}\"", subdir.to_string_lossy()))
                                    .collect::<Vec<String>>().join(", ")
                        );
                        for subdir in subdirs_in_cache_to_remove {
                            let result = fs::remove_dir_all(&subdir).await;
                            if let Err(error) = result {
                                eprintln!("Warning : unable to remove directory in cache \"{}\" : {}", subdir.display(), error);
                            }
                        }
                    }
                }

                // Load subdirectories recursively
                if !subdirs.is_empty() {
                    for subdir in subdirs {
                        let mut subdir_rel_path = rel_path.clone();
                        subdir_rel_path.push(&subdir);
                        let mut subdir_full_path = full_path.clone();
                        subdir_full_path.push(&subdir);
                        self.load_rec(&subdir_full_path, &subdir_rel_path, db_conn, main_config, configs_stack, &default_config, photos_to_insert, photos_to_remove, paths_found).await?;
                    }
                }
            }

            // Remove this entry in the stack of configs
            configs_stack.pop();

            Ok(())
        })
    }


    /// Load all available photos in the photos folder, add them to the given gallery, and sync them with the database
    async fn load(&self, config: &Config, db_conn: &mut PoolConnection<Sqlite>) -> Result<(), Error> {
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
        
        // Keep these paths on hand
        let full_path = PathBuf::from(&config.PHOTOS_DIR);
        let rel_path = PathBuf::new();

        // Create a default config to use as a reference for default value of settings
        let default_config = Config::default();

        // Initialize the stack of configs with the main config
        let subdir_config = Config::read_as_table().unwrap_or_else(|_| Table::new());
        let mut configs_stack: Vec<(PathBuf, Table)> = Vec::new();
        configs_stack.push((PathBuf::from("[main]"), subdir_config));

        // Get all existing UIDs from the database
        let mut existing_uids = db::get_existing_uids(db_conn).await?;

        // Load the photos recursively, comparing the filesystem and the database.
        // If some differences are found, they will be returned in these Vec's.
        let mut photos_to_insert: Vec<Photo> = Vec::new();
        let mut photos_to_remove: Vec<Photo> = Vec::new();
        let mut paths_found: Vec<PathBuf> = Vec::new();
        self.load_rec(
            &full_path, &rel_path, db_conn,
            &config, &mut configs_stack, &default_config,
            &mut Some(&mut photos_to_insert), &mut Some(&mut photos_to_remove), &mut Some(&mut paths_found)
        ).await?;

        // Get the list of all known subdirs of the current path in the database, check if some have been removed,
        // and if so add their photos to the 'to_remove' list
        if config.INDEX_SUBDIRS {
            let mut deleted_paths:Vec<PathBuf> = Vec::new();
            let known_paths_in_db = db::get_all_paths(db_conn).await?;
            for known_path in known_paths_in_db {
                if !paths_found.contains(&known_path) {
                    deleted_paths.push(known_path);
                }
            }
            if !deleted_paths.is_empty() {
                let photos_in_deleted_paths = db::get_photos_in_paths(db_conn, &deleted_paths, &config).await?;
                for photo in photos_in_deleted_paths {
                    photos_to_remove.push(photo);
                }
            }
        }

        // Calculate the MD5 hashes of the new files in parallel background tasks
        if !photos_to_insert.is_empty() {
            let now = Instant::now();
            let n = photos_to_insert.len();
            let mut last_percent: usize = 0;
            let mut tasks = JoinSet::new();
            let mut results: Vec<(usize, String)> = Vec::new();
            let mut offset = 0;
            while results.len() < n {
                // Create up to LOADING_WORKERS background tasks
                let batch_size = min(n - offset, config.LOADING_WORKERS);
                for idx in offset..offset+batch_size {
                    let photo = &photos_to_insert[idx];
                    let full_path = photo.full_path.clone();
                    tasks.spawn(async move {
                        let full_path = full_path;
                        (idx, calculate_file_md5(&full_path).await)
                    });
                }
                offset += batch_size;

                // Wait for these tasks to complete and add their results to the list
                while let Some(result) = tasks.join_next().await {
                    match result {
                        Ok((i, Ok(md5))) => {
                            results.push((i, md5));
                            let percent: usize = (i + 1) * 100 / n;
                            if percent > last_percent {
                                print!("\rCalculating MD5 hashes of {} new files... {}%", n, percent);
                                std::io::stdout().flush().ok();
                                last_percent = percent;
                            }
                        }
                        Ok((i, Err(error))) => eprintln!("Error : unable to compute MD5 for \"{}\" : {}", photos_to_insert.get(i).unwrap().filename, error),
                        Err(error) => eprintln!("Error : unable to join background task while computing MD5 : {}", error),
                    }
                }
            }
            for (i, md5) in results {
                photos_to_insert.get_mut(i).unwrap().md5 = md5;
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

        // If there were some modifications to the photos we will need to reload the photos
        let need_to_reload = !photos_to_insert.is_empty() || !photos_to_remove.is_empty() || !photos_to_move.is_empty();

        // Apply detected modifications (photos added, moved, or deleted) to the database
        if !photos_to_insert.is_empty() {
            // Generate a new UID for each photo
            for photo in photos_to_insert.iter_mut() {
                photo.uid = UID::new(&existing_uids);
                existing_uids.push(photo.uid.clone());
            }

            // Since we will handle the list from last to first (because of pop()) we need to reverse
            // the array first to keep the original order
            photos_to_insert.reverse();

            // Parse the photos' metadata and, if set in the config, generate their thumbnails
            let pre_generate_thumbnails = config.PRE_GENERATE_THUMBNAILS;
            let thumbnail_max_size = ResizedType::THUMBNAIL.max_size(config);
            let thumbnail_quality = ResizedType::THUMBNAIL.quality(config);
            let large_size_max_size = ResizedType::LARGE.max_size(config);
            let large_size_quality = ResizedType::LARGE.quality(config);
            let mut photos_to_insert_in_db: Vec<Photo> = Vec::new();
            while !photos_to_insert.is_empty() {
                // Spawn background tasks to parallelize computation, up to the LOADING_WORKERS setting in the config
                let mut counter = 0;
                let mut tasks = Vec::with_capacity(config.LOADING_WORKERS);
                while let Some(mut photo) = photos_to_insert.pop() {
                    // Background task which takes ownership of the photo object
                    let cache_dir = config.CACHE_DIR.clone();
                    let image_format = config.RESIZED_IMAGE_FORMAT;
                    tasks.push(tokio::spawn( async move {
                        // Parse the metadata
                        photo.parse_metadata(true).await
                            .or_else(|e| {
                                eprintln!("Error : unable to open \"{}/{}\" : {}", photo.path.to_string_lossy(), photo.filename, e);
                                Err(e)
                            }).ok(); // Ignore error after printing it
                        
                        // Generate thumbnails
                        if pre_generate_thumbnails {
                            photo.create_resized_from_params(ResizedType::THUMBNAIL, image_format, cache_dir.clone(), thumbnail_max_size, thumbnail_quality).await.ok();
                            photo.create_resized_from_params(ResizedType::LARGE, image_format, cache_dir.clone(), large_size_max_size, large_size_quality).await.ok();
                        }

                        // Return ownership of the photo to the main thread
                        photo
                    }));

                    counter += 1;
                    if counter >= config.LOADING_WORKERS {
                        // We have enough workers for now
                        break;
                    }
                }

                // Wait for the background tasks to finish
                for task in tasks.into_iter() {
                    if let Ok(photo) = task.await {
                        photos_to_insert_in_db.push(photo);
                    }
                }
            }

            // Log the list of photos to insert
            println!("Inserting {} photo(s) into the database : {}",
                    photos_to_insert_in_db.len(),
                    photos_to_insert_in_db.iter()
                        .map(|photo| format!("\"{}/{}\"", photo.path.to_string_lossy(), photo.filename))
                        .collect::<Vec<String>>().join(", ")
            );

            // Insert them into the database
            db::insert_photos(db_conn, &photos_to_insert_in_db).await?;
        }
        if !photos_to_remove.is_empty() {
            // Log the list of photos to remove
            println!("Removing {} photo(s) from the database : {}",
                    photos_to_remove.len(),
                    photos_to_remove.iter()
                        .map(|photo| format!("\"{}/{}\"", photo.path.to_string_lossy(), photo.filename))
                        .collect::<Vec<String>>().join(", ")
            );

            // Remove them from the database
            db::remove_photos(db_conn, &photos_to_remove).await?;
        }
        if !photos_to_move.is_empty() {
            // Log the list of photos to rename/move
            println!("Renaming/moving {} photo(s) in the database : {}",
                    photos_to_move.len(),
                    photos_to_move.iter()
                        .map(|pair| format!("\"{}/{}\" -> \"{}/{}\"", pair.0.path.to_string_lossy(), pair.0.filename, pair.1.path.to_string_lossy(), pair.1.filename))
                        .collect::<Vec<String>>().join(", ")
            );

            // Update the database
            db::move_photos(db_conn, &photos_to_move).await?;
        }

        // Reload if required
        if need_to_reload {
            self.clear().await;
            self.load_rec(
                &full_path, &rel_path, db_conn,
                &config, &mut configs_stack, &default_config,
                &mut None,&mut None, &mut None
            ).await?;
        }

        // Good job.
        Ok(())
    }

}


/// RAII read lock on the gallery, obtained with `Gallery::read()`. Every access on the gellery's photos
/// must pass through an instance of this lock. Concurrent reads are allowed, which means access will
/// be immediately granted as long as the gallery is not reloading.
pub struct GalleryReadLock<'a>{
    guard: RwLockReadGuard<'a, GalleryContent>,
    path: String,
    pub start: usize,
    pub count: usize,
    pub total: usize,
}

impl<'a> GalleryReadLock<'a> {
    fn new(guard: RwLockReadGuard<'a, GalleryContent>, path: String, start: usize, count: usize, total: usize) -> Self {
        Self { guard, path, start, count, total }
    }

    /// Get a slice on the photos inside this lock following the parameters given during construction of the lock.
    /// At most 100 photos are returned.
    pub fn as_slice(&self) -> &[Arc<Photo>] {
        // We can safely unwrap() here because the presence of the key has already been checked in Gallery::read()
        // and since we have had a lock since then the hashmap cannot have been modified. Same for the slice
        // parameters of the Vec.
        &self.guard.get(&self.path).unwrap().as_slice()[self.start..self.start+self.count]
    }
}



/// Fairing callback used to load/sync the photos with the database at startup
pub async fn init(rocket: Rocket<rocket::Build>) -> fairing::Result {
    // Make sure the database has been initialized (fairings have been attached in the correct order)
    match db::DB::fetch(&rocket) {
        Some(db) => match db.0.acquire().await {
            Ok(mut db_conn) => {
                let config = rocket.state::<Config>().expect("Error : unable to obtain the config");
                let gallery = rocket.state::<Gallery>().expect("Error : unable to obtain the gallery");

                println!("Loading photos...");
                let now = Instant::now();
                match gallery.load(&config, &mut db_conn).await {
                    Ok(_) => {
                        println!("Loaded {} photos successfully in {}ms", gallery.len().await, now.elapsed().as_millis());
                        Ok(rocket)
                    }
                    Err(error) => {
                        eprintln!("Error : unable to load photos : {}", error);
                        Err(rocket)
                    }
                }}

            Err(error) => {
                eprintln!("Error : unable to acquire a connection to the database : {}", error);
                Err(rocket)
            }
        }
        None => {
            eprintln!("Error : unable to obtain a handle to the database");
            Err(rocket)
        }
    }
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

        // The directory doesn't exist, try to create it and return the result
        // of that operation directly
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!("Creating empty directory \"{}\"", path.display());
            fs::create_dir_all(path).await.map_err(|e| Error::FileError(e, path.clone()))
        }

        // Return any other error directly
        Err(error) => Err(Error::FileError(error, path.clone())),
    }
}


/// Return the list of valid subdirectories in the given path in the photos folder
pub async fn list_subdirs(path: &PathBuf, folder: &str, include_hidden: bool, error_if_missing: bool) -> Result<Vec<String>, Error> {
    let mut subdirs: Vec<String> = Vec::new();
    let mut full_path = PathBuf::from(folder);
    full_path.push(path);

    // Try to open a Stream to the content of this path
    let dir = match fs::read_dir(&full_path).await {
        Ok(dir) => dir,

        // This directory doesn't exist, but error_is_missing is set to false, just return as if the directory is empty
        Err(error) if error.kind() == io::ErrorKind::NotFound && !error_if_missing => return Ok(Vec::new()),

        // Return any other error directly
        Err(error) => return Err(Error::FileError(error, full_path.clone())),
    };
    let mut dir_stream = ReadDirStream::new(dir);

    // Iterate over the entries found in this path
    while let Some(entry) = dir_stream.next().await {
        let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
        if let Ok(file_type) = entry.file_type().await {
            if let Ok(dir_name) = entry.file_name().into_string() {
                if file_type.is_dir() && !dir_name.starts_with(".") {
                    // This is a valid subdirectory, check if it contains a config that would forbid including it in the results
                    let mut subdir_path = full_path.clone();
                    subdir_path.push(&dir_name);
                    let mut subdir_config_table: Table = Table::new();
                    Config::update_with_subdir(&subdir_path, &mut subdir_config_table);
                    let subdir_config = Config::from_table(subdir_config_table).unwrap_or_default();
                    if subdir_config.INDEX && (include_hidden || !subdir_config.HIDDEN) {
                        subdirs.push(dir_name);
                    }
                }
            }
        }
    }

    subdirs.sort_by(|a, b| natord::compare_ignore_case(a, b));
    Ok(subdirs)
}


/// Calculate and return the MD5 hash of the file located at the given path
async fn calculate_file_md5(path: &PathBuf) -> Result<String, Error> {
    let file_content = fs::read(path).await.map_err(|e| Error::FileError(e, path.clone()))?;
    let hash = Md5::digest(file_content);
    Ok(format!("{:x}", hash))
}


/// Kinds of resized versions of photos generated in the cache folder
pub enum ResizedType {
    /// Thumbnail-sized photos displayed in the grid
    THUMBNAIL,

    /// Large-size photos displayed in loupe mode
    LARGE,
}

impl ResizedType {
    pub fn prefix(&self) -> &'static str {
        match self {
            ResizedType::THUMBNAIL => "thumbnail",
            ResizedType::LARGE => "large",
        }
    }

    pub fn max_size(&self, config: &Config) -> usize {
        match self {
            ResizedType::THUMBNAIL => config.THUMBNAIL_MAX_SIZE,
            ResizedType::LARGE => config.LARGE_VIEW_MAX_SIZE,
        }
    }

    pub fn quality(&self, config: &Config) -> usize {
        match self {
            ResizedType::THUMBNAIL => config.THUMBNAIL_QUALITY,
            ResizedType::LARGE => config.LARGE_VIEW_QUALITY,
        }
    }
}

/// Available image formats for cache files
#[derive(Debug, Serialize, Deserialize, Copy, Clone, Default)]
pub enum ImageFormat {
    JPEG,
    #[default]
    WEBP,
}