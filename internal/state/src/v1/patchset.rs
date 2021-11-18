use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{file_revision, patchset::Mark};

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PatchSet {
    pub branch: Vec<u8>,
    pub time: SystemTime,
    pub file_revisions: Vec<file_revision::ID>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    /// Base storage for patchsets. This is keyed by Mark because patchsets
    /// always have a Mark by definition.
    pub(crate) patchsets: BTreeMap<Mark, Arc<PatchSet>>,

    pub(crate) by_file_revision: BTreeMap<file_revision::ID, Vec<Mark>>,

    pub(crate) by_branch: HashMap<Vec<u8>, Vec<Mark>>,
}
