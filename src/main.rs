#[macro_use] extern crate rocket;

mod config;
mod nav_data;
mod photos;

use std::{io, fs};
use std::path::PathBuf;
use std::sync::Mutex;
use rocket::{fs::FileServer, State};
use rocket_dyn_templates::{Template, context};
use rusqlite::{Connection, OptionalExtension};
use config::{Config, uid_chars};
use nav_data::NavData;


#[derive(Responder)]
pub enum PageResult {
    Ok(Template),
    #[response(status = 404)]
    NotFound(Template),
    #[response(status = 500)]
    Err(io::Error),
}


#[launch]
fn rocket() -> _ {
    // Try to read the config file
    let config = read_config_or_exit();

    // Try to open a connection to the SQLite database
    let db = open_db_or_exit(&config);

    // Load the photos, or exit immediately in case of an error
    // Note : photos::load() will print the error message on stderr
    photos::load(&PathBuf::from(""), &config)
        .or_else(|_| -> io::Result<Vec<photos::Photo>> { std::process::exit(-1) }).unwrap();

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
        .manage(Mutex::new(db))
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
                uid_chars: uid_chars(),
                load_grid_url: load_grid_url(&path),
                load_nav_url: uri!(get_nav(&path)).to_string(),
            }))
        }

        Err(error) => match error.kind() {
            // The path is either not found or invalid for the current config, return the 404 template
            io::ErrorKind::NotFound => page_404(&config),
            
            // For any other error, forward the error to the 500 Internal Error catcher
            _ => PageResult::Err(error),
        }
    }
}


/// Route handler called by AJAX to return the grid items for the given path and parameters
#[get("/<path..>?grid&<start>&<count>&<uid>", rank=1)]
fn get_grid(path: PathBuf, start: Option<u32>, count: Option<u32>, uid: Option<String>, config: &State<Config>, db: &State<Mutex<Connection>>) -> PageResult {
    // Try to load the photos in the given path
    match photos::load(&path, &config) {

        // We have a valid (but possibly empty) list of photos, render it as a template
        Ok(photos) => PageResult::Ok(Template::render("grid", context! {
            config: config.inner(),
            photos: photos,
        })),

        Err(error) => match error.kind() {
            // The path is either not found or invalid for the current config, return the 404 template
            io::ErrorKind::NotFound => page_404(&config),

            // For any other error, forward the error to the 500 Internal Error catcher
            _ => PageResult::Err(error)
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

/// Try to read and parse the config file
/// In case of error, print it to stderr and exit with a status code of -1
fn read_config_or_exit() -> Config {
    // Read the config file and parse it into a Config struct
    Config::read()
        .unwrap_or_else(|e| match e {
            config::Error::FileError(e) => {
                eprintln!("Error, unable to open the config file \"{}\" : {}", config::FILENAME, e);
                std::process::exit(-1);
            }
            config::Error::ParseError(e) => {
                eprintln!("Error, unable to parse the config file \"{}\" : {}", config::FILENAME, e);
                std::process::exit(-1);
            }
        })
}


/// Try to open the sqlite database used to store the photos information
/// If it does not exist, try to create it and initialize it with the default schema
/// In case of error, print it to stderr and exit with a status code of -1
fn open_db_or_exit(config: &Config) -> Connection {
    // Try to open the database
    // If it doesn't exist, an empty one will be created thanks to the default SQLITE_OPEN_CREATE flag
    let db = Connection::open(&config.DATABASE_PATH).unwrap_or_else(|error| {
            eprintln!("Error, unable to open the database : {}", error);
            std::process::exit(-1);
    });

    // Check if the main 'photos' table exist, and if not, try to create it
    match db.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='photo';",
        [], |row| row.get::<_, String>(0)
    ).optional() {
        Ok(result) => result.unwrap_or_else(|| {
            // The main table doesn't exist, import the default schema to initialize the database
            print!("Database is empty, creating schema... ");
            let schema = fs::read_to_string("schema.sql").unwrap_or_else(|error| {
                println!("");
                eprintln!("Error, unable to open \"schema.sql\" : {}", error);
                std::process::exit(-1);
            });
            db.execute_batch(&schema).unwrap_or_else(|error| {
                println!("");
                eprintln!("Error, unable to open \"schema.sql\" : {}", error);
                std::process::exit(-1);
            });
            println!("ok");
            String::new()
        }),
        Err(error) => {
            eprintln!("Error, unable to read from the database : {}", error);
            std::process::exit(-1);
        }
    };

    db
}
