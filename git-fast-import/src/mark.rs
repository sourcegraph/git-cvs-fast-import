use std::fmt::Display;

use derive_more::{From, FromStr, Into};

/// A mark representing a Git object.
///
/// Marks are primarily created from blobs and commits, and can be used to refer
/// back to previous objects.
#[derive(Debug, Clone, Copy, From, FromStr, Into, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mark(pub(super) usize);

impl Display for Mark {
    /// Formats the mark in the fast-import wire format.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{}", self.0)
    }
}
