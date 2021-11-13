//! `process` manages the `git fast-import` process, and provides methods to
//! send data to that process.

use std::{
    ffi::OsString,
    fmt::Debug,
    path::{Path, PathBuf},
};

use git_fast_import::{Mark, Writer};
use structopt::StructOpt;
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::{self, JoinHandle},
};

mod error;
mod preflight;
mod process;

pub use self::error::Error;
pub use self::preflight::preflight;

// Command line options that are required by the [`Output`] object.
//
// These should be injected into the global `StructOpt` implementation using the
// `flatten` attribute.
#[derive(Clone, Debug, StructOpt)]
pub struct Opt {
    #[structopt(
        long = "--git",
        default_value = "git",
        help = "path to the git command"
    )]
    git_command: OsString,

    #[structopt(
        long,
        help = "a fast-import specific Git option to add when invoking git fast-import"
    )]
    git_fast_import_option: Vec<String>,

    #[structopt(
        long,
        help = "a global Git option to add when invoking git fast-import"
    )]
    git_global_option: Vec<String>,

    #[structopt(short = "-g", long, help = "path to the Git repository to import into")]
    git_repo: OsString,
}

/// `Output` provides methods to send data to the `git fast-import` process.
#[derive(Debug, Clone)]
pub struct Output {
    tx: UnboundedSender<Command>,
}

/// Spawns a new `git fast-import` process, and returns an [`Output`] object
/// along with a [`Worker`] handle. The mark file will be imported if it exists,
/// and the marks will exported back to the same  mark file before [`Worker`]
/// completes.
///
/// Under the hood, this spawns a git fast-import process with the given options
/// and writes to it. It's important that the git process be managed by the
/// [`Output`] object (or, more specifically, the worker within it): we can't be
/// sure that the import proper and mark export are complete until the process
/// actually exits.
pub fn new<P>(mark_file_path: P, opt: &Opt) -> (Output, Worker)
where
    P: AsRef<Path>,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let mark_file = mark_file_path.as_ref().to_path_buf();
    let opt = opt.clone();

    (
        Output { tx },
        Worker {
            handle: task::spawn(async move { worker(opt, rx, mark_file).await }),
        },
    )
}

impl Output {
    pub async fn blob(&self, blob: git_fast_import::Blob) -> Result<Mark, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Blob(blob, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub async fn commit(&self, commit: git_fast_import::Commit) -> Result<Mark, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Commit(commit, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub async fn lightweight_tag(&self, name: &str, commit_mark: Mark) -> Result<(), Error> {
        Ok(self.tx.send(Command::Reset {
            branch_ref: format!("refs/tags/{}", name),
            from: Some(commit_mark),
        })?)
    }

    pub async fn tag(&self, tag: git_fast_import::Tag) -> Result<Mark, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Tag(tag, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    // TODO: extend with other types we need to send.
}

/// `Worker` provides a future that, when awaited, waits for the `git
/// fast-import` process to exit.
#[derive(Debug)]
pub struct Worker {
    handle: JoinHandle<Result<(), Error>>,
}

impl Worker {
    /// Wait until the `git fast-import` process is complete.
    ///
    /// All [`Output`] objects related to this [`Worker`] must be dropped before
    /// this will return.
    pub async fn wait(self) -> Result<(), Error> {
        self.handle.await?
    }
}

async fn worker(
    opt: Opt,
    mut rx: UnboundedReceiver<Command>,
    mark_file: PathBuf,
) -> Result<(), Error> {
    let process = process::Process::new(opt)?;

    let mut client = Writer::new(process.stdin(), mark_file)?;
    let handle_send_result = |r| match r {
        Ok(_) => Ok(()),
        Err(mark) => Err(Error::MarkSend(mark)),
    };

    while let Some(command) = rx.recv().await {
        match command {
            Command::Blob(blob, tx) => {
                handle_send_result(tx.send(client.command(blob)?))?;
            }
            Command::Checkpoint => {
                client.checkpoint()?;
            }
            Command::Commit(commit, tx) => {
                handle_send_result(tx.send(client.command(commit)?))?;
            }
            Command::Progress(message) => {
                client.progress(&message)?;
            }
            Command::Reset { branch_ref, from } => {
                client.reset(&branch_ref, from)?;
            }
            Command::Tag(tag, tx) => {
                handle_send_result(tx.send(client.command(tag)?))?;
            }
        }
    }

    // Destroy the client, which will send the done command, and then wait for
    // git to exit.
    drop(client);
    process.wait().await?;

    Ok(())
}

type MarkSender = oneshot::Sender<Mark>;

#[allow(dead_code)]
#[derive(Debug)]
enum Command {
    Blob(git_fast_import::Blob, MarkSender),
    Checkpoint,
    Commit(git_fast_import::Commit, MarkSender),
    Progress(String),
    Reset {
        branch_ref: String,
        from: Option<Mark>,
    },
    Tag(git_fast_import::Tag, MarkSender),
}
