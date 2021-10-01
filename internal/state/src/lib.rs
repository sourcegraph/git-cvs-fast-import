//! In-memory state management for `git-cvs-fast-import`.
//!
//! `git-cvs-fast-import-store` essentially acts as a persistence layer for this
//! package.

use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    sync::Arc,
    time::SystemTime,
};

use git_cvs_fast_import_store::Store;
use git_fast_import::Mark;
use tokio::sync::RwLock;

mod error;
pub use self::error::Error;

pub type FileRevisionID = usize;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileRevisionKey {
    pub path: OsString,
    pub revision: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub branches: Vec<Vec<u8>>,
    pub author: String,
    pub message: String,
    pub time: SystemTime,
}

#[derive(Debug)]
pub struct PatchSet {
    pub branch: Vec<u8>,
    pub time: SystemTime,
    pub file_revisions: Vec<FileRevisionID>,
}

#[derive(Debug, Clone)]
pub struct MarkedCommit {
    pub mark: Option<Mark>,
    pub commit: Commit,
}

#[derive(Debug)]
pub struct MarkedPatchSet {
    mark: Mark,
    patchset: Arc<PatchSet>,
}

#[derive(Debug, Clone, Default)]
pub struct Manager {
    // Base storage of every revision seen. Not exposed to the outside.
    file_revisions: Arc<RwLock<BTreeMap<Arc<FileRevisionKey>, FileRevisionID>>>,

    // Mapping of revisions to commits and marks.
    #[allow(clippy::type_complexity)]
    file_revision_commits: Arc<RwLock<Vec<(Arc<FileRevisionKey>, MarkedCommit)>>>,

    // Mapping of file marks to revisions and commits.
    file_marks: Arc<RwLock<HashMap<Mark, FileRevisionID>>>,

    // Mapping of tags to revisions and commits.
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevisionID>>>>,

    // Mapping of patchset marks to patchsets.
    patchset_marks: Arc<RwLock<BTreeMap<Mark, Arc<PatchSet>>>>,

    // Mapping of file revisions to patchsets.
    file_revision_patchsets: Arc<RwLock<BTreeMap<FileRevisionID, Vec<Mark>>>>,
}

// TODO: methods to interact with a database store.
impl Manager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn persist_to_store(&self, store: &Store) -> Result<(), Error> {
        let file_revision_commits_vec = self.file_revision_commits.read().await;

        log::trace!("inserting file revisions");
        let mut inserter = store.file_revision_inserter()?;
        for (file_revision, marked_commit) in file_revision_commits_vec.iter() {
            inserter.insert(
                &file_revision.path,
                &file_revision.revision,
                &marked_commit.commit.time,
                marked_commit.mark.map(|mark| mark.as_usize()),
                &marked_commit.commit.branches,
            )?;
        }
        inserter.finalise();
        log::trace!("done inserting file revisions");

        log::trace!("inserting tags");
        let mut inserter = store.tag_inserter()?;
        for (tag, ids) in self.tags.read().await.iter() {
            for id in ids {
                match file_revision_commits_vec.get(*id) {
                    Some((file_revision, _marked_commit)) => {
                        inserter.insert(tag, &file_revision.path, &file_revision.revision)?
                    }
                    None => {
                        return Err(Error::NoFileRevisionForID(*id));
                    }
                }
            }
        }
        inserter.finalise();
        log::trace!("done inserting tags");

        log::trace!("inserting patchsets");
        let mut inserter = store.patchset_inserter()?;
        for (mark, patchset) in self.patchset_marks.read().await.iter() {
            inserter.insert(
                mark.as_usize(),
                &patchset.branch,
                &patchset.time,
                patchset.file_revisions.iter().filter_map(|id| {
                    file_revision_commits_vec
                        .get(*id)
                        .map(|(file_revision, _marked_commit)| {
                            (
                                file_revision.path.as_os_str(),
                                file_revision.revision.as_slice(),
                            )
                        })
                }),
            )?;
        }
        inserter.finalise();
        log::trace!("done inserting patchsets");

