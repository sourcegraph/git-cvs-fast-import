use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    sync::Arc,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::file_revision::{Mark, ID};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct Key {
    pub path: OsString,
    pub revision: Vec<u8>,
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
    pub(crate) file_revisions: Vec<Arc<FileRevision>>,

    /// Access to revisions by key.
    by_key: HashMap<Key, ID>,

    /// Access to revisions by mark.
    by_mark: BTreeMap<Mark, ID>,
}
