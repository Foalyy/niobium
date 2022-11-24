#[macro_use] extern crate rocket;

mod config;
mod nav_data;
mod photo;

use std::{path::PathBuf, fs};
use rocket::{fs::FileServer, State};
use rocket_dyn_templates::{Template, context};
use config::{Config, uid_chars};
use nav_data::NavData;
use photo::Photo;


#[launch]
fn rocket() -> _ {
    // Read the config file and parse it into a Config struct
    let config: Config = toml::from_str(fs::read_to_string("niobium.config").unwrap().as_str()).unwrap();

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
}



#[get("/<path..>", rank=5)]
fn get_gallery(path: PathBuf, config: &State<Config>) -> Template {
    let nav_data = NavData::from_path(&path);
    Template::render("main", context! {
        config: config.inner(),
        nav: nav_data,
        uid_chars: uid_chars(),
        load_grid_url: load_grid_url(&path),
        load_nav_url: uri!(get_nav(&path)).to_string(),
    })
}

#[get("/<path..>?grid&<start>&<count>&<uid>", rank=1)]
fn get_grid(path: PathBuf, start: Option<u32>, count: Option<u32>, uid: Option<String>, config: &State<Config>) -> Template {
    let photos: Vec<Photo> = vec![];
    Template::render("grid", context! {
        config: config.inner(),
        photos: photos,
    })
}

#[get("/<path..>?nav", rank=2)]
fn get_nav(path: PathBuf, config: &State<Config>) -> () {
    let nav_data = NavData::from_path(&path);
    //Template::render("nav", context! {
    //    config: config.inner(),
    //})
}


fn load_grid_url(path: &PathBuf) -> String {
    uri!(get_grid(path, None as Option<u32>, None as Option<u32>, None as Option<String>)).to_string()
}