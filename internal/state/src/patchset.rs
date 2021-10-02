use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

use derive_more::{Display, From, Into};

use crate::file_revision;

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, From, Into)]
pub struct Mark(git_fast_import::Mark);

#[derive(Debug)]
pub struct PatchSet {
    pub branch: Vec<u8>,
    pub time: SystemTime,
    pub file_revisions: Vec<file_revision::ID>,
}

#[derive(Debug, Default)]
pub(crate) struct Store {
    /// Base storage for patchsets. This is keyed by Mark because patchsets
    /// always have a Mark by definition.
    patchsets: BTreeMap<Mark, Arc<PatchSet>>,

    by_file_revision: BTreeMap<file_revision::ID, Vec<Mark>>,
}

impl Store {
    pub(crate) fn add<I>(
        &mut self,
        mark: Mark,
        branch: &[u8],
        time: &SystemTime,
        file_revision_iter: I,
    ) where
        I: Iterator<Item = file_revision::ID>,
    {
        let file_revisions: Vec<file_revision::ID> = file_revision_iter.collect();

        for id in file_revisions.iter() {
            self.by_file_revision.entry(*id).or_default().push(mark);
        }

        self.patchsets.insert(
            mark,
            Arc::new(PatchSet {
                branch: Vec::from(branch),
                time: *time,
                file_revisions,
            }),
        );
    }

    pub(crate) fn get_by_mark(&self, mark: &Mark) -> Option<Arc<PatchSet>> {
        self.patchsets.get(mark).cloned()
    }

    pub(crate) fn get_patchset_marks(&self, id: file_revision::ID) -> Option<&Vec<Mark>> {
        self.by_file_revision.get(&id)
    }
}
