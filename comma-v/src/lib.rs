use nom::Finish;

mod error;
mod num;
mod parser;
mod types;

pub use error::Error;
pub use num::Num;
pub use types::*;

/// Parses a full RCS file.
pub fn parse(input: &[u8]) -> Result<File, Error> {
    Ok(Finish::finish(parser::file(input))
        .map_err(|e| Error::ParseError {
            location: Vec::from(e.input),
            kind: e.code,
        })?
        .1)
}
