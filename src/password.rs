use base64::{prelude::BASE64_STANDARD, Engine};
use rocket::{
    http::Status,
    request::{self, FromRequest},
};
use std::{collections::HashMap, fmt::Display};

pub type Passwords = HashMap<String, String>;

/// Custom type used as a request guard which represents a password optionally
/// sent by the client with an Authorization header. This request guard never
/// forwards : it succeeds even if no header is provided (in which case it simply
/// stores None), and fails if the provided header is not a valid base64-encoded
/// UTF8 string.
pub struct OptionalPassword(Option<String>);

impl OptionalPassword {
    pub fn none() -> Self {
        Self(None)
    }

    /// Return a new OptionalPassword from the given base64-encoded string
    fn from_base64(encoded: String) -> Result<Self, PasswordDecodeError> {
        let decoded_buffer = BASE64_STANDARD.decode(encoded)?;
        let password = String::from_utf8(decoded_buffer)?;
        Ok(Self(Some(password)))
    }

    /// Return the internal optional string
    pub fn as_string(&self) -> &Option<String> {
        &self.0
    }
}

/// Implementation that tries to return an OptionalPassword from a Request, allowing
/// this type to be used as a request guard
#[rocket::async_trait]
impl<'r> FromRequest<'r> for OptionalPassword {
    type Error = PasswordDecodeError;

    async fn from_request(request: &'r rocket::Request<'_>) -> request::Outcome<Self, Self::Error> {
        match request
            .headers()
            .get_one(rocket::http::hyper::header::AUTHORIZATION.as_str())
            .map(|v| v.to_string())
        {
            Some(header) => match OptionalPassword::from_base64(header) {
                Ok(password) => request::Outcome::Success(password),
                Err(error) => {
                    eprintln!("Warning : a client sent an invalid Authorization header : {error}");
                    request::Outcome::Error((Status::BadGateway, error))
                }
            },
            None => request::Outcome::Success(OptionalPassword(None)),
        }
    }
}

/// Possible errors that can happen when decoding a password from a header
#[derive(Debug)]
pub enum PasswordDecodeError {
    Base64DecodeError(base64::DecodeError),
    Utf8DecodeError(std::string::FromUtf8Error),
}

/// Create a Base64DecodeError variant from a base64::DecodeError, intended
/// to use for automatic transtyping
impl From<base64::DecodeError> for PasswordDecodeError {
    fn from(error: base64::DecodeError) -> Self {
        PasswordDecodeError::Base64DecodeError(error)
    }
}

/// Create a Utf8DecodeError variant from a std::string::FromUtf8Error, intended
/// to use for automatic transtyping
impl From<std::string::FromUtf8Error> for PasswordDecodeError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        PasswordDecodeError::Utf8DecodeError(error)
    }
}

impl Display for PasswordDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid header")
    }
}

/// Kinds of error when checking if a password
pub enum PasswordError {
    Required(String),
    Invalid(String),
}

impl PasswordError {
    pub fn message(&self) -> String {
        match self {
            PasswordError::Required(path) => {
                if path.is_empty() {
                    "A password is required to access this gallery".to_string()
                } else {
                    format!("A password is required to access \"{path}\"")
                }
            }
            PasswordError::Invalid(path) => {
                if path.is_empty() {
                    "Invalid password".to_string()
                } else {
                    format!("Invalid password for \"{path}\"")
                }
            }
        }
    }
}

pub fn cookie_name(path: &str) -> String {
    format!("niobium_pw_{path}")
}
