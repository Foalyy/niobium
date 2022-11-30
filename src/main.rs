#[macro_use] extern crate rocket;

mod config;
mod nav_data;
mod photos;
mod db;

use config::Config;
use nav_data::NavData;
use rocket::fs::NamedFile;
use rocket::http::Header;
use std::{io, fmt::Display};
use std::path::{PathBuf, Path};
use rocket::{fs::FileServer, State, tokio::sync::Mutex};
use rocket_dyn_templates::{Template, context};
use rusqlite::Connection;



#[launch]
async fn rocket() -> _ {
    // Try to read the config file
    let config = Config::read_or_exit();

    // Try to open a connection to the SQLite database
    let db_conn = Mutex::new(db::open_or_exit(&config));

    // Load the photos, or exit immediately in case of an error
    // Note : photos::load() will print the error message on stderr
    photos::load(&PathBuf::from(""), &config, &db_conn).await
        .unwrap_or_else(|error| {
            eprintln!("Error : unable to load photos : {}", error);
            std::process::exit(-1)
        });

    // Let's go to spaaace !
    rocket::build()
        .mount("/", routes![
            get_gallery,
            get_grid,
            get_nav,
            get_grid_item,
            get_thumbnail,
            get_large,
            get_photo,
            download_photo,
        ])
        .mount("/static", FileServer::from("static/").rank(0))
        .attach(Template::fairing())
        .manage(config)
        .manage(db_conn)
}


/// Route handler called to render the main layout of the gallery
#[get("/<path..>", rank=5)]
fn get_gallery(path: PathBuf, config: &State<Config>) -> PageResult {
    // Check the requested path
    match photos::check_path(&path, &config.inner()) {

        Ok(_full_path) => {
            // Path looks valid, render the template
            let nav_data = NavData::from_path(&path);
            PageResult::Page(Template::render("main", context! {
                config: config.inner(),
                nav: nav_data,
                uid_chars: photos::UID_CHARS,
                load_grid_url: load_grid_url(&path),
                load_nav_url: uri!(get_nav(&path)).to_string(),
            }))
        }

        Err(error) => match error {
            Error::FileError(error, _) => match error.kind() {
                // The path is either not found or invalid for the current config, return the 404 template
                io::ErrorKind::NotFound => page_404(&config),
                
                // For any other error, forward the error to the 500 Internal Error catcher
                _ => PageResult::Err(()),
            },
            _ => PageResult::Err(()),
        }
    }
}


/// Route handler called by AJAX to return the grid items for the given path and parameters
#[get("/<path..>?grid&<start>&<count>&<uid>", rank=1)]
async fn get_grid(path: PathBuf, start: Option<usize>, count: Option<usize>, uid: Option<String>, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    // Try to load the photos in the given path
    match photos::load(&path, config, db_conn).await {

        // We have a valid (possibly empty) list of photos, render it as a template
        Ok(mut photos) => {
            let n_photos = photos.len();

            // Add the load url to each photo
            for photo in photos.iter_mut() {
                photo.get_grid_item_url = uri!(get_grid_item(&photo.uid)).to_string();
            }

            let mut photos_filtered = photos.as_mut_slice();
            if let Some(uid) = uid {
                // Only return a single UID if requested
                if let Some(idx) = photos_filtered.iter().position(|p| p.uid == uid) {
                    photos_filtered = &mut photos[idx..=idx]
                }
            } else {
                // Only return a subset if requested
                let mut start = start.unwrap_or(0);
                let mut count = count.unwrap_or(n_photos);
                if start >= n_photos {
                    start = 0;
                }
                if start + count > n_photos {
                    count = n_photos - start;
                }
                photos_filtered = &mut photos[start..start+count];
            }

            // If the requested set is small enough, calculate the image sizes to improve the first display
            if photos_filtered.len() <= 100 {
                for mut photo in photos_filtered.iter_mut() {
                    let result = photos::parse_metadata(&mut photo, &config, &db_conn).await;
                    if let Err(error) = result {
                        eprintln!("Warning : unable to parse metadata of photo {} : {}", photo.full_path(&config).display(), error);
                    }
                }
            }

            PageResult::Page(Template::render("grid", context! {
                config: &config.inner(),
                photos: &photos_filtered,
                n_photos: n_photos,
            }))
        }

        Err(error) => {
            let result = match &error {
                Error::FileError(file_error, _) => match &file_error.kind() {
                    // The path is either not found or invalid for the current config, return the 404 template
                    io::ErrorKind::NotFound => Ok(page_404(&config)),

                    // For any other error, forward the error to the 500 Internal Error catcher
                    _ => Err(&error)
                }
                error => Err(error)
            };
            match result {
                Ok(page_result) => page_result,
                Err(error) => {
                    eprintln!("Error : unable to load the photos grid : {}", error);
                    PageResult::Err(())
                }
            }
        }
    }
}


/// Route handler called by AJAX to return the nav menu for the given path
#[get("/<path..>?nav", rank=2)]
fn get_nav(path: PathBuf, _config: &State<Config>) -> () {
    //let nav_data = NavData::from_path(&path);
    //Template::render("nav", context! {
    //    config: config.inner(),
    //})
}


