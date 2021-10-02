//! In-memory state management for `git-cvs-fast-import`.
//!
//! `git-cvs-fast-import-store` essentially acts as a persistence layer for this
//! package.

use std::{
    collections::{BTreeMap, HashMap},
    ffi::{OsStr, OsString},
    os::unix::prelude::OsStrExt,
    sync::Arc,
    time::SystemTime,
};

use git_cvs_fast_import_store::{Connection, Store};
use git_fast_import::Mark;
use tokio::{sync::RwLock, task};

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
    /// Base storage of every revision seen. Not exposed to the outside.
    ///
    /// Derived from the `file_revision_commits` store table.
    file_revisions: Arc<RwLock<BTreeMap<Arc<FileRevisionKey>, FileRevisionID>>>,

    /// Mapping of revision IDs to keys, commits, and marks. Essentially the
    /// base storage for other types that use FileRevisionID: those IDs are the
    /// keys of this vector.
    ///
    /// Maps to the `file_revision_commits` store table.
    #[allow(clippy::type_complexity)]
    file_revision_commits: Arc<RwLock<Vec<(Arc<FileRevisionKey>, MarkedCommit)>>>,

    /// Mapping of file marks to revisions and commits.
    ///
    /// Derived from the `file_revision_commits` store table.
    file_marks: Arc<RwLock<HashMap<Mark, FileRevisionID>>>,

    /// Mapping of tags to revisions and commits.
    ///
    /// Maps to the `tags` store table.
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevisionID>>>>,

    /// Mapping of patchset marks to patchsets.
    ///
    /// Maps to the `patchsets` store table.
    patchset_marks: Arc<RwLock<BTreeMap<Mark, Arc<PatchSet>>>>,

    /// Mapping of file revisions to patchsets.
    ///
    /// Maps to the `file_revision_commit_patchsets` table.
    file_revision_patchsets: Arc<RwLock<BTreeMap<FileRevisionID, Vec<Mark>>>>,
}

impl Manager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn load_from_store(&self, store: &Store) -> Result<(), Error> {
        let mut conn = store.connection()?;

        // We need to keep track of the FileRevisionIDs that we have in the
        // state manager and which store ID they map to.
        //
        // This is store::ID -> FileRevisionID.
        let mut file_revision_ids = BTreeMap::new();

        // We have to load the file revisions first because we need the ID
        // mapping for the other types.
        log::trace!("reading file revisions");
        {
            let mut file_revisions = self.file_revisions.write().await;
            let mut file_revision_commits = self.file_revision_commits.write().await;
            let mut file_marks = self.file_marks.write().await;

            conn.get_file_revisions(|rev| -> Result<(), Error> {
                let state_id = file_revision_commits.len();
                file_revision_ids.insert(rev.id, state_id);

                let key = Arc::new(FileRevisionKey {
                    path: OsStr::from_bytes(&rev.path).to_os_string(),
                    revision: rev.revision,
                });
                let mark = rev.mark.map(Mark::from);

                file_revision_commits.push((
                    key.clone(),
                    MarkedCommit {
                        mark,
                        commit: Commit {
                            branches: rev.branches,
                            author: rev.author,
                            message: rev.message,
                            time: rev.time,
                        },
                    },
                ));

                file_revisions.insert(key, state_id);

                if let Some(mark) = mark {
                    file_marks.insert(mark, state_id);
                }

                Ok(())
            })
            .map_err(|err| Error::Load(err.to_string()))?;
        }
        log::trace!("file revisions read");

        // Now we can load the tags and patchsets in parallel.
        let file_revision_ids = Arc::new(file_revision_ids);
        let (tag_result, patchset_result) = tokio::try_join!(
            task::spawn(load_tags(
                store.connection()?,
                file_revision_ids.clone(),
                self.tags.clone()
            )),
            task::spawn(load_patchsets(
                store.connection()?,
                file_revision_ids,
                self.patchset_marks.clone(),
                self.file_revision_patchsets.clone()
            ))
        )?;
        tag_result?;
        patchset_result?;

