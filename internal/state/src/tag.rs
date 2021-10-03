use std::collections::HashMap;

use crate::file_revision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    tags: HashMap<Vec<u8>, Vec<file_revision::ID>>,
}

impl Store {
    pub(crate) fn add(&mut self, tag: &[u8], file_revision_id: file_revision::ID) {
        self.tags
            .entry(Vec::from(tag))
            .or_default()
            .push(file_revision_id);
    }

    pub(crate) fn get_file_revisions(&self, tag: &[u8]) -> Option<&Vec<file_revision::ID>> {
        self.tags.get(tag)
    }

    pub(crate) fn get_tags(&self) -> impl Iterator<Item = &[u8]> {
        self.tags.keys().map(|key| key.as_slice())
    }
}
