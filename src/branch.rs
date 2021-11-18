use std::collections::HashSet;

pub(crate) struct BranchFilter {
    branches: Option<HashSet<Vec<u8>>>,
}

impl BranchFilter {
    pub(crate) fn new<I>(branches: I) -> Self
    where
        I: Iterator,
        I::Item: AsRef<[u8]>,
    {
        let branches = branches
            .map(|slice| Vec::from(slice.as_ref()))
            .collect::<HashSet<Vec<u8>>>();

        Self {
            branches: if branches.is_empty() {
                None
            } else {
                Some(branches)
            },
        }
    }

    pub(crate) fn contains(&self, branch: &[u8]) -> bool {
        if let Some(branches) = &self.branches {
            branches.contains(branch)
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_filter() -> anyhow::Result<()> {
        // Empty branch filters should always match.
        let filter = BranchFilter::new(Vec::<Vec<u8>>::new().iter());
        assert!(filter.contains(b""));
        assert!(filter.contains(b"foo"));

        // Otherwise, we should filter based on the allowed branches.
        let filter = BranchFilter::new([b"foo", b"bar"].iter());
        assert!(filter.contains(b"foo"));
        assert!(filter.contains(b"bar"));
        assert!(!filter.contains(b""));
        assert!(!filter.contains(b"quux"));

        Ok(())
    }
}
