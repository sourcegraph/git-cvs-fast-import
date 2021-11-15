use std::collections::{BTreeSet, HashMap};

use crate::{file_revision, patchset::Mark};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    /// Track the marks created for the fake commits used for tagging.
    marks: HashMap<Vec<u8>, (Mark, BTreeSet<file_revision::ID>)>,

    /// Track files that were observed during this run and need to be tagged.
    tags: HashMap<Vec<u8>, BTreeSet<file_revision::ID>>,
}

impl Store {
    pub(crate) fn add_mark<KI>(&mut self, tag: &[u8], mark: Mark, keys: KI)
    where
        KI: Iterator<Item = file_revision::ID>,
    {
        self.marks.insert(Vec::from(tag), (mark, keys.collect()));
    }

    pub(crate) fn add_tag(&mut self, tag: &[u8], file_revision_id: file_revision::ID) {
        self.tags
            .entry(Vec::from(tag))
            .or_default()
            .insert(file_revision_id);
    }

    pub(crate) fn get_file_revisions(&self, tag: &[u8]) -> Option<&BTreeSet<file_revision::ID>> {
        self.tags.get(tag)
    }

    pub(crate) fn get_mark(&self, tag: &[u8]) -> Option<&(Mark, BTreeSet<file_revision::ID>)> {
        self.marks.get(tag)
    }

    pub(crate) fn get_tags(&self) -> impl Iterator<Item = &[u8]> {
        self.tags.keys().map(|key| key.as_slice())
    }
}
