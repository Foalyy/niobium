#[macro_use] extern crate rocket;

mod config;
mod db;
mod nav_data;
mod photos;
mod uid;
mod password;

use config::Config;
use db::DB;
use nav_data::NavData;
use photos::Gallery;
use uid::UID;
use password::OptionalPassword;
use rocket::fairing::AdHoc;
use rocket::response::Redirect;
use rocket::fs::NamedFile;
use rocket::http::{Header, CookieJar};
use rocket::{fs::FileServer, State};
use rocket_dyn_templates::{Template, context};
use rocket_db_pools::{sqlx, Database, Connection};
use std::net::IpAddr;
use std::ops::Deref;
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
        .merge(("ident", "Niobium"))
        .merge(("secret_key", config::get_secret_key_or_exit()))
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
            reload,
        ])
        .mount("/static", FileServer::from("static/").rank(0))
        .attach(Template::fairing())
        .attach(DB::init())
        .attach(AdHoc::try_on_ignite("Database schema init", db::init_schema))
        .manage(config)
        .manage(Gallery::new())
        .attach(AdHoc::try_on_ignite("Photos init", photos::init))
        .attach(AdHoc::on_liftoff("Startup message", move |_| Box::pin(async move {
            println!("## Niobium started on {}:{}", address, port);
        })))
}


