//! RCS file discovery and parsing.

use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

use async_recursion::async_recursion;
use comma_v::{Delta, DeltaText, Num, Sym};
use flume::{Receiver, Sender};
use git_cvs_fast_import_process::Output;
use git_cvs_fast_import_state::Manager;
use git_fast_import::{Blob, Mark};
use log::Level;
use rcs_ed::{File, Script};
use tokio::task;

use crate::observer::Observer;

/// A task that parses each file it's given.
///
/// This is responsible for three things:
///
/// 1. Sending all tags (symbols) in each file to the `Observer`.
/// 2. Sending each file revision's content as a blob to the `Output`, which
///    then creates a `git-fast-import` mark that can be used to refer back to
///    the file revision later.
/// 3. Sending each file revision to the `Observer`, which will in turn persist
///    the revision to the state and store.
#[derive(Debug, Clone)]
pub(crate) struct Discovery {
    tx: Sender<PathBuf>,
}

impl Discovery {
    /// Instantiates a new Discovery task.
    ///
    /// Parallelism is controlled by the `jobs` argument, which specifies the
    /// number of worker tasks to create.
    pub fn new(
        state: &Manager,
        output: &Output,
        observer: &Observer,
        head_branch: &str,
        ignore_errors: bool,
        jobs: usize,
        prefix: &Path,
    ) -> Self {
        // This is a multi-producer, multi-consumer channel that we use to fan
        // paths out to workers.
        let (tx, rx) = flume::unbounded::<PathBuf>();

        // Start each worker.
        for _i in 0..jobs {
            let worker = Worker::new(
                &rx,
                observer,
                output,
                prefix,
                state,
                head_branch,
                ignore_errors,
            );
            task::spawn(async move { worker.work().await });
        }

        Self { tx }
    }

    /// Queues the given path for parsing on the next available worker.
    pub fn discover(&self, path: &Path) -> anyhow::Result<()> {
        Ok(self.tx.send(path.to_path_buf())?)
    }
}

/// Worker represents an individual worker task processing RCS files.
struct Worker {
    observer: Observer,
    output: Output,
    prefix: PathBuf,
    rx: Receiver<PathBuf>,
    state: Manager,
    head_branch: Vec<u8>,
    ignore_errors: bool,
}

impl Worker {
    /// Instantiates a new worker.
    fn new(
        rx: &Receiver<PathBuf>,
        observer: &Observer,
        output: &Output,
        prefix: &Path,
        state: &Manager,
        head_branch: &str,
        ignore_errors: bool,
    ) -> Self {
        Self {
            observer: observer.clone(),
            output: output.clone(),
            prefix: prefix.to_path_buf(),
            rx: rx.clone(),
            state: state.clone(),
            head_branch: head_branch.as_bytes().into(),
            ignore_errors,
        }
    }

