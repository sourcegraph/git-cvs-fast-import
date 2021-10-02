//! Low level types mapping to the database tables.

use std::time::SystemTime;

pub type ID = i64;

#[derive(Debug, Clone)]
pub struct FileRevisionCommit {
    pub id: ID,
    pub path: Vec<u8>,
    pub revision: Vec<u8>,
    pub mark: Option<usize>,
    pub author: Vec<u8>,
    pub message: Vec<u8>,
    pub time: SystemTime,
}

#[derive(Debug, Clone)]
pub struct FileRevisionCommitBranch {
    pub file_revision_commit_id: ID,
    pub branch: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub id: ID,
    pub tag: Vec<u8>,
    pub file_revision_commit_id: ID,
}

#[derive(Debug, Clone)]
pub struct PatchSet {
    pub id: ID,
    pub mark: usize,
    pub branch: Vec<u8>,
    pub time: SystemTime,
}

#[derive(Debug, Clone)]
pub struct FileRevisionCommitPatchSet {
    pub file_revision_commit_id: ID,
    pub patchset_id: ID,
}
