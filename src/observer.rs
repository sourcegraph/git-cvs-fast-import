use std::{
    borrow::Borrow,
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText, Num, Sym};
use git_cvs_fast_import_state::{FileRevisionID, Manager};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{self, error::SendError, UnboundedSender},
        oneshot,
    },
    task::{self, JoinHandle},
};

/// An `Observer` receives a stream of file revisions and hands them to both the
/// patchset detector and the state manager.
#[derive(Clone, Debug)]
pub(crate) struct Observer {
    file_revision_tx: UnboundedSender<Message>,
    state: Manager,
}

/// A message sent to the observer worker.
///
/// This is public because it's exposed within the error type, but otherwise is
/// an implementation detail.
#[derive(Debug)]
pub(crate) struct Message {
    file_revision: FileRevision,
    id_tx: oneshot::Sender<FileRevisionID>,
}

/// A file revision sent to an observer worker.
///
/// This is public because it's exposed within the error type, but otherwise is
/// an implementation detail.
#[derive(Debug)]
pub(crate) struct FileRevision {
    path: PathBuf,
    revision: String,
    mark: Option<Mark>,
    branches: Vec<Vec<u8>>,
    author: String,
    message: String,
    time: SystemTime,
}

impl Observer {
    /// Constructs a new file revision observer, along with a collector that can
    /// be awaited once all observers have been dropped to receive the final
    /// result of the observations.
    pub(crate) fn new(delta: Duration, state: Manager) -> (Self, Collector) {
        let (file_revision_tx, mut file_revision_rx) = mpsc::unbounded_channel::<Message>();

        let task_state = state.clone();
        let join_handle = task::spawn(async move {
            let mut detectors = HashMap::new();

            while let Some(msg) = file_revision_rx.recv().await {
                let id = task_state
                    .add_file_revision(
                        msg.file_revision.path.as_path(),
                        &msg.file_revision.revision,
                        msg.file_revision.mark,
                        msg.file_revision.branches.iter(),
                        &msg.file_revision.author,
                        &msg.file_revision.message,
                        &msg.file_revision.time,
                    )
                    .await?;

                for branch in msg.file_revision.branches.iter() {
                    let detector = detectors
                        .entry(branch.clone())
                        .or_insert_with(|| Detector::new(delta));

                    detector.add_file_commit(
                        msg.file_revision.path.clone(),
                        id,
                        msg.file_revision.author.clone(),
                        msg.file_revision.message.clone(),
                        msg.file_revision.time,
                    );
                }

                msg.id_tx
                    .send(id)
                    .expect("cannot return file ID back to caller")
            }

            Ok::<HashMap<Vec<u8>, Detector<FileRevisionID>>, Error>(detectors)
        });

        (
            Self {
                file_revision_tx,
                state,
            },
            Collector { join_handle },
        )
    }

    /// Observe a single file revision, and return its ID as stored in the state
    /// manager.
    pub(crate) async fn file_revision<I>(
        &self,
        path: &Path,
        revision: &Num,
        branches: I,
        mark: Option<Mark>,
        delta: &Delta,
        text: &DeltaText,
    ) -> Result<FileRevisionID, Error>
    where
        I: Iterator,
        I::Item: Borrow<Sym>,
    {
        let (tx, rx) = oneshot::channel();

        self.file_revision_tx.send(Message {
            file_revision: FileRevision {
                path: path.to_path_buf(),
                revision: revision.to_string(),
                mark,
                branches: branches.map(|branch| branch.borrow().to_vec()).collect(),
                author: String::from_utf8_lossy(&delta.author).into_owned(),
                message: String::from_utf8_lossy(&text.log).into_owned(),
                time: delta.date,
            },
            id_tx: tx,
        })?;

        Ok(rx.await?)
    }

    /// Observe a single file revision tag.
    pub(crate) async fn tag(&self, tag: &Sym, file_revision_id: FileRevisionID) {
        self.state.add_tag(tag, file_revision_id).await;
    }
}

type BranchDetectorHashMap = HashMap<Vec<u8>, Detector<FileRevisionID>>;

/// The `Collector` is used to wait for all file revisions to be observed, and
/// then can be used to access the observation result.
#[derive(Debug)]
pub(crate) struct Collector {
    join_handle: JoinHandle<Result<BranchDetectorHashMap, Error>>,
}

/// An object that can be joined to wait for the results of the [`Observer`].
impl Collector {
    /// Waits for the observations to be complete, the results their results.
    pub(crate) async fn join(self) -> Result<ObservationResult, Error> {
        Ok(ObservationResult {
            branches: self
                .join_handle
                .await??
                .into_iter()
                .map(|(branch, detector)| (branch, detector.into_patchset_iter().collect()))
                .collect(),
        })
    }
}

/// The result of observing file revisions and tags with [`Observer`].
pub(crate) struct ObservationResult {
    branches: HashMap<Vec<u8>, Vec<PatchSet<FileRevisionID>>>,
}

impl ObservationResult {
    pub(crate) fn branch_iter(
        &self,
    ) -> impl Iterator<Item = (&Vec<u8>, &Vec<PatchSet<FileRevisionID>>)> {
        self.branches.iter()
    }
}

/// Errors that can be returned when observing.
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Join(#[from] task::JoinError),

    #[error(transparent)]
    OneshotRecv(#[from] oneshot::error::RecvError),

    #[error(transparent)]
    Send(#[from] SendError<Message>),

    #[error(transparent)]
    State(#[from] git_cvs_fast_import_state::Error),
}
