use crate::config::Config;
use crate::password::{self, OptionalPassword, PasswordError, Passwords};
use crate::uid::UID;
use crate::{db, Error};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use md5::{Digest, Md5};
use rocket::futures::StreamExt;
use rocket::http::{Cookie, CookieJar};
use rocket::serde::Serialize;
use rocket::tokio::fs;
use rocket::tokio::fs::create_dir_all;
use rocket::tokio::sync::{RwLock, RwLockReadGuard};
use rocket::tokio::task::JoinSet;
use rocket::tokio::time::Instant;
use rocket::{fairing, tokio, Rocket};
use rocket_db_pools::sqlx::pool::PoolConnection;
use rocket_db_pools::sqlx::Sqlite;
use rocket_db_pools::Database;
use serde::Deserialize;
use std::cmp::min;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::ReadDirStream;
use toml::value::Table;

/// Main struct representing a photo and its metadata
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
    /// Try to open the photo file to extract its metadata.
    /// If this has already been done according to the `metadata_parsed` field, this is a no-op.
    #[allow(clippy::identity_op)]
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
                eprintln!(
                    "Warning : converting \"{}\" from {:?} to RGB8, this is not efficient",
                    self.full_path.display(),
                    img.color()
                );
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
        self.color = format!(
            "{:02x}{:02x}{:02x}",
            average_r / darken_factor,
            average_g / darken_factor,
            average_b / darken_factor
        );

        // Parse EXIF metadata
        if read_exif {
            if let Err(Error::EXIFParserError(error, _)) = self.parse_exif() {
                match error {
                    exif::Error::NotFound(_) => (), // Ignore
                    _ => eprintln!(
                        "Warning : unable to parse EXIF data from \"{}\" : {}",
                        &self.full_path.display(),
                        error
                    ),
                }
            }
        }

        self.metadata_parsed = true;
        Ok(())
    }

    /// Try to parse exif metadata
    pub fn parse_exif(&mut self) -> Result<(), Error> {
        fn remove_quotes(value: String) -> String {
            #[allow(clippy::redundant_clone)]
            let mut value = value.clone();
            if value.starts_with('"') {
                value.remove(0);
            }
            if value.ends_with('"') {
                value.pop();
            }
            value
        }

        // Read the EXIF data from the file
        let exif_file = std::fs::File::open(&self.full_path)
            .map_err(|e| Error::FileError(e, self.full_path.clone()))?;
        let mut buf_reader = std::io::BufReader::new(&exif_file);
        let exif_reader = exif::Reader::new();
        let exif = exif_reader
            .read_from_container(&mut buf_reader)
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
    async fn create_resized(
        &self,
        resized_type: ResizedType,
        image_format: ImageFormat,
        config: &Config,
    ) -> Result<PathBuf, Error> {
        let max_size = resized_type.max_size(config);
        let quality = resized_type.quality(config);
        self.create_resized_from_params(
            resized_type,
            image_format,
            config.CACHE_DIR.clone(),
            max_size,
            quality,
        )
        .await
    }

    async fn create_resized_from_params(
        &self,
        resized_type: ResizedType,
        image_format: ImageFormat,
        cache_dir: String,
        max_size: usize,
        quality: usize,
    ) -> Result<PathBuf, Error> {
        // Extention according to the configured image format
        let file_extension = match image_format {
            ImageFormat::JPEG => "jpg",
            ImageFormat::WEBP => "webp",
        };

        // Path of the resized version of this photo in the cache folder
        let mut resized_file_path = PathBuf::from(&cache_dir);
        resized_file_path.push(&self.path);
        resized_file_path.push(format!(
            "{}_{}.{}",
            resized_type.prefix(),
            &self.uid,
            file_extension
        ));

        // Check if the file already exists
        if resized_file_path.exists() {
            return Ok(resized_file_path);
        }

        // Extract parameters from the config
        let file_path = &self.full_path;
        println!("Generating resized version ({}, max {}x{}, quality {}%) of \"{}\" in the cache directory... ",
            resized_type.prefix(),
            max_size, max_size, quality,
            file_path.display()
        );

        // Make sure the directory exists in the cache folder
        let cache_dir = PathBuf::from(&cache_dir);
        let dir_path = PathBuf::from(resized_file_path.parent().unwrap_or(&cache_dir));
        if !dir_path.is_dir() {
            create_dir_all(&dir_path).await.map_err(|e| {
                eprintln!(
                    "Error : unable to create a directory in the cache folder : {}",
                    dir_path.display()
                );
                Error::FileError(e, dir_path.clone())
            })?;
        }

        // Load the image
        let img = image::io::Reader::open(file_path)
            .map_err(|e| Error::FileError(e, file_path.clone()))?
            .decode()
            .map_err(|e| {
                eprintln!(
                    "Error : unable to decode photo at \"{}\" : {}",
                    file_path.display(),
                    e
                );
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
                encoder
                    .encode_image(&img_resized)
                    .map_err(|e| Error::ImageError(e, file_path.clone()))?;
            }
            ImageFormat::WEBP => {
                // Create the WEPB encoder
                let encoder = webp::Encoder::from_image(&img_resized).map_err(|error| {
                    eprintln!(
                        "Error : failed to create a WEBP encoder for \"{}\" : {}",
                        resized_file_path.display(),
                        error
                    );
                    Error::WebpEncoderError(error.to_string(), resized_file_path.clone())
                })?;

                // Encode the image to a memory buffer
                let data = encoder.encode(quality as f32);

                // Write the buffer to the output file
                writer.write(&data).map_err(|error| {
                    eprintln!(
                        "Error : unable to write to \"{}\" : {}",
                        resized_file_path.display(),
                        error
                    );
                    Error::FileError(error, resized_file_path.clone())
                })?;
            }
        }

        Ok(resized_file_path)
    }
}

