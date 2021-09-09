use std::{
    borrow::Borrow,
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt::Debug,
    hash::Hash,
    mem,
    time::{Duration, SystemTime},
};

use binary_heap_plus::{BinaryHeap, MinComparator};

/// Detector sets up a patchset detector for a stream of file commits.
///
/// The type parameters bear some explanation:
///
/// * `ID`: this is simply given back within patchsets, and should represent an
///   opaque ID that the caller can use to tie a file back to its content.
#[derive(Debug)]
pub struct Detector<ID>
where
    ID: Debug + Clone + Eq,
{
    delta: Duration,
    file_commits: HashMap<CommitKey, BinaryHeap<Commit<ID>, MinComparator>>,
}

impl<ID> Detector<ID>
where
    ID: Debug + Clone + Eq,
{
    pub fn new(delta: Duration) -> Self {
        Self {
            delta,
            file_commits: HashMap::new(),
        }
    }

    /// TODO: document
    ///
    /// If id is None, it will be treated as a deletion.
    pub fn add_file_commit<I>(
        &mut self,
        path: &OsStr,
        id: Option<&ID>,
        branches: I,
        author: &String,
        message: &String,
        time: SystemTime,
    ) where
        I: IntoIterator,
        I::Item: Borrow<String>,
    {
        let key = CommitKey {
            author: author.clone(),
            message: message.clone(),
        };
        let value = Commit {
            path: OsString::from(path),
            branches: branches.into_iter().map(|s| s.borrow().clone()).collect(),
            id: id.cloned(),
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

    pub fn into_patchset_iter(self) -> impl Iterator<Item = PatchSet<ID>> {
        let mut patchsets = BinaryHeap::new_min();

        for (key, commits) in self.file_commits.into_iter() {
            let mut last = None;
            let mut pending_files = HashMap::new();

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

                // This will overwrite a previous instance of this file in the
                // commit, which is _probably_ what we want in all scenarios.
                pending_files.insert(commit.path, commit.id);
            }

            if pending_files.len() > 0 {
                patchsets.push(PatchSet {
                    time: last.unwrap(),
                    author: key.author.clone(),
                    message: key.message.clone(),
                    files: pending_files,
                });
            }
        }

        patchsets.into_iter_sorted()
    }
}

#[derive(Debug, Clone, Eq)]
pub struct PatchSet<ID>
where
    ID: Debug + Clone + Eq,
{
    pub time: SystemTime,
    pub author: String,
    pub message: String,
    pub files: HashMap<OsString, Option<ID>>,
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
pub struct Commit<ID>
where
    ID: Debug + Clone + Eq,
{
    path: OsString,
    branches: Vec<String>,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn it_works() {
        let mut detector = Detector::new(Duration::from_secs(120));
        let branches = vec![String::from("branches")];

        // Add two files on the same commit.
        let author = String::from("author");
        let message = String::from("message in a bottle");

        detector.add_file_commit(
            &path("foo"),
            Some(&1),
            &branches,
            &author,
            &message,
            timestamp(100),
        );

        detector.add_file_commit(
            &path("bar"),
            Some(&2),
            &branches,
            &author,
            &message,
            timestamp(101),
        );

        // Delete foo on a new commit.
        detector.add_file_commit(
            &path("foo"),
            None,
            &branches,
            &author,
            &message,
            timestamp(300),
        );

        // Add a file on a separate commit.
        detector.add_file_commit(
            &path("bar"),
            Some(&3),
            &branches,
            &author,
            &String::from("this is a different message"),
            timestamp(90),
        );

        // Re-add foo on the same commit as the first one.
        detector.add_file_commit(
            &path("foo"),
            Some(&4),
            &branches,
            &author,
            &message,
            timestamp(120),
        );

        dbg!(detector
            .into_patchset_iter()
            .collect::<Vec<PatchSet<i32>>>());
    }

    fn path(s: &str) -> OsString {
        OsString::from_str(s).unwrap()
    }

    fn timestamp(ts: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(ts)
    }
}