/// Route handler called to render the main layout of the gallery
#[get("/<path..>", rank=15)]
async fn get_gallery(path: PathBuf, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    if gallery.path_exists(&path).await {
        // This path exists in the gallery, calculate the content of the nav panel
        match NavData::from_path(&path, &gallery, &config).await {
            // Render the template
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
    } else {
        page_404(&config)
    }
}


/// Route handler called by javascript to return the grid items for the given path and parameters
#[get("/<path..>?grid&<start>&<count>&<uid>", rank=10)]
async fn get_grid(
        path: PathBuf,
        start: Option<usize>, count: Option<usize>, uid: Option<UID>,
        gallery: &State<Gallery>,
        config: &State<Config>,
        cookies: &CookieJar<'_>,
        password: OptionalPassword
) -> PageResult {
    // Check if a password is required to access this path
    match gallery.check_password(&path, cookies, &password).await {
        // A password is required and is either missing or invalid
        Some(message) => PageResult::PasswordRequired(message),

        // Either no password is required or a valid one has been provided
        None => {
            // Try to obtain a read pointer to some photos in this path in the gallery based on the request parameters
            match gallery.read(&path, start, count, uid).await {

                // We have a valid (possibly empty) list of photos, render it as a template
                Some(gallery_lock) => {
                    // Convert the sublist of photos to a Vec with individual index and URLs
                    let photos = gallery_lock.as_slice().iter().enumerate()
                        .map(|(index, photo_pointer)| (
                            gallery_lock.start + index,
                            photo_pointer.deref(),
                            uri!(get_grid_item(&photo_pointer.uid)).to_string(),
                            uri!(get_thumbnail(&photo_pointer.uid)),
                            uri!(get_large(&photo_pointer.uid)),
                            uri!(get_photo(&photo_pointer.uid)),
                            uri!(download_photo(&photo_pointer.uid)),
                        ))
                        .collect::<Vec<_>>();

                    PageResult::Page(Template::render("grid", context! {
                        config: &config.inner(),
                        photos: &photos,
                        n_photos: gallery_lock.total,
                    }))
                }

                // The path is either not found or invalid for the current config, return an empty 404 response
                None => PageResult::NotFoundEmpty(()),
            }
        }
    }
}


/// Route handler called by javascript to return the nav menu for the given path
#[get("/<path..>?nav", rank=11)]
async fn get_nav(path: PathBuf, gallery: &State<Gallery>, config: &State<Config>, cookies: &CookieJar<'_>, password: OptionalPassword) -> PageResult {
    // Check if a password is required to access this path
    match gallery.check_password(&path, cookies, &password).await {
        // A password is required and is either missing or invalid
        Some(message) => PageResult::PasswordRequired(message),

        // Either no password is required or a valid one has been provided
        None => {
            match NavData::from_path(&path, &gallery, &config).await {
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
    }
}


/// Route handler called asynchronously to render a single photo inside the grid
#[get("/<uid>/grid-item", rank=2)]
async fn get_grid_item(uid: UID, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    match gallery.get_from_uid(&uid).await {
        Some(photo) => PageResult::Page(Template::render("grid-item", context! {
            config: &config.inner(),
            photo: photo,
            url_get_thumbnail: uri!(get_thumbnail(&uid)),
            url_get_large: uri!(get_large(&uid)),
            url_get_photo: uri!(get_photo(&uid)),
            url_download_photo:uri!(download_photo(&uid)),
        })),
        None => page_404(&config),
    }
}


/// Route handler that returns the thumbnail version of the requested UID
#[get("/<uid>/thumbnail", rank=3)]
async fn get_thumbnail(uid: UID, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    get_resized(&uid, photos::ResizedType::THUMBNAIL, &gallery, &config).await
}


/// Route handler that returns the large resized version of the requested UID
#[get("/<uid>/large", rank=4)]
async fn get_large(uid: UID, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    get_resized(&uid, photos::ResizedType::LARGE, &gallery, &config).await
}


/// Returns the resized version of the requested UID for the given prefix
async fn get_resized(uid: &UID, resized_type: photos::ResizedType, gallery: &Gallery, config: &Config) -> PageResult {
    match gallery.get_resized_from_uid(uid, resized_type, config).await {
        Ok(Some((photo, resized_file_path))) => {
            // Try to open the file
            match NamedFile::open(&resized_file_path).await {
                Ok(file) => PageResult::Photo(file),
                Err(error) => {
                    eprintln!("Error : unable to read or create cache file for \"{}\" at \"{}\" : {}", photo.full_path.display(), resized_file_path.display(), error);
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
#[get("/<uid>", rank=6)]
async fn get_photo(uid: UID, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    match gallery.get_from_uid(&uid).await {
        Some(photo) => {
            // Try to open the file
            match NamedFile::open(&photo.full_path).await {
                Ok(file) => PageResult::Photo(file),
                Err(error) => {
                    eprintln!("Error : unable to read file \"{}\" : {}", photo.full_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        None => page_404(&config),
    }
}


/// Route handler that returns the photo file for the requested UID as a download
#[get("/<uid>/download", rank=5)]
async fn download_photo(uid: UID, gallery: &State<Gallery>, config: &State<Config>) -> PageResult {
    match gallery.get_from_uid(&uid).await {
        Some(photo) => {
            // Try to open the file
            match DownloadedNamedFile::open(&photo.full_path, &photo.uid, &config).await {
                Ok(file) => PageResult::PhotoDownload(file),
                Err(error) => {
                    eprintln!("Error : unable to read file \"{}\" : {}", photo.full_path.display(), error);
                    PageResult::Err(())
                }
            }
        }
        None => page_404(&config),
    }
}


/// Route handler that reloads the photos from the filesystem and sync them with the database
/// TODO : add a cooldown timer to prevent DOS attempts through this computation-heavy endpoint
#[get("/.reload", rank=1)]
async fn reload(gallery: &State<Gallery>, config: &State<Config>, mut db_conn: Connection<DB>) -> Result<Redirect, ()> {
    gallery.reload(config, &mut db_conn).await.map_err(|_| ())?;
    Ok(Redirect::to(uri!(get_gallery(PathBuf::new()))))
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
    #[response(status = 404)]
    NotFoundEmpty(()),
    #[response(status = 401)]
    PasswordRequired(String),
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
    WebpEncoderError(String, PathBuf),
    EXIFParserError(exif::Error, PathBuf),
    OtherError(String),
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
            Error::WebpEncoderError(error, path) => write!(f, "WEPB encoder error for \"{}\" : {}", path.display(), error),
            Error::EXIFParserError(error, path) => write!(f, "EXIF parser error for \"{}\" : {}", path.display(), error),
            Error::OtherError(error) => write!(f, "other error : {}", error),
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