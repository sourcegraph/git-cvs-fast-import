use nom::{error::ErrorKind, Finish};
use thiserror::Error;

mod parser;
mod types;
pub use types::*;

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse error of kind {kind:?} at location {location:?}")]
    ParseError { location: Vec<u8>, kind: ErrorKind },
}

/// Parses a full RCS file.
pub fn parse(input: &[u8]) -> Result<File, Error> {
    Ok(Finish::finish(parser::file(input))
        .map_err(|e| Error::ParseError {
            location: Vec::from(e.input),
            kind: e.code,
        })?
        .1)
}
