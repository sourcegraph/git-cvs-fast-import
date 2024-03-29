//! A writer for the [git-fast-import
//! format](https://git-scm.com/docs/git-fast-import).

use std::{fmt::Debug, fs::File, io::Write, path::Path};

mod blob;
pub use blob::Blob;

mod commit;
pub use commit::{Commit, CommitBuilder, FileCommand, Mode};

mod error;
pub use error::Error;

mod identity;
pub use identity::Identity;

mod mark;
pub use mark::Mark;

mod mark_file;

mod tag;
pub use tag::Tag;

/// A writer that writes data in the [git-fast-import command
/// format](https://git-scm.com/docs/git-fast-import).
///
/// The writer will send a `done` command when dropped to ensure data integrity,
/// so be careful not to reuse the same underlying writer with multiple `Writer`
/// instances.
///
/// Note that `git fast-import` must have been invoked with
/// `--allow-unsafe-features`: as this object needs to know what the last mark
/// was, it reads the mark file and then uses the `feature` command to set the
/// import and export flags in `git fast-import`, which are considered unsafe.
/// As this type does not provide a way of specifying arbitrary features outside
/// of naming the mark file, this is safe provided the mark file path is _not_
/// user controlled.
#[derive(Debug)]
pub struct Writer<W>
where
    W: Write + Debug,
{
    writer: W,
    next_mark: usize,
}

impl<W> Writer<W>
where
    W: Write + Debug,
{
    /// Constructs a new git-fast-import writer that wraps the given writer with
    /// a persistent mark file.
    ///
    /// Note that `writer` must be ready to receive commands immediately, as
    /// `feature` commands will be sent to configure the receiver.
    pub fn new<P>(writer: W, mark_file: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        Self {
            writer,
            // The mark file doesn't have to exist, so we'll fall back to the
            // default initial mark of 1 if we can't open it.
            next_mark: if let Ok(file) = File::open(&mark_file) {
                let last_mark = mark_file::get_last_mark(&file)?;
                last_mark.map(|mark| mark.0 + 1).unwrap_or(1)
            } else {
                1
            },
        }
        .send_generic_header()?
        .send_mark_header(mark_file)
    }

    /// Sends a command that returns a mark to fast-import.
    pub fn command<C>(&mut self, command: C) -> Result<Mark, Error>
    where
        C: Command,
    {
        let mark = Mark(self.next_mark);
        self.next_mark += 1;

        command.write(&mut self.writer, mark)?;
        Ok(mark)
    }

    /// Sends a `checkpoint` command to fast-import.
    pub fn checkpoint(&mut self) -> Result<(), Error> {
        Ok(writeln!(self.writer, "checkpoint")?)
    }

    /// Sends a `progress` command to fast-import.
    pub fn progress(&mut self, message: &str) -> Result<(), Error> {
        Ok(writeln!(self.writer, "progress {}", message)?)
    }

    /// Sends a `reset` command to fast-import.
    pub fn reset(&mut self, branch_ref: &str, from: Option<Mark>) -> Result<(), Error> {
        writeln!(self.writer, "reset {}", branch_ref)?;
        if let Some(from) = from {
            writeln!(self.writer, "from {}", from)?;
        }

        Ok(())
    }

    /// Returns the next mark that will be created.
    pub fn next_mark(&self) -> usize {
        self.next_mark
    }

    fn send_generic_header(mut self) -> Result<Self, Error> {
        writeln!(self.writer, "feature done")?;
        writeln!(self.writer, "feature date-format=raw")?;

        Ok(self)
    }

    fn send_mark_header<P>(mut self, mark_file: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let path = mark_file.as_ref().to_string_lossy();

        writeln!(self.writer, "feature import-marks-if-exists={}", path,)?;
        writeln!(self.writer, "feature export-marks={}", path,)?;

        Ok(self)
    }
}

impl<W> Drop for Writer<W>
where
    W: Write + Debug,
{
    fn drop(&mut self) {
        writeln!(self.writer, "done").unwrap();
    }
}

/// A mark-returning `git fast-import` command.
pub trait Command {
    /// A function that writes the command in wire format to the given writer.
    fn write(&self, writer: &mut impl Write, mark: Mark) -> Result<(), Error>;
}
