#[macro_use] extern crate rocket;

mod config;
mod db;
mod nav_data;
mod photos;
mod uid;

use config::Config;
use db::DB;
use nav_data::NavData;
use rocket::fairing::AdHoc;
use uid::UID;
use rocket::fs::NamedFile;
use rocket::http::Header;
use rocket::{fs::FileServer, State};
use rocket_dyn_templates::{Template, context};
use rocket_db_pools::{Connection, sqlx, Database};
use std::net::IpAddr;
use std::time::Instant;
use std::{io, fmt::Display};
use std::path::{PathBuf, Path};



#[launch]
async fn rocket() -> _ {
    // Try to read the config file
    let config = Config::read_or_exit();
    let address = config.ADDRESS.clone();
    let port = config.PORT;

    // Send some of the settings to Rocket
    let figment = rocket::Config::figment()
        .merge(("address", config.ADDRESS.parse::<IpAddr>().map_err(|e| {
            eprintln!("Error : invalid value for ADDRESS in {} : {}", config::FILENAME, e);
            std::process::exit(-1);
        }).unwrap()))
        .merge(("port", config.PORT))
        .merge(("databases.niobium.url", &config.DATABASE_PATH));


    // Let's go to spaaace !
    rocket::custom(figment)
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
        .attach(DB::init())
        .attach(AdHoc::try_on_ignite("Database schema init", db::init_schema))
        .manage(config)
        .attach(AdHoc::try_on_ignite("Photos init", photos::init_load))
        .attach(AdHoc::on_liftoff("Startup message", move |_| Box::pin(async move {
            println!("## Niobium started on {}:{}", address, port);
        })))
}


/// Route handler called to render the main layout of the gallery
#[get("/<path..>", rank=15)]
async fn get_gallery(path: PathBuf, config: &State<Config>) -> PageResult {
    // Check the requested path
    match photos::check_path(&path, &config.inner()) {
        Ok(_full_path) => {
            // Path looks valid, render the template
            match NavData::from_path(&path, &config).await {
                Ok(nav_data) => PageResult::Page(Template::render("main", context! {
                    config: config.inner(),
                    nav: nav_data,
                    uid_chars: UID::CHARS,
                    uid_length: UID::LENGTH,
                    load_grid_url: uri!(get_grid(&path, None as Option<usize>, None as Option<usize>, None as Option<UID>)).to_string(),
                    load_nav_url: uri!(get_nav(&path)).to_string(),
                })),
                Err(error) => {
                    eprintln!("Error : unable to generate nav data for \"{}\" : {}", path.display(), error);
                    PageResult::Err(())
                }
            }
        }

        // The path is either not found or invalid for the current config, return the 404 template
        Err(Error::FileError(error, _)) if error.kind() == io::ErrorKind::NotFound => page_404(&config),

        // For any other error, forward it to the 500 Internal Error catcher
        Err(error) => {
            eprintln!("Error : unable to load the gallery at \"{}\" : {}", path.display(), error);
            PageResult::Err(())
        }
    }
}


