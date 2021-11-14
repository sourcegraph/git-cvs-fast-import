use std::{
    fs::File,
    io::ErrorKind,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use discovery::Discovery;

use flexi_logger::{AdaptiveFormat, Logger};
use git_cvs_fast_import_process::Output;
use git_cvs_fast_import_state::{FileRevisionID, Manager};
use git_fast_import::{CommitBuilder, FileCommand, Identity, Mark};
use observer::{Collector, Observer};
use patchset::PatchSet;
use structopt::StructOpt;
use tempfile::NamedTempFile;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use walkdir::WalkDir;

mod discovery;
mod observer;

#[derive(Debug, StructOpt)]
#[structopt(about = "A Git importer for CVS repositories.")]
struct Opt {
    #[structopt(
        short,
        long,
        env = "CVSROOT",
        parse(from_os_str),
        help = "the CVSROOT, which must be a local directory; if omitted, the $CVSROOT environment variable will be used"
    )]
    cvsroot: PathBuf,

    #[structopt(
        short,
        long,
        default_value = "120s",
        parse(try_from_str = parse_duration::parse::parse),
        help = "maximum time between file commits before they'll be considered different patch sets"
    )]
    delta: Duration,

    #[structopt(long, help = "treat file discovery and parsing errors as non-fatal")]
    ignore_file_errors: bool,

    #[structopt(short, long, help = "number of parallel workers")]
    jobs: Option<usize>,

    #[structopt(
        long,
        default_value = "info",
        help = "set the log level (possible values: error, warn, info, debug, trace)"
    )]
    log: log::Level,

    #[structopt(flatten)]
    output: git_cvs_fast_import_process::Opt,

    #[structopt(
        short,
        long,
        parse(from_os_str),
        help = "the file storing the repository metadata. If this file doesn't exist, it will be created, and the import will be treated as being from scratch, rather than incremental"
    )]
    store: PathBuf,

    #[structopt(
        name = "DIRECTORY",
        parse(from_os_str),
        help = "the top level directories to import from the CVSROOT; if omitted, all directories will be imported"
    )]
    directories: Vec<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments.
    let opt = Opt::from_args();

    // Set up logging.
    Logger::try_with_env_or_str(opt.log.as_str())?
        .adaptive_format_for_stderr(AdaptiveFormat::Detailed)
        .start()?;

    // Preflight git to make sure we have a sane environment.
    git_cvs_fast_import_process::preflight(&opt.output)?;

    // Set up our state manager, loading the store if it exists.
    let state = match File::open(&opt.store) {
        Ok(file) => {
            log::info!("loading state from {}", opt.store.display());
            Manager::deserialize_from(&file).await?
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            log::info!("setting up new state");
            Manager::new()
        }
        Err(e) => anyhow::bail!(e),
    };

    // Set up the mark file for git-fast-import to import.
    let mark_file = dump_marks_to_file(&state).await?;

    // Set up our git-fast-import export using the marks, if any.
    let (output, worker) = git_cvs_fast_import_process::new(mark_file.as_ref(), &opt.output);

    // Discover all files in the CVSROOT, and process each one into a new
    // Collector and the state.
    log::info!("starting file discovery");
    let collector = discover_files(&state, &output, &opt)?;
    log::info!("discovery phase done; parsing files");

    // Collect our observations into patchsets so we can send them.
    let result = collector.join().await?;
    log::info!("file parsing complete; sending patchsets");

    for (branch, patchsets) in result.branch_iter() {
        send_patchsets(&state, &output, branch, patchsets.iter()).await?;
    }
    log::info!("main patchsets sent; sending tags");

    send_tags(&state, &output).await?;
    log::info!("tags sent");

    // We need to ensure all references to output are dropped before the output
    // handle will finish up.
    drop(output);

    // Now we wait for any remaining items to be written.
    worker.wait().await?;

    // git-fast-import wrote the marks to the mark file before exiting while we
    // were waiting for the output handle, so we can now store that in the
    // persistent store as well and remove the temporary file.
    log::info!("saving marks");
    save_marks_from_file(&state, &mark_file).await?;
    mark_file.close()?;

    // Finally, we can now store the in-memory state to the persistent store.
    log::info!("persisting state to {}", opt.store.display());
    {
        let file = File::create(&opt.store)?;
        state.serialize_into(&file).await?;
    }

    log::info!("export complete!");
    Ok(())
}

/// Discover all files in the given path input and parse them into a Collector.
///
/// If an item when iterating `opt.directories` returns an error, then that
/// error will be returned from this function.
fn discover_files(state: &Manager, output: &Output, opt: &Opt) -> Result<Collector, anyhow::Error> {
    // Set up the observer and collector that we'll use during file discovery to
    // persist file revisions and detect patchsets.
    let (observer, collector) = Observer::new(opt.delta, state.clone());

    // Create our discovery worker pool.
    let discovery = Discovery::new(
        state,
        output,
        &observer,
        opt.ignore_file_errors,
        opt.jobs.unwrap_or_else(num_cpus::get),
        &opt.cvsroot,
    );

    // Send all the input paths to the discovery workers.
    let paths: Vec<PathBuf> = if opt.directories.is_empty() {
        vec![opt.cvsroot.clone()]
    } else {
        opt.directories
            .iter()
            .map(|dir| {
                let mut pb = PathBuf::new();
                pb.push(&opt.cvsroot);
                pb.push(dir);

                pb
            })
            .collect()
    };
    for path in paths {
        for entry in WalkDir::new(path) {
            log::trace!("sending {:?} to discovery", &entry);
            discovery.discover(entry?.path())?;
        }
    }

    Ok(collector)
}

