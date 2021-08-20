use crate::Command;

#[derive(Debug)]
pub struct Blob {
    data: Vec<u8>,
}

impl Blob {
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: Vec::from(data),
        }
    }
}

impl Command for Blob {
    fn write(&self, writer: &mut impl std::io::Write, mark: crate::Mark) -> anyhow::Result<()> {
        write!(writer, "blob\nmark {}\ndata {}\n", mark, self.data.len())?;
        writer.write_all(&self.data)?;
        Ok(write!(writer, "\n")?)
    }
}
