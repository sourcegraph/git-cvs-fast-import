use std::{
    ffi::{OsStr, OsString},
    io::{self, BufRead, BufReader, Write},
    os::unix::prelude::OsStrExt,
    sync::Arc,
    time::{Duration, SystemTime},
};

use discovery::Discovery;

use git_cvs_fast_import_store::Store;
use git_fast_import::{CommitBuilder, FileCommand, Identity, Mark};
use observer::{Collector, Observer};
use output::Output;
use patchset::PatchSet;
use state::FileRevisionID;
use structopt::StructOpt;
use tempfile::NamedTempFile;

use crate::state::Manager;

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
    let state = Manager::new();
    let store = Store::new(opt.state_file.as_os_str())?;

    // If we have marks, dump them to a temporary file.
    let mark_file = dump_marks_to_file(&store)?;

    // Set up our git-fast-import export.
    let (output, output_handle) = output::new(io::stdout(), mark_file.as_ref());

    // Discover all files from stdin, and process each one into a new Collector
    // and the state.
    log::debug!("starting file discovery");
    let collector = discover_files(
        &state,
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

    send_patchsets(&state, &output, result.patchset_iter()).await?;
    log::debug!("main patchsets sent; sending tags");

    send_tags(&state, &output).await?;
    log::debug!("tags sent");

    // We need to ensure all references to output are dropped before the output
    // handle will finish up.
    drop(output);

    // Now we wait for any remaining items to be written.
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
    state: &Manager,
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
    let (observer, collector) = Observer::new(delta, state.clone());

    // Create our discovery worker pool.
    let discovery = Discovery::new(output, &observer, parallel_jobs, prefix);

    // Send all the input paths to the discovery workers.
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

/// Send patchsets to git-fast-import.
async fn send_patchsets<'a, I>(
    state: &Manager,
    output: &Output,
    patchset_iter: I,
) -> anyhow::Result<()>
where
    I: Iterator<Item = &'a PatchSet<FileRevisionID>>,
{
    // All commits except for the very first one will refer to their parent via
    // the from marker, so let's set that up.
    let mut from = None;

    for patchset in patchset_iter {
        // We have a patchset, so let's turn it into a Git commit.
        let mut builder = CommitBuilder::new("refs/heads/main".into());
        builder
            .committer(Identity::new(None, patchset.author.clone(), patchset.time)?)
            .message(patchset.message.clone());

        // As alluded to earlier, if we have a parent mark (and we usually
        // will), we need to ensure that gets set up.
        if let Some(mark) = from {
            builder.from(mark);
        }

        // Now we set up the file commands in the commit: the patchset will give
        // us the file revision ID for each file that was modified or deleted in
        // the commit. From there, we need to ascertain if that maps to a mark
        // (in which case it's a modification, since there's content associated
        // with the file revision) or not (in which case it's a deletion).
        for (path, file_id) in patchset.file_content_iter() {
            match state.get_mark_from_file_id(*file_id).await? {
                Some(mark) => builder.add_file_command(FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark,
                    path: path.to_string_lossy().into(),
                }),
                None => builder.add_file_command(FileCommand::Delete {
                    path: path.to_string_lossy().into(),
                }),
            };
        }

        // Actually send the commit to git-fast-import and get the commit mark
        // back.
        let mark = output.commit(builder.build()?).await?;

        // Save the patchset and its mark to the state (and eventually the
        // store).
        state
            .add_patchset(
                mark,
                b"main".to_vec(),
                patchset.time,
                patchset
                    .file_revision_iter()
                    .map(|(_path, ids)| ids)
                    .flatten()
                    .copied(),
            )
            .await;

        from = Some(mark);
    }

    Ok(())
}

/// Send tags to git-fast-import.
async fn send_tags(state: &Manager, output: &Output) -> anyhow::Result<()> {
    // TODO: allow the identity to be configured.
    let identity = Identity::new(None, "git-cvs-fast-import".into(), SystemTime::now())?;

    for tag in state.tag_iter().await {
        // For each tag, we need to fake a Git commit with the correct content,
        // since CVS tags don't map onto Git tags especially gracefully, then
        // send a relevant tag.
        //
        // The tricky part here is knowing what the parent commit should be:
        // different CVS file revisions might have different patchsets as their
        // logical parents! Since this is essentially unsolvable without
        // splitting tags into per-file tags (which obfuscates the underlying
        // CVS tag), we'll use a heuristic: the _last_ patchset that any
        // revision in the tag belongs to will be the parent.

        let mut parent_patchset: Option<(Mark, SystemTime)> = None;
        let tag_str = String::from_utf8_lossy(&tag).into_owned();

        let mut builder = CommitBuilder::new(format!("refs/heads/tags/{}", &tag_str));
        // TODO: allow the identity to be configured.
        builder
            .committer(identity.clone())
            .message(format!("Fake commit for tag {}.", &tag_str));

        // Unlike regular commits, we'll remove all the file content and
        // then attach the new content that is known to be on the tag. This
        // means that Git will have to figure out what the diffs look like.
        builder.add_file_command(FileCommand::DeleteAll);
        for (key, marked_commit) in state.get_tag(&tag).await? {
            let path = key.path.to_string_lossy().into_owned();

            match marked_commit.mark {
                Some(mark) => builder.add_file_command(FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark,
                    path,
                }),
                None => builder.add_file_command(FileCommand::Delete { path }),
            };

            // Find out which patchset this file revision is in, if any, and
            // check if it's newer than what we've seen.
            if let Some(patchset_mark) = state.get_patchsets_for_file_revision(&key).await?.last() {
                let patchset = state.get_patchset_from_mark(patchset_mark).await?;

                if let Some((mark, time)) = &parent_patchset {
                    if time < &patchset.time {
                        parent_patchset = Some((*mark, patchset.time));
                    }
                } else {
                    parent_patchset = Some((*patchset_mark, patchset.time));
                }
            }
        }

        // Set the parent commit, if any.
        if let Some((from, _)) = parent_patchset {
            builder.from(from);
        }

        // Now we can send the commit.
        let mark = output.commit(builder.build()?).await?;

        // And we can tag the commit.
        output
            .tag(git_fast_import::Tag::new(
                tag_str.clone(),
                mark,
                identity.clone(),
                format!("Replicating CVS tag {}.", tag_str),
            ))
            .await?;
    }

    Ok(())
}
