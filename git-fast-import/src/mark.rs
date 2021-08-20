use std::fmt::Display;

#[derive(Debug, Clone, Copy)]
pub struct Mark(pub(super) usize);

impl Display for Mark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{}", self.0)
    }
}