/// A photo stored in cache with extra metadata
#[derive(Debug)]
pub struct CachedPhoto {
    photo: Arc<Photo>,
    passwords: Vec<(String, String)>,
}

impl CachedPhoto {
    fn new(photo: Photo, passwords: Vec<(String, String)>) -> Self {
        Self {
            photo: Arc::new(photo),
            passwords,
        }
    }

    fn clone_from(photo: &CachedPhoto, passwords: Vec<(String, String)>) -> Self {
        Self {
            photo: Arc::clone(&photo.photo),
            passwords,
        }
    }
}

impl Deref for CachedPhoto {
    type Target = Photo;

    fn deref(&self) -> &Self::Target {
        self.photo.as_ref()
    }
}

pub type GalleryContent = HashMap<String, Vec<CachedPhoto>>;

/// Thread-safe struct that holds a list of photos and allows them to be accessed efficiently once loaded
/// This is supposed to be managed by Rocket
pub struct Gallery {
    gallery: RwLock<GalleryContent>,
    photos: RwLock<HashMap<UID, CachedPhoto>>,
    subdirs: RwLock<HashMap<String, Subdirs>>,
    passwords: RwLock<HashMap<String, String>>,
    counts: RwLock<HashMap<String, HashMap<Vec<String>, usize>>>,
    subdirs_configs: RwLock<HashMap<String, Config>>,
}

impl Gallery {
    /// Create a new, empty gallery
    pub fn new() -> Gallery {
        Self {
            gallery: RwLock::new(HashMap::new()),
            photos: RwLock::new(HashMap::new()),
            subdirs: RwLock::new(HashMap::new()),
            passwords: RwLock::new(HashMap::new()),
            counts: RwLock::new(HashMap::new()),
            subdirs_configs: RwLock::new(HashMap::new()),
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
        let mut passwords_lock = self.passwords.write().await;
        gallery_lock.clear();
        photos_lock.clear();
        passwords_lock.clear();
    }

    /// Reload the gallery. This will `clear()` then `load()`.
    pub async fn reload(
        &self,
        config: &Config,
        db_conn: &mut PoolConnection<Sqlite>,
    ) -> Result<(), Error> {
        self.clear().await;
        println!("Reloading photos...");
        let now = Instant::now();
        match self.load(config, db_conn).await {
            Ok(_) => {
                println!(
                    "Loaded {} photos successfully in {}ms",
                    self.len().await,
                    now.elapsed().as_millis()
                );
                Ok(())
            }
            Err(error) => {
                eprintln!("Error : unable to load photos : {}", error);
                Err(error)
            }
        }
    }

    /// Insert an empty array at the given path if it doesn't already exist in the gallery
    pub async fn insert_path(&self, path: &Path) {
        // If this path is not already in the hashmap, insert an empty vec at this key
        // This is used to make sure that
        let mut gallery_lock = self.gallery.write().await;
        gallery_lock
            .entry(path.to_string_lossy().into_owned())
            .or_insert_with(Vec::new);
    }

    /// Check if the given path exists in the gallery
    pub async fn path_exists(&self, path: &Path) -> bool {
        self.gallery
            .read()
            .await
            .contains_key(&path.to_string_lossy().to_string())
    }

    /// Insert the given photo in the gallery at the given path. If this photo is already registered in the gallery for a given path,
    /// it will not get duplicated, instead the smart pointer that will be inserted will point to the same Photo internally
    async fn insert_photo(&self, path: &Path, photo: &Photo, passwords: Vec<(String, String)>) {
        // If this photo has already been inserted somewhere in the gallery, it also has an Arc pointer stored in the `photos`
        // hashmap that we can retreive efficiently; otherwise, create a new Arc and insert it into the hashmap
        let mut photos_lock = self.photos.write().await;
        let cached_photo = photos_lock
            .entry(photo.uid.clone())
            .or_insert_with(|| CachedPhoto::new(photo.clone(), passwords.clone()));

        // Create a clone of this Arc pointer to share the underlying Photo object
        let photo_pointer = CachedPhoto::clone_from(cached_photo, passwords);

        // If this path has already been inserted in the gallery, retrieve its Vec of Arc pointers to Photo objects; otherwise,
        // create an empty Vec
        let mut gallery_lock = self.gallery.write().await;
        let vec = gallery_lock
            .entry(path.to_string_lossy().into_owned())
            .or_insert_with(Vec::new);

        // Add this Arc pointer to the list of photos for this path in the gallery if it hasn't already been inserted
        if !vec.iter().any(|cp| cp.photo.uid == photo.uid) {
            vec.push(photo_pointer);
        }
    }

    /// Acquire a read lock on the gallery if the path exists, or return None otherwise
    pub async fn read<'a>(
        &'a self,
        path: &'a Path,
        start: Option<usize>,
        count: Option<usize>,
        uid: Option<UID>,
        provided_passwords: Passwords,
    ) -> Option<GalleryReadLock<'a>> {
        let path = path.to_string_lossy().to_string();