/// Route handler called by javascript to return the grid items for the given path and parameters
#[get("/<path..>?grid&<start>&<count>&<uid>", rank=10)]
async fn get_grid(path: PathBuf, start: Option<usize>, count: Option<usize>, uid: Option<UID>, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    let now = Instant::now();
    
    // Try to load the photos in the given path
    match photos::load(&path, config, &mut db_conn).await {

        // We have a valid (possibly empty) list of photos, render it as a template
        Ok(mut photos) => {
            println!("Finished loading in {}ms", now.elapsed().as_millis());
            let n_photos = photos.len();

            // Add the load url to each photo
            for photo in photos.iter_mut() {
                photo.get_grid_item_url = uri!(get_grid_item(&photo.uid)).to_string();
            }

            // Filter the results
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
                for photo in photos_filtered.iter_mut() {
                    let result = photo.parse_metadata(&config, &mut db_conn).await;
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

        // The path is either not found or invalid for the current config, return the 404 template
        Err(Error::FileError(file_error, _)) if file_error.kind() == io::ErrorKind::NotFound => page_404(&config),

        // For any other error, forward it to the 500 Internal Error catcher
        Err(error) => {
            eprintln!("Error : unable to load the photos grid at \"{}\" : {}", path.display(), error);
            PageResult::Err(())
        }
    }
}


/// Route handler called by javascript to return the nav menu for the given path
#[get("/<path..>?nav", rank=11)]
async fn get_nav(path: PathBuf, config: &State<Config>) -> PageResult {
    match NavData::from_path(&path, &config).await {
        Ok(nav_data) => PageResult::Page(Template::render("nav", context! {
            config: &config.inner(),
            nav: nav_data,
        })),
        Err(error) => {
            eprintln!("Error : unable to generate nav data for \"{}\" : {}", path.display(), error);
            PageResult::Err(())
        }
    }
}


/// Route handler called asynchronously to render a single photo inside the grid
#[get("/<uid>/grid-item", rank=1)]
async fn get_grid_item(uid: UID, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    match photos::get_from_uid(&uid, config, &mut db_conn).await {
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
            eprintln!("Error : unable to render a grid item for UID #{} : {}", uid, error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the thumbnail version of the requested UID
#[get("/<uid>/thumbnail", rank=2)]
async fn get_thumbnail(uid: UID, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    get_resized(&uid, photos::ResizedType::THUMBNAIL, &config, &mut db_conn).await
}


/// Route handler that returns the large resized version of the requested UID
#[get("/<uid>/large", rank=3)]
async fn get_large(uid: UID, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    get_resized(&uid, photos::ResizedType::LARGE, &config, &mut db_conn).await
}


/// Returns the resized version of the requested UID for the given prefix
async fn get_resized(uid: &UID, resized_type: photos::ResizedType, config: &Config, db_conn: &mut Connection<DB>) -> PageResult {
    match photos::get_resized_from_uid(uid, resized_type, config, db_conn).await {
        Ok(Some((photo, resized_file_path))) => {
            // Try to open the file
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
            eprintln!("Error : unable to return a resized photo for UID #{} : {}", uid, error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the photo file for the requested UID
#[get("/<uid>", rank=5)]
async fn get_photo(uid: UID, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    match photos::get_from_uid(&uid, config, &mut db_conn).await {
        Ok(Some(photo)) => {
            // Try to open the file
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
            eprintln!("Error : unable to return a photo for UID #{} : {}", uid, error);
            PageResult::Err(())
        }
    }
}


/// Route handler that returns the photo file for the requested UID as a download
#[get("/<uid>/download", rank=4)]
async fn download_photo(uid: UID, config: &State<Config>, mut db_conn: Connection<DB>) -> PageResult {
    match photos::get_from_uid(&uid, config, &mut db_conn).await {
        Ok(Some(photo)) => {
            // Try to open the file
            let full_path = photo.full_path(config);
            match DownloadedNamedFile::open(&full_path, &photo.uid, &config).await {
                Ok(file) => PageResult::PhotoDownload(file),
                Err(error) => {
                    eprintln!("Error : unable to read file \"{}\" : {}", full_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        Ok(None) => page_404(&config),
        Err(error) => {
            eprintln!("Error : unable to return a photo as a download for UID #{} : {}", uid, error);
            PageResult::Err(())
        }
    }
}


/// Render the 404 page
fn page_404(config: &Config) -> PageResult {
    PageResult::NotFound(Template::render("404", context! {
        config: config,
        url_gallery_root: uri!(get_gallery(PathBuf::from("/")))
    }))
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
    pub async fn open<P>(path: P, uid: &UID, config: &Config) -> io::Result<Self>
    where
        P: AsRef<Path>
    {
        NamedFile::open(path).await.map(|file|
            Self {
                inner: file,
                content_disposition: Header::new(
                    rocket::http::hyper::header::CONTENT_DISPOSITION.as_str(),
                    format!("attachment; filename=\"{}{}.jpg\"", &config.DOWNLOAD_PREFIX, uid.to_string())
                ),
            }
        )
    }
}


/// General type used to standardize errors across the crate
#[derive(Debug)]
pub enum Error {
    InvalidRequestError(PathBuf),
    InvalidUIDError(UID),
    UIDParserError(String),
    FileError(io::Error, PathBuf),
    TomlParserError(toml::de::Error),
    DatabaseError(sqlx::Error),
    ImageError(image::ImageError, PathBuf),
    EXIFParserError(exif::Error, PathBuf),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidRequestError(path) => write!(f, "invalid request : \"{}\"", path.display()),
            Error::InvalidUIDError(uid) => write!(f, "invalid UID : \"{}\"", uid),
            Error::UIDParserError(uid) => write!(f, "invalid UID format : \"{}\"", uid),
            Error::FileError(error, path) => write!(f, "file error for \"{}\" : {}", path.display(), error),
            Error::TomlParserError(error) => write!(f, "TOML parser error : {}", error),
            Error::DatabaseError(error) => write!(f, "database error : {}", error),
            Error::ImageError(error, path) => write!(f, "image error for \"{}\" : {}", path.display(), error),
            Error::EXIFParserError(error, path) => write!(f, "EXIF parser error for \"{}\" : {}", path.display(), error),
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        Error::DatabaseError(error)
    }
}

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Self {
        Error::TomlParserError(error)
    }
}