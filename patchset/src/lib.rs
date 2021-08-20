use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    ffi::{OsStr, OsString},
    fmt::Debug,
    hash::Hash,
    time::SystemTime,
};

#[derive(Debug)]
pub struct Detector<ID, Revision>
where
    ID: Debug + Clone,
    Revision: Debug + Clone + Ord + Hash,
{
    file_commits: HashMap<CommitKey<Revision>, BTreeMap<SystemTime, Commit<ID>>>,
}

impl<ID, Revision> Detector<ID, Revision>
where
    ID: Debug + Clone,
    Revision: Debug + Clone + Ord + Hash,
{
    pub fn new() -> Self {
        Self {
            file_commits: HashMap::new(),
        }
    }

    pub fn add_file_commit<I>(
        &mut self,
        path: &OsStr,
        id: &ID,
        revision: &Revision,
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
            revision: revision.clone(),
        };
        let value = Commit {
            path: OsString::from(path),
            branches: branches.into_iter().map(|s| s.borrow().clone()).collect(),
            id: id.clone(),
        };

        if let Some(v) = self.file_commits.get_mut(&key) {
            v.insert(time, value);
        } else {
            let mut map = BTreeMap::new();
            map.insert(time, value);
            self.file_commits.insert(key, map);
        }
    }

    pub fn as_patchset_iter(&self) -> impl Iterator<Item = PatchSet<ID>> + '_ {
        self.file_commits.iter().flat_map(|(key, commits)| {
            // TODO: sort commits into timed buckets and yield them.

            commits
                .into_iter()
                .map(|(time, commit)| PatchSet {
                    author: key.author.clone(),
                    message: key.message.clone(),
                    time: *time,
                    files: Vec::new(),
                })
                .collect::<Vec<PatchSet<ID>>>()
        })
    }
}

#[derive(Debug, Clone)]
pub struct PatchSet<ID>
where
    ID: Debug + Clone,
{
    author: String,
    message: String,
    time: SystemTime,
    files: Vec<Commit<ID>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CommitKey<Revision>
where
    Revision: Debug + Clone + Ord + Hash,
{
    revision: Revision,
    author: String,
    message: String,
}

#[derive(Debug, Clone)]
pub struct Commit<ID>
where
    ID: Debug + Clone,
{
    path: OsString,
    branches: Vec<String>,
    id: ID,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
