use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use regex::Regex;
use serde::{de::Unexpected, Deserialize};

use crate::Error;

/// A list of Collection's, as deserialized from the dedicated TOML config file
#[derive(Deserialize, Debug)]
pub struct Collections {
    #[serde(default = "collections_default_empty_collections")]
    collections: Vec<Collection>,
}

impl Collections {
    /// Create a new, empty collections list
    pub fn new() -> Self {
        Self {
            collections: vec![],
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
    /// In case of error, print it to stderr and exit with a status code of -1
    pub fn read_from_or_exit<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        match Self::read_from(path) {
            Ok(collections) => collections,
            Err(Error::TomlParserError(error)) => {
                eprintln!("Error: unable to parse the collections file : {error}");
                std::process::exit(-1);
            }
            Err(Error::FileError(error, pathbuf)) => {
                eprintln!(
                    "Error: unable to read the collections file \"{}\" : {}",
                    pathbuf.display(),
                    error
                );
                std::process::exit(-1);
            }
            _ => std::process::exit(-1),
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
pub struct Collection {
    NAME: String,
    DIRS: Vec<CollectionDir>,
}

/// A directory included in a collection with a regex filter
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
pub struct CollectionDir {
    PATH: PathBuf,

    #[serde(
        deserialize_with = "collection_filter_deserialize",
        default = "collection_dir_filter_default"
    )]
    FILTER: Option<CollectionFilter>,
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
                Unexpected::Str(string.as_str()),
                &"a valid regex",
            ))
        }
    }
}

/// Default filter (none) when undefined
fn collection_dir_filter_default() -> Option<CollectionFilter> {
    None
}
