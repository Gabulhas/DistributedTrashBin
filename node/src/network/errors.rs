use crate::network::jobs::JobState;
use serde::{Deserialize, Serialize};
use std::error;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoValueOrPointerFound {
    pub peer_id: String,
    pub key: Vec<u8>,
    pub source: Vec<u8>,
}

impl fmt::Display for NoValueOrPointerFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "No Value or Pointer {:?}", self)
    }
}

impl error::Error for NoValueOrPointerFound {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnexpectedRequest {
    pub key: Vec<u8>,
}

impl fmt::Display for UnexpectedRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UnexpectedRequest for key {:?}", self.key)
    }
}

impl error::Error for UnexpectedRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDoesNotExist {
    pub key: Vec<u8>,
}

impl fmt::Display for KeyDoesNotExist {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Key {:?} does not exist", self.key)
    }
}

impl error::Error for KeyDoesNotExist {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyAlreadyExists {
    pub key: Vec<u8>,
}

impl fmt::Display for KeyAlreadyExists {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Key {:?} already exists", self.key)
    }
}

impl error::Error for KeyAlreadyExists {}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Self::Waiting => "Waiting",
            Self::Finished => "Finished",
            Self::Failed(_) => "Failed",
        };

        write!(f, "{}", s)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DirectorySpecificErrors {
    NoValueOrPointerFound(NoValueOrPointerFound),
    UnexpectedRequest(UnexpectedRequest),
    KeyDoesNotExist(KeyDoesNotExist),
    KeyAlreadyExists(KeyAlreadyExists),
}

impl fmt::Display for DirectorySpecificErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DirectorySpecificErrors::NoValueOrPointerFound(err) => write!(f, "{}", err),
            DirectorySpecificErrors::UnexpectedRequest(err) => write!(f, "{}", err),
            DirectorySpecificErrors::KeyDoesNotExist(err) => write!(f, "{}", err),
            DirectorySpecificErrors::KeyAlreadyExists(err) => write!(f, "{}", err),
            // Handle other variants here as well
        }
    }
}
