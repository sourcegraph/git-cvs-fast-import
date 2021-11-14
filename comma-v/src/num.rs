use std::{convert::TryFrom, fmt::Display, num::ParseIntError, str::FromStr};

use itertools::Itertools;

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Num {
    Branch(Vec<u64>),
    Commit(Vec<u64>),
}

impl Num {
    pub fn contains(&self, other: &Num) -> Result<bool, Error> {
        if let Num::Branch(branch) = self {
            if let Num::Commit(other) = other {
                // for (prefix, max) in prefixes.iter() {
                //     if other.len() < prefix.len() {
                //         // This means that we're looking at a commit from before
                //         // this branch was branched, but since we know the
                //         // previous checks passed, that means the commit must be
                //         // part of this branch.
                //         return Ok(true);
                //     }
                //     if &other[0..prefix.len()] != prefix.as_slice() {
                //         return Ok(false);
                //     }
                //     if let Some(max) = max {
                //         if other[prefix.len()] > *max {
                //             return Ok(false);
                //         }
                //     }
                // }

                // return Ok(true);

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
                            return Err(Error::NotBranch);
                        }
                    } else {
                        // We're done; previous branches matched, and the
                        // revision isn't as deep.
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

        Err(Error::NotBranch)
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
    fn test_num_contains() {
        // Contained because it's on this specific branch.
        assert!(num("1.1.2").contains(&num("1.1.2.1")).unwrap());
        assert!(num("1.1.2").contains(&num("1.1.2.2")).unwrap());

        // Contained because it's an ancestor of this branch.
        assert!(num("1.1.2").contains(&num("1.1")).unwrap());

        // Not contained because it's on a different branch.
        assert!(!num("1.1.2").contains(&num("1.1.3.1")).unwrap());

        // Not contained because it's only on a descendant branch.
        assert!(!num("1.1.2").contains(&num("1.1.2.1.1.1")).unwrap());

        // Not contained because it's after the branch was made.
        assert!(!num("1.1.2").contains(&num("1.2")).unwrap());
    }

    #[test]
    fn test_num_parse() {
        assert_eq!(Num::from_str("1.1").unwrap(), Num::Commit(vec![1, 1]));
        assert_eq!(
            Num::from_str("1.2.3.4").unwrap(),
            Num::Commit(vec![1, 2, 3, 4])
        );

        assert_eq!(Num::from_str("1.2.3").unwrap(), Num::Branch(vec![1, 2, 3]),);

        assert_eq!(
            Num::from_str("1.2.0.3").unwrap(),
            Num::Branch(vec![1, 2, 3])
        );
    }

    fn num(s: &str) -> Num {
        Num::from_str(s).unwrap()
    }
}
