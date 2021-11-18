use std::{convert::TryFrom, fmt::Display, num::ParseIntError, str::FromStr};

use itertools::Itertools;

use crate::Error;

/// A number within a ,v file: this is generally a sequence of dotted, positive
/// integers, such as `1.1` or `1.1.2.2.2.1`.
///
/// Note that these come in two closely related variants: branches, which have
/// an odd number of elements, and commits, which have an even number. A branch
/// can (and usually does) contain many commits. As an added complication,
/// branches sometimes appear with an even number of elements when used in CVS,
/// but with the penultimate element set to 0.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Num {
    Branch(Vec<u64>),
    Commit(Vec<u64>),
}

impl Num {
    /// Checks if the current branch contains the given commit. This generally
    /// implies that the commit was on an ancestral branch, or is on the exact
    /// same branch.
    ///
    /// `Error::InvalidTypesForContains` is returned if `self` is not a branch
    /// or `other` is not a commit.
    pub fn contains(&self, other: &Num) -> Result<bool, Error> {
        if let Num::Branch(branch) = self {
            if let Num::Commit(other) = other {
                if other.len() > (branch.len() + 1) {
                    // Commit is deeper, and therefore cannot be on this branch.
                    return Ok(false);
                }

                // Check intermediate branches.
                for i in (0..branch.len() - 1).step_by(2) {
                    if let Some(other_branch) = other.get(i) {
                        if *other_branch != branch[i] {
                            // The branch number doesn't match.
                            return Ok(false);
                        }
                        if let Some(rev) = other.get(i + 1) {
                            if *rev > branch[i + 1] {
                                // The revision on the commit branch is after
                                // the branch we're comparing against.
                                return Ok(false);
                            }
                        } else {
                            // This would imply the commit isn't really a
                            // commit, since it has an odd number of entries,
                            // and there's nothing sensible to be done.
                            return Err(Error::InvalidTypesForContains);
                        }
                    } else {
                        // We're done; previous branches matched, and the commit
                        // is shallower than the branch we're matching against.
                        return Ok(true);
                    }
                }

                // Check the leaf branch.
                if let Some(other_branch) = other.get(branch.len() - 1) {
                    if *other_branch != branch[branch.len() - 1] {
                        return Ok(false);
                    }
                }

                return Ok(true);
            }
        }

        Err(Error::InvalidTypesForContains)
    }

    pub fn to_branch(&self) -> Self {
        match self {
            Num::Branch(_) => self.clone(),
            Num::Commit(parts) => Num::Branch(parts[0..parts.len() - 1].to_vec()),
        }
    }
}

impl TryFrom<&[u8]> for Num {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::from_str(std::str::from_utf8(value)?)
    }
}

impl FromStr for Num {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s
            .split('.')
            .filter_map(|part| match part.parse::<u64>() {
                // We want to strip out 0 components, as they indicate magic
                // revisions in CVS that we don't need direct knowledge of in
                // the importer.
                Ok(v) if v == 0 => None,
                Ok(v) => Some(Ok(v)),
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<u64>, ParseIntError>>()
        {
            Ok(parts) if parts.len() % 2 == 0 => Ok(Self::Commit(parts)),
            Ok(parts) => Ok(Self::Branch(parts)),
            Err(e) => Err(e.into()),
        }
    }
}

impl Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Branch(parts) => fmt_u64_slice(f, parts.as_slice()),
            Self::Commit(parts) => fmt_u64_slice(f, parts.as_slice()),
        }
    }
}

// This rule is disabled because we currently use `intersperse` from itertools,
// but this is going to be added to Rust proper at some point and rustc is
// already warning about it.
#[allow(unstable_name_collisions)]
fn fmt_u64_slice(f: &mut std::fmt::Formatter, input: &[u64]) -> std::fmt::Result {
    write!(
        f,
        "{}",
        input
            .iter()
            .map(|part| part.to_string())
            .intersperse(String::from("."))
            .collect::<String>()
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_num_contains() -> anyhow::Result<()> {
        // Contained because it's on this specific branch.
        assert!(num("1.1.2").contains(&num("1.1.2.1"))?);
        assert!(num("1.1.2").contains(&num("1.1.2.2"))?);

        // Contained because it's an ancestor of this branch.
        assert!(num("1.1.2").contains(&num("1.1"))?);

        // Not contained because it's on a different branch.
        assert!(!num("1.1.2").contains(&num("1.1.3.1"))?);

        // Not contained because it's only on a descendant branch.
        assert!(!num("1.1.2").contains(&num("1.1.2.1.1.1"))?);

        // Not contained because it's after the branch was made.
        assert!(!num("1.1.2").contains(&num("1.2"))?);

        Ok(())
    }

    #[test]
    fn test_num_parse() -> anyhow::Result<()> {
        assert_eq!(num("1.1"), Num::Commit(vec![1, 1]));
        assert_eq!(num("1.2.3.4"), Num::Commit(vec![1, 2, 3, 4]));

        assert_eq!(num("1.2.3"), Num::Branch(vec![1, 2, 3]),);

        assert_eq!(num("1.2.0.3"), Num::Branch(vec![1, 2, 3]));

        // Now the failures.
        for input in ["", "x", "1.", "1.x"] {
            assert!(Num::from_str(input).is_err());
        }

        Ok(())
    }

    fn num(s: &str) -> Num {
        Num::from_str(s).unwrap()
    }
}
