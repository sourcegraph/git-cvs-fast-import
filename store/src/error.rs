use std::{io, sync::mpsc};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(
        "mark file is too large to write to the database: {size} bytes; maximum is {max} bytes"
    )]
    LargeMarkFile { max: i32, size: usize },

    #[error(transparent)]
    Refinery(#[from] refinery::Error),

    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),

    #[error("channel send error: {0}")]
    Send(String),
}

impl<T> From<mpsc::SendError<T>> for Error {
    fn from(err: mpsc::SendError<T>) -> Self {
        Self::Send(err.to_string())
    }
}
