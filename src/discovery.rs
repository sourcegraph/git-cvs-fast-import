use std::{
    ffi::{OsStr, OsString},
    iter::FromIterator,
    os::unix::prelude::{OsStrExt, OsStringExt},
};

use flume::Sender;
use git_fast_import::Blob;
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
    let real_path = strip_comma_v_suffix(path);

    // Start at the head and work our way down.
    let num = match cv.head() {
        Some(num) => num,
        None => anyhow::bail!("{}: cannot find HEAD revision", disp),
    };
    let (mut delta, mut delta_text) = cv.revision(num).unwrap();
    log::trace!("{}: found HEAD revision {}", disp, num);
    let mut file = File::new(delta_text.text.as_cursor())?;

    // TODO: detect deletion and prevent a mark.
    let mark = output.blob(Blob::new(&file.as_bytes())).await?;
    commit
        .observe(&real_path, Some(mark), delta, delta_text)
        .await?;

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

        let mark = output.blob(Blob::new(&file.as_bytes())).await?;
        commit
            .observe(&real_path, Some(mark), delta, delta_text)
            .await?;
    }

    Ok(())
}

fn strip_comma_v_suffix(input: &OsStr) -> OsString {
    let mut buf = Vec::from(input.as_bytes());
    if buf.ends_with(b",v") {
        buf.pop();
        buf.pop();
    }

    OsString::from_vec(buf)
}
