use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    hash::Hash,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use derive_more::{Display, From, Into};
use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(
    Debug, Display, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, Into,
)]
pub struct ID(usize);

#[derive(
    Debug, Display, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, Into,
)]
pub struct Mark(git_fast_import::Mark);

// The key stuff is adapted from
// https://stackoverflow.com/questions/36480845/how-to-avoid-temporary-allocations-when-using-a-complex-key-for-a-hashmap.
//
// Basically, we need to be able to key by a complex type without having to
// clone its members just to do a lookup, so we want to be able to treat a
// (&Path, &[u8]) tuple as being equivalent to the owned fields in Key.
trait Keyer {
    fn to_key(&self) -> (&Path, &[u8]);
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct Key {
    pub path: PathBuf,
    pub revision: Vec<u8>,
}

impl Keyer for Key {
    fn to_key(&self) -> (&Path, &[u8]) {
        (self.path.as_path(), self.revision.as_slice())
    }
}

impl Keyer for (&Path, &[u8]) {
    fn to_key(&self) -> (&Path, &[u8]) {
        (self.0, self.1)
    }
}

impl<'a> Borrow<dyn Keyer + 'a> for Key {
    fn borrow(&self) -> &(dyn Keyer + 'a) {
        self
    }
}

impl<'a> Borrow<dyn Keyer + 'a> for (&'a Path, &'a [u8]) {
    fn borrow(&self) -> &(dyn Keyer + 'a) {
        self
    }
}

impl PartialEq for dyn Keyer + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.to_key() == other.to_key()
    }
}

impl Eq for dyn Keyer + '_ {}

impl Hash for dyn Keyer + '_ {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_key().hash(state)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileRevision {
    pub key: Key,
    pub mark: Option<Mark>,
    pub branches: Vec<Vec<u8>>,
    pub author: String,
    pub message: String,
    pub time: SystemTime,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    /// Base storage for file revisions.
    file_revisions: Vec<Arc<FileRevision>>,

    /// Access to revisions by key.
    by_key: HashMap<Key, ID>,

    /// Access to revisions by mark.
    by_mark: BTreeMap<Mark, ID>,
}

impl Store {
    pub(crate) fn add<I>(
        &mut self,
        key: Key,
        mark: Option<Mark>,
        branches: I,
        author: &str,
        message: &str,
        time: &SystemTime,
    ) -> Result<ID, Error>
    where
        I: Iterator,
        I::Item: AsRef<[u8]>,
    {
        let id = self.file_revisions.len().into();

        self.file_revisions.push(Arc::new(FileRevision {
            key: key.clone(),
            mark,
            branches: branches.map(|branch| branch.as_ref().to_vec()).collect(),
            author: author.to_string(),
            message: message.to_string(),
            time: *time,
        }));

        self.by_key.insert(key, id);
        if let Some(mark) = mark {
            self.by_mark.insert(mark, id);
        }

        Ok(id)
    }

    pub(crate) fn get_by_id(&self, id: ID) -> Option<Arc<FileRevision>> {
        self.file_revisions.get(id.0).cloned()
    }

    pub(crate) fn get_by_key(&self, path: &Path, revision: &[u8]) -> Option<Arc<FileRevision>> {
        self.by_key
            .get((path, revision).borrow() as &dyn Keyer)
            .map(|id| self.get_by_id(*id))
            .flatten()
    }
}