    /// Listens on the worker queue for RCS paths and handles them.
    async fn work(&self) -> anyhow::Result<()> {
        // recv_async() ultimately returns a RecvError in the error path, which
        // only has one possible value: Disconnected. Therefore we don't need to
        // interrogate an error return any further, it just means we should
        // terminate the worker.
        while let Ok(path) = self.rx.recv_async().await {
            if fs::metadata(&path)?.is_dir() {
                continue;
            }

            if !path.as_os_str().as_bytes().ends_with(b",v") {
                log::trace!("ignoring {} due to non-,v suffix", path.display());
                continue;
            }

            log::trace!("processing {}", path.display());
            if let Err(e) = self.handle_path(&path).await {
                log::log!(
                    if self.ignore_errors {
                        Level::Warn
                    } else {
                        Level::Error
                    },
                    "error processing {}: {:?}",
                    path.display(),
                    e
                );
                if self.ignore_errors {
                    continue;
                } else {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Handles an individual RCS file.
    async fn handle_path(&self, path: &Path) -> anyhow::Result<()> {
        // Parse the ,v file.
        let cv = comma_v::parse(&fs::read(path)?)?;

        // Set up an easier to display version of the path for logging purposes.
        let disp = path.display();

        // Calculate the real path of the file in the repository.
        let real_path = munge_raw_path(path, &self.prefix);

        // Branches and tags are defined as symbols in the RCS admin area, so we
        // have them up front rather than as we parse each revision. Let's set
        // up a revision -> tags map that we can use to send tags as we send
        // revisions, along with a branch -> head revision map for branches.
        let mut branches: HashMap<Sym, Num> = HashMap::new();
        let mut revision_tags: HashMap<Num, Vec<Sym>> = HashMap::new();
        for (tag, revision) in cv.admin.symbols.iter() {
            match revision {
                Num::Branch(_) => {
                    branches.insert(tag.clone(), revision.clone());
                }
                Num::Commit(_) => {
                    revision_tags
                        .entry(revision.clone())
                        .or_default()
                        .push(tag.clone());
                }
            }
        }

        // We also need to include the HEAD branch.
        if let Some(ref head) = cv.admin.head {
            branches.insert(Sym::from(self.head_branch.clone()), head.to_branch());
        }

        // Set up the file revision handler.
        let handler = FileRevisionHandler {
            worker: self,
            branches,
            revision_tags,
            real_path: &real_path,
        };

        // It's time to parse each revision and send each one to the various
        // places they need to go. Let's start at the HEAD.
        let head_num = match cv.head() {
            Some(num) => num,
            None => anyhow::bail!("{}: cannot find HEAD revision", disp),
        };
        log::trace!("{}: found HEAD revision {}", disp, head_num);

        handle_tree(&handler, &cv, path, None, head_num).await
    }
}

#[async_recursion]
async fn handle_tree(
    handler: &FileRevisionHandler<'_>,
    cv: &comma_v::File,
    path: &Path,
    mut contents: Option<File>,
    revision: &Num,
) -> anyhow::Result<()> {
    let mut revision = revision;

    loop {
        let (delta, delta_text) = cv.revision(revision).unwrap();
        log::trace!("{}: iterated to {}", path.display(), revision);

        if let Some(ref mut contents) = contents {
            let commands = Script::parse(delta_text.text.as_cursor()).into_command_list()?;
            contents.apply_in_place(&commands)?;
        } else {
            contents = Some(File::new(delta_text.text.as_cursor())?);
        }

        let revision_content = match contents.as_ref() {
            Some(contents) => contents.as_bytes(),
            None => {
                anyhow::bail!("unexpected lack of contents")
            }
        };

        let mark = handler
            .handle_revision(&revision_content, revision, delta, delta_text)
            .await?;
        log::trace!("{}: wrote {} to mark {:?}", path.display(), revision, mark);

        // If there are branches upwards from here, we need to also handle them.
        for branch_revision in delta.branches.iter() {
            // Note that we clone contents here: since we're modifying the contents in place each
            // time a new revision is seen, we have to have a separate state for each branch.
            handle_tree(handler, cv, path, contents.clone(), branch_revision).await?;
        }

        if let Some(next) = &delta.next {
            revision = next;
        } else {
            return Ok(());
        }
    }
}

/// Handles individual revisions of a single file.
struct FileRevisionHandler<'a> {
    worker: &'a Worker,
    branches: HashMap<Sym, Num>,
    revision_tags: HashMap<Num, Vec<Sym>>,
    real_path: &'a Path,
}

impl FileRevisionHandler<'_> {
    /// Handles a single revision of a file.
    async fn handle_revision(
        &self,
        content: &[u8],
        revision: &Num,
        delta: &Delta,
        delta_text: &DeltaText,
    ) -> anyhow::Result<Option<Mark>> {
        // Check if this revision has already been seen.
        if let Ok(revision) = self
            .worker
            .state
            .get_file_revision(self.real_path, revision.to_string().as_str())
            .await
        {
            return Ok(revision.mark.map(|mark| mark.into()));
        }

        let branch_iter = self.branches.iter().filter_map(|(name, head)| {
            if head.contains(revision).unwrap() {
                Some(name)
            } else {
                None
            }
        });

        let mark = match &delta.state {
            Some(state) if state == b"dead".as_ref() => None,
            _ => Some(self.worker.output.blob(Blob::new(content)).await?),
        };

        let id = self
            .worker
            .observer
            .file_revision(
                self.real_path,
                revision,
                branch_iter,
                mark,
                delta,
                delta_text,
            )
            .await?;

        if let Some(tags) = self.revision_tags.get(revision) {
            for tag in tags {
                self.worker.observer.tag(tag, id).await;
            }
        }

        Ok(mark)
    }
}

/// Strips CVSROOT-specific components of the file path: specifically, removing
/// the ,v suffix if present and stripping the Attic if it's the last directory
/// in the path. Returns a newly allocated OsString.
fn munge_raw_path(input: &Path, prefix: &Path) -> PathBuf {
    let unprefixed = input.strip_prefix(prefix).unwrap_or(input);

    if let Some(input_file) = unprefixed.file_name() {
        let file = strip_comma_v_suffix(input_file).unwrap_or_else(|| PathBuf::from(input_file));
        let path = strip_attic_suffix(unprefixed)
            .map(|path| path.join(file))
            .unwrap_or_else(|| input_file.into());

        path
    } else {
        unprefixed.into()
    }
}

fn strip_attic_suffix(path: &Path) -> Option<&Path> {
    path.parent()
        .map(|parent| {
            if parent.ends_with(OsStr::from_bytes(b"Attic")) {
                parent.parent()
            } else {
                Some(parent)
            }
        })
        .flatten()
}

fn strip_comma_v_suffix(file: &OsStr) -> Option<PathBuf> {
    // We use OsStr here because it has methods we need: Path doesn't allow for
    // easy slicing within path components, and doesn't consider comma a file
    // extension separator.
    if let Some(stripped) = file.as_bytes().strip_suffix(b",v") {
        return Some(PathBuf::from(OsStr::from_bytes(stripped)));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_munge {
        ($input:expr, $prefix:expr, $want:expr) => {
            assert_eq!(
                munge_raw_path(
                    Path::new(OsStr::from_bytes($input)),
                    Path::new(OsStr::from_bytes($prefix)),
                ),
                PathBuf::from(OsStr::from_bytes($want))
            )
        };
    }

    #[test]
    fn test_munge_raw_path() {
        // Basic relative and absolute cases with ,v suffixes.
        assert_munge!(b"foo", b"", b"foo");
        assert_munge!(b"foo,v", b"", b"foo");
        assert_munge!(b"foo/bar", b"", b"foo/bar");
        assert_munge!(b"/foo", b"", b"/foo");
        assert_munge!(b"/foo,v", b"", b"/foo");
        assert_munge!(b"/foo/bar,v", b"", b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar", b"", b"/foo/bar");

        // Basic Attic cases.
        assert_munge!(b"foo/Attic/bar", b"", b"foo/bar");
        assert_munge!(b"foo/Attic/bar,v", b"", b"foo/bar");
        assert_munge!(b"/foo/Attic/bar", b"", b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar,v", b"", b"/foo/bar");

        // Non-standard Attic cases where it shouldn't be stripped.
        assert_munge!(b"Attic", b"", b"Attic");
        assert_munge!(b"Attic,v", b"", b"Attic");
        assert_munge!(b"foo/Attic", b"", b"foo/Attic");
        assert_munge!(b"/foo/Attic", b"", b"/foo/Attic");
        assert_munge!(
            b"Attic/Attic/Attic/foo/bar,v",
            b"",
            b"Attic/Attic/Attic/foo/bar"
        );
        assert_munge!(b"/Attic/Attic/foo,v", b"", b"/Attic/foo");

        // Prefix stripping.
        assert_munge!(b"/foo/bar/Attic/quux,v", b"/foo/bar", b"quux");
        assert_munge!(b"/foo/bar/quux,v", b"/bar", b"/foo/bar/quux");
    }
}
