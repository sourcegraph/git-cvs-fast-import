use std::{
    fmt::{Display, Write},
    io,
};

use crate::{Command, Identity, Mark};

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
        write!(buf, "commit {}\n", self.branch_ref)?;
        write!(buf, "mark {}\n", mark)?;
        if let Some(author) = &self.author {
            write!(buf, "author {}\n", author)?;
        }
        write!(buf, "committer {}\n", self.committer)?;
        write!(buf, "data {}\n{}\n", self.message.len(), self.message)?;
        if let Some(from) = &self.from {
            write!(buf, "from {}\n", from)?;
        }
        if let Some(merge) = &self.merge {
            write!(buf, "merge {}\n", merge)?;
        }
        for command in self.commands.iter() {
            write!(buf, "{}\n", command)?;
        }

        Ok(write!(writer, "{}", buf)?)
    }
}

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

    pub fn author(&mut self, identity: Identity) -> &mut Self {
        self.author = Some(identity);
        self
    }

    pub fn committer(&mut self, committer: Identity) -> &mut Self {
        self.committer = Some(committer);
        self
    }

    pub fn message(&mut self, message: String) -> &mut Self {
        self.message = Some(message);
        self
    }

    pub fn from(&mut self, from: Mark) -> &mut Self {
        self.from = Some(from);
        self
    }

    pub fn merge(&mut self, merge: Mark) -> &mut Self {
        self.merge = Some(merge);
        self
    }

    pub fn add_file_command(&mut self, command: FileCommand) -> &mut Self {
        self.commands.push(command);
        self
    }

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

#[derive(Debug, Clone)]
pub enum FileCommand {
    Modify {
        mode: Mode,
        mark: Mark,
        path: String,
    },
    Delete {
        path: String,
    },
    Copy {
        from: String,
        to: String,
    },
    Rename {
        from: String,
        to: String,
    },
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

#[derive(Debug, Copy, Clone)]
pub enum Mode {
    Normal,
    Executable,
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a committer must be provided")]
    MissingCommitter,

    #[error("a commit message must be provided")]
    MissingCommitMessage,
}