/// Route handler called asynchronously to render a single photo inside the grid
#[get("/<uid>/grid-item")]
async fn get_grid_item(uid: String, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    match photos::get_from_uid(&uid, config, db_conn).await {
        Ok(Some(photo)) => PageResult::Page(Template::render("grid-item", context! {
            config: &config.inner(),
            photo: &photo,
            url_get_thumbnail: uri!(get_thumbnail(&uid)),
            url_get_large: uri!(get_large(&uid)),
            url_get_photo: uri!(get_photo(&uid)),
            url_download_photo:uri!(download_photo(&uid)),
        })),
        Ok(None) => page_404(&config),
        Err(error) => {
            eprintln!("Error : unable to render a grid item : {}", error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the thumbnail version of the requested UID
#[get("/<uid>/thumbnail")]
async fn get_thumbnail(uid: String, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    get_resized(&uid, photos::ResizedType::THUMBNAIL, &config, &db_conn).await
}


/// Route handler that returns the large resized version of the requested UID
#[get("/<uid>/large")]
async fn get_large(uid: String, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    get_resized(&uid, photos::ResizedType::LARGE, &config, &db_conn).await
}


/// Returns the resized version of the requested UID for the given prefix
async fn get_resized(uid: &String, resized_type: photos::ResizedType, config: &Config, db_conn: &Mutex<Connection>) -> PageResult {
    match photos::get_resized_from_uid(uid, resized_type, config, db_conn).await {
        Ok(Some((photo, resized_file_path))) => {
            match NamedFile::open(&resized_file_path).await {
                Ok(file) => PageResult::Photo(file),
                Err(error) => {
                    eprintln!("Error : unable to read or create cache file for \"{}\" at \"{}\" : {}", photo.full_path(config).display(), resized_file_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        Ok(None) => page_404(&config),
        Err(error) => {
            eprintln!("Error : unable to return a resized photo : {}", error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the photo file for the requested UID
#[get("/<uid>")]
async fn get_photo(uid: String, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    match photos::get_from_uid(&uid, config, db_conn).await {
        Ok(Some(photo)) => {
            let full_path = photo.full_path(config);
            match NamedFile::open(&full_path).await {
                Ok(file) => PageResult::Photo(file),
                Err(error) => {
                    eprintln!("Error : unable to read file \"{}\" : {}", full_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        Ok(None) => page_404(&config),
        Err(error) => {
            eprintln!("Error : unable to return a photo : {}", error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the photo file for the requested UID as a download
#[get("/<uid>/download")]
async fn download_photo(uid: String, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    match photos::get_from_uid(&uid, config, db_conn).await {
        Ok(Some(photo)) => {
            let full_path = photo.full_path(config);
            match DownloadedNamedFile::open(&full_path, &photo.uid).await {
                Ok(file) => PageResult::PhotoDownload(file),
                Err(error) => {
                    eprintln!("Error : unable to read file \"{}\" : {}", full_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        Ok(None) => page_404(&config),
        Err(error) => {
            eprintln!("Error : unable to return a photo as a download : {}", error);
            PageResult::Err(())
        }
    }
}


/// Render the 404 page
fn page_404(config: &Config) -> PageResult {
    PageResult::NotFound(Template::render("404", context! {
        config: config,
        url_gallery_root: uri!(get_gallery(""))
    }))
}


fn load_grid_url(path: &PathBuf) -> String {
    uri!(get_grid(path, None as Option<usize>, None as Option<usize>, None as Option<String>)).to_string()
}


/// Responder used by most routes
#[derive(Responder)]
pub enum PageResult {
    Page(Template),
    Photo(NamedFile),
    PhotoDownload(DownloadedNamedFile),
    #[response(status = 404)]
    NotFound(Template),
    #[response(status = 500)]
    Err(()),
}


/// A wrapper around NamedFile that offers the file as a download by setting the Content-Disposition header
#[derive(Responder)]
pub struct DownloadedNamedFile {
    inner: NamedFile,
    content_disposition: Header<'static>,
}

impl DownloadedNamedFile {
    pub async fn open<P>(path: P, uid: &String) -> io::Result<Self>
    where
        P: AsRef<Path>
    {
        NamedFile::open(path).await.map(|file|
            Self {
                inner: file,
                content_disposition: Header::new(
                    rocket::http::hyper::header::CONTENT_DISPOSITION.as_str(),
                    format!("attachment; filename=\"niobium_{}.jpg\"", uid)
                ),
            }
        )
    }
}


/// Generic error type used to uniformize errors across the crate
#[derive(Debug)]
pub enum Error {
    FileError(io::Error, PathBuf),
    TomlParserError(toml::de::Error),
    DatabaseError(rusqlite::Error),
    ImageError(image::error::ImageError, PathBuf),
    EXIFParserError(exif::Error, PathBuf),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::FileError(error, path) => write!(f, "file error for \"{}\" : {}", path.display(), error),
            Error::TomlParserError(error) => write!(f, "TOML parser error : {}", error),
            Error::DatabaseError(error) => write!(f, "database error : {}", error),
            Error::ImageError(error, path) => write!(f, "image error for \"{}\" : {}", path.display(), error),
            Error::EXIFParserError(error, path) => write!(f, "EXIF parser error for \"{}\" : {}", path.display(), error),
        }
    }
}