//! RCS file discovery and parsing.

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    os::unix::prelude::OsStrExt,
    path::Path,
};

use comma_v::{Delta, DeltaText, Num, Sym};
use flume::{Receiver, Sender};
use git_cvs_fast_import_process::Output;
use git_cvs_fast_import_state::Manager;
use git_fast_import::{Blob, Mark};
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
    tx: Sender<OsString>,
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
        jobs: usize,
        prefix: Option<&OsStr>,
    ) -> Self {
        // This is a multi-producer, multi-consumer channel that we use to fan
        // paths out to workers.
        let (tx, rx) = flume::unbounded::<OsString>();

        // Start each worker.
        for _i in 0..jobs {
            let worker = Worker::new(&rx, observer, output, prefix, state);
            task::spawn(async move { worker.work().await });
        }

        Self { tx }
    }

    /// Queues the given path for parsing on the next available worker.
    pub fn discover(&self, path: &OsStr) -> anyhow::Result<()> {
        Ok(self.tx.send(OsString::from(path))?)
    }
}

/// Worker represents an individual worker task processing RCS files.
struct Worker {
    observer: Observer,
    output: Output,
    prefix: Option<OsString>,
    rx: Receiver<OsString>,
    state: Manager,
}

impl Worker {
    /// Instantiates a new worker.
    fn new(
        rx: &Receiver<OsString>,
        observer: &Observer,
        output: &Output,
        prefix: Option<&OsStr>,
        state: &Manager,
    ) -> Self {
        Self {
            observer: observer.clone(),
            output: output.clone(),
            prefix: prefix.map(OsString::from),
            rx: rx.clone(),
            state: state.clone(),
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

            log::trace!("processing {}", String::from_utf8_lossy(path.as_bytes()));
            if let Err(e) = self.handle_path(&path).await {
                log::warn!(
                    "error processing {}: {:?}",
                    String::from_utf8_lossy(path.as_bytes()),
                    e
                );
                continue;
            }
        }

        Ok(())
    }

    /// Handles an individual RCS file.
    async fn handle_path(&self, path: &OsStr) -> anyhow::Result<()> {
        // Parse the ,v file.
        let cv = comma_v::parse(&fs::read(path)?)?;

        // Set up an easier to display version of the path for logging purposes.
        let disp = path.to_string_lossy();

        // Calculate the real path of the file in the repository.
        let real_path = munge_raw_path(Path::new(path), &self.prefix);

        // Tags are defined as symbols in the RCS admin area, so we have them up
        // front rather than as we parse each revision. Let's set up a revision
        // -> tags map that we can use to send tags as we send revisions.
        let mut revision_tags: HashMap<Num, Vec<Sym>> = HashMap::new();
        for (tag, revision) in cv.admin.symbols.iter() {
            revision_tags
                .entry(revision.clone())
                .or_default()
                .push(tag.clone());
        }

        // Set up the file revision handler.
        let handler = FileRevisionHandler {
            worker: self,
            revision_tags,
            real_path: &real_path,
        };

        // It's time to parse each revision and send each one to the various
        // places they need to go. Let's start at the HEAD.
        let head_num = match cv.head() {
            Some(num) => num,
            None => anyhow::bail!("{}: cannot find HEAD revision", disp),
        };
        let (mut delta, mut delta_text) = cv.revision(head_num).unwrap();
        log::trace!("{}: found HEAD revision {}", disp, head_num);
        let mut file = File::new(delta_text.text.as_cursor())?;

        let mark = handler
            .handle_revision(&file, head_num, delta, delta_text)
            .await?;
        log::trace!("{}: wrote HEAD to mark {:?}", disp, mark);

        // Now we can work our way down the branch.
        //
        // TODO: handle moving back up on branches.
        while let Some(next_num) = &delta.next {
            let rev = cv.revision(next_num).unwrap();
            delta = rev.0;
            delta_text = rev.1;

            log::trace!("{}: iterated to {}", &disp, next_num);

            let commands = Script::parse(delta_text.text.as_cursor()).into_command_list()?;
            file.apply_in_place(&commands)?;

            let mark = handler
                .handle_revision(&file, next_num, delta, delta_text)
                .await?;
            log::trace!("{}: wrote {} to mark {:?}", disp, next_num, mark);
        }

        Ok(())
    }
}

