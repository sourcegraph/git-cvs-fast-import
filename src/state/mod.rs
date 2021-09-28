use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    sync::Arc,
    time::SystemTime,
};

use bimap::BiMap;
use git_cvs_fast_import_store::Store;
use git_fast_import::Mark;
use tokio::sync::RwLock;

mod error;
pub(crate) use self::error::{Error, Result};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct FileRevision {
    pub(crate) path: OsString,
    pub(crate) revision: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct Commit {
    pub(crate) branches: Vec<Vec<u8>>,
    pub(crate) author: String,
    pub(crate) message: String,
    pub(crate) time: SystemTime,
}

#[derive(Debug)]
struct PatchSet {
    branch: Vec<u8>,
    time: SystemTime,
    revisions: Vec<Arc<FileRevision>>,
}

#[derive(Debug)]
pub(crate) struct MarkedCommit {
    mark: Option<Mark>,
    commit: Commit,
}

#[derive(Debug)]
pub(crate) struct MarkedPatchSet {
    mark: Mark,
    patchset: Arc<PatchSet>,
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    // Commits include all file revisions, including deletions.
    commits: Arc<RwLock<HashMap<Arc<FileRevision>, MarkedCommit>>>,

    // File revisions include all non-deletion file revisions and their
    // associated marks.
    file_revisions: Arc<RwLock<BiMap<Arc<FileRevision>, Mark>>>,

    // Patchsets include all patchsets, keyed by their commit mark.
    patchsets: Arc<RwLock<HashMap<Mark, Arc<PatchSet>>>>,

    // Revision patchsets are all patchsets keyed by file revision.
    // revision_patchsets: Arc<RwLock<HashMap<Arc<FileRevision>, MarkedPatchSet>>>,

    // Tags include all tags, keyed by name.
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevision>>>>,
}

// TOOD: methods to interact with a database store.
impl State {
    pub(crate) fn new() -> Self {
        Self {
            commits: Arc::new(RwLock::new(HashMap::new())),
            file_revisions: Arc::new(RwLock::new(BiMap::new())),
            patchsets: Arc::new(RwLock::new(HashMap::new())),
            // revision_patchsets: Arc::new(RwLock::new(HashMap::new())),
            tags: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) async fn persist_to_store(&self, store: &Store) -> Result<()> {
        let mut inserter = store.file_revision_inserter()?;
        for (file_revision, marked_commit) in self.commits.read().await.iter() {
            inserter.insert(
                &file_revision.path,
                &file_revision.revision,
                &marked_commit.commit.time,
                marked_commit.mark.map(|mark| mark.as_usize()),
                &marked_commit.commit.branches,
            )?;
        }

        let mut inserter = store.tag_inserter()?;
        for (tag, file_revisions) in self.tags.read().await.iter() {
            for file_revision in file_revisions {
                inserter.insert(tag, &file_revision.path, &file_revision.revision)?;
            }
        }

        let mut inserter = store.patchset_inserter()?;
        for (mark, patchset) in self.patchsets.read().await.iter() {
            inserter.insert(
                mark.as_usize(),
                &patchset.branch,
                &patchset.time,
                patchset.revisions.iter().map(|file_revision| {
                    (
                        file_revision.path.as_os_str(),
                        file_revision.revision.as_slice(),
                    )
                }),
            );
        }

        Ok(())
    }

    pub(crate) async fn add_file_revision(
        &self,
        file_revision: FileRevision,
        commit: Commit,
        mark: Option<Mark>,
    ) -> Result<()> {
        let file_revision = Arc::new(file_revision);

        self.commits
            .write()
            .await
            .insert(file_revision.clone(), MarkedCommit { mark, commit });

        if let Some(mark) = mark {
            self.file_revisions
                .write()
                .await
                .insert_no_overwrite(file_revision, mark)?;
        }

        Ok(())
    }

    pub(crate) async fn add_patchset<I>(
        &self,
        mark: Mark,
        branch: Vec<u8>,
        time: SystemTime,
        file_revision_iter: I,
    ) where
        I: Iterator<Item = FileRevision>,
    {
        self.patchsets.write().await.insert(
            mark,
            Arc::new(PatchSet {
                branch,
                time,
                // FIXME: this is inefficient, since it almost certainly
                // duplicates revisions we have elsewhere, but it's cheaper than
                // looking them up with the current data structure.
                revisions: file_revision_iter.map(|rev| Arc::new(rev)).collect(),
            }),
        );

        // let file_revisions_map = self.file_revisions.read().await;
        // let mut revision_patchsets = self.revision_patchsets.write().await;
        // for revision in
        //     file_revision_iter.filter_map(|file_mark| file_revisions_map.get_by_right(&file_mark))
        // {
        //     // let
        //     revision_patchsets.insert(
        //         revision.clone(),
        //         MarkedPatchSet {
        //             mark,
        //             patchset: patchset.clone(),
        //         },
        //     );
        // }
    }

    pub(crate) async fn add_tag(&self, tag: Vec<u8>, file_revision: FileRevision) {
        self.tags
            .write()
            .await
            .entry(tag)
            .or_default()
            .push(file_revision);
    }

    pub(crate) async fn get_file_revision_from_mark(
        &self,
        mark: &Mark,
    ) -> Result<Arc<FileRevision>> {
        match self.file_revisions.read().await.get_by_right(mark) {
            Some(file_revision) => Ok(file_revision.clone()),
            None => Err(Error::NoFileRevision(*mark)),
        }
    }

    pub(crate) async fn get_mark_from_file_revision(
        &self,
        file_revision: &FileRevision,
    ) -> Result<Option<Mark>> {
        if let Some(maybe_mark) = self
            .commits
            .read()
            .await
            .get(file_revision)
            .map(|marked_commit| marked_commit.mark)
        {
            Ok(maybe_mark)
        } else {
            Err(Error::NoMark(file_revision.clone()))
        }
    }

    pub(crate) async fn get_tag(&self, tag: &[u8]) -> Result<Vec<FileRevision>> {
        if let Some(revisions) = self.tags.read().await.get(tag) {
            Ok(revisions.to_vec())
        } else {
            Err(Error::NoTag(String::from_utf8_lossy(tag).into()))
        }
    }

    pub(crate) async fn tag_iter(&self) -> impl Iterator<Item = Vec<u8>> {
        self.tags
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<Vec<u8>>>()
            .into_iter()
    }
}
