use std::{
    fmt::{Display, Write},
    io,
};

use crate::{Command, Identity, Mark};

/// A `commit` command stores a commit in the Git repository.
#[derive(Debug)]
pub struct Commit {
    branch_ref: String,
    author: Option<Identity>,
    committer: Identity,
    message: String,
    from: Option<Mark>,
    merge: Option<Mark>,
    commands: Vec<FileCommand>,
}

impl Command for Commit {
    fn write(&self, writer: &mut impl io::Write, mark: Mark) -> anyhow::Result<()> {
        // Build up a buffer and then write.
        let mut buf = String::new();
        writeln!(buf, "commit {}", self.branch_ref)?;
        writeln!(buf, "mark {}", mark)?;
        if let Some(author) = &self.author {
            writeln!(buf, "author {}", author)?;
        }
        writeln!(buf, "committer {}", self.committer)?;
        writeln!(buf, "data {}\n{}", self.message.len(), self.message)?;
        if let Some(from) = &self.from {
            writeln!(buf, "from {}", from)?;
        }
        if let Some(merge) = &self.merge {
            writeln!(buf, "merge {}", merge)?;
        }
        for command in self.commands.iter() {
            writeln!(buf, "{}", command)?;
        }

        Ok(write!(writer, "{}", buf)?)
    }
}

/// A builder to create a [`Commit`].
#[derive(Debug)]
pub struct CommitBuilder {
    branch_ref: String,
    author: Option<Identity>,
    committer: Option<Identity>,
    message: Option<String>,
    from: Option<Mark>,
    merge: Option<Mark>,
    commands: Vec<FileCommand>,
}

impl CommitBuilder {
    /// Constructs a new commit builder.
    pub fn new(branch_ref: String) -> Self {
        Self {
            branch_ref,
            author: None,
            committer: None,
            message: None,
            from: None,
            merge: None,
            commands: Vec::new(),
        }
    }

    /// Sets the commit author.
    pub fn author(&mut self, identity: Identity) -> &mut Self {
        self.author = Some(identity);
        self
    }

    /// Sets the commit committer.
    pub fn committer(&mut self, committer: Identity) -> &mut Self {
        self.committer = Some(committer);
        self
    }

    /// Sets the commit message.
    pub fn message(&mut self, message: String) -> &mut Self {
        self.message = Some(message);
        self
    }

    /// Sets the previous commit that this commit extends from.
    ///
    /// Note that this is _not_ an implementation of the `From` trait.
    pub fn from(&mut self, from: Mark) -> &mut Self {
        self.from = Some(from);
        self
    }

    /// Sets the commit that is merged into this commit.
    pub fn merge(&mut self, merge: Mark) -> &mut Self {
        self.merge = Some(merge);
        self
    }

    /// Adds a file command to the commit.
    pub fn add_file_command(&mut self, command: FileCommand) -> &mut Self {
        self.commands.push(command);
        self
    }

    /// Builds a [`Commit`] from the builder.
    ///
    /// If [`committer()`][Self::committer] and [`message()`][Self::message]
    /// have not been called, this will return an error.
    pub fn build(self) -> Result<Commit, Error> {
        let committer = match self.committer {
            Some(committer) => committer,
            None => {
                return Err(Error::MissingCommitter);
            }
        };
        let message = match self.message {
            Some(message) => message,
            None => {
                return Err(Error::MissingCommitMessage);
            }
        };

        Ok(Commit {
            branch_ref: self.branch_ref,
            author: self.author,
            committer,
            message,
            from: self.from,
            merge: self.merge,
            commands: self.commands,
        })
    }
}

/// A file command within a commit, representing a change to a particular file.
#[derive(Debug, Clone)]
pub enum FileCommand {
    /// A modified file, with a [`Mark`][crate::Mark] representing the new file
    /// content and the file mode.
    Modify {
        mode: Mode,
        mark: Mark,
        path: String,
    },

    /// A deleted file.
    Delete { path: String },

    /// A copied file.
    Copy { from: String, to: String },

    /// A renamed file.
    Rename { from: String, to: String },

    /// A special command that deletes all files in the working tree. All files
    /// that should exist after this commit must be added using
    /// [`Modify`][FileCommand::Modify] after this command.
    DeleteAll,
}

impl Display for FileCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileCommand::Modify { mode, mark, path } => write!(f, "M {} {} {}", mode, mark, path),
            FileCommand::Delete { path } => write!(f, "D {}", path),
            FileCommand::Copy { from, to } => write!(f, "C {} {}", from, to),
            FileCommand::Rename { from, to } => write!(f, "R {} {}", from, to),
            FileCommand::DeleteAll => write!(f, "deleteall"),
        }
    }
}

/// A file mode.
#[derive(Debug, Copy, Clone)]
pub enum Mode {
    /// A normal, non-executable file.
    Normal,

    /// A normal, executable file.
    Executable,

    /// A symbolic link, in which case the file content is expected to be the
    /// path to the target file.
    Symlink,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Normal => write!(f, "100644"),
            Mode::Executable => write!(f, "100755"),
            Mode::Symlink => write!(f, "120000"),
        }
    }
}

/// Possible errors when creating [`Commit`] instances.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a committer must be provided")]
    MissingCommitter,

    #[error("a commit message must be provided")]
    MissingCommitMessage,
}
