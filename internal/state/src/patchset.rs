use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::SystemTime,
};

use derive_more::{Display, From, Into};
use serde::{Deserialize, Serialize};

use crate::file_revision;

#[derive(
    Debug, Display, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, From, Into,
)]
pub struct Mark(git_fast_import::Mark);

#[derive(Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct PatchSet {
    pub time: SystemTime,
    pub file_revisions: BTreeSet<file_revision::ID>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    /// Base storage for patchsets. This is keyed by Mark because patchsets
    /// always have a Mark by definition.
    patchsets: BTreeMap<Mark, Arc<PatchSet>>,

    by_file_revision: BTreeMap<file_revision::ID, Vec<Mark>>,

    by_branch: HashMap<Vec<u8>, Vec<Mark>>,

    by_content: HashMap<Arc<PatchSet>, Mark>,
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
        let branch = Vec::from(branch);

        if let Some(marks) = self.by_branch.get_mut(&branch) {
            marks.push(mark);
        } else {
            self.by_branch.insert(branch.clone(), vec![mark]);
        }

        let patchset = Arc::new(build_patchset(*time, file_revision_iter));
        for id in patchset.file_revisions.iter() {
            self.by_file_revision.entry(*id).or_default().push(mark);
        }

        self.by_content.insert(patchset.clone(), mark);
        self.patchsets.insert(mark, patchset);
    }

    pub(crate) fn add_branch_to_patchset(&mut self, mark: Mark, branch: &[u8]) {
        self.by_branch
            .entry(branch.to_vec())
            .or_default()
            .push(mark);
    }

    pub(crate) fn get_mark_for_content<I>(
        &self,
        time: SystemTime,
        file_revision_iter: I,
    ) -> Option<Mark>
    where
        I: Iterator<Item = file_revision::ID>,
    {
        self.by_content
            .get(&Arc::new(build_patchset(time, file_revision_iter)))
            .copied()
    }

    pub(crate) fn get_by_mark(&self, mark: &Mark) -> Option<Arc<PatchSet>> {
        self.patchsets.get(mark).cloned()
    }

    pub(crate) fn get_patchset_marks(&self, id: file_revision::ID) -> Option<&Vec<Mark>> {
        self.by_file_revision.get(&id)
    }

    pub(crate) fn get_last_mark_on_branch(&self, branch: &[u8]) -> Option<Mark> {
        self.by_branch
            .get(branch)
            .map(|marks| marks.last().copied())
            .flatten()
    }
}

fn build_patchset<I>(time: SystemTime, file_revision_iter: I) -> PatchSet
where
    I: Iterator<Item = file_revision::ID>,
{
    PatchSet {
        time,
        file_revisions: file_revision_iter.collect(),
    }
}
