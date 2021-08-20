use std::{fmt::Debug, io::Write};

mod blob;
pub use blob::Blob;

mod commit;
pub use commit::{Commit, CommitBuilder};

mod identity;
pub use identity::Identity;

mod mark;
pub use mark::Mark;

mod tag;
pub use tag::Tag;

#[derive(Debug)]
pub struct Client<W>
where
    W: Write + Debug,
{
    writer: W,
    next_mark: usize,
}

impl<W> Client<W>
where
    W: Write + Debug,
{
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            next_mark: 0,
        }
    }

    pub fn command<C>(&mut self, command: C) -> anyhow::Result<Mark>
    where
        C: Command,
    {
        let mark = Mark(self.next_mark);
        self.next_mark += 1;

        command.write(&mut self.writer, mark)?;
        Ok(mark)
    }

    pub fn checkpoint(&mut self) -> anyhow::Result<()> {
        Ok(write!(self.writer, "checkpoint\n")?)
    }

    pub fn progress(&mut self, message: &String) -> anyhow::Result<()> {
        Ok(write!(self.writer, "progress {}\n", message)?)
    }

    pub fn reset(&mut self, branch_ref: &String, from: Option<Mark>) -> anyhow::Result<()> {
        write!(self.writer, "reset {}\n", branch_ref)?;
        if let Some(from) = from {
            write!(self.writer, "from {}\n", from)?;
        }

        Ok(())
    }
}

pub trait Command {
    fn write(&self, writer: &mut impl Write, mark: Mark) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
