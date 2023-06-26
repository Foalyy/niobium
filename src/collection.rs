use std::{
    collections::{hash_map, HashMap},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use regex::Regex;
use serde::{de::Unexpected, Deserialize};

use crate::{
    photos::{CachedPhoto, GalleryContent},
    Error,
};

/// A list of Collection's, as deserialized from the dedicated TOML config file
#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Collections {
    #[serde(default = "collections_default_empty_collections")]
    collections: Vec<Collection>,

    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl Collections {
    /// Create a new, empty collections list
    pub fn new() -> Self {
        Self {
            collections: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Get the collection with the given name, if any
    pub fn get(&self, name: &str) -> Option<&Collection> {
        self.collections.get(*self.index.get(name).unwrap())
    }

    /// Find a collection that relates to the given path (meaning that the path starts
    /// with the name of the collection), if any
    pub fn find(&self, path: &Path) -> (Option<&Collection>, Option<String>, String) {
        // Find the collection that this path refers to, if any
        let path_buf = path.to_path_buf();
        let first_dir = match path_buf.components().next() {
            Some(std::path::Component::Normal(dir)) => dir.to_string_lossy().to_string(),
            _ => return (None, None, path.to_string_lossy().to_string()),
        };
        let collection = self.get(&first_dir);

        // Name if this collection, if any
        let collection_name = collection.map(|c| c.name.clone());

        // Compute the path inside the gallery or the collection, as a string
        let path_str = match &collection_name {
            Some(name) => path
                .strip_prefix(name)
                .unwrap()
                .to_string_lossy()
                .to_string(),
            None => path.to_string_lossy().to_string(),
        };

        (collection, collection_name, path_str)
    }

    /// Clear the list of collections
    pub fn clear(&mut self) {
        self.collections.clear();
        self.index.clear();
    }

    /// Replace the content of this object with a new list of collections
    pub fn replace_with(&mut self, new: Collections) {
        self.collections = new.collections;
        self.compute_index();
    }

    /// Compute the index based on the current list of collections
    fn compute_index(&mut self) {
        self.index.clear();
        for (i, collection) in self.collections.iter().enumerate() {
            self.index.insert(collection.name.clone(), i);
        }
    }

    /// Read the collections definition file and deserialize it into a Config struct
    pub fn read_from<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let pathbuf = path.as_ref().to_path_buf();

        // Try to read the content of the file
        match fs::read_to_string(&path) {
            // File read successfully : try to parse it as TOML data
            Ok(file_content) => {
                let parsing_result: Result<Self, toml::de::Error> =
                    toml::from_str(file_content.as_str());
                match parsing_result {
                    Ok(collections) => Ok(collections),
                    Err(error) => Err(Error::TomlParserError(error)),
                }
            }

            // File not found : ignore and return an empty Collections
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self::new()),

            // Other file error
            Err(error) => Err(Error::FileError(error, pathbuf)),
        }
    }

    /// Try to read the collections definition file and deserialize it into a Config struct
    pub fn try_read_from<P>(path: P) -> Option<Self>
    where
        P: AsRef<Path>,
    {
        match Self::read_from(path) {
            Ok(collections) => Some(collections),
            Err(Error::TomlParserError(error)) => {
                eprintln!("Error: unable to parse the collections file : {error}");
                None
            }
            Err(Error::FileError(error, pathbuf)) => {
                eprintln!(
                    "Error: unable to read the collections file \"{}\" : {}",
                    pathbuf.display(),
                    error
                );
                None
            }
            _ => None,
        }
    }

    /// Fill the collections with the photos from the given gallery
    pub fn fill(&mut self, gallery: &GalleryContent) {
        for collection in &mut self.collections {
            collection.fill(gallery);
        }
    }
}

/// Default empty collections list when no `[[collections]]` object is defined in the config file
fn collections_default_empty_collections() -> Vec<Collection> {
    vec![]
}

/// A curated collection of photos from the global gallery
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Collection {
    #[serde(deserialize_with = "collection_name_deserialize")]
    pub name: String,
    pub title: Option<String>,
    pub dirs: Vec<CollectionDir>,

    #[serde(skip)]
    pub photos: GalleryContent,
}

impl Collection {
    /// Fill the collection with the photos from the given gallery that matches its requirements
    pub fn fill(&mut self, gallery: &GalleryContent) {
        for dir in &self.dirs {
            for (path, photos) in gallery {
                let path_buf = PathBuf::from(path);
                if dir.path_matches(&path_buf) {
                    let filtered_photos: Vec<CachedPhoto> = photos
                        .iter()
                        .filter(|&photo| dir.photo_matches(photo))
                        .map(|photo| CachedPhoto::clone_from(photo, vec![]))
                        .collect();
                    if !filtered_photos.is_empty() {
                        if let Ok(sub_path) = path_buf.strip_prefix(&dir.path) {
                            let sub_path_str = sub_path.to_string_lossy().to_string();
                            match self.photos.entry(sub_path_str) {
                                hash_map::Entry::Occupied(mut entry) => {
                                    // Merge the new photos into the existing list while avoiding duplicates
                                    let entry_photos = entry.get_mut();
                                    for photo in filtered_photos {
                                        if !entry_photos.iter().any(|p| p.uid == photo.uid) {
                                            entry_photos.push(photo);
                                        }
                                    }
                                }
                                hash_map::Entry::Vacant(entry) => {
                                    entry.insert(filtered_photos);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Deserialize the name of a collection and checks that its format is valid
fn collection_name_deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let name: String = serde::de::Deserialize::deserialize(deserializer)?;
    let validator = Regex::new("^[a-zA-Z0-9\\-_]+$").unwrap();
    if validator.is_match(&name) {
        Ok(name)
    } else {
        Err(serde::de::Error::invalid_value(
            Unexpected::Str(&name),
            &"a name only containing alphanumeric characters, dashes and underscores",
        ))
    }
}

/// A directory included in a collection with a regex filter
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct CollectionDir {
    pub path: PathBuf,

    #[serde(
        deserialize_with = "collection_filter_deserialize",
        default = "collection_dir_filter_default"
    )]
    pub filter: Option<CollectionFilter>,

    #[serde(default)]
    pub inverse_filter: bool,
}

impl CollectionDir {
    /// Check if the given path should be included in this collection
    pub fn path_matches(&self, path: &Path) -> bool {
        path.starts_with(&self.path)
    }

    /// Check if the given photo should be included in this collection
    pub fn photo_matches(&self, photo: &CachedPhoto) -> bool {
        let regex_matches = self
            .filter
            .as_ref()
            .map(|filter| filter.matches(photo))
            .unwrap_or(true);
        match self.inverse_filter {
            true => !regex_matches,
            false => regex_matches,
        }
    }
}

/// A filter applied to every file in a given dir before adding it to the collection
#[derive(Debug)]
pub struct CollectionFilter {
    regex: Regex,
}

impl CollectionFilter {
    /// Create a new filter with the given regex
    pub fn new(regex: Regex) -> Self {
        Self { regex }
    }

    /// Check if the given photo matches this filter
    pub fn matches(&self, photo: &CachedPhoto) -> bool {
        let photo_full_path = photo.path_with_filename().to_string_lossy().to_string();
        self.regex.is_match(&photo_full_path)
    }
}

/// Deserialize a Regex from a string in a serde struct
fn collection_filter_deserialize<'de, D>(
    deserializer: D,
) -> Result<Option<CollectionFilter>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let string: String = serde::de::Deserialize::deserialize(deserializer)?;
    match Regex::new(string.as_str()) {
        Ok(regex) => Ok(Some(CollectionFilter::new(regex))),
        Err(error) => {
            eprintln!("Error: unable to parse \"{string}\" as a regex : {error}");
            Err(serde::de::Error::invalid_type(
                Unexpected::Str(&string),
                &"a valid regex",
            ))
        }
    }
}

/// Default filter (none) when undefined
fn collection_dir_filter_default() -> Option<CollectionFilter> {
    None
}
