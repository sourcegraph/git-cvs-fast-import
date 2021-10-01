use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("mark parsing error: {0:?}")]
    MarkParsingError(nom::error::ErrorKind),

    #[error("a committer must be provided")]
    MissingCommitter,

    #[error("a commit message must be provided")]
    MissingCommitMessage,
}
