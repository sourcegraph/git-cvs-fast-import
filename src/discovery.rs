use std::{
    ffi::{OsStr, OsString},
    os::unix::prelude::OsStrExt,
    path::Path,
};

use comma_v::{Delta, DeltaText};
use flume::Sender;
use git_fast_import::{Blob, Mark};
use rcs_ed::{File, Script};
use tokio::task;

use crate::{commit, output::Output};

#[derive(Debug, Clone)]
pub(crate) struct Discovery {
    tx: Sender<OsString>,
}

impl Discovery {
    pub fn new(output: &Output, commit: &commit::Commit, jobs: usize) -> Self {
        let (tx, rx) = flume::unbounded::<OsString>();

        for _i in 0..jobs {
            let local_rx = rx.clone();
            let local_commit = commit.clone();
            let local_output = output.clone();

            task::spawn(async move {
                loop {
                    let path = local_rx.recv_async().await?;
                    log::trace!("processing {}", String::from_utf8_lossy(path.as_bytes()));
                    if let Err(e) = handle_path(&local_output, &local_commit, &path).await {
                        log::warn!(
                            "error processing {}: {:?}",
                            String::from_utf8_lossy(path.as_bytes()),
                            e
                        );
                        continue;
                    }
                }

                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            });
        }

        Self { tx }
    }

    pub fn discover(&self, path: &OsStr) -> anyhow::Result<()> {
        Ok(self.tx.send(OsString::from(path))?)
    }
}

async fn handle_path(output: &Output, commit: &commit::Commit, path: &OsStr) -> anyhow::Result<()> {
    let cv = comma_v::parse(&std::fs::read(path)?)?;
    let disp = path.to_string_lossy();
    let real_path = munge_raw_path(Path::new(path));

    // Start at the head and work our way down.
    let num = match cv.head() {
        Some(num) => num,
        None => anyhow::bail!("{}: cannot find HEAD revision", disp),
    };
    let (mut delta, mut delta_text) = cv.revision(num).unwrap();
    log::trace!("{}: found HEAD revision {}", disp, num);
    let mut file = File::new(delta_text.text.as_cursor())?;

    let mark = handle_file_version(output, commit, &file, delta, delta_text, &real_path).await?;
    log::trace!("{}: wrote HEAD to mark {:?}", disp, mark);

    loop {
        // TODO: handle branches.
        let num = match &delta.next {
            Some(next) => next,
            None => {
                break;
            }
        };
        let rev = cv.revision(num).unwrap();
        delta = rev.0;
        delta_text = rev.1;

        log::trace!("{}: iterated to {}", &disp, num);

        let commands = Script::parse(delta_text.text.as_cursor()).into_command_list()?;
        file.apply_in_place(&commands)?;

        let mark =
            handle_file_version(output, commit, &file, delta, delta_text, &real_path).await?;
        log::trace!("{}: wrote {} to mark {:?}", disp, num, mark);
    }

    Ok(())
}

async fn handle_file_version(
    output: &Output,
    commit: &commit::Commit,
    file: &File,
    delta: &Delta,
    delta_text: &DeltaText,
    real_path: &OsStr,
) -> anyhow::Result<Option<Mark>> {
    let mark = match &delta.state {
        Some(state) if state == b"dead".as_ref() => None,
        _ => Some(output.blob(Blob::new(&file.as_bytes())).await?),
    };

    commit.observe(&real_path, mark, delta, delta_text).await?;
    Ok(mark)
}

/// Strips CVSROOT-specific components of the file path: specifically, removing
/// the ,v suffix if present and stripping the Attic if it's the last directory
/// in the path. Returns a newly allocated OsString.
fn munge_raw_path(input: &Path) -> OsString {
    if let Some(input_file) = input.file_name() {
        let file = strip_comma_v_suffix(input_file).unwrap_or_else(|| OsString::from(input_file));
        let path = strip_attic_suffix(input)
            .map(|path| path.join(file))
            .unwrap_or_else(|| input_file.into());

        path.into_os_string()
    } else {
        input.into()
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
        ($input:expr, $want:expr) => {
            assert_eq!(
                munge_raw_path(Path::new(OsStr::from_bytes($input))),
                OsString::from(OsStr::from_bytes($want))
            )
        };
    }

    #[test]
    fn test_munge_raw_path() {
        // Basic relative and absolute cases with ,v suffixes.
        assert_munge!(b"foo", b"foo");
        assert_munge!(b"foo,v", b"foo");
        assert_munge!(b"foo/bar", b"foo/bar");
        assert_munge!(b"/foo", b"/foo");
        assert_munge!(b"/foo,v", b"/foo");
        assert_munge!(b"/foo/bar,v", b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar", b"/foo/bar");

        // Basic Attic cases.
        assert_munge!(b"foo/Attic/bar", b"foo/bar");
        assert_munge!(b"foo/Attic/bar,v", b"foo/bar");
        assert_munge!(b"/foo/Attic/bar", b"/foo/bar");
        assert_munge!(b"/foo/Attic/bar,v", b"/foo/bar");

        // Non-standard Attic cases where it shouldn't be stripped.
        assert_munge!(b"Attic", b"Attic");
        assert_munge!(b"Attic,v", b"Attic");
        assert_munge!(b"foo/Attic", b"foo/Attic");
        assert_munge!(b"/foo/Attic", b"/foo/Attic");
        assert_munge!(b"Attic/Attic/Attic/foo/bar,v", b"Attic/Attic/Attic/foo/bar");
        assert_munge!(b"/Attic/Attic/foo,v", b"/Attic/foo");
    }
}