        let gallery_read_lock = self.gallery.read().await;
        if gallery_read_lock.contains_key(&path) {
            // All photos available in this path (including some that may not be displayed to the user, if they
            // require a password)
            let photos = gallery_read_lock.get(&path).unwrap();

            // Compute the number of available photos for the set of passwords provided
            let mut n_photos = 0;
            let counts_lock = self.counts.read().await;
            let counts_in_path = counts_lock.get(&path).unwrap();
            'loop_aggregates: for (required_passwords, count) in counts_in_path {
                // Check that all required passwords for this count are provided
                for password in required_passwords {
                    if !provided_passwords.contains_key(password) {
                        continue 'loop_aggregates;
                    }
                }

                // Add to the total
                n_photos += count;
            }

            // Compute pagination
            let mut start = start.unwrap_or(0);
            let mut max_count = count.unwrap_or(n_photos);
            if let Some(uid) = uid {
                // Only return a single UID if requested
                if let Some(idx) = photos.iter().position(|cp| cp.photo.uid == uid) {
                    start = idx;
                    max_count = 1;
                }
                // If the requested UID hasn't been found, ignore this constraint and return a list based on `start` and `count` if provided,
                // or default values otherwise.
            }
            if start >= n_photos {
                start = 0;
            }
            if start + max_count > n_photos {
                max_count = n_photos - start;
            }
            if max_count > 100 {
                // Limit the maximum number of results to 100
                max_count = 100;
            }

