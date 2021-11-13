use thiserror::Error;

use crate::{file_revision, patchset};

#[derive(Error, Debug)]
pub enum Error {
    #[error("error returned from callback: {0:?}")]
    Callback(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("error loading from store: {0}")]
    Load(String),

    #[error("no file revision exists for ID {0}")]
    NoFileRevisionForID(file_revision::ID),

    #[error("no file revision exists for key {0:?}")]
    NoFileRevisionForKey(file_revision::Key),

    #[error("no file revision exists for mark {0}")]
    NoFileRevisionForMark(file_revision::Mark),

    #[error("no patchset exists for mark {0}")]
    NoPatchSetForMark(patchset::Mark),

    #[error("tag {0} does not exist")]
    NoTag(String),

    #[error("serialisation error: {0:?}")]
    Serialisation(#[from] bincode::Error),

    #[error("speedy error: {0:?}")]
    Speedy(#[from] speedy::Error),

    #[error("unknown serialised data version: {0}")]
    UnknownSerialisationVersion(u8),
}
