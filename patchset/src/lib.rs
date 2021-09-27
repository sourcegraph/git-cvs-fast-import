//! Patchset detection based  time: (), author, message, files: ()  time: (), author, message, files: ()  time: (), author, message, files: () on a stream of file commits.

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt::Debug,
    hash::Hash,
    mem,
    time::{Duration, SystemTime},
};

use binary_heap_plus::{BinaryHeap, MinComparator};
use thiserror::Error;

/// A `Detector` ingests a stream of file commits, and yields an iterator over
/// the patchsets detected within those file commits.
///
/// This is required because CVS treats each file commit as an independent
/// commit, and doesn't have a concept of a repo-wide commit like later VCSes
/// such as Subversion and Git. Therefore the same logical patchset can be
/// represented as a set of file commits over a period of time (since each file
/// commit gets the timestamp of when that _file_ was committed, rather than
/// when the user ran `cvs commit`).
///
/// Commits are considered to be linked into a single patchset when they have
/// matching "commit keys" within a certain duration (represented by the `delta`
/// argument to [`Detector::new()`]). The commit key is generated based on the
/// commit message and author.
///
/// The `ID` type parameter refers to the opaque ID used to represent a file:
/// this will be passed back to the caller when yielding patchsets.
#[derive(Debug)]
pub struct Detector<ID>
where
    ID: Debug + Clone + Eq,
{
    delta: Duration,

    // Implementation-wise, this field is the main reason this works
    // efficiently. Keying by CommitKey should be fairly obvious: commits can't
    // be linked into a patchset if they have differing CommitKeys.
    //
    // The interesting part here is the value: to bucket commits together, the
    // simplest way to handle that is to walk them in time order. Doing it in
    // any other order means you have to handle cases where the extremes of a
    // patchset are more than the delta duration apart, but there are commits in
    // between. An example of that would be having a delta of 10, and getting
    // commits in the order [40, 55, 47]: you'd have to have logic to stitch the
    // patchset back together when you see the 47; if you handle this in sorted
    // order, you sidestep all that.
    //
    // So we use a BinaryHeap here to keep our commits sorted as we insert them,
    // and amortise the cost of sorting them later. Commit<ID> is defined with
    // an ordering that is only based on the commit time, so this works as we
    // need.
    file_commits: HashMap<CommitKey, BinaryHeap<Commit<ID>, MinComparator>>,
}

impl<ID> Detector<ID>
where
    ID: Debug + Clone + Eq,
{
    /// Constructs a new detector.
    ///
    /// The `delta` duration will be used as the maximum time two otherwise
    /// matching file commits may diverge by before they are considered to be
    /// separate patchsets.
    pub fn new(delta: Duration) -> Self {
        Self {
            delta,
            file_commits: HashMap::new(),
        }
    }

    /// Adds a file commit to the detector.
    ///
    /// `id` is used to link the commit back to the file content. It is the
    /// responsibility of the caller to be able to map that back.
    ///
    /// If `id` is `None`, then this commit represents the file being deleted.
    pub fn add_file_commit<BI>(
        &mut self,
        path: OsString,
        id: Option<ID>,
        branches: BI,
        author: String,
        message: String,
        time: SystemTime,
    ) where
        BI: IntoIterator<Item = Vec<u8>>,
    {
        let key = CommitKey { author, message };
        let value = Commit {
            path,
            branches: branches.into_iter().collect(),
            id,
            time,
        };

        if let Some(v) = self.file_commits.get_mut(&key) {
            v.push(value);
        } else {
            let mut heap = BinaryHeap::new_min();
            heap.push(value);
            self.file_commits.insert(key, heap);
        }
    }

    /// Consumes the detector and returns the detected patchsets in ascending
    /// time order.
    pub fn into_patchset_iter(self) -> impl Iterator<Item = PatchSet<ID>> {
        self.into_binary_heap().into_iter_sorted()
    }

    fn into_binary_heap(self) -> BinaryHeap<PatchSet<ID>, MinComparator> {
        let mut patchsets = BinaryHeap::new_min();

        for (key, commits) in self.file_commits.into_iter() {
            let mut last = None;
            let mut pending_files = HashMap::new();

            // TODO: do something useful with the branches.
            for commit in commits.into_iter_sorted() {
                if let Some(last) = last {
                    if commit.time.duration_since(last).unwrap_or_default() > self.delta {
                        patchsets.push(PatchSet {
                            time: last,
                            author: key.author.clone(),
                            message: key.message.clone(),
                            files: mem::take(&mut pending_files),
                        });
                    }
                }

                last = Some(commit.time);

                // Add the new state of the file to the pending files. This
                // effectively overwrites previous versions of the file within
                // the same patchset, but that's generally what we want: it's
                // not an exact commit-for-commit representation, but should
                // accurately reflect what the user really did.
                pending_files
                    .entry(commit.path)
                    .or_insert_with(Vec::new)
                    .push(commit.id);
            }

            if !pending_files.is_empty() {
                patchsets.push(PatchSet {
                    time: last.unwrap(),
                    author: key.author.clone(),
                    message: key.message.clone(),
                    files: pending_files,
                });
            }
        }

        patchsets
    }
}

