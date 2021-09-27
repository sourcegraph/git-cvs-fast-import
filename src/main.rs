use std::{
    ffi::{OsStr, OsString},
    io::{self, BufRead, BufReader, Write},
    os::unix::prelude::OsStrExt,
    time::Duration,
};

use discovery::Discovery;

use git_cvs_fast_import_store::Store;
use structopt::StructOpt;
use tempfile::NamedTempFile;

use crate::state::State;

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
    // Approximate strategy:
    //
    // 1. Walk ,v files in whatever path we're given. Parallelise the following
    //    steps on a per file basis.
    // 2. Immediately send a blob object for each revision of each one, tracking
    //    which file/num corresponds to which mark. (Eventually we'll need to
    //    check this against a list of things we've already put in the repo.)
    // 3. Simultaneously send delta+log to a coroutine for later patchset
    //    detection.
    // 4. Once file reads are complete, attempt to detect patchsets.
    // 5. Filter patchsets to the trunk to start with and construct the Git
    //    tree.
    //
    // Eventually, we also need to handle branches:
    //
    // 1. Construct plausible branch names by examining symbols across the repo.
    // 2. Attempt to construct coherent histories by assuming project branching.
    // 3. Send commits.

    let opt = Opt::from_args();
    pretty_env_logger::init();

    // Set up our state.
    let state = State::new();
    let store = Store::new(opt.state_file.as_os_str())?;

    // If we have marks, dump them to a temporary file.
    let mut conn = store.connection()?;
    let mark_file = conn
        .get_raw_marks()?
        .map(|mut reader| -> Result<NamedTempFile, anyhow::Error> {
            let mut file = NamedTempFile::new()?;

            io::copy(&mut reader, &mut file)?;
            file.flush()?;

            Ok(file)
        })
        .transpose()?;

    // Set up our git-fast-import export.
    let (output, output_handle) = output::new(io::stdout(), mark_file.as_ref());

    let (observer, collector) = observer::Observer::new(opt.delta, state);

    // Set up our file discovery.
    let discovery = Discovery::new(
        &output,
        &observer,
        opt.jobs.unwrap_or_else(num_cpus::get),
        match &opt.prefix {
            Some(pfx) => Some(pfx.as_os_str()),
            None => None,
        },
    );

    for r in BufReader::new(io::stdin()).split(b'\n').into_iter() {
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

    // We're done with discovery, so we can drop the input halves of the objects
    // that won't be receiving any further data, which will trigger their
    // workers to end.
    log::trace!("discovery phase done; getting patchsets");
    drop(discovery);
    drop(observer);

    let result = collector.join().await?;
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

        for (path, mark) in patch_set.file_content_iter() {
            match mark {
                Some(mark) => builder.add_file_command(git_fast_import::FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark: *mark,
                    path: path.to_string_lossy().into(),
                }),
                None => builder.add_file_command(git_fast_import::FileCommand::Delete {
                    path: path.to_string_lossy().into(),
                }),
            };
        }

        from = Some(output.commit(builder.build()?).await?);
    }

    // We need to ensure all references to output are done before the output
    // handle will finish up.
    drop(output);

    // And now we wait.
    output_handle.await?

    // TODO: write the mark file contents back into the store.
}
