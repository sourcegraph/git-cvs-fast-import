use git_fast_import::Mark;
use thiserror::Error;

use super::FileRevision;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("duplicate file revision {mark} for {file_revision:?}")]
    DuplicateFileRevision {
        file_revision: FileRevision,
        mark: Mark,
    },

    #[error("no file revision exists for mark {0}")]
    NoFileRevision(Mark),

    #[error("no mark exists for file revision {0:?}")]
    NoMark(FileRevision),

    #[error("tag {0} does not exist")]
    NoTag(String),
}

impl From<(FileRevision, Mark)> for Error {
    fn from((file_revision, mark): (FileRevision, Mark)) -> Self {
        Self::DuplicateFileRevision {
            file_revision,
            mark,
        }
    }
}
