use rand::seq::SliceRandom;
use rand::thread_rng;
use rocket::http::{impl_from_uri_param_identity, RawStr};
use rocket::http::uri::fmt::UriDisplay;
use rocket::form::{self, FromFormField, DataField, ValueField};
use rocket::request::FromParam;
use rocket::serde::Serialize;


#[derive(Default, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UID {
    uid: String,
}

impl UID {
    pub const LENGTH: usize = 10;

    // List of chars used when building an UID (biased)
    pub const CHARS_BIASED: [(char, u32); 36] = [
        ('0', 4), ('1', 4), ('2', 4), ('3', 4), ('4', 4), ('5', 4), ('6', 4), ('7', 4), ('8', 4), ('9', 4),
        ('a', 1), ('b', 1), ('c', 1), ('d', 1), ('e', 1), ('f', 1), ('g', 1), ('h', 1), ('i', 1), ('j', 1), ('k', 1), ('l', 1), ('m', 1),
        ('n', 1), ('o', 1), ('p', 1), ('q', 1), ('r', 1), ('s', 1), ('t', 1), ('u', 1), ('v', 1), ('w', 1), ('x', 1), ('y', 1), ('z', 1)
    ];

    // List of chars used when building an UID (set)
    pub const CHARS: &str = "0123456789abcdefghijklmnopqrstuvwxyz";

    /// Generate an UID of the given length that doesn't already exist in the given list
    pub fn new(existing_uids: &Vec<Self>) -> Self {
        let mut rng = thread_rng();
        loop {
            let uid_string = Self::CHARS_BIASED.choose_multiple_weighted(&mut rng, Self::LENGTH as usize, |item| item.1).unwrap()
                .map(|item| String::from(item.0))
                .collect::<Vec<String>>()
                .join("");
            let uid = Self::try_from(&uid_string).unwrap();
            if !existing_uids.contains(&uid) {
                break uid;
            }
        }
    }
}

/// Display this UID as a String
impl std::fmt::Display for UID {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.uid, f)
    }
}

/// Try to parse a valid UID from the given string slice
impl TryFrom<&str> for UID {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() == Self::LENGTH {
            let value_string = String::from(value);
            let chars = Self::CHARS_BIASED.map(|t| t.0);
            for c in value_string.chars() {
                if !chars.contains(&c) {
                    return Err("Invalid char");
                }
            }
            Ok(Self { uid: value_string })
        } else {
            Err("Invalid UID length")
        }
    }
}

/// Try to parse a valid UID from the given String
impl TryFrom<&String> for UID {
    type Error = &'static str;

    #[inline]
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

/// Try to parse a valid UID from the given route parameter, based on the TryFrom<&str> impl
impl<'r> FromParam<'r> for UID {
    type Error = &'r str;

    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        let mut s = param;
        if &s[0..=0] == "." {
            s = &s[1..];
            Self::try_from(s).or_else(|_| return Err(param))
        } else {
            return Err(param)
        }
    }
}

#[rocket::async_trait]
impl<'r> FromFormField<'r> for UID {
    fn from_value(field: ValueField<'r>) -> form::Result<'r, Self> {
        Ok(UID::try_from(field.value).map_err(|_| field.unexpected())?)
    }

    async fn from_data(field: DataField<'r, '_>) -> form::Result<'r, Self> {
        Err(field.unexpected())?
    }
}

/// Format a UID to be used a part of a URI's path
impl UriDisplay<rocket::http::uri::fmt::Path> for UID {
    fn fmt(&self, f: &mut rocket::http::uri::fmt::Formatter<'_, rocket::http::uri::fmt::Path>) -> std::fmt::Result {
        f.write_raw(".")?;
        f.write_raw(RawStr::new(&self.uid).percent_encode().as_str())?;
        Ok(())
    }
}

/// Format a UID to be used a part of a URI's query parameters
impl UriDisplay<rocket::http::uri::fmt::Query> for UID {
    fn fmt(&self, f: &mut rocket::http::uri::fmt::Formatter<'_, rocket::http::uri::fmt::Query>) -> std::fmt::Result {
        f.write_raw(RawStr::new(&self.uid).percent_encode().as_str())?;
        Ok(())
    }
}

// Macros used to automatically implement the FromUriParam trait based on the UriDisplay impls above
impl_from_uri_param_identity!([rocket::http::uri::fmt::Path] UID);
impl_from_uri_param_identity!([rocket::http::uri::fmt::Query] UID);