            // Return a read lock on the gallery that provides an iterator to access the photos
            Some(GalleryReadLock::new(
                gallery_read_lock,
                path,
                start,
                max_count,
                n_photos,
                provided_passwords,
            ))
        } else {
            // This path is not found in the gallery
            None
        }
    }

    /// Return a copy of a single photo from the cache, based on its UID
    pub async fn get_from_uid(&self, uid: &UID) -> Option<Photo> {
        Some(Photo::clone(&self.photos.read().await.get(uid)?.photo))
    }

    /// Check if the current session state stored in private cookies as well as any password provided
    /// with the current request grants access to the given path. If a new valid password has been
    /// provided in the current request, add it to the session cookie.
    /// Returns Ok with the list of all passwords in the user's session either if no password is required
    /// or if access is granted, or Err with the kind of error if the password is invalid.
    pub async fn check_password(
        &self,
        path: &Path,
        cookies: &CookieJar<'_>,
        request_password: &OptionalPassword,
    ) -> Result<Passwords, PasswordError> {
        // Get a lock on the gallery's password list
        let gallery_passwords = self.passwords.read().await;

        // Compute the list of user-provided passwords
        let mut user_passwords = Passwords::new();
        for (gallery_path, required_password) in gallery_passwords.deref() {
            if let Some(provided_password) = cookies
                .get_private(&password::cookie_name(gallery_path))
                .map(|c| c.value().to_string())
            {
                if &provided_password == required_password {
                    user_passwords.insert(gallery_path.clone(), provided_password);
                }
            }
        }

        // Compute the list of required passwords to access this path
        let mut required_passwords = Passwords::new();
        let mut path_current = path.to_path_buf();
        let empty_path = PathBuf::new();
        while path_current != empty_path {
            let path_current_str = path_current.to_string_lossy().to_string();
            if let Some(password) = gallery_passwords.get(&path_current_str) {
                required_passwords.insert(path_current_str, password.clone());
            }
            path_current.pop();
        }
        if let Some(password) = gallery_passwords.get("") {
            required_passwords.insert("".to_string(), password.clone());
        }
        let mut paths_requiring_passwords = required_passwords.keys().collect::<Vec<&String>>();
        paths_requiring_passwords.sort();

        if required_passwords.is_empty() {
            // No password required
            Ok(user_passwords)
        } else {
            // At least one password is required, check all of them in order
            for required_password_path in paths_requiring_passwords {
                let required_password = required_passwords.get(required_password_path).unwrap();

                // Check if a matching password was provided through the request guard (the Authorization header)
                if let Some(user_provided_password) = request_password.as_string() {
                    // Check if the password matches
                    if user_provided_password == required_password {
                        // It does : save it in the session cookies
                        cookies.add_private(Cookie::new(
                            password::cookie_name(required_password_path),
                            user_provided_password.clone(),
                        ));
                        user_passwords.insert(
                            required_password_path.clone(),
                            user_provided_password.clone(),
                        );

                        // Jump to the next required password in the list, if any
                        continue;
                    }
                }

                // Check if there is a valid password in the session cookies
                if let Some(user_password) = user_passwords.get(required_password_path) {
                    // Check if the password matches
                    if user_password == required_password {
                        // It matches : jump to the next required password in the list, if any
                        continue;
                    } else {
                        // It doesn't match : return "invalid password"
                        return Err(PasswordError::Invalid(required_password_path.clone()));
                    }
                } else if request_password.as_string().is_some() {
                    // An invalid password was provided in the current request : return "invalid password"
                    return Err(PasswordError::Invalid(required_password_path.clone()));
                } else {
                    // No password found in the request or the session cookies : return "password required"
                    return Err(PasswordError::Required(required_password_path.clone()));
                }
            }

            // All passwords have been provided
            Ok(user_passwords)
        }
    }

    /// Return a copy of a single photo from the cache based on its UID, with the path to its resized version,
    /// after generating it if necessary
    pub async fn get_resized_from_uid(
        &self,
        uid: &UID,
        resized_type: ResizedType,
        config: &Config,
    ) -> Result<Option<(Photo, PathBuf)>, Error> {
        match self.get_from_uid(uid).await {
            Some(photo) => {
                // Generate this file if it doesn't exist
                let resized_file_path = photo
                    .create_resized(resized_type, config.RESIZED_IMAGE_FORMAT, config)
                    .await?;

                Ok(Some((photo, resized_file_path)))
            }
            None => Ok(None),
        }
    }

    // Private function used to load photos recursively
    #[allow(clippy::too_many_arguments)]
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
        paths_found: &'a mut Option<&mut Vec<PathBuf>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(async move {
            let rel_path_str = rel_path.to_string_lossy().to_string();

            // Append this path to the list of paths found
            if let Some(paths_found) = paths_found {
                paths_found.push(rel_path.clone());
            }
            self.insert_path(rel_path).await;

            // Try to find a config file in this directory, append it to a copy of the current one (so it won't propagate to
            // sibling directories), and put it on the stack
            let mut cfg = configs_stack.last().unwrap().1.clone();
            cfg.remove("HIDDEN"); // These settings don't propagate from the parent
            if rel_path != &PathBuf::new() {
                cfg.remove("PASSWORD");
            }
            Config::update_with_subdir(full_path, &mut cfg);
            configs_stack.push((rel_path.clone(), cfg));
            let subdir_config = &configs_stack.last().unwrap().1;
            match Config::from_table(subdir_config.clone()) {
                Ok(config) => {
                    let mut subdirs_configs_lock = self.subdirs_configs.write().await;
                    subdirs_configs_lock.insert(rel_path_str.clone(), config);
                }
                Err(error) => eprintln!(
                    "Warning : unable to read config in {} : {}",
                    rel_path.display(),
                    error
                ),
            }

            // If this directory is password-protected, add it to the list
            let password = match subdir_config.get("PASSWORD") {
                Some(value) if value.is_str() => value.as_str().unwrap().to_string(),
                Some(_) => {
                    eprintln!(
                        "Warning : invalid value for PASSWORD in {}",
                        rel_path.display()
                    );
                    // Generate a random password to prevent access
                    base64::encode(rand::random::<[u8; 32]>())
                }
                None => "".to_string(),
            };
            if !password.is_empty() {
                self.passwords
                    .write()
                    .await
                    .insert(rel_path_str.clone(), password);
            }

            // List the files inside this path in the photos directory
            let mut filenames_in_fs: Vec<String> = Vec::new();
            if photos_to_insert.is_some() || photos_to_remove.is_some() {
                let dir = fs::read_dir(full_path)
                    .await
                    .map_err(|e| Error::FileError(e, full_path.clone()))?;
                let mut dir_stream = ReadDirStream::new(dir);
                while let Some(entry) = dir_stream.next().await {
                    let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
                    if let Ok(file_type) = entry.file_type().await {
                        if let Ok(filename) = entry.file_name().into_string() {
                            let filename_lowercase = filename.to_lowercase();
                            if file_type.is_file()
                                && !filename_lowercase.starts_with('.')
                                && (filename_lowercase.ends_with(".jpg")
                                    || filename_lowercase.ends_with(".jpeg"))
                            {
                                filenames_in_fs.push(filename);
                            }
                        }
                    }
                }
                filenames_in_fs.sort_by(|a, b| natord::compare_ignore_case(a, b));
            }

            // Get the list of photos saved in the database for this path exactly
            let sort_columns = String::from(
                subdir_config
                    .get("SORT_ORDER")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_config.SORT_ORDER),
            )
            .split(',')
            .map(|s| String::from(s.trim()))
            .collect::<Vec<String>>();
            let photos_in_db =
                db::get_photos_in_path(db_conn, rel_path, &sort_columns, main_config).await?;

            // Find photos in the filesystem that are not in the database yet
            if let Some(ref mut photos_to_insert) = photos_to_insert {
                let filenames_in_db = photos_in_db
                    .iter()
                    .map(|photo| &photo.filename)
                    .collect::<Vec<&String>>();
                for filename in &filenames_in_fs {
                    if !filenames_in_db.contains(&filename) {
                        let mut full_path = PathBuf::from(&main_config.PHOTOS_DIR);
                        full_path.push(rel_path);
                        full_path.push(filename);
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
            let all_uids_in_path = photos_in_db
                .iter()
                .map(|photo| &photo.uid)
                .collect::<Vec<&UID>>();
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
                                    if file_type.is_file()
                                        && filename_lowercase.starts_with(prefix)
                                        && filename_lowercase.ends_with(suffix)
                                    {
                                        // Extract the UID from the filename
                                        let file_uid = UID::try_from(
                                            &filename
                                                .chars()
                                                .skip(prefix.len())
                                                .take(filename.len() - prefix.len() - suffix.len())
                                                .collect::<String>(),
                                        );
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
                        eprintln!(
                            "Warning : unable to open cache directory \"{}\" : {}",
                            &cache_path.display(),
                            error
                        );
                    }
                }
            }
            if !resized_photos_to_delete.is_empty() {
                // Log the list of files to delete
                println!(
                    "Deleting {} obsolete resized photos in \"{}\" from cache : {}",
                    resized_photos_to_delete.len(),
                    &cache_path.display(),
                    resized_photos_to_delete
                        .iter()
                        .map(|filename| format!("\"{}\"", filename))
                        .collect::<Vec<String>>()
                        .join(", ")
                );

                // Delete the files
                for filename in resized_photos_to_delete {
                    let mut path = cache_path.clone();
                    path.push(filename);
                    fs::remove_file(&path)
                        .await
                        .map_err(|e| Error::FileError(e, path.clone()))?;
                }
            }

            // Add these photos recursively to this path and its parent paths in the gallery as long as SHOW_PHOTOS_FROM_SUBDIRS is set and HIDDEN isn't
            if !photos_in_db.is_empty() {
                let mut is_parent = false; // False for the first iteration, then set to true for the parent paths
                let mut passwords: Vec<(String, String)> = Vec::new();
                for (path, entry_config) in configs_stack[1..configs_stack.len()].iter().rev() {
                    // If we are in a parent directory of the current path, stop adding photos when a SHOW_PHOTOS_FROM_SUBDIRS=false is encountered
                    let show_photos_from_subdir = entry_config
                        .get("SHOW_PHOTOS_FROM_SUBDIRS")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(default_config.SHOW_PHOTOS_FROM_SUBDIRS);
                    if is_parent && !show_photos_from_subdir {
                        break;
                    }

                    // Remember that a password is required for this photo to be displayed
                    if let Some(password) = entry_config
                        .get("PASSWORD")
                        .and_then(|v| v.as_str())
                        .filter(|&s| !s.is_empty())
                    {
                        passwords.push((path.to_string_lossy().to_string(), password.to_string()));
                    }

                    // Add all the photos at this level
                    for photo in &photos_in_db {
                        self.insert_photo(path, photo, passwords.clone()).await;
                    }

                    // If the current path is marked as hidden, don't add the photos to the parents paths
                    let hidden = entry_config
                        .get("HIDDEN")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(default_config.HIDDEN);
                    if hidden {
                        break;
                    }

                    is_parent = true;
                }
            }

            // If the INDEX_SUBDIRS config is enabled, recursively load photos from subdirectories
            if main_config.INDEX_SUBDIRS {
                // Find the list of valid subdirectories in the path, in the filesystem
                let subdirs = list_subdirs(rel_path, main_config).await?;
                let subdirs_names = subdirs.list_names();

                // Clean obsolete subdirectories (that do not correspond to a subdirectory in the photos folder) from the cache folder
                let subdirs_in_cache = list_subdirs_in_cache(rel_path, main_config).await?;
                if !subdirs_in_cache.is_empty() {
                    let mut subdirs_in_cache_to_remove: Vec<PathBuf> = Vec::new();
                    for subdir in subdirs_in_cache {
                        if !subdirs_names.contains(&&subdir) {
                            let mut subdir_path = PathBuf::from(&main_config.CACHE_DIR);
                            subdir_path.push(rel_path);
                            subdir_path.push(subdir);
                            subdirs_in_cache_to_remove.push(subdir_path);
                        }
                    }
                    if !subdirs_in_cache_to_remove.is_empty() {
                        println!(
                            "Removing {} obsolete directory(ies) in cache : {}",
                            subdirs_in_cache_to_remove.len(),
                            subdirs_in_cache_to_remove
                                .iter()
                                .map(|subdir| format!("\"{}\"", subdir.to_string_lossy()))
                                .collect::<Vec<String>>()
                                .join(", ")
                        );
                        for subdir in subdirs_in_cache_to_remove {
                            let result = fs::remove_dir_all(&subdir).await;
                            if let Err(error) = result {
                                eprintln!(
                                    "Warning : unable to remove directory in cache \"{}\" : {}",
                                    subdir.display(),
                                    error
                                );
                            }
                        }
                    }
                }

                // Remember the subdirs internally
                {
                    let mut subdirs_lock = self.subdirs.write().await;
                    subdirs_lock.insert(rel_path_str.clone(), subdirs.clone());
                }

                // Load subdirectories recursively
                if !subdirs.is_empty() {
                    for subdir in subdirs.list_names() {
                        let mut subdir_rel_path = rel_path.clone();
                        subdir_rel_path.push(subdir);
                        let mut subdir_full_path = full_path.clone();
                        subdir_full_path.push(subdir);
                        self.load_rec(
                            &subdir_full_path,
                            &subdir_rel_path,
                            db_conn,
                            main_config,
                            configs_stack,
                            default_config,
                            photos_to_insert,
                            photos_to_remove,
                            paths_found,
                        )
                        .await?;
                    }
                }
            }

            // Remove this entry in the stack of configs
            configs_stack.pop();

            Ok(())
        })
    }

    /// Load all available photos in the photos folder, add them to the given gallery, and sync them with the database
    async fn load(
        &self,
        config: &Config,
        db_conn: &mut PoolConnection<Sqlite>,
    ) -> Result<(), Error> {
        // Make sure the main directories (photos and cache) exist, and if not, try to create them
        check_config_dir(&PathBuf::from(&config.PHOTOS_DIR)).await
            .map_err(|e| {
                if let Error::FileError(error, path) = &e {
                    println!("There is an issue with the PHOTOS_DIR setting in the config file (\"{}\") : {} : {}", path.display(), error.kind(), error);
                }
                e
            })?;
        check_config_dir(&PathBuf::from(&config.CACHE_DIR)).await
            .map_err(|error| {
                if let Error::FileError(error, path) = &error {
                    eprintln!("There is an issue with the CACHE_DIR setting in the config file (\"{}\") : {} : {}", path.display(), error.kind(), error);
                }
                error
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
            &full_path,
            &rel_path,
            db_conn,
            config,
            &mut configs_stack,
            &default_config,
            &mut Some(&mut photos_to_insert),
            &mut Some(&mut photos_to_remove),
            &mut Some(&mut paths_found),
        )
        .await?;

        // Get the list of all known subdirs of the current path in the database, check if some have been removed,
        // and if so add their photos to the 'to_remove' list
        if config.INDEX_SUBDIRS {
            let mut deleted_paths: Vec<PathBuf> = Vec::new();
            let known_paths_in_db = db::get_all_paths(db_conn).await?;
            for known_path in known_paths_in_db {
                if !paths_found.contains(&known_path) {
                    deleted_paths.push(known_path);
                }
            }
            if !deleted_paths.is_empty() {
                let photos_in_deleted_paths =
                    db::get_photos_in_paths(db_conn, &deleted_paths, config).await?;
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
                for (idx, photo) in photos_to_insert
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(batch_size)
                {
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
                                print!(
                                    "\rCalculating MD5 hashes of {} new files... {}%",
                                    n, percent
                                );
                                std::io::stdout().flush().ok();
                                last_percent = percent;
                            }
                        }
                        Ok((i, Err(error))) => eprintln!(
                            "Error : unable to compute MD5 for \"{}\" : {}",
                            photos_to_insert.get(i).unwrap().filename,
                            error
                        ),
                        Err(error) => eprintln!(
                            "Error : unable to join background task while computing MD5 : {}",
                            error
                        ),
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
        let need_to_reload = !photos_to_insert.is_empty()
            || !photos_to_remove.is_empty()
            || !photos_to_move.is_empty();

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
                    tasks.push(tokio::spawn(async move {
                        // Parse the metadata
                        photo
                            .parse_metadata(true)
                            .await
                            .map_err(|e| {
                                eprintln!(
                                    "Error : unable to open \"{}/{}\" : {}",
                                    photo.path.to_string_lossy(),
                                    photo.filename,
                                    e
                                );
                                e
                            })
                            .ok(); // Ignore error after printing it

                        // Generate thumbnails
                        if pre_generate_thumbnails {
                            photo
                                .create_resized_from_params(
                                    ResizedType::THUMBNAIL,
                                    image_format,
                                    cache_dir.clone(),
                                    thumbnail_max_size,
                                    thumbnail_quality,
                                )
                                .await
                                .ok();
                            photo
                                .create_resized_from_params(
                                    ResizedType::LARGE,
                                    image_format,
                                    cache_dir.clone(),
                                    large_size_max_size,
                                    large_size_quality,
                                )
                                .await
                                .ok();
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
            println!(
                "Inserting {} photo(s) into the database : {}",
                photos_to_insert_in_db.len(),
                photos_to_insert_in_db
                    .iter()
                    .map(|photo| format!("\"{}/{}\"", photo.path.to_string_lossy(), photo.filename))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            // Insert them into the database
            db::insert_photos(db_conn, &photos_to_insert_in_db).await?;
        }
        if !photos_to_remove.is_empty() {
            // Log the list of photos to remove
            println!(
                "Removing {} photo(s) from the database : {}",
                photos_to_remove.len(),
                photos_to_remove
                    .iter()
                    .map(|photo| format!("\"{}/{}\"", photo.path.to_string_lossy(), photo.filename))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            // Remove them from the database
            db::remove_photos(db_conn, &photos_to_remove).await?;
        }
        if !photos_to_move.is_empty() {
            // Log the list of photos to rename/move
            println!(
                "Renaming/moving {} photo(s) in the database : {}",
                photos_to_move.len(),
                photos_to_move
                    .iter()
                    .map(|pair| format!(
                        "\"{}/{}\" -> \"{}/{}\"",
                        pair.0.path.to_string_lossy(),
                        pair.0.filename,
                        pair.1.path.to_string_lossy(),
                        pair.1.filename
                    ))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            // Update the database
            db::move_photos(db_conn, &photos_to_move).await?;
        }

        // Reload if required
        if need_to_reload {
            self.clear().await;
            self.load_rec(
                &full_path,
                &rel_path,
                db_conn,
                config,
                &mut configs_stack,
                &default_config,
                &mut None,
                &mut None,
                &mut None,
            )
            .await?;
        }

        // Update the counts of photos
        self.update_counts().await;

        // Apply REVERSE_SORT_ORDER to the gallery
        let mut gallery_lock = self.gallery.write().await;
        let subdirs_configs_lock = self.subdirs_configs.read().await;
        for (path, config) in subdirs_configs_lock.deref() {
            if config.REVERSE_SORT_ORDER {
                if let Some(ref mut photos) = gallery_lock.get_mut(path) {
                    photos.reverse();
                }
            }
        }

        // Good job.
        Ok(())
    }

    /// Recalculate the internal `counts` data structure based on the current `gallery` : for each path,
    /// `counts` will aggregate the number of photos based on the exact combination of passwords they
    /// require.
    async fn update_counts(&self) {
        // Acquire locks on the gallery's internal data structures
        let mut counts_lock = self.counts.write().await;
        let gallery_lock = self.gallery.read().await;

        // Reset the counts
        counts_lock.clear();

        // Process each path in the gallery
        for (path, photos) in gallery_lock.deref() {
            // Aggregate of the number of photos found which require a certain list of passwords
            let mut counts_in_path: HashMap<Vec<String>, usize> = HashMap::new();

            // Process each photo in this path
            for photo in photos {
                // List of passwords required to access this photo
                let mut photo_passwords = photo
                    .passwords
                    .iter()
                    .map(|(p, _)| p)
                    .collect::<Vec<&String>>();
                photo_passwords.sort_unstable();

                // Try to find this list of passwords in the current hashmap
                let mut key_found = None;
                for key in counts_in_path.keys() {
                    let key_pw = key.iter().collect::<Vec<&String>>();
                    if key_pw == photo_passwords {
                        key_found = Some(key.clone());
                        break;
                    }
                }

                // If this list of passwords already exists, increment its count
                if let Some(key_found) = key_found {
                    *counts_in_path.get_mut(&key_found).unwrap() += 1;
                } else {
                    // Otherwise, insert it with an initial value of 1
                    let mut new_key = photo_passwords
                        .iter()
                        .map(|&s| s.clone())
                        .collect::<Vec<String>>();
                    new_key.sort_unstable();
                    counts_in_path.insert(new_key, 1);
                }
            }

            // Add the counts for this path to the main hashmap
            counts_lock.insert(path.clone(), counts_in_path);
        }
    }

    // Return the list of known non-hidden subdirectories for the given path
    pub async fn get_subdirs(&self, path: &Path, always_include: Option<&String>) -> Vec<String> {
        let subdirs_lock = self.subdirs.read().await;
        if let Some(subdirs) = subdirs_lock.get(&path.to_string_lossy().to_string()) {
            let mut subdirs = subdirs.list_visible();
            if let Some(always_include) = always_include {
                if !subdirs.contains_name(always_include) {
                    subdirs.push(Subdir::new(always_include.clone(), false));
                }
            }
            subdirs.sort();
            subdirs.list_names_visible_owned()
        } else {
            // This path is not found, return an empty list
            Vec::new()
        }
    }

    // Return a read lock on the internal list of passwords
    pub async fn get_passwords(&self) -> RwLockReadGuard<HashMap<String, String>> {
        self.passwords.read().await
    }
}

/// RAII read lock on the gallery, obtained with `Gallery::read()`. Every access on the gellery's photos
/// must pass through an instance of this lock. Concurrent reads are allowed, which means access will
/// be immediately granted as long as the gallery is not reloading.
pub struct GalleryReadLock<'a> {
    guard: RwLockReadGuard<'a, GalleryContent>,
    path: String,
    pub start: usize,
    pub max_count: usize,
    pub total: usize,
    provided_passwords: Passwords,
}

impl<'a> GalleryReadLock<'a> {
    fn new(
        guard: RwLockReadGuard<'a, GalleryContent>,
        path: String,
        start: usize,
        max_count: usize,
        total: usize,
        provided_passwords: Passwords,
    ) -> Self {
        Self {
            guard,
            path,
            start,
            max_count,
            total,
            provided_passwords,
        }
    }

    pub fn iter(&'a self) -> GalleryReadIterator<'a> {
        GalleryReadIterator::new(self)
    }
}

pub struct GalleryReadIterator<'a> {
    lock: &'a GalleryReadLock<'a>,
    index: usize,   // Current index in the collection, relative to lock.start
    counter: usize, // Number of photos that have currently be returned to the user, <= index
}

impl<'a> GalleryReadIterator<'a> {
    fn new(lock: &'a GalleryReadLock<'a>) -> Self {
        Self {
            lock,
            index: 0,
            counter: 0,
        }
    }
}

impl<'a> Iterator for GalleryReadIterator<'a> {
    type Item = &'a Photo;

    fn next(&mut self) -> Option<Self::Item> {
        // Find the next photo available in this path in the gallery
        let photos = self.lock.guard.get(&self.lock.path).unwrap();
        'find_a_photo: while self.counter < self.lock.max_count {
            // Get the next photo
            if let Some(photo) = photos.get(self.lock.start + self.index) {
                self.index += 1;

                // Check if this photo requires some passwords
                for (required_password_path, required_password) in &photo.passwords {
                    if let Some(provided_password) =
                        self.lock.provided_passwords.get(required_password_path)
                    {
                        if required_password != provided_password {
                            // This required password is invalid in the user's session
                            continue 'find_a_photo;
                        }
                    } else {
                        // This required password is not found in the user's session
                        continue 'find_a_photo;
                    }
                }

                self.counter += 1;
                return Some(&photo.photo);
            } else {
                // No more photos in this gallery
                return None;
            }
        }
        // We have returned enough photos
        None
    }
}

/// Fairing callback used to load/sync the photos with the database at startup
pub async fn init(rocket: Rocket<rocket::Build>) -> fairing::Result {
    // Make sure the database has been initialized (fairings have been attached in the correct order)
    match db::DB::fetch(&rocket) {
        Some(db) => match db.0.acquire().await {
            Ok(mut db_conn) => {
                let config = rocket
                    .state::<Config>()
                    .expect("Error : unable to obtain the config");
                let gallery = rocket
                    .state::<Gallery>()
                    .expect("Error : unable to obtain the gallery");

                println!("Loading photos...");
                let now = Instant::now();
                match gallery.load(config, &mut db_conn).await {
                    Ok(_) => {
                        println!(
                            "Loaded {} photos successfully in {}ms",
                            gallery.len().await,
                            now.elapsed().as_millis()
                        );
                        Ok(rocket)
                    }
                    Err(error) => {
                        eprintln!("Error : unable to load photos : {}", error);
                        Err(rocket)
                    }
                }
            }

            Err(error) => {
                eprintln!(
                    "Error : unable to acquire a connection to the database : {}",
                    error
                );
                Err(rocket)
            }
        },
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
                Err(Error::FileError(
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("\"{}\" is not a valid directory", path.display()),
                    ),
                    path.clone(),
                ))
            }
        }

        // The directory doesn't exist, try to create it and return the result
        // of that operation directly
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!("Creating empty directory \"{}\"", path.display());
            fs::create_dir_all(path)
                .await
                .map_err(|e| Error::FileError(e, path.clone()))
        }

        // Return any other error directly
        Err(error) => Err(Error::FileError(error, path.clone())),
    }
}

#[derive(Clone)]
struct Subdir {
    name: String,
    hidden: bool,
}

impl Subdir {
    fn new(name: String, hidden: bool) -> Self {
        Self { name, hidden }
    }
}

impl PartialEq for Subdir {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl PartialOrd for Subdir {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.hidden.partial_cmp(&other.hidden)
    }
}

#[derive(Clone)]
struct Subdirs {
    subdirs: Vec<Subdir>,
}

impl Subdirs {
    fn new() -> Self {
        Self {
            subdirs: Vec::new(),
        }
    }

    fn contains_name(&self, name: &str) -> bool {
        self.iter().any(|s| s.name == name)
    }

    fn sort(&mut self) {
        self.subdirs
            .sort_by(|a, b| natord::compare_ignore_case(&a.name, &b.name));
    }

    fn list_visible(&self) -> Subdirs {
        let subdirs = self
            .subdirs
            .iter()
            .filter(|s| !s.hidden)
            .cloned()
            .collect::<Vec<Subdir>>();
        Self { subdirs }
    }

    fn list_names(&self) -> Vec<&String> {
        self.subdirs
            .iter()
            .map(|subdir| &subdir.name)
            .collect::<Vec<_>>()
    }

    fn list_names_visible(&self) -> Vec<&String> {
        self.subdirs
            .iter()
            .filter(|subdir| !subdir.hidden)
            .map(|subdir| &subdir.name)
            .collect::<Vec<_>>()
    }

    fn list_names_visible_owned(&self) -> Vec<String> {
        self.list_names_visible()
            .iter()
            .map(|&s| s.clone())
            .collect::<Vec<String>>()
    }
}

impl Deref for Subdirs {
    type Target = Vec<Subdir>;

    fn deref(&self) -> &Self::Target {
        &self.subdirs
    }
}

impl DerefMut for Subdirs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.subdirs
    }
}

