use std::{
    ffi::{OsStr, OsString},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub(crate) struct Commit {
    tx: mpsc::UnboundedSender<Message>,
}

#[derive(Debug)]
pub(crate) struct Worker {
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

pub(crate) fn new() -> (Commit, Worker) {
    let (tx, rx) = mpsc::unbounded_channel();

    (Commit { tx }, Worker { rx })
}

impl Commit {
    pub(crate) async fn observe(
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

impl Worker {
    pub(crate) async fn join(
        mut self,
        delta: Duration,
    ) -> anyhow::Result<impl Iterator<Item = PatchSet<Mark>>> {
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

        Ok(detector.into_patchset_iter())
    }
}
