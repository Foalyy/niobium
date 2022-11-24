use std::path::PathBuf;
use rocket::serde::Serialize;

#[derive(Serialize, Debug)]
pub struct NavData {
    is_root: bool,
    path_current: PathBuf,
    path_parent: PathBuf,
    path_split: Vec<String>,
    subdirs: Vec<String>,
    locked_subdirs: Vec<String>,
}

impl NavData {
    pub fn from_path(path: &PathBuf) -> Self {
        let parent = path.parent();
        println!("{:?} {:?}", path, parent);
        Self {
            is_root: parent == None,
            path_current: PathBuf::from(path),
            path_parent: parent.unwrap_or(path.as_path()).to_path_buf(),
            path_split: path.components()
                .map(|c| String::from(c.as_os_str().to_str().unwrap_or("")))
                .filter(|c| c != "")
                .collect(),
            subdirs: vec![],
            locked_subdirs: vec![],
        }
    }
}
