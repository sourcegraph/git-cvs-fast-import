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
pub(crate) use self::error::{Error, Result};

pub type FileRevisionID = usize;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FileRevisionKey {
    pub(crate) path: OsString,
    pub(crate) revision: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct Commit {
    pub(crate) branches: Vec<Vec<u8>>,
    pub(crate) author: String,
    pub(crate) message: String,
    pub(crate) time: SystemTime,
}

#[derive(Debug)]
pub(crate) struct PatchSet {
    pub(crate) branch: Vec<u8>,
    pub(crate) time: SystemTime,
    pub(crate) file_revisions: Vec<FileRevisionID>,
}

#[derive(Debug, Clone)]
pub(crate) struct MarkedCommit {
    pub(crate) mark: Option<Mark>,
    pub(crate) commit: Commit,
}

#[derive(Debug)]
pub(crate) struct MarkedPatchSet {
    mark: Mark,
    patchset: Arc<PatchSet>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Manager {
    // Base storage of every revision seen. Not exposed to the outside.
    file_revisions: Arc<RwLock<BTreeMap<Arc<FileRevisionKey>, FileRevisionID>>>,

    // Mapping of revisions to commits and marks.
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
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn persist_to_store(&self, store: &Store) -> Result<()> {
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

    pub(crate) async fn add_file_revision(
        &self,
        file_revision: FileRevisionKey,
        commit: Commit,
        mark: Option<Mark>,
    ) -> Result<usize> {
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

    pub(crate) async fn add_patchset<I>(
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

    pub(crate) async fn add_tag(&self, tag: Vec<u8>, id: FileRevisionID) {
        self.tags.write().await.entry(tag).or_default().push(id);
    }

    pub(crate) async fn get_file_revision_from_id(
        &self,
        id: FileRevisionID,
    ) -> Result<Arc<FileRevisionKey>> {
        match self.file_revision_commits.read().await.get(id) {
            Some((file_revision, marked_commit)) => Ok(file_revision.clone()),
            None => Err(Error::NoFileRevisionForID(id)),
        }
    }

    pub(crate) async fn get_file_revision_from_mark(
        &self,
        mark: &Mark,
    ) -> Result<Arc<FileRevisionKey>> {
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

    pub(crate) async fn get_mark_from_file_id(&self, id: FileRevisionID) -> Result<Option<Mark>> {
        match self.file_revision_commits.read().await.get(id) {
            Some((_file_revision, marked_commit)) => Ok(marked_commit.mark),
            None => Err(Error::NoFileRevisionForID(id)),
        }
    }

    pub(crate) async fn get_mark_from_file_revision(
        &self,
        file_revision: &FileRevisionKey,
    ) -> Result<Option<Mark>> {
        if let Some(id) = self.file_revisions.read().await.get(file_revision) {
            let (_revision, marked_commit) = &self.file_revision_commits.read().await[*id];

            Ok(marked_commit.mark)
        } else {
            Err(Error::NoMark(file_revision.clone()))
        }
    }

    pub(crate) async fn get_patchsets_for_file_revision(
        &self,
        file_revision: &FileRevisionKey,
    ) -> Result<Vec<Mark>> {
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

    pub(crate) async fn get_patchset_from_mark(&self, mark: &Mark) -> Result<Arc<PatchSet>> {
        match self.patchset_marks.read().await.get(mark) {
            Some(patchset) => Ok(patchset.clone()),
            None => Err(Error::NoPatchSetForMark(*mark)),
        }
    }

    pub(crate) async fn get_tag(
        &self,
        tag: &[u8],
    ) -> Result<Vec<(Arc<FileRevisionKey>, MarkedCommit)>> {
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
