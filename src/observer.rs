use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    sync::Arc,
    time::{Duration, SystemTime},
};

use binary_heap_plus::{BinaryHeap, MinComparator};
use comma_v::{Delta, DeltaText, Num, Sym};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{error::SendError, unbounded_channel, UnboundedSender},
        Mutex,
    },
    task::{self, JoinHandle},
};

use crate::state::{self, FileRevision, State};

#[derive(Clone, Debug)]
pub(crate) struct Observer {
    commit_tx: UnboundedSender<Commit>,
    state: State,
}

#[derive(Debug)]
pub(crate) struct Collector {
    join_handle: JoinHandle<Result<Detector<Mark>, Error>>,
    state: State,
}

#[derive(Debug)]
pub(crate) struct Commit {
    path: OsString,
    id: Option<Mark>,
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
        let (commit_tx, mut commit_rx) = unbounded_channel::<Commit>();

        let join_handle = task::spawn(async move {
            let mut detector = Detector::new(delta);

            while let Some(commit) = commit_rx.recv().await {
                detector.add_file_commit(
                    commit.path,
                    commit.id,
                    commit.branches,
                    commit.author,
                    commit.message,
                    commit.time,
                );
            }

            Ok::<Detector<Mark>, Error>(detector)
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
    ) -> Result<(), Error> {
        self.commit_tx.send(Commit {
            path: path.to_os_string(),
            id,
            branches: branches
                .iter()
                .map(|branch| branch.to_vec())
                .collect::<Vec<Vec<u8>>>(),
            author: String::from_utf8_lossy(&delta.author).into(),
            message: String::from_utf8_lossy(&text.log).into(),
            time: delta.date,
        })?;

        self.state
            .add_file_revision(
                FileRevision {
                    path: path.to_os_string(),
                    revision: revision.to_vec(),
                },
                id,
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn file_tags(&self, path: &OsStr, symbols: &HashMap<Sym, Num>) {
        for (tag, revision) in symbols {
            self.state
                .add_tag(
                    tag.to_vec(),
                    FileRevision {
                        path: path.to_os_string(),
                        revision: revision.to_vec(),
                    },
                )
                .await;
        }
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
    patchsets: Vec<PatchSet<Mark>>,
    state: State,
}

impl ObservationResult {
    pub(crate) fn patchset_iter(&self) -> impl Iterator<Item = &PatchSet<Mark>> {
        self.patchsets.iter()
    }
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Join(#[from] task::JoinError),

    #[error(transparent)]
    Send(#[from] SendError<Commit>),

    #[error(transparent)]
    State(#[from] state::Error),
}
