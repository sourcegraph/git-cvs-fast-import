use std::{
    ffi::{OsStr, OsString},
    panic,
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::SystemTime,
};

use rusqlite::{params, Connection};

use crate::{error::Error, sql};

pub struct PatchSet {
    join: JoinHandle<()>,
    tx: Sender<Message>,
}

struct Message {
    mark: usize,
    branch: Vec<u8>,
    time: SystemTime,
    revisions: Vec<(OsString, Vec<u8>)>,
}

impl PatchSet {
    pub(crate) fn new(conn: Connection) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();

        let join = thread::spawn(move || {
            let mut patchset_stmt = conn
                .prepare("REPLACE INTO patchsets (mark, branch, time) VALUES (?, ?, ?)")
                .unwrap();
            let mut revision_stmt = conn.prepare("REPLACE INTO patchset_file_revisions (patchset, file_revision) VALUES (?, (SELECT id FROM file_revisions WHERE path = ? AND revision = ?))").unwrap();

            while let Ok(msg) = rx.recv() {
                patchset_stmt
                    .execute(params![msg.mark, &msg.branch, sql::time(&msg.time)])
                    .unwrap();

                for (path, revision) in msg.revisions.into_iter() {
                    revision_stmt
                        .execute(params![msg.mark, sql::os_str(path.as_os_str()), &revision])
                        .unwrap();
                }
            }
        });

        Self { join, tx }
    }

    pub fn insert<'a, I>(
        &mut self,
        mark: usize,
        branch: &[u8],
        time: &SystemTime,
        revisions: I,
    ) -> Result<(), Error>
    where
        I: Iterator<Item = (&'a OsStr, &'a [u8])>,
    {
        Ok(self.tx.send(Message {
            mark,
            branch: branch.to_vec(),
            time: *time,
            revisions: revisions
                .map(|(path, revision)| (path.to_os_string(), revision.to_vec()))
                .collect(),
        })?)
    }

    pub fn finalise(self) {
        drop(self.tx);

        if let Err(e) = self.join.join() {
            panic::resume_unwind(e);
        }
    }
}
