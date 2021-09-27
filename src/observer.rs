use std::{collections::HashMap, ffi::OsStr, sync::Arc, time::Duration};

use binary_heap_plus::{BinaryHeap, MinComparator};
use comma_v::{Delta, DeltaText, Num, Sym};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::state::{FileRevision, State};

#[derive(Clone, Debug)]
pub(crate) struct Observer {
    detector: Arc<Mutex<MaybeDetector>>,
    state: State,
}

#[derive(Debug)]
struct MaybeDetector(Option<Detector<Mark>>);

impl MaybeDetector {
    fn new(delta: Duration) -> Self {
        Self(Some(Detector::new(delta)))
    }

    fn detector(&mut self) -> Result<&mut Detector<Mark>, Error> {
        match self.0.as_mut() {
            Some(detector) => Ok(detector),
            None => Err(Error::NoDetector),
        }
    }
}

impl Observer {
    /// Constructs a new commit observer, along with a collector that can be
    /// awaited once the observer has been dropped to receive the final result
    /// of the observations.
    pub(crate) fn new(delta: Duration, state: State) -> Self {
        Self {
            detector: Arc::new(Mutex::new(MaybeDetector::new(delta))),
            state,
        }
    }

    pub(crate) async fn commit(
        &self,
        path: &OsStr,
        revision: &Num,
        branches: &[Num],
        id: Option<Mark>,
        delta: &Delta,
        text: &DeltaText,
    ) -> anyhow::Result<()> {
        self.detector.lock().await.detector()?.add_file_commit(
            path.to_os_string(),
            id,
            branches
                .iter()
                .map(|branch| branch.to_vec())
                .collect::<Vec<Vec<u8>>>(),
            String::from_utf8_lossy(&delta.author).into(),
            String::from_utf8_lossy(&text.log).into(),
            delta.date,
        );

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

    pub(crate) async fn into_observation_result(self) -> anyhow::Result<ObservationResult> {
        let detector = match self.detector.lock().await.0.take() {
            Some(detector) => detector,
            None => return Err(anyhow::format_err!("no detectorino")),
        };

        Ok(ObservationResult {
            patchsets: detector.into_binary_heap(),
            state: self.state,
        })
    }
}

pub(crate) struct ObservationResult {
    patchsets: BinaryHeap<PatchSet<Mark>, MinComparator>,
    state: State,
}

impl ObservationResult {
    pub(crate) fn patchset_iter(&self) -> impl Iterator<Item = &PatchSet<Mark>> {
        self.patchsets.iter()
    }
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("no detector is available in the observer; this likely indicates that into_observation_result() was invoked before all other instances of Detector were dropped")]
    NoDetector,
}
