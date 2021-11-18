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

use crate::{v1, Error};

#[derive(
    Debug,
    Display,
    Deserialize,
    Serialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    From,
    Into,
    Hash,
)]
pub struct ID(usize);

#[derive(
    Debug,
    Display,
    Deserialize,
    Serialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    From,
    Into,
    Hash,
)]
pub struct Mark(git_fast_import::Mark);

// The key stuff is adapted from
// https://stackoverflow.com/questions/36480845/how-to-avoid-temporary-allocations-when-using-a-complex-key-for-a-hashmap.
//
// Basically, we need to be able to key by a complex type without having to
// clone its members just to do a lookup, so we want to be able to treat a
// (&Path, &[u8]) tuple as being equivalent to the owned fields in Key.
trait Keyer {
    fn to_key(&self) -> (&Path, &str);
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct Key {
    pub path: PathBuf,
    pub revision: String,
}

impl Keyer for Key {
    fn to_key(&self) -> (&Path, &str) {
        (self.path.as_path(), self.revision.as_str())
    }
}

impl Keyer for (&Path, &str) {
    fn to_key(&self) -> (&Path, &str) {
        (self.0, self.1)
    }
}

impl<'a> Borrow<dyn Keyer + 'a> for Key {
    fn borrow(&self) -> &(dyn Keyer + 'a) {
        self
    }
}

impl<'a> Borrow<dyn Keyer + 'a> for (&'a Path, &'a str) {
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
        // Short circuit: if this revision has already been seen, then we don't
        // need to insert it again.
        if let Some(id) = self.by_key.get(&key) {
            return Ok(*id);
        }

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

    pub(crate) fn get_by_key(&self, path: &Path, revision: &str) -> Option<Arc<FileRevision>> {
        self.by_key
            .get((path, revision).borrow() as &dyn Keyer)
            .map(|id| self.get_by_id(*id))
            .flatten()
    }
}

impl From<v1::file_revision::Store> for Store {
    fn from(v1: v1::file_revision::Store) -> Self {
        let mut v2 = Store {
            file_revisions: Vec::new(),
            by_key: HashMap::new(),
            by_mark: BTreeMap::new(),
        };

        for v1_file_revision in v1.file_revisions.into_iter() {
            let v1_file_revision = Arc::try_unwrap(v1_file_revision).unwrap();

            let v2_key = Key {
                path: v1_file_revision.key.path.into(),
                revision: String::from_utf8_lossy(&v1_file_revision.key.revision).into_owned(),
            };

            let v2_file_revision = Arc::new(FileRevision {
                key: v2_key.clone(),
                mark: v1_file_revision.mark,
                branches: v1_file_revision.branches,
                author: v1_file_revision.author,
                message: v1_file_revision.message,
                time: v1_file_revision.time,
            });

            let id = v2.file_revisions.len().into();
            v2.file_revisions.push(v2_file_revision);
            v2.by_key.insert(v2_key, id);
            if let Some(mark) = v1_file_revision.mark {
                v2.by_mark.insert(mark, id);
            }
        }

        v2
    }
}
