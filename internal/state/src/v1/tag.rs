use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::file_revision;

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    pub(crate) tags: HashMap<Vec<u8>, Vec<file_revision::ID>>,
}
