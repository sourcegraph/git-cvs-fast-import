use std::{
    ffi::{OsStr, OsString},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};

#[derive(Clone, Debug)]
pub(crate) struct Observer {
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Debug)]
pub(crate) struct Collector {
    rx: mpsc::UnboundedReceiver<Message>,
}

#[derive(Debug)]
struct Message {
    path: OsString,
    id: Option<Mark>,
    // TODO: branches
    author: String,
    message: String,
    time: SystemTime,
}

/// Constructs a new commit observer, along with a collector that can be awaited
/// once the observer has been dropped to receive the final result of the
/// observations.
pub(crate) fn new(delta: Duration) -> (Observer, JoinHandle<anyhow::Result<Detector<Mark>>>) {
    let (tx, rx) = mpsc::unbounded_channel();

    (
        Observer { tx },
        task::spawn(async move { Collector { rx }.join(delta).await }),
    )
}

impl Observer {
    pub(crate) async fn commit(
        &self,
        path: &OsStr,
        id: Option<Mark>,
        delta: &Delta,
        text: &DeltaText,
    ) -> anyhow::Result<()> {
        Ok(self.tx.send(Message {
            path: OsString::from(path),
            id,
            author: String::from_utf8_lossy(&delta.author).into(),
            message: String::from_utf8_lossy(&text.log).into(),
            time: delta.date,
        })?)
    }
}

impl Collector {
    pub(crate) async fn join(mut self, delta: Duration) -> anyhow::Result<Detector<Mark>> {
        let mut detector = Detector::new(delta);

        while let Some(message) = self.rx.recv().await {
            detector.add_file_commit(
                &message.path,
                message.id.as_ref(),
                vec![String::from("HEAD")],
                &message.author,
                &message.message,
                message.time,
            );
        }

        Ok(detector)
    }
}