/// Return the list of valid subdirectories in the given path in the photos folder
async fn list_subdirs(path: &PathBuf, config: &Config) -> Result<Subdirs, Error> {
    let mut subdirs = Subdirs::new();
    let mut full_path = PathBuf::from(&config.PHOTOS_DIR);
    full_path.push(path);

    // Try to open a Stream to the content of this path
    let dir = match fs::read_dir(&full_path).await {
        Ok(dir) => dir,

        // Return any other error directly
        Err(error) => return Err(Error::FileError(error, full_path.clone())),
    };
    let mut dir_stream = ReadDirStream::new(dir);

    // Iterate over the entries found in this path
    while let Some(entry) = dir_stream.next().await {
        let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
        if let Ok(file_type) = entry.file_type().await {
            if let Ok(dir_name) = entry.file_name().into_string() {
                if file_type.is_dir() && !dir_name.starts_with('.') {
                    // This is a valid subdirectory, check if it contains a config that would forbid including it in the results
                    let mut subdir_path = full_path.clone();
                    subdir_path.push(&dir_name);
                    let mut subdir_config_table: Table = Table::new();
                    Config::update_with_subdir(&subdir_path, &mut subdir_config_table);
                    let subdir_config =
                        Config::from_table(subdir_config_table).unwrap_or_else(|error| {
                            eprintln!(
                                "Warning : unable to deserialize local config file in \"{}\" : {}",
                                subdir_path.display(),
                                error
                            );
                            Config::default()
                        });
                    if subdir_config.INDEX {
                        subdirs.push(Subdir::new(dir_name, subdir_config.HIDDEN));
                    }
                }
            }
        }
    }

    subdirs.sort();
    Ok(subdirs)
}

