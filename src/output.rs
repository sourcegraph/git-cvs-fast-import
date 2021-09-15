use std::{
    fmt::Debug,
    io::Write,
    path::{Path, PathBuf},
};

use git_fast_import::{Mark, Writer};
use thiserror::Error;
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot,
};

#[derive(Debug, Clone)]
pub(crate) struct Output {
    tx: UnboundedSender<Command>,
}

#[derive(Debug)]
pub(crate) struct Worker<W>
where
    W: Debug + Write + Send,
{
    w: W,
    rx: UnboundedReceiver<Command>,
    mark_file: Option<PathBuf>,
}

pub(crate) fn new<P, W>(w: W, mark_file: Option<P>) -> (Output, Worker<W>)
where
    P: AsRef<Path>,
    W: Debug + Write + Send,
{
    let (tx, rx) = unbounded_channel();

    (
        Output { tx },
        Worker {
            w,
            rx,
            mark_file: mark_file.map(|path| PathBuf::from(path.as_ref())),
        },
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
}

impl<W> Worker<W>
where
    W: Debug + Write + Send,
{
    pub(crate) async fn join(&mut self) -> anyhow::Result<()> {
        let mut client = match &self.mark_file {
            Some(mark_file) => Writer::new_with_mark_file(&mut self.w, mark_file)?,
            None => Writer::new(&mut self.w)?,
        };
        let handle_send_result = |r| match r {
            Ok(_) => Ok(()),
            Err(mark) => Err(Error::MarkSendFailed(mark)),
        };

        while let Some(command) = self.rx.recv().await {
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