/// A `PatchSet` represents a single patchset detected by a [`Detector`].
///
/// This contains the commit time, author, message, and the files that are
/// modified by the patchset, along with all file IDs that were squashed into
/// the patchset.
#[derive(Debug, Clone, Eq)]
pub struct PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    pub time: SystemTime,
    pub author: String,
    pub message: String,
    files: HashMap<OsString, Vec<Option<ID>>>,
}

impl<ID> PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    /// Returns the content ID for the given file. If the file is deleted in
    /// this patchset, None is returned.
    pub fn file_content(&self, file: &OsStr) -> Result<Option<&ID>, Error> {
        match self.files.get(file) {
            Some(ids) => Ok(Self::content(ids)),
            None => Err(Error::file_not_found(file)),
        }
    }

    /// Iterates over each file in the patchset, in arbitrary order, along with
    /// the content ID for the file. If the file is deleted in the patchset, the
    /// ID will be None.
    pub fn file_content_iter(&self) -> impl Iterator<Item = (&OsString, Option<&ID>)> {
        self.files
            .iter()
            .map(|(file, ids)| (file, Self::content(ids)))
    }

    /// Iterates over each file in the patchset, in arbitrary order, and
    /// provides the file and a Vec of all the content IDs that were squashed
    /// into the patchset for that file.
    pub fn file_revision_iter(&self) -> impl Iterator<Item = (&OsString, &Vec<Option<ID>>)> {
        self.files.iter()
    }

    /// Checks if the file is deleted in the patchset.
    ///
    /// In most cases, [`file_content()`] will be more useful.
    pub fn is_deleted(&self, file: &OsStr) -> Result<bool, Error> {
        Ok(self.file_content(file)?.is_none())
    }

    fn content(ids: &[Option<ID>]) -> Option<&ID> {
        ids.last().map(|id| id.as_ref()).flatten()
    }
}

impl<ID> Default for PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    fn default() -> Self {
        Self {
            time: SystemTime::UNIX_EPOCH,
            author: Default::default(),
            message: Default::default(),
            files: Default::default(),
        }
    }
}

impl<ID> Ord for PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}

impl<ID> PartialOrd for PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.partial_cmp(&other.time)
    }
}

impl<ID> PartialEq for PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CommitKey {
    author: String,
    message: String,
}

#[derive(Debug, Clone, Eq)]
struct Commit<ID>
where
    ID: Debug + Clone + Eq,
{
    path: OsString,
    branches: Vec<Vec<u8>>,
    id: Option<ID>,
    time: SystemTime,
}

impl<ID> Ord for Commit<ID>
where
    ID: Debug + Clone + Eq,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}

impl<ID> PartialOrd for Commit<ID>
where
    ID: Debug + Clone + Eq,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.time.partial_cmp(&other.time)
    }
}

impl<ID> PartialEq for Commit<ID>
where
    ID: Debug + Clone + Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("file does not exist: {0}")]
    FileNotFound(String),
}

impl Error {
    fn file_not_found(name: &OsStr) -> Self {
        Self::FileNotFound(name.to_string_lossy().into())
    }
}

#[cfg(test)]
mod tests {
    use std::{iter::FromIterator, str::FromStr};

    use super::*;

    #[test]
    fn test_detector() {
        let mut detector = Detector::new(Duration::from_secs(120));
        let branches = vec![b"HEAD".to_vec()];

        // Add two files on the same commit.
        let author = String::from("author");
        let message = String::from("message in a bottle");

        detector.add_file_commit(
            path("foo"),
            Some(1),
            branches.clone(),
            author.clone(),
            message.clone(),
            timestamp(100),
        );

        detector.add_file_commit(
            path("bar"),
            Some(2),
            branches.clone(),
            author.clone(),
            message.clone(),
            timestamp(101),
        );

        // Delete foo on a new commit.
        detector.add_file_commit(
            path("foo"),
            None,
            branches.clone(),
            author.clone(),
            message.clone(),
            timestamp(300),
        );

        // Add a file on a separate commit.
        detector.add_file_commit(
            path("bar"),
            Some(3),
            branches.clone(),
            author.clone(),
            String::from("this is a different message"),
            timestamp(90),
        );

        // Re-add foo on the same commit as the first one.
        detector.add_file_commit(
            path("foo"),
            Some(4),
            branches,
            author.clone(),
            message,
            timestamp(120),
        );

        let have: Vec<PatchSet<i32>> = detector.into_patchset_iter().collect();
        let want: Vec<PatchSet<i32>> = vec![
            PatchSet {
                time: timestamp(90),
                author: author.clone(),
                message: String::from("this is a different message"),
                files: HashMap::from_iter([(path("bar"), [Some(3)].to_vec())]),
            },
            PatchSet {
                time: timestamp(120),
                author: author.clone(),
                message: String::from("message in a bottle"),
                files: HashMap::from_iter([
                    (path("foo"), [Some(1), Some(4)].to_vec()),
                    (path("bar"), [Some(2)].to_vec()),
                ]),
            },
            PatchSet {
                time: timestamp(300),
                author,
                message: String::from("message in a bottle"),
                files: HashMap::from_iter([(path("foo"), [None].to_vec())]),
            },
        ];
        assert_eq!(have, want);
    }

    fn path(s: &str) -> OsString {
        OsString::from_str(s).unwrap()
    }

    fn timestamp(ts: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(ts)
    }
}