/// If marks exist in the store, dump them to a named temporary file that
/// git-fast-import can read from.
///
/// If marks do not exist, then a new temporary file will be created and
/// returned.
async fn dump_marks_to_file(state: &Manager) -> anyhow::Result<NamedTempFile> {
    let file = NamedTempFile::new()?;

    let mut writer = OpenOptions::new().write(true).open(file.path()).await?;
    state.get_raw_marks(&mut writer).await?;
    writer.flush().await?;

    Ok(file)
}

/// Send patchsets to git-fast-import.
async fn send_patchsets<'a, I>(
    state: &Manager,
    output: &Output,
    branch: &[u8],
    patchset_iter: I,
) -> anyhow::Result<()>
where
    I: Iterator<Item = &'a PatchSet<FileRevisionID>>,
{
    let branch_str = std::str::from_utf8(branch)?;

    // All commits except for the very first one will refer to their parent via
    // the from marker, so let's set that up.
    let mut from: Option<Mark> = state
        .get_last_patchset_mark_on_branch(branch)
        .await
        .map(|mark| mark.into());

    for patchset in patchset_iter {
        // We have a patchset, so let's turn it into a Git commit.
        let mut builder = CommitBuilder::new(format!("refs/heads/{}", branch_str));
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
            let revision = state.get_file_revision_by_id(*file_id).await?;
            match revision.mark {
                Some(mark) => builder.add_file_command(FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark: mark.into(),
                    path: path.clone(),
                }),
                None => builder.add_file_command(FileCommand::Delete { path: path.clone() }),
            };
        }

        // Calculate the file revision IDs.
        let file_revision_ids = patchset
            .file_revision_iter()
            .map(|(_path, ids)| ids)
            .flatten()
            .copied()
            .collect::<Vec<FileRevisionID>>();

        // Check if we have already sent the commit to git-fast-import.
        if let Some(mark) = state
            .get_mark_from_patchset_content(&patchset.time, file_revision_ids.iter().copied())
            .await
        {
            from = Some(mark);

            // Let's add this branch to the patchset.
            state.add_branch_to_patchset_mark(mark, branch).await;
        } else {
            // Actually send the commit to git-fast-import and get the commit
            // mark back.
            let mark = output.commit(builder.build()?).await?;

            // Save the patchset and its mark to the state (and eventually the
            // store).
            state
                .add_patchset(mark, branch, &patchset.time, file_revision_ids.into_iter())
                .await;

            from = Some(mark);
        }
    }

    // Set the HEAD of the branch in Git.
    if let Some(head_mark) = from {
        output.branch(branch_str, head_mark).await?;
    }

    Ok(())
}

/// Send tags to git-fast-import.
async fn send_tags(state: &Manager, output: &Output) -> anyhow::Result<()> {
    // TODO: allow the identity to be configured.
    let identity = Identity::new(None, "git-cvs-fast-import".into(), SystemTime::now())?;

    for tag in state.get_tags().await.iter() {
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
        let tag_str = String::from_utf8_lossy(tag).into_owned();

        let mut builder = CommitBuilder::new(format!("refs/heads/tags/{}", &tag_str));
        // TODO: allow the identity to be configured.
        builder
            .committer(identity.clone())
            .message(format!("Fake commit for tag {}.", &tag_str));

        // Unlike regular commits, we'll remove all the file content and
        // then attach the new content that is known to be on the tag. This
        // means that Git will have to figure out what the diffs look like.
        builder.add_file_command(FileCommand::DeleteAll);
        if let Some(file_revision_ids) = state.get_file_revisions_for_tag(tag).await.iter() {
            for file_revision_id in file_revision_ids.iter() {
                let file_revision = state.get_file_revision_by_id(*file_revision_id).await?;

                match file_revision.mark {
                    Some(mark) => builder.add_file_command(FileCommand::Modify {
                        mode: git_fast_import::Mode::Normal,
                        mark: mark.into(),
                        path: file_revision.key.path.clone(),
                    }),
                    None => builder.add_file_command(FileCommand::Delete {
                        path: file_revision.key.path.clone(),
                    }),
                };

                // Find out which patchset this file revision is in, if any, and
                // check if it's newer than what we've seen.
                if let Some((patchset_mark, patchset)) = state
                    .get_last_patchset_for_file_revision(*file_revision_id)
                    .await
                {
                    if let Some((mark, time)) = &parent_patchset {
                        if time < &patchset.time {
                            parent_patchset = Some((*mark, patchset.time));
                        }
                    } else {
                        parent_patchset = Some((patchset_mark, patchset.time));
                    }
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
        output.lightweight_tag(&tag_str, mark).await?;
    }

    Ok(())
}

/// Save the created marks back into the database.
async fn save_marks_from_file(state: &Manager, mark_file: &NamedTempFile) -> anyhow::Result<()> {
    // git fast-import will replace the temporary file under the same name,
    // rather than just writing to it, so mark_file.reopen() fails as a result.
    // Instead, we'll just use the path to open the file anew.
    let mut file = OpenOptions::new().read(true).open(mark_file.path()).await?;
    Ok(state.set_raw_marks(&mut file).await?)
}