        Ok(())
    }

    pub async fn persist_to_store(self, store: &Store) -> Result<(), Error> {
        let mut conn = store.connection()?;
        let file_revision_commits_vec = self.file_revision_commits.read().await;

        // We need to keep track of the FileRevisionIDs that we have in the
        // state manager and which store ID they map to.
        //
        // This is FileRevisionID -> store::ID.
        let mut file_revision_ids = BTreeMap::new();

        // We have to persist the file revisions first because we need the ID
        // mapping for the other types.
        log::trace!("inserting file revisions");
        for (state_id, (file_revision, marked_commit)) in
            file_revision_commits_vec.iter().enumerate()
        {
            file_revision_ids.insert(
                state_id,
                conn.insert_file_revision_commit(
                    file_revision.path.as_bytes(),
                    &file_revision.revision,
                    marked_commit.mark.map(|mark| mark.as_usize()),
                    &marked_commit.commit.author,
                    &marked_commit.commit.message,
                    &marked_commit.commit.time,
                    marked_commit
                        .commit
                        .branches
                        .iter()
                        .map(|branch| branch.as_slice()),
                )?,
            );
        }
        log::trace!("done inserting file revisions");

        // Now we can persist the tags and patchsets in parallel.
        let file_revision_ids = Arc::new(file_revision_ids);
        let (tag_result, patchset_result) = tokio::try_join!(
            task::spawn(persist_tags(
                store.connection()?,
                self.tags.clone(),
                file_revision_ids.clone()
            )),
            task::spawn(persist_patchsets(
                store.connection()?,
                self.patchset_marks.clone(),
                file_revision_ids
            )),
        )?;
        tag_result?;
        patchset_result?;

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

async fn load_patchsets(
    mut conn: Connection,
    revision_ids: Arc<BTreeMap<i64, usize>>,
    patchset_marks: Arc<RwLock<BTreeMap<Mark, Arc<PatchSet>>>>,
    file_revision_patchsets: Arc<RwLock<BTreeMap<FileRevisionID, Vec<Mark>>>>,
) -> Result<(), Error> {
    log::trace!("reading patchsets");
    let mut patchset_marks = patchset_marks.write().await;
    let mut file_revision_patchsets = file_revision_patchsets.write().await;

    conn.get_patchsets(|patchset| -> Result<(), Error> {
        let mark = Mark::from(patchset.mark);
        let file_revisions: Vec<FileRevisionID> = patchset
            .file_revisions
            .into_iter()
            .filter_map(|store_id| revision_ids.get(&store_id))
            .copied()
            .collect();

        for id in file_revisions.iter().copied() {
            file_revision_patchsets.entry(id).or_default().push(mark);
        }

        patchset_marks.insert(
            mark,
            Arc::new(PatchSet {
                branch: patchset.branch,
                time: patchset.time,
                file_revisions,
            }),
        );

        Ok(())
    })
    .map_err(|err| Error::Load(err.to_string()))?;

    log::trace!("patchsets read");
    Ok(())
}

async fn load_tags(
    mut conn: Connection,
    revision_ids: Arc<BTreeMap<i64, usize>>,
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevisionID>>>>,
) -> Result<(), Error> {
    log::trace!("reading tags");
    let mut tags = tags.write().await;

    conn.get_tags(|tag, store_ids| -> Result<(), Error> {
        tags.insert(
            tag,
            store_ids
                .into_iter()
                .map(|store_id| match revision_ids.get(&store_id) {
                    Some(state_id) => Ok(*state_id),
                    None => Err(Error::Load(format!(
                        "state ID not found for store ID {}",
                        store_id
                    ))),
                })
                .collect::<Result<Vec<FileRevisionID>, Error>>()?,
        );

        Ok(())
    })
    .map_err(|err| Error::Load(err.to_string()))?;

    log::trace!("tags read");
    Ok(())
}

async fn persist_patchsets(
    mut conn: Connection,
    patchset_marks: Arc<RwLock<BTreeMap<Mark, Arc<PatchSet>>>>,
    revision_ids: Arc<BTreeMap<usize, i64>>,
) -> Result<(), Error> {
    log::trace!("inserting patchsets");

    for (mark, patchset) in patchset_marks.read().await.iter() {
        conn.insert_patchset(
            mark.as_usize(),
            &patchset.branch,
            &patchset.time,
            patchset
                .file_revisions
                .iter()
                .filter_map(|state_id| revision_ids.get(state_id).copied()),
        )?;
    }

    log::trace!("done inserting patchsets");
    Ok(())
}

async fn persist_tags(
    mut conn: Connection,
    tags: Arc<RwLock<HashMap<Vec<u8>, Vec<FileRevisionID>>>>,
    revision_ids: Arc<BTreeMap<usize, i64>>,
) -> Result<(), Error> {
    log::trace!("inserting tags");

    for (tag, ids) in tags.read().await.iter() {
        conn.insert_tag(
            tag,
            ids.iter()
                .filter_map(|state_id| revision_ids.get(state_id).copied()),
        )?;
    }

    log::trace!("done inserting tags");
    Ok(())
}
