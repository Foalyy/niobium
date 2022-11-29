#[macro_use] extern crate rocket;

mod config;
mod nav_data;
mod photos;
mod db;

use config::Config;
use nav_data::NavData;
use std::{io, fmt::Display};
use std::path::PathBuf;
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
            PageResult::Ok(Template::render("main", context! {
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
async fn get_grid(path: PathBuf, start: Option<u32>, count: Option<u32>, uid: Option<String>, config: &State<Config>, db_conn: &State<Mutex<Connection>>) -> PageResult {
    // Try to load the photos in the given path
    match photos::load(&path, config, db_conn).await {

        // We have a valid (possibly empty) list of photos, render it as a template
        Ok(photos) => PageResult::Ok(Template::render("grid", context! {
            config: config.inner(),
            photos: photos,
        })),

        Err(error) => match error {
            Error::FileError(error, _) => match error.kind() {
                // The path is either not found or invalid for the current config, return the 404 template
                io::ErrorKind::NotFound => page_404(&config),

                // For any other error, forward the error to the 500 Internal Error catcher
                _ => PageResult::Err(())
            }
            _ => PageResult::Err(()),
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

/// Render the 404 page
fn page_404(config: &State<Config>) -> PageResult {
    PageResult::NotFound(Template::render("404", context! {
        config: config.inner(),
        url_gallery_root: uri!(get_gallery(""))
    }))
}


fn load_grid_url(path: &PathBuf) -> String {
    uri!(get_grid(path, None as Option<u32>, None as Option<u32>, None as Option<String>)).to_string()
}


/// Tri-state responder used by most routes
#[derive(Responder)]
pub enum PageResult {
    Ok(Template),
    #[response(status = 404)]
    NotFound(Template),
    #[response(status = 500)]
    Err(()),
}


/// Generic error type used to uniformize errors across the crate
#[derive(Debug)]
pub enum Error {
    FileError(io::Error, PathBuf),
    ParseError(toml::de::Error),
    DatabaseError(rusqlite::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::FileError(error, path) => write!(f, "file error for \"{}\" : {}", path.display(), error),
            Error::ParseError(error) => write!(f, "parser error : {}", error),
            Error::DatabaseError(error) => write!(f, "database error : {}", error),
        }
    }
}