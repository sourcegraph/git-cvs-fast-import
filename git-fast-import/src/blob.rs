use crate::{Command, Error};

/// A `blob` command stores data in the Git repository.
#[derive(Debug)]
pub struct Blob {
    data: Vec<u8>,
}

impl Blob {
    /// Constructs a new blob from the given data.
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: Vec::from(data),
        }
    }
}

impl Command for Blob {
    fn write(&self, writer: &mut impl std::io::Write, mark: crate::Mark) -> Result<(), Error> {
        writeln!(writer, "blob\nmark {}\ndata {}", mark, self.data.len())?;
        writer.write_all(&self.data)?;
        Ok(writeln!(writer)?)
    }
}
