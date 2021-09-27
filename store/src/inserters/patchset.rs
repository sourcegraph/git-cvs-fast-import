use std::{
    ffi::{OsStr, OsString},
    sync::mpsc::{self, Sender},
    thread,
    time::SystemTime,
};

use rusqlite::{params, Connection};

use crate::{error::Error, sql};

pub struct PatchSet {
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

        thread::spawn(move || {
            let mut patchset_stmt = conn
                .prepare("INSERT INTO patchsets (mark, branch, time) VALUES (?, ?, ?)")
                .unwrap();
            let mut revision_stmt=conn.prepare("INSERT INTO patchset_file_revisions (patchset, file_revision) VALUES (?, (SELECT id FROM file_revisions WHERE path = ? AND revision = ?))").unwrap();

            while let Ok(msg) = rx.recv() {
                let id = patchset_stmt
                    .insert(params![msg.mark, &msg.branch, sql::time(&msg.time)])
                    .unwrap();

                for (path, revision) in msg.revisions.into_iter() {
                    revision_stmt
                        .execute(params![id, sql::os_str(path.as_os_str()), &revision])
                        .unwrap();
                }
            }
        });

        Self { tx }
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
}
