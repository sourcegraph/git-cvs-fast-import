//! In-memory state management for `git-cvs-fast-import`.
//!
//! `git-cvs-fast-import-store` essentially acts as a persistence layer for this
//! package.

use std::{ffi::OsStr, sync::Arc, time::SystemTime};

use git_fast_import::Mark;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{RwLock, RwLockReadGuard},
};

mod error;
pub use self::error::Error;

mod file_revision;
pub use file_revision::{FileRevision, ID as FileRevisionID};

mod patchset;
pub use patchset::PatchSet;

mod tag;

#[derive(Debug, Clone, Default)]
pub struct Manager {
    file_revisions: Arc<RwLock<file_revision::Store>>,
    patchsets: Arc<RwLock<patchset::Store>>,
    tags: Arc<RwLock<tag::Store>>,

    raw_marks: Arc<RwLock<Vec<u8>>>,
}

impl Manager {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_file_revision<I>(
        &self,
        path: &OsStr,
        revision: &[u8],
        mark: Option<Mark>,
        branches: I,
        author: &str,
        message: &str,
        time: &SystemTime,
    ) -> Result<file_revision::ID, Error>
    where
        I: Iterator,
        I::Item: AsRef<[u8]>,
    {
        self.file_revisions.write().await.add(
            file_revision::Key {
                path: path.to_os_string(),
                revision: revision.to_vec(),
            },
            mark.map(|mark| mark.into()),
            branches,
            author,
            message,
            time,
        )
    }

    pub async fn add_patchset<I>(
        &self,
        mark: Mark,
        branch: &[u8],
        time: &SystemTime,
        file_revision_iter: I,
    ) where
        I: Iterator<Item = file_revision::ID>,
    {
        self.patchsets
            .write()
            .await
            .add(mark.into(), branch, time, file_revision_iter)
    }

    pub async fn add_tag(&self, tag: &[u8], file_revision_id: file_revision::ID) {
        self.tags.write().await.add(tag, file_revision_id)
    }

    pub async fn get_file_revision(
        &self,
        path: &OsStr,
        revision: &[u8],
    ) -> Result<Arc<FileRevision>, Error> {
        match self.file_revisions.read().await.get_by_key(path, revision) {
            Some(revision) => Ok(revision),
            None => Err(Error::NoFileRevisionForKey(file_revision::Key {
                path: path.to_os_string(),
                revision: revision.to_vec(),
            })),
        }
    }

    pub async fn get_file_revision_by_id(
        &self,
        id: file_revision::ID,
    ) -> Result<Arc<FileRevision>, Error> {
        match self.file_revisions.read().await.get_by_id(id) {
            Some(revision) => Ok(revision),
            None => Err(Error::NoFileRevisionForID(id)),
        }
    }

    pub async fn get_patchset_from_mark(&self, mark: &Mark) -> Result<Arc<PatchSet>, Error> {
        let patchset_mark = patchset::Mark::from(*mark);
        if let Some(patchset) = self.patchsets.read().await.get_by_mark(&patchset_mark) {
            Ok(patchset)
        } else {
            Err(Error::NoPatchSetForMark(patchset_mark))
        }
    }

    pub async fn get_file_revisions_for_tag(&self, tag: &[u8]) -> TagFileRevisionIterator<'_> {
        TagFileRevisionIterator {
            guard: self.tags.read().await,
            tag: tag.to_vec(),
        }
    }

    pub async fn get_last_patchset_for_file_revision(
        &self,
        file_revision_id: file_revision::ID,
    ) -> Option<(Mark, Arc<PatchSet>)> {
        let patchsets = self.patchsets.read().await;

        if let Some(marks) = patchsets.get_patchset_marks(file_revision_id) {
            marks
                .iter()
                .fold(None, |prev: Option<(Mark, Arc<PatchSet>)>, mark| {
                    let maybe_patchset = patchsets.get_by_mark(mark);

                    if let Some(prev) = &prev {
                        if let Some(patchset) = maybe_patchset {
                            if prev.1.time < patchset.time {
                                return Some(((*mark).into(), patchset));
                            }
                        }
                    } else if let Some(patchset) = maybe_patchset {
                        return Some(((*mark).into(), patchset));
                    }

                    prev
                })
        } else {
            None
        }
    }

    pub async fn get_patchset_ids_for_file_revision(
        &self,
        id: file_revision::ID,
    ) -> PatchSetFileRevisionIterator<'_> {
        PatchSetFileRevisionIterator {
            guard: self.patchsets.read().await,
            file_revision_id: id,
        }
    }

    pub async fn get_tags(&self) -> TagIterator<'_> {
        TagIterator {
            guard: self.tags.read().await,
        }
    }

    pub async fn get_raw_marks<W>(&self, mut writer: W) -> Result<(), Error>
    where
        W: AsyncWrite + Unpin,
    {
        tokio::io::copy(&mut self.raw_marks.read().await.as_slice(), &mut writer).await?;
        Ok(())
    }

    pub async fn set_raw_marks<R>(&self, mut reader: R) -> Result<(), Error>
    where
        R: AsyncRead + Unpin,
    {
        // There's a little hackery here because AsyncWrite is implemented on
        // Vec<u8>, but not behind a RwLockGuard. Instead, we'll clear
        // self.raw_marks, write to a temporary buffer, and then move that into
        // raw_marks. Works out about the same in practice, since we hold a
        // write lock the whole time.

        let mut raw_marks = self.raw_marks.write().await;
        raw_marks.clear();

        let mut buf = Vec::new();
        tokio::io::copy(&mut reader, &mut buf).await?;

        raw_marks.extend(buf.into_iter());

        Ok(())
    }
}

pub struct PatchSetFileRevisionIterator<'a> {
    guard: RwLockReadGuard<'a, patchset::Store>,
    file_revision_id: file_revision::ID,
}

impl<'a> PatchSetFileRevisionIterator<'a> {
    pub fn iter(&self) -> Option<&Vec<patchset::Mark>> {
        self.guard.get_patchset_marks(self.file_revision_id)
    }
}

pub struct TagIterator<'a> {
    guard: RwLockReadGuard<'a, tag::Store>,
}

impl<'a> TagIterator<'a> {
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        self.guard.get_tags()
    }
}

pub struct TagFileRevisionIterator<'a> {
    guard: RwLockReadGuard<'a, tag::Store>,
    tag: Vec<u8>,
}

impl<'a> TagFileRevisionIterator<'a> {
    pub fn iter(&self) -> Option<&Vec<file_revision::ID>> {
        self.guard.get_file_revisions(&self.tag)
    }
}
