use std::{
    fmt::Debug,
    io::Write,
    path::{Path, PathBuf},
};

use git_fast_import::{Mark, Writer};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::{self, JoinHandle},
};

#[derive(Debug, Clone)]
pub(crate) struct Output {
    tx: UnboundedSender<Command>,
}

/// Constructs a new output object, and returns it along with a join handle that
/// must be awaited to ensure all output has been written.
pub(crate) fn new<P, W>(w: W, mark_file: Option<P>) -> (Output, JoinHandle<anyhow::Result<()>>)
where
    P: AsRef<Path>,
    W: Debug + Write + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let mark_file = mark_file.map(|path| path.as_ref().to_path_buf());

    (
        Output { tx },
        task::spawn(async move { worker(w, rx, mark_file).await }),
    )
}

impl Output {
    pub(crate) async fn blob(&self, blob: git_fast_import::Blob) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Blob(blob, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub(crate) async fn commit(&self, commit: git_fast_import::Commit) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Commit(commit, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub(crate) async fn tag(&self, tag: git_fast_import::Tag) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Tag(tag, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    // TODO: extend with other types we need to send.
}

async fn worker<W>(
    mut w: W,
    mut rx: UnboundedReceiver<Command>,
    mark_file: Option<PathBuf>,
) -> anyhow::Result<()>
where
    W: Debug + Write,
{
    let mut client = match &mark_file {
        Some(mark_file) => Writer::new_with_mark_file(&mut w, mark_file)?,
        None => Writer::new(&mut w)?,
    };
    let handle_send_result = |r| match r {
        Ok(_) => Ok(()),
        Err(mark) => Err(Error::MarkSendFailed(mark)),
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

    Ok(())
}

type MarkSender = oneshot::Sender<Mark>;

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

#[derive(Debug, Error)]
enum Error {
    #[error("cannot send mark back to caller: {0}")]
    MarkSendFailed(Mark),
}
