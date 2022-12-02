use std::path::{PathBuf, Component};
use rocket::serde::Serialize;

use crate::{photos, config::Config, Error};

/// Data used to fill the template for the navigation panel
#[derive(Serialize, Debug)]
pub struct NavData {
    is_root: bool,
    url_path_root: String,
    current: String,
    current_open: String,
    path_current: PathBuf,
    url_path_current: String,
    parent: String,
    path_parent: PathBuf,
    url_path_parent: String,
    url_navigate_up: String,
    path_split: Vec<String>,
    path_split_open: Vec<String>,
    path_split_open_with_urls: Vec<(String, String)>,
    subdirs: Vec<String>,
    subdirs_with_urls: Vec<(String, String)>,
    open_subdir: Option<String>,
    locked_subdirs: Vec<String>,
}


trait Split {
    fn split(&self) -> Vec<String>;
}

impl Split for PathBuf {
    fn split(&self) -> Vec<String> {
        self.components()
            .map(|c|
                if let Component::Normal(c) = c {
                    c.to_str().unwrap().to_string()
                } else {
                    "".to_string()
                }
            )
            .filter(|c| c != "")
            .collect()
    }
}


impl NavData {

    /// Generate a full NavaData struct based on the given path and config
    pub async fn from_path(path: &PathBuf, config: &Config) -> Result<Self, Error> {
        let mut path_current = path.clone();
        let mut path_parent = path_current.parent().map(|p| p.to_path_buf());
        let path_navigate_up = path_parent.clone().map(|p| p.to_path_buf());
        let mut is_root = path_parent.is_none();
        let mut path_split = path_current.split();
        let path_split_open = path_split.clone();
        let mut current = path_split.last().map(|s| s.clone()).unwrap_or("/".to_string());
        let current_open = current.clone();
        let mut subdirs = photos::list_subdirs(&path_current, &config.PHOTOS_DIR, false, true).await?;
        let mut open_subdir: Option<String> = None;

        // If this directory doesn't contain subdirectories, keep showing its parent instead and simply mark it as 'open'
        let keep_parent_open = !is_root && subdirs.is_empty();
        if keep_parent_open {
            open_subdir = path_split.pop().map(|s| s.to_owned());
            current = path_split.last().map(|s| s.clone()).unwrap_or("/".to_string());
            path_current = path_parent.unwrap().to_path_buf();
            path_parent = path_current.parent().map(|p| p.to_path_buf());
            is_root = path_parent.is_none();
            subdirs = photos::list_subdirs(&path_current, &config.PHOTOS_DIR, false, true).await?;
            if !subdirs.contains(&open_subdir.as_ref().unwrap()) {
                subdirs.push(open_subdir.as_ref().unwrap().clone());
            }
        };
        let parent = if path_split.len() >= 2 {
            path_split.get(path_split.len() - 2).map(|s| s.clone()).unwrap_or("/".to_string())
        } else {
            "/".to_string()
        };
        let path_parent = path_parent.map(|p| p.to_path_buf()).unwrap_or(PathBuf::new());

        // Generate URIs for every subdirs
        let subdirs_with_urls = subdirs.iter()
            .map(|s| {
                let mut subdir_path = PathBuf::from(&path_current);
                subdir_path.push(&s);
                (s.clone(), uri!(crate::get_gallery(&subdir_path)).to_string())
            })
            .collect();

        // Generate URIs for the breadcrumbs at the top of the panel
        let mut subdir_path = PathBuf::from("/");
        let path_split_open_with_urls = path_split_open.iter()
            .map(|s| {
                subdir_path.push(&s);
                (s.clone(), uri!(crate::get_gallery(&subdir_path)).to_string())
            })
            .collect();

        // Generate URI for the Navigate Up button
        let url_navigate_up = path_navigate_up
            .map(|p|
                uri!(crate::get_gallery(PathBuf::from(&p))).to_string()
            )
            .unwrap_or("".to_string());

        // Check which subdirs are locked
        let mut locked_subdirs: Vec<String> = Vec::new();
        for subdir in &subdirs {
            let mut subdir_config_table = toml::value::Table::new();
            let mut subdir_path = PathBuf::from(&config.PHOTOS_DIR);
            subdir_path.push(&path_current);
            subdir_path.push(&subdir);
            Config::update_with_subdir(&subdir_path, &mut subdir_config_table);
            if subdir_config_table.contains_key("PASSWORD") {
                locked_subdirs.push(subdir.clone());
            }
        }

        Ok(Self {
            is_root,
            url_path_root: uri!(crate::get_gallery(PathBuf::from("/"))).to_string(),
            current,
            current_open,
            path_current: path_current.clone(),
            url_path_current: uri!(crate::get_gallery(PathBuf::from(&path_current))).to_string(),
            parent,
            path_parent: path_parent.clone(),
            url_path_parent: uri!(crate::get_gallery(PathBuf::from(&path_parent))).to_string(),
            url_navigate_up,
            path_split,
            path_split_open,
            path_split_open_with_urls,
            subdirs,
            subdirs_with_urls,
            open_subdir,
            locked_subdirs,
        })
    }

}
