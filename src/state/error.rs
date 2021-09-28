use std::sync::Arc;

use git_fast_import::Mark;
use thiserror::Error;

use super::{FileID, FileRevision};

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("duplicate file revision {mark} for {file_revision:?}")]
    DuplicateFileRevision {
        file_revision: Arc<FileRevision>,
        mark: Mark,
    },

    #[error("no file revision exists for ID {0}")]
    NoFileRevisionForID(FileID),

    #[error("no file revision exists for mark {0}")]
    NoFileRevisionForMark(Mark),

    #[error("no mark exists for file revision {0:?}")]
    NoMark(FileRevision),

    #[error("tag {0} does not exist")]
    NoTag(String),

    #[error(transparent)]
    Store(#[from] git_cvs_fast_import_store::Error),
}

impl From<(Arc<FileRevision>, Mark)> for Error {
    fn from((file_revision, mark): (Arc<FileRevision>, Mark)) -> Self {
        Self::DuplicateFileRevision {
            file_revision,
            mark,
        }
    }
}
