use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    sync::Arc,
};

use bimap::BiMap;
use git_fast_import::Mark;
use tokio::sync::RwLock;

mod error;
pub(crate) use self::error::{Error, Result};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct FileRevision {
    pub(crate) path: OsString,
    pub(crate) revision: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    deleted_file_revisions: Arc<RwLock<HashSet<FileRevision>>>,
    file_revisions: Arc<RwLock<BiMap<FileRevision, Mark>>>,
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevision>>>>,
    // TODO: patchset tracking, which will probably need to include FileRevision -> PatchSet.
}

// TOOD: methods to interact with a database store.
impl State {
    pub(crate) fn new() -> Self {
        Self {
            deleted_file_revisions: Arc::new(RwLock::new(HashSet::new())),
            file_revisions: Arc::new(RwLock::new(BiMap::new())),
            tags: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) async fn add_file_revision(
        &self,
        file_revision: FileRevision,
        mark: Option<Mark>,
    ) -> Result<()> {
        if let Some(mark) = mark {
            self.file_revisions
                .write()
                .await
                .insert_no_overwrite(file_revision, mark)?
        } else {
            self.deleted_file_revisions
                .write()
                .await
                .insert(file_revision);
        };

        Ok(())
    }

    pub(crate) async fn add_tag(&self, tag: Vec<u8>, file_revision: FileRevision) {
        self.tags
            .write()
            .await
            .entry(tag)
            .or_default()
            .push(file_revision);
    }

    pub(crate) async fn get_file_revision_from_mark(&self, mark: &Mark) -> Result<FileRevision> {
        match self.file_revisions.read().await.get_by_right(mark) {
            Some(file_revision) => Ok(file_revision.clone()),
            None => Err(Error::NoFileRevision(*mark)),
        }
    }

    pub(crate) async fn get_mark_from_file_revision(
        &self,
        file_revision: &FileRevision,
    ) -> Result<Option<Mark>> {
        if let Some(mark) = self.file_revisions.read().await.get_by_left(file_revision) {
            Ok(Some(*mark))
        } else if self
            .deleted_file_revisions
            .read()
            .await
            .contains(file_revision)
        {
            Ok(None)
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
