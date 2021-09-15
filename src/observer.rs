use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText, Num, Sym};
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
enum Message {
    Commit(Commit),
    FileTags(FileTags),
}

#[derive(Debug)]
struct Commit {
    path: OsString,
    id: Option<Mark>,
    // TODO: branches
    author: String,
    message: String,
    time: SystemTime,
}

#[derive(Debug)]
struct FileTags {
    path: OsString,
    symbols: HashMap<Sym, Num>,
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
    pub(crate) fn commit(
        &self,
        path: &OsStr,
        id: Option<Mark>,
        delta: &Delta,
        text: &DeltaText,
    ) -> anyhow::Result<()> {
        Ok(self.tx.send(Message::Commit(Commit {
            path: OsString::from(path),
            id,
            author: String::from_utf8_lossy(&delta.author).into(),
            message: String::from_utf8_lossy(&text.log).into(),
            time: delta.date,
        }))?)
    }

    pub(crate) fn file_tags(
        &self,
        path: &OsStr,
        symbols: &HashMap<Sym, Num>,
    ) -> anyhow::Result<()> {
        Ok(self.tx.send(Message::FileTags(FileTags {
            path: OsString::from(path),
            symbols: symbols.clone(),
        }))?)
    }
}

impl Collector {
    pub(crate) async fn join(mut self, delta: Duration) -> anyhow::Result<Detector<Mark>> {
        let mut detector = Detector::new(delta);
        let mut tags = HashMap::<OsString, HashMap<Sym, Num>>::new();

        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Commit(commit) => {
                    detector.add_file_commit(
                        &commit.path,
                        commit.id.as_ref(),
                        vec![String::from("HEAD")],
                        &commit.author,
                        &commit.message,
                        commit.time,
                    );
                }
                Message::FileTags(file_tags) => {
                    tags.insert(file_tags.path, file_tags.symbols);
                }
            }
        }

        // TODO: don't just return the detector, but return a Result struct that
        // also includes the tag information.
        Ok(detector)
    }
}