/// Handles individual revisions of a single file.
struct FileRevisionHandler<'a> {
    worker: &'a Worker,
    revision_tags: HashMap<Num, Vec<Sym>>,
    real_path: &'a OsStr,
}

impl FileRevisionHandler<'_> {
    /// Handles a single revision of a file.
    async fn handle_revision(
        &self,
        file: &File,
        revision: &Num,
        delta: &Delta,
        delta_text: &DeltaText,
    ) -> anyhow::Result<Option<Mark>> {
        // Check if this revision has already been seen.
        if let Ok(revision) = self
            .worker
            .state
            .get_file_revision(self.real_path, revision)
            .await
        {
            return Ok(revision.mark.map(|mark| mark.into()));
        }

        let mark = match &delta.state {
            Some(state) if state == b"dead".as_ref() => None,
            _ => Some(self.worker.output.blob(Blob::new(&file.as_bytes())).await?),
        };

        let id = self
            .worker
            .observer
            .file_revision(
                self.real_path,
                revision,
                &delta.branches,
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
fn munge_raw_path(input: &Path, prefix: &Option<OsString>) -> OsString {
    let unprefixed = match prefix {
        Some(prefix) => input.strip_prefix(prefix).unwrap_or(input),
        None => input,
    };

    if let Some(input_file) = unprefixed.file_name() {
        let file = strip_comma_v_suffix(input_file).unwrap_or_else(|| OsString::from(input_file));
        let path = strip_attic_suffix(unprefixed)
            .map(|path| path.join(file))
            .unwrap_or_else(|| input_file.into());

        path.into_os_string()
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

fn strip_comma_v_suffix(file: &OsStr) -> Option<OsString> {
    if let Some(stripped) = file.as_bytes().strip_suffix(b",v") {
        return Some(OsString::from(OsStr::from_bytes(stripped)));
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
                    &$prefix.map(|prefix| OsString::from(OsStr::from_bytes(prefix)))
                ),
                OsString::from(OsStr::from_bytes($want))
            )
        };
    }

    #[test]
    fn test_munge_raw_path() {
        // Basic relative and absolute cases with ,v suffixes.
        assert_munge!(b"foo", None, b"foo");
        assert_munge!(b"foo,v", None, b"foo");
        assert_munge!(b"foo/bar", None, b"foo/bar");
        assert_munge!(b"/foo", None, b"/foo");
        assert_munge!(b"/foo,v", None, b"/foo");
        assert_munge!(b"/foo/bar,v", None, b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar", None, b"/foo/bar");

        // Basic Attic cases.
        assert_munge!(b"foo/Attic/bar", None, b"foo/bar");
        assert_munge!(b"foo/Attic/bar,v", None, b"foo/bar");
        assert_munge!(b"/foo/Attic/bar", None, b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar,v", None, b"/foo/bar");

        // Non-standard Attic cases where it shouldn't be stripped.
        assert_munge!(b"Attic", None, b"Attic");
        assert_munge!(b"Attic,v", None, b"Attic");
        assert_munge!(b"foo/Attic", None, b"foo/Attic");
        assert_munge!(b"/foo/Attic", None, b"/foo/Attic");
        assert_munge!(
            b"Attic/Attic/Attic/foo/bar,v",
            None,
            b"Attic/Attic/Attic/foo/bar"
        );
        assert_munge!(b"/Attic/Attic/foo,v", None, b"/Attic/foo");

        // Prefix stripping.
        assert_munge!(b"/foo/bar/Attic/quux,v", Some(b"/foo/bar"), b"quux");
        assert_munge!(b"/foo/bar/quux,v", Some(b"/bar"), b"/foo/bar/quux");
    }
}
