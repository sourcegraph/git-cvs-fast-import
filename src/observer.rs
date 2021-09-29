use std::{
    ffi::{OsStr, OsString},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText, Num, Sym};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{error::SendError, unbounded_channel, UnboundedSender},
        oneshot,
    },
    task::{self, JoinHandle},
};

use crate::state::{self, FileID, FileRevision, State};

#[derive(Clone, Debug)]
pub(crate) struct Observer {
    commit_tx: UnboundedSender<CommitMessage>,
    state: State,
}

#[derive(Debug)]
pub(crate) struct CommitMessage {
    commit: Commit,
    id_tx: oneshot::Sender<FileID>,
}

#[derive(Debug)]
pub(crate) struct Collector {
    join_handle: JoinHandle<Result<Detector<usize>, Error>>,
    state: State,
}

#[derive(Debug)]
pub(crate) struct Commit {
    path: OsString,
    revision: Vec<u8>,
    mark: Option<Mark>,
    branches: Vec<Vec<u8>>,
    author: String,
    message: String,
    time: SystemTime,
}

impl Observer {
    /// Constructs a new commit observer, along with a collector that can be
    /// awaited once all observers have been dropped to receive the final result
    /// of the observations.
    pub(crate) fn new(delta: Duration, state: State) -> (Self, Collector) {
        let (commit_tx, mut commit_rx) = unbounded_channel::<CommitMessage>();

        let task_state = state.clone();
        let join_handle = task::spawn(async move {
            let mut detector = Detector::new(delta);

            while let Some(msg) = commit_rx.recv().await {
                let id = task_state
                    .add_file_revision(
                        FileRevision {
                            path: msg.commit.path.clone(),
                            revision: msg.commit.revision,
                        },
                        state::Commit {
                            branches: msg.commit.branches.clone(),
                            author: msg.commit.author.clone(),
                            message: msg.commit.message.clone(),
                            time: msg.commit.time,
                        },
                        msg.commit.mark,
                    )
                    .await?;

                detector.add_file_commit(
                    msg.commit.path,
                    id,
                    msg.commit.branches,
                    msg.commit.author,
                    msg.commit.message,
                    msg.commit.time,
                );

                msg.id_tx
                    .send(id)
                    .expect("cannot return file ID back to caller")
            }

            Ok::<Detector<usize>, Error>(detector)
        });

        (
            Self {
                commit_tx,
                state: state.clone(),
            },
            Collector { join_handle, state },
        )
    }

    pub(crate) async fn commit(
        &self,
        path: &OsStr,
        revision: &Num,
        branches: &[Num],
        id: Option<Mark>,
        delta: &Delta,
        text: &DeltaText,
    ) -> Result<FileID, Error> {
        let (tx, rx) = oneshot::channel();

        self.commit_tx.send(CommitMessage {
            commit: Commit {
                path: path.to_os_string(),
                revision: revision.to_vec(),
                mark: id,
                branches: branches.iter().map(|branch| branch.to_vec()).collect(),
                author: String::from_utf8_lossy(&delta.author).into_owned(),
                message: String::from_utf8_lossy(&text.log).into_owned(),
                time: delta.date,
            },
            id_tx: tx,
        })?;

        Ok(rx.await?)
    }

    pub(crate) async fn tag(&self, tag: &Sym, file_id: FileID) {
        self.state.add_tag(tag.to_vec(), file_id).await;
    }
}

impl Collector {
    pub(crate) async fn join(self) -> Result<ObservationResult, Error> {
        Ok(ObservationResult {
            patchsets: self.join_handle.await??.into_patchset_iter().collect(),
            state: self.state,
        })
    }
}

pub(crate) struct ObservationResult {
    patchsets: Vec<PatchSet<usize>>,
    state: State,
}

impl ObservationResult {
    pub(crate) fn patchset_iter(&self) -> impl Iterator<Item = &PatchSet<usize>> {
        self.patchsets.iter()
    }
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Join(#[from] task::JoinError),

    #[error(transparent)]
    OneshotRecv(#[from] oneshot::error::RecvError),

    #[error(transparent)]
    Send(#[from] SendError<CommitMessage>),

    #[error(transparent)]
    State(#[from] state::Error),
}
