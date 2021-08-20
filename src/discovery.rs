use std::{
    ffi::{OsStr, OsString},
    os::unix::prelude::OsStrExt,
};

use flume::Sender;
use git_fast_import::Blob;
use rcs_ed::{File, Script};
use tokio::task;

use crate::output::Output;

#[derive(Debug, Clone)]
pub(crate) struct Discovery {
    tx: Sender<OsString>,
}

impl Discovery {
    pub fn new(output: &Output, jobs: usize) -> Self {
        let (tx, rx) = flume::unbounded::<OsString>();

        for _i in 0..jobs {
            let local_rx = rx.clone();
            let local_output = output.clone();

            task::spawn(async move {
                loop {
                    let path = local_rx.recv_async().await?;
                    log::trace!("processing {}", String::from_utf8_lossy(path.as_bytes()));
                    if let Err(e) = handle_path(&local_output, &path).await {
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

async fn handle_path(output: &Output, path: &OsStr) -> anyhow::Result<()> {
    let cv = comma_v::parse(&std::fs::read(path)?)?;
    let disp = path.to_string_lossy();

    // Start at the head and work our way down.
    let (mut num, mut delta_text) = cv.head_delta_text().unwrap();
    log::trace!(
        "found HEAD {} for file {}",
        num,
        String::from_utf8_lossy(path.as_bytes())
    );
    let mut file = File::new(delta_text.text.as_cursor())?;

    // TODO: do something with the mark.
    output.blob(Blob::new(&file.as_bytes())).await?;

    loop {
        // TODO: handle branches.
        match cv.delta.get(num) {
            Some(delta) => match &delta.next {
                Some(next) => {
                    num = next;
                }
                None => {
                    break;
                }
            },
            None => {
                anyhow::bail!(
                    "cannot find delta {}, even though we got it from somewhere!",
                    num
                )
            }
        }

        log::trace!("{}: iterated to {}", &disp, num);

        delta_text = match cv.delta_text.get(num) {
            Some(dt) => dt,
            None => anyhow::bail!("cannot find delta text {}", num),
        };

        let commands = Script::parse(delta_text.text.as_cursor()).into_command_list()?;
        eprintln!("{} parsed: {:?}", &disp, &commands);
        file.apply_in_place(&commands)?;

        // TODO: do something with the mark.
        output.blob(Blob::new(&file.as_bytes())).await?;
    }

    Ok(())
}