        Ok(())
    }

    pub async fn add_file_revision(
        &self,
        file_revision: FileRevisionKey,
        commit: Commit,
        mark: Option<Mark>,
    ) -> Result<usize, Error> {
        let file_revision = Arc::new(file_revision);

        let id = {
            let mut file_revision_commit_vec = self.file_revision_commits.write().await;

            file_revision_commit_vec.push((file_revision.clone(), MarkedCommit { mark, commit }));
            file_revision_commit_vec.len() - 1
        };

        self.file_revisions
            .write()
            .await
            .insert(file_revision.clone(), id);

        if let Some(mark) = mark {
            self.file_marks.write().await.insert(mark, id);
        }

        Ok(id)
    }

    pub async fn add_patchset<I>(
        &self,
        mark: Mark,
        branch: Vec<u8>,
        time: SystemTime,
        file_id_iter: I,
    ) where
        I: Iterator<Item = FileRevisionID>,
    {
        let patchset = PatchSet {
            branch,
            time,
            file_revisions: file_id_iter.collect(),
        };

        let mut file_revision_patchsets_map = self.file_revision_patchsets.write().await;
        for id in patchset.file_revisions.iter() {
            file_revision_patchsets_map
                .entry(*id)
                .or_default()
                .push(mark);
        }

        self.patchset_marks
            .write()
            .await
            .insert(mark, Arc::new(patchset));
    }

    pub async fn add_tag(&self, tag: Vec<u8>, id: FileRevisionID) {
        self.tags.write().await.entry(tag).or_default().push(id);
    }

    pub async fn get_file_revision_from_id(
        &self,
        id: FileRevisionID,
    ) -> Result<Arc<FileRevisionKey>, Error> {
        match self.file_revision_commits.read().await.get(id) {
            Some((file_revision, _marked_commit)) => Ok(file_revision.clone()),
            None => Err(Error::NoFileRevisionForID(id)),
        }
    }

    pub async fn get_file_revision_from_mark(
        &self,
        mark: &Mark,
    ) -> Result<Arc<FileRevisionKey>, Error> {
        let file_revision_commits_vec = self.file_revision_commits.read().await;

        match self
            .file_marks
            .read()
            .await
            .get(mark)
            .map(|id| file_revision_commits_vec.get(*id))
            .flatten()
        {
            Some((revision, _marked_commit)) => Ok(revision.clone()),
            None => Err(Error::NoFileRevisionForMark(*mark)),
        }
    }

    pub async fn get_mark_from_file_id(&self, id: FileRevisionID) -> Result<Option<Mark>, Error> {
        match self.file_revision_commits.read().await.get(id) {
            Some((_file_revision, marked_commit)) => Ok(marked_commit.mark),
            None => Err(Error::NoFileRevisionForID(id)),
        }
    }

    pub async fn get_mark_from_file_revision(
        &self,
        file_revision: &FileRevisionKey,
    ) -> Result<Option<Mark>, Error> {
        if let Some(id) = self.file_revisions.read().await.get(file_revision) {
            let (_revision, marked_commit) = &self.file_revision_commits.read().await[*id];

            Ok(marked_commit.mark)
        } else {
            Err(Error::NoMark(file_revision.clone()))
        }
    }

    pub async fn get_patchsets_for_file_revision(
        &self,
        file_revision: &FileRevisionKey,
    ) -> Result<Vec<Mark>, Error> {
        if let Some(file_revision_id) = self.file_revisions.read().await.get(file_revision) {
            if let Some(patchsets) = self
                .file_revision_patchsets
                .read()
                .await
                .get(file_revision_id)
            {
                Ok(patchsets.clone())
            } else {
                Ok(Vec::new())
            }
        } else {
            Err(Error::NoFileRevisionForKey(file_revision.clone()))
        }
    }

    pub async fn get_patchset_from_mark(&self, mark: &Mark) -> Result<Arc<PatchSet>, Error> {
        match self.patchset_marks.read().await.get(mark) {
            Some(patchset) => Ok(patchset.clone()),
            None => Err(Error::NoPatchSetForMark(*mark)),
        }
    }

    pub async fn get_tag(
        &self,
        tag: &[u8],
    ) -> Result<Vec<(Arc<FileRevisionKey>, MarkedCommit)>, Error> {
        if let Some(ids) = self.tags.read().await.get(tag) {
            let file_revision_commits_vec = self.file_revision_commits.read().await;

            Ok(ids
                .iter()
                .map(|id| &file_revision_commits_vec[*id])
                .cloned()
                .collect())
        } else {
            Err(Error::NoTag(String::from_utf8_lossy(tag).into()))
        }
    }

    pub async fn tag_iter(&self) -> impl Iterator<Item = Vec<u8>> {
        self.tags
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<Vec<u8>>>()
            .into_iter()
    }
}
