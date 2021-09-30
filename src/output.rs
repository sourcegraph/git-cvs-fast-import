use std::{
    ffi::OsString,
    fmt::Debug,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, ExitStatus, Stdio},
};

use git_fast_import::{Mark, Writer};
use structopt::StructOpt;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::{self, JoinHandle},
};

#[derive(Debug, StructOpt)]
pub(crate) struct Opt {
    #[structopt(
        long = "--git",
        default_value = "git",
        help = "path to the git command"
    )]
    git_command: OsString,

    #[structopt(
        long,
        help = "a fast-import specific Git option to add when invoking git fast-import"
    )]
    git_fast_import_option: Vec<String>,

    #[structopt(
        long,
        help = "a global Git option to add when invoking git fast-import"
    )]
    git_global_option: Vec<String>,

    #[structopt(short = "-C", long, help = "path to the Git repository to import into")]
    git_repo: OsString,
}

#[derive(Debug, Clone)]
pub(crate) struct Output {
    tx: UnboundedSender<Command>,
}

/// Constructs a new output object, and returns it along with a join handle that
/// must be awaited to ensure all output has been written. The mark file will be
/// imported if it exists, and the marks will exported to the mark file before
/// the join handle returns.
///
/// Under the hood, this spawns a git fast-import process with the given options
/// and writes to it. It's important that the git process be managed by the
/// [`Output`] object: we can't be sure that the import proper and mark export
/// are complete until the process actually exits.
pub(crate) fn new<P>(mark_file_path: P, opt: Opt) -> (Output, JoinHandle<anyhow::Result<()>>)
where
    P: AsRef<Path>,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let mark_file = mark_file_path.as_ref().to_path_buf();

    (
        Output { tx },
        task::spawn(async move { worker(opt, rx, mark_file).await }),
    )
}

impl Output {
    pub(crate) async fn blob(&self, blob: git_fast_import::Blob) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Blob(blob, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub(crate) async fn commit(&self, commit: git_fast_import::Commit) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Commit(commit, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    pub(crate) async fn tag(&self, tag: git_fast_import::Tag) -> anyhow::Result<Mark> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Tag(tag, tx)).map_err(|e| {
            log::error!("received command error: {}", &e);
            e
        })?;
        Ok(rx.await?)
    }

    // TODO: extend with other types we need to send.
}

async fn worker(
    opt: Opt,
    mut rx: UnboundedReceiver<Command>,
    mark_file: PathBuf,
) -> anyhow::Result<()> {
    let mut process = Process::new(opt)?;

    let mut client = Writer::new(process.stdin()?, mark_file)?;
    let handle_send_result = |r| match r {
        Ok(_) => Ok(()),
        Err(mark) => Err(Error::MarkSend(mark)),
    };

    while let Some(command) = rx.recv().await {
        match command {
            Command::Blob(blob, tx) => {
                handle_send_result(tx.send(client.command(blob)?))?;
            }
            Command::Checkpoint => {
                client.checkpoint()?;
            }
            Command::Commit(commit, tx) => {
                handle_send_result(tx.send(client.command(commit)?))?;
            }
            Command::Progress(message) => {
                client.progress(&message)?;
            }
            Command::Reset { branch_ref, from } => {
                client.reset(&branch_ref, from)?;
            }
            Command::Tag(tag, tx) => {
                handle_send_result(tx.send(client.command(tag)?))?;
            }
        }
    }

    // Destroy the client, which will send the done command, and then wait for
    // git to exit.
    drop(client);
    task::spawn_blocking(move || process.wait()).await??;

    Ok(())
}

type MarkSender = oneshot::Sender<Mark>;

#[allow(dead_code)]
#[derive(Debug)]
enum Command {
    Blob(git_fast_import::Blob, MarkSender),
    Checkpoint,
    Commit(git_fast_import::Commit, MarkSender),
    Progress(String),
    Reset {
        branch_ref: String,
        from: Option<Mark>,
    },
    Tag(git_fast_import::Tag, MarkSender),
}

/// A git fast-import process.
#[derive(Debug)]
struct Process {
    child: Child,
}

impl Process {
    fn new(opt: Opt) -> Result<Self, Error> {
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

        // Wire up the logging pipes.
        //
        // We'll use unwrap here because we've specifically requested the pipes
        // when starting the process above: if they're not there, then that's a
        // logic error and panicking is probably appropriate.
        let stdout = tokio::process::ChildStdout::from_std(child.stdout.take().unwrap())
            .map_err(Error::stdout_pipe)?;
        task::spawn(log_pipe(stdout, log::Level::Debug));

        let stderr = tokio::process::ChildStderr::from_std(child.stderr.take().unwrap())
            .map_err(Error::stderr_pipe)?;
        task::spawn(log_pipe(stderr, log::Level::Info));

        Ok(Self { child })
    }

    fn stdin(&self) -> Result<impl Write + Debug + '_, Error> {
        match &self.child.stdin {
            Some(input) => Ok(input),
            None => Err(Error::StdinPipe),
        }
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}

async fn log_pipe<R: AsyncRead + Unpin>(rdr: R, level: log::Level) -> anyhow::Result<()> {
    let mut buf = BufReader::new(rdr).split(b'\n');
    while let Some(line) = buf.next_segment().await? {
        log::log!(level, "{}", String::from_utf8_lossy(&line));
    }

    Ok(())
}

#[derive(Debug, Error)]
enum Error {
    #[error("cannot send mark back to caller: {0}")]
    MarkSend(Mark),

    #[error("cannot establish a {pipe} pipe to git fast-import: {err:?}")]
    OutputPipe { err: io::Error, pipe: String },

    #[error("error spawning git fast-import: {0:?}")]
    Spawn(io::Error),

    #[error("cannot establish an input pipe to git fast-import")]
    StdinPipe,
}

impl Error {
    fn stderr_pipe(err: io::Error) -> Self {
        Self::OutputPipe {
            err,
            pipe: String::from("stderr"),
        }
    }

    fn stdout_pipe(err: io::Error) -> Self {
        Self::OutputPipe {
            err,
            pipe: String::from("stdout"),
        }
    }
}
