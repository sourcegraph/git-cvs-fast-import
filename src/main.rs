use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader},
    os::unix::prelude::OsStrExt,
};

use discovery::Discovery;

use structopt::StructOpt;
use tokio::task;

mod discovery;
mod output;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(short, long, help = "number of parallel workers")]
    jobs: Option<usize>,
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

    // Set up our git-fast-import export. Note that we need to immediately spawn
    // the worker onto a new task, but we'll join it later.
    let (output, mut worker) = output::new(io::stdout());
    let worker = task::spawn(async move { worker.join().await });

    // Set up our file discovery.
    let discovery = Discovery::new(&output, opt.jobs.unwrap_or_else(|| num_cpus::get()));

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
    drop(discovery);

    // TODO: branch inference.

    // We need to ensure all references to output are done before worker will
    // finish up.
    drop(output);

    // And now we wait.
    Ok(worker.await??)
}
