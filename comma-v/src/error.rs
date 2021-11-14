use std::{num::ParseIntError, str::Utf8Error};

use nom::error::ErrorKind;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a branch")]
    NotBranch,

    #[error("parse error of kind {kind:?} at location {location:?}")]
    ParseError { location: Vec<u8>, kind: ErrorKind },

    #[error(transparent)]
    ParseInt(#[from] ParseIntError),

    #[error(transparent)]
    ParseUtf8(#[from] Utf8Error),
}
