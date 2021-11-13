use std::fmt::Debug;

use git_fast_import::Mark;
use thiserror::Error;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinError,
};

/// Possible errors from the `process` module.
#[derive(Debug, Error)]
pub enum Error {
    #[error("exit due to signal {0:?}")]
    ExitSignal(Option<i32>),

    #[error("exit code {0}")]
    ExitStatus(i32),

    #[error(transparent)]
    GitFastImport(#[from] git_fast_import::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Join(#[from] JoinError),

    #[error("cannot send mark back to caller: {0}")]
    MarkSend(Mark),

    #[error(transparent)]
    OneshotRecv(#[from] oneshot::error::RecvError),

    #[error("cannot establish a {pipe} pipe to git fast-import: {err:?}")]
    OutputPipeCreate { err: std::io::Error, pipe: String },

    #[error("cannot read from git fast-import output/error pipe: {0:?}")]
    OutputPipeRead(std::io::Error),

    #[error("channel send error: {0}")]
    Send(String),

    #[error("error spawning git fast-import: {0:?}")]
    Spawn(std::io::Error),

    #[error("cannot establish an input pipe to git fast-import")]
    StdinPipe,
}

impl Error {
    pub(crate) fn stderr_pipe(err: std::io::Error) -> Self {
        Self::OutputPipeCreate {
            err,
            pipe: String::from("stderr"),
        }
    }

    pub(crate) fn stdout_pipe(err: std::io::Error) -> Self {
        Self::OutputPipeCreate {
            err,
            pipe: String::from("stdout"),
        }
    }
}

impl<T: Debug> From<mpsc::error::SendError<T>> for Error {
    fn from(err: mpsc::error::SendError<T>) -> Self {
        Self::Send(format!("{:?}", err))
    }
}
