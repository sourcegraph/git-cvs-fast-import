use std::{fmt::Display, os::unix::prelude::OsStrExt, process::Output};

use crate::Opt;

/// Preflights git using the given options, ensuring that git is executable and
/// the repository is valid.
pub fn preflight(opt: &Opt) -> Result<(), crate::Error> {
    // git rev-parse without further arguments will do nothing, successfully, as
    // long as the underlying repository is valid.
    let output = std::process::Command::new(&opt.git_command)
        .arg("-C")
        .arg(&opt.git_repo)
        .arg("rev-parse")
        .output()?;

    match output.status.code() {
        Some(code) if code == 0 => Ok(()),
        _ => Err(crate::Error::Preflight(Error::new(opt, output))),
    }
}

#[derive(Debug)]
pub struct Error {
    command: String,
    output: Output,
}

impl Error {
    fn new(opt: &Opt, output: Output) -> Self {
        Self {
            command: format!(
                "{} -C {} rev-parse",
                String::from_utf8_lossy(opt.git_command.as_bytes()),
                String::from_utf8_lossy(opt.git_repo.as_bytes())
            ),
            output,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "running {} failed with {}\n\nstdout:\n{}\n\nstderr:\n{}\n",
            self.command,
            match self.output.status.code() {
                Some(code) => format!("exit code {}", code),
                None => "signal".into(),
            },
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        )
    }
}
