use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ffi::{OsStr, OsString},
    time::{Duration, SystemTime},
};

use comma_v::{Delta, DeltaText, Num, Sym};
use git_fast_import::Mark;
use patchset::{Detector, PatchSet};
use tokio::{
    join,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::{self, JoinHandle},
};

#[derive(Clone, Debug)]
pub(crate) struct Observer {
    commit_tx: mpsc::UnboundedSender<Commit>,
    tag_tx: mpsc::UnboundedSender<TagMessage>,
}

#[derive(Debug)]
pub(crate) struct Collector {
    commit_join: JoinHandle<anyhow::Result<Detector<Mark>>>,
    tag_join: JoinHandle<anyhow::Result<HashMap<Sym, RepoTag>>>,
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
enum TagMessage {
    FileTags(FileTags),
    Commit {
        path: OsString,
        revision: Num,
        mark: Mark,
        time: SystemTime,
    },
}

#[derive(Debug)]
struct FileTags {
    path: OsString,
    symbols: HashMap<Sym, Num>,
}

#[derive(Debug)]
struct RepoTag {
    time: SystemTime,
    files: HashMap<OsString, Mark>,
}

/// Constructs a new commit observer, along with a collector that can be awaited
/// once the observer has been dropped to receive the final result of the
/// observations.
pub(crate) fn new(delta: Duration) -> (Observer, JoinHandle<anyhow::Result<Result>>) {
    let (commit_tx, commit_rx) = mpsc::unbounded_channel();
    let (file_tags_tx, file_tags_rx) = mpsc::unbounded_channel();

    (
        Observer {
            commit_tx,
            tag_tx: file_tags_tx,
        },
        task::spawn(async move {
            Collector::new(commit_rx, file_tags_rx, file_tags_tx.clone(), delta)
                .join()
                .await
        }),
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
        Ok(self.commit_tx.send(Commit {
            path: OsString::from(path),
            id,
            author: String::from_utf8_lossy(&delta.author).into(),
            message: String::from_utf8_lossy(&text.log).into(),
            time: delta.date,
        })?)
    }

    pub(crate) fn file_tags(
        &self,
        path: &OsStr,
        symbols: &HashMap<Sym, Num>,
    ) -> anyhow::Result<()> {
        Ok(self.tag_tx.send(TagMessage::FileTags(FileTags {
            path: OsString::from(path),
            symbols: symbols.clone(),
        }))?)
    }
}

impl Collector {
    fn new(
        commit_rx: UnboundedReceiver<Commit>,
        tag_rx: UnboundedReceiver<TagMessage>,
        tag_tx: UnboundedSender<TagMessage>,
        delta: Duration,
    ) -> Self {
        Self {
            commit_join: task::spawn(async move { commit_worker(commit_rx, tag_tx, delta).await }),
            tag_join: task::spawn(async move { tags_worker(tag_rx).await }),
        }
    }

    pub(crate) async fn join(self) -> anyhow::Result<Result<'_>> {
        let (detector, repo_tags) = join!(self.commit_join, self.tag_join);

        Ok(Result::new(detector??, repo_tags??))
    }
}

async fn commit_worker(
    mut rx: UnboundedReceiver<Commit>,
    tag_tx: UnboundedSender<TagMessage>,
    delta: Duration,
) -> anyhow::Result<Detector<Mark>> {
    let mut detector = Detector::new(delta);

    while let Some(commit) = rx.recv().await {
        detector.add_file_commit(
            &commit.path,
            commit.id.as_ref(),
            vec![String::from("HEAD")],
            &commit.author,
            &commit.message,
            commit.time,
        );
    }

    // TODO: don't just return the detector, but return a Result struct that
    // also includes the tag information.
    Ok(detector)
}

async fn tags_worker(
    mut rx: UnboundedReceiver<TagMessage>,
) -> anyhow::Result<HashMap<Sym, RepoTag>> {
    let mut file_tags = HashMap::<OsString, HashMap<Num, HashSet<Sym>>>::new();
    let mut repo_tags = HashMap::new();

    while let Some(message) = rx.recv().await {
        match message {
            TagMessage::FileTags(tags) => {
                for sym in tags.symbols.keys() {
                    repo_tags.entry(sym.clone()).or_insert_with(|| RepoTag {
                        time: SystemTime::UNIX_EPOCH,
                        files: HashMap::new(),
                    });
                }

                for (sym, num) in tags.symbols.iter() {
                    file_tags
                        .entry(tags.path.clone())
                        .or_insert_with(HashMap::new)
                        .entry(num.clone())
                        .or_insert_with(HashSet::new)
                        .insert(sym.clone());
                }
            }
            TagMessage::Commit {
                path,
                revision,
                mark,
                time,
            } => {
                if let Some(revisions) = file_tags.get(&path) {
                    if let Some(tags) = revisions.get(&revision) {
                        for tag in tags.iter() {
                            let mut repo_tag =
                                repo_tags.entry(tag.clone()).or_insert_with(|| RepoTag {
                                    time,
                                    files: HashMap::new(),
                                });
                            repo_tag.time = repo_tag.time.max(time);
                            repo_tag.files.insert(path.clone(), mark);
                        }
                    }
                }
            }
        }
    }

    Ok(repo_tags)
}

// TODO: This result stuff isn't really working as is. The problem is that we
// can associate tags to PatchSet instances, but that doesn't get us to the mark
// corresponding to the patchset. A rethink is required.
//
// I think once the detector is done, we need to insert the results into a
// database that we can more easily query. This means that we also need to track
// the CVS revisions that went into each patchset so we can look them up again
// later when handling tags and branches, along with the mapping of patchset ->
// mark.
//
// It's going to be something like:
//
// CVS file revision -> mark
// CVS file revisions -> patchset
// Patchset -> mark
// Tag -> CVS file revisions

#[derive(Debug)]
pub(crate) struct Result<'a> {
    patchsets: BTreeSet<PatchSet<Mark>>,
    tags: Vec<Tag<'a>>,
}

#[derive(Debug)]
pub(crate) struct Tag<'a> {
    pub(crate) name: Sym,
    pub(crate) file_marks: HashMap<OsString, Mark>,
    pub(crate) from: Option<&'a PatchSet<Mark>>,
}

impl Result<'_> {
    fn new(detector: Detector<Mark>, repo_tags: HashMap<Sym, RepoTag>) -> Self {
        let patchsets = detector.into_patchsets();
        let mut tags = Vec::new();

        for (sym, repo_tag) in repo_tags.into_iter() {
            tags.insert(Tag {
                name: sym,
                file_marks: repo_tag.files,
                from: patchsets
                    .range(
                        PatchSet::default()..=PatchSet {
                            time: repo_tag.time,
                            ..PatchSet::default()
                        },
                    )
                    .next_back(),
            })
        }

        Self { patchsets, tags }
    }

    pub(crate) fn as_patchset_iter(&self) -> impl Iterator<Item = &PatchSet<Mark>> {
        self.patchsets.iter()
    }

    pub(crate) fn as_tag_iter(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter()
    }
}
