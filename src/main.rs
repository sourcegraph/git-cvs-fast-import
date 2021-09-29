use std::{
    ffi::{OsStr, OsString},
    io::{self, BufRead, BufReader, Write},
    os::unix::prelude::OsStrExt,
    path::Path,
    time::Duration,
};

use discovery::Discovery;

use git_cvs_fast_import_store::Store;
use observer::{Collector, Observer};
use output::Output;
use structopt::StructOpt;
use tempfile::NamedTempFile;

use crate::state::{FileRevision, State};

mod discovery;
mod observer;
mod output;
mod state;

#[derive(Debug, StructOpt)]
#[structopt(
    about = "An exporter for CVS repositories into the git fast-import format. Provide a list of files to parse on STDIN, and a git fast-import stream will be output on STDOUT."
)]
struct Opt {
    #[structopt(
        short,
        long,
        default_value = "120s",
        parse(try_from_str = parse_duration::parse::parse),
        help = "maximum time between file commits before they'll be considered different patch sets"
    )]
    delta: Duration,

    #[structopt(short, long, help = "number of parallel workers")]
    jobs: Option<usize>,

    #[structopt(
        short,
        long,
        parse(from_os_str),
        help = "prefix to strip from incoming paths when creating files in the output repository"
    )]
    prefix: Option<OsString>,

    #[structopt(short, long, parse(from_os_str), help = "state file")]
    state_file: OsString,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments.
    let opt = Opt::from_args();

    // Set up logging.
    pretty_env_logger::init_timed();

    // Set up our state, and ensure we have a connection to our persistent
    // store.
    let state = State::new();
    let store = Store::new(opt.state_file.as_os_str())?;

    // If we have marks, dump them to a temporary file.
    let mark_file = dump_marks_to_file(&store)?;

    // Set up our git-fast-import export.
    let (output, output_handle) = output::new(io::stdout(), mark_file.as_ref());

    // Discover all files from stdin, and process each one into a new Collector
    // and the state.
    log::debug!("starting file discovery");
    let collector = discover_files(
        state.clone(),
        &output,
        opt.delta,
        BufReader::new(io::stdin()).split(b'\n'),
        opt.jobs.unwrap_or_else(num_cpus::get),
        opt.prefix.as_deref(),
    )?;
    log::debug!("discovery phase done; parsing files");

    // Collect our observations into patchsets so we can send them.
    let result = collector.join().await?;
    log::debug!("file parsing complete; sending patchsets");

    let mut from = None;
    for patch_set in result.patchset_iter() {
        let mut builder = git_fast_import::CommitBuilder::new("refs/heads/main".into());
        builder
            .committer(git_fast_import::Identity::new(
                None,
                patch_set.author.clone(),
                patch_set.time,
            )?)
            .message(patch_set.message.clone());

        if let Some(mark) = from {
            builder.from(mark);
        }

        for (path, file_id) in patch_set.file_content_iter() {
            match state.get_mark_from_file_id(*file_id).await? {
                Some(mark) => builder.add_file_command(git_fast_import::FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark,
                    path: path.to_string_lossy().into(),
                }),
                None => builder.add_file_command(git_fast_import::FileCommand::Delete {
                    path: path.to_string_lossy().into(),
                }),
            };
        }

        let mark = output.commit(builder.build()?).await?;

        state
            .add_patchset(
                mark,
                b"main".to_vec(),
                patch_set.time,
                patch_set
                    .file_revision_iter()
                    .map(|(_path, ids)| ids)
                    .flatten()
                    .copied(),
            )
            .await;

        from = Some(mark);
    }
    log::debug!("main patchsets sent; starting tag detection");

    // We need to ensure all references to output are done before the output
    // handle will finish up.
    drop(output);

    // And now we wait.
    output_handle.await??;

    log::debug!("persisting state to store");
    state.persist_to_store(&store).await?;

    // TODO: write the mark file contents back into the store.

    log::debug!("persist complete; exiting");
    Ok(())
}

/// Discover all files in the given path input and parse them into a Collector.
///
/// If an item when iterating `paths` returns an error, then that error will be
/// returned from this function.
fn discover_files<Error, PathIterator>(
    state: State,
    output: &Output,
    delta: Duration,
    paths: PathIterator,
    parallel_jobs: usize,
    prefix: Option<&OsStr>,
) -> Result<Collector, anyhow::Error>
where
    Error: std::error::Error,
    PathIterator: Iterator<Item = Result<Vec<u8>, Error>>,
{
    // Set up the observer and collector that we'll use during file discovery to
    // persist file revisions and detect patchsets.
    let (observer, collector) = Observer::new(delta, state);

    let discovery = Discovery::new(output, &observer, parallel_jobs, prefix);

    for r in paths {
        match r {
            Ok(path) => {
                log::trace!("sending {} to Discovery", String::from_utf8_lossy(&path));
                discovery.discover(OsStr::from_bytes(&path))?;
            }
            Err(e) => {
                anyhow::bail!("error reading path from stdin: {:?}", e);
            }
        }
    }

    Ok(collector)
}

/// If marks exist in the store, dump them to a named temporary file that
/// git-fast-import can read from.
fn dump_marks_to_file(store: &Store) -> anyhow::Result<Option<NamedTempFile>> {
    match store.connection()?.get_raw_marks()? {
        Some(mut mark_reader) => {
            let mut file = NamedTempFile::new()?;

            io::copy(&mut mark_reader, &mut file)?;
            file.flush()?;

            Ok(Some(file))
        }
        None => Ok(None),
    }
}
