use std::sync::Arc;

use git_fast_import::Mark;
use thiserror::Error;

use super::{FileRevisionID, FileRevisionKey};

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("duplicate file revision {mark} for {file_revision:?}")]
    DuplicateFileRevision {
        file_revision: Arc<FileRevisionKey>,
        mark: Mark,
    },

    #[error("no file revision exists for ID {0}")]
    NoFileRevisionForID(FileRevisionID),

    #[error("no file revision exists for key {0:?}")]
    NoFileRevisionForKey(FileRevisionKey),

    #[error("no file revision exists for mark {0}")]
    NoFileRevisionForMark(Mark),

    #[error("no mark exists for file revision {0:?}")]
    NoMark(FileRevisionKey),

    #[error("no patchset exists for mark {0}")]
    NoPatchSetForMark(Mark),

    #[error("tag {0} does not exist")]
    NoTag(String),

    #[error(transparent)]
    Store(#[from] git_cvs_fast_import_store::Error),
}

impl From<(Arc<FileRevisionKey>, Mark)> for Error {
    fn from((file_revision, mark): (Arc<FileRevisionKey>, Mark)) -> Self {
        Self::DuplicateFileRevision {
            file_revision,
            mark,
        }
    }
}
