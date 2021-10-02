//! Low level types mapping to the database tables.
//!
//! Note that there's no mapping for the `tags` table, mostly because it's so
//! simple that there's no point.

use std::time::SystemTime;

pub type ID = i64;

#[derive(Debug, Clone)]
pub struct FileRevisionCommit {
    pub id: ID,
    pub path: Vec<u8>,
    pub revision: Vec<u8>,
    pub mark: Option<usize>,
    pub author: String,
    pub message: String,
    pub time: SystemTime,
    pub branches: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct PatchSet {
    pub id: ID,
    pub mark: usize,
    pub branch: Vec<u8>,
    pub time: SystemTime,
    pub file_revisions: Vec<ID>,
}
