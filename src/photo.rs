use std::path::PathBuf;

use rocket::serde::Serialize;

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

impl Photo {
    pub fn new() -> Self {
        Default::default()
    }
}
