use std::{fmt::Debug, io::Write, process::Stdio};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    task::{self, JoinHandle},
};

use crate::{error::Error, Opt};

/// `Process` manages the `git fast-import` process.
#[derive(Debug)]
pub struct Process {
    handle: JoinHandle<Result<(), Error>>,
    stdin: std::process::ChildStdin,
}

impl Process {
    pub(crate) fn new(opt: Opt) -> Result<Self, Error> {
        // Create the git fast-import process.
        let mut child = std::process::Command::new(opt.git_command)
            .arg("-C")
            .arg(opt.git_repo)
            .args(opt.git_global_option.iter())
            .arg("fast-import")
            .arg("--allow-unsafe-features")
            .args(opt.git_fast_import_option.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(Error::Spawn)?;

        // Capture the stdin pipe.
        //
        // We'll use unwrap here because we've specifically requested the pipes
        // when starting the process above: if they're not there, then that's a
        // logic error and panicking is probably appropriate.
        let stdin = child.stdin.take().unwrap();

        // Wire up the logging pipes.
        let stdout = tokio::process::ChildStdout::from_std(child.stdout.take().unwrap())
            .map_err(Error::stdout_pipe)?;
        task::spawn(log_pipe(stdout, log::Level::Debug));

        let stderr = tokio::process::ChildStderr::from_std(child.stderr.take().unwrap())
            .map_err(Error::stderr_pipe)?;
        task::spawn(log_pipe(stderr, log::Level::Debug));

        Ok(Self {
            handle: task::spawn_blocking(move || {
                if let Err(e) = child.wait() {
                    log::error!("git fast-import exited with a non-zero status: {:?}", &e);
                    Err(e.into())
                } else {
                    Ok(())
                }
            }),
            stdin,
        })
    }

    pub(crate) fn stdin(&self) -> impl Write + Debug + '_ {
        &self.stdin
    }

    /// Wait for the `git fast-import` process to complete.
    ///
    /// Generally speaking, the process won't exit until the `done` command is
    /// sent, which in turn occurs when all writers are dropped.
    pub async fn wait(self) -> Result<(), Error> {
        self.handle.await?
    }
}

async fn log_pipe<R: AsyncRead + Unpin>(rdr: R, level: log::Level) -> Result<(), Error> {
    let mut buf = BufReader::new(rdr).split(b'\n');
    while let Some(line) = buf.next_segment().await.map_err(Error::OutputPipeRead)? {
        log::log!(level, "{}", String::from_utf8_lossy(&line));
    }

    Ok(())
}