/// Return the list of the names of the valid subdirectories in the given path in the cache folder
async fn list_subdirs_in_cache(path: &PathBuf, config: &Config) -> Result<Vec<String>, Error> {
    let mut subdirs: Vec<String> = Vec::new();
    let mut full_path = PathBuf::from(&config.CACHE_DIR);
    full_path.push(path);

    // Try to open a Stream to the content of this path
    let dir = match fs::read_dir(&full_path).await {
        Ok(dir) => dir,

        // This directory doesn't exist, just return as if the directory is empty
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),

        // Return any other error directly
        Err(error) => return Err(Error::FileError(error, full_path.clone())),
    };
    let mut dir_stream = ReadDirStream::new(dir);

    // Iterate over the entries found in this path
    while let Some(entry) = dir_stream.next().await {
        let entry = entry.map_err(|e| Error::FileError(e, full_path.clone()))?;
        if let Ok(file_type) = entry.file_type().await {
            if let Ok(dir_name) = entry.file_name().into_string() {
                if file_type.is_dir() && !dir_name.starts_with('.') {
                    subdirs.push(dir_name);
                }
            }
        }
    }

    Ok(subdirs)
}

/// Calculate and return the MD5 hash of the file located at the given path
async fn calculate_file_md5(path: &PathBuf) -> Result<String, Error> {
    let file_content = fs::read(path)
        .await
        .map_err(|e| Error::FileError(e, path.clone()))?;
    let hash = Md5::digest(file_content);
    Ok(format!("{:x}", hash))
}

/// Kinds of resized versions of photos generated in the cache folder
#[allow(clippy::upper_case_acronyms)]
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
#[allow(clippy::upper_case_acronyms)]
pub enum ImageFormat {
    JPEG,
    #[default]
    WEBP,
}
