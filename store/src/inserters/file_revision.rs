use std::ffi::OsString;
use std::panic;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::{ffi::OsStr, time::SystemTime};

use rusqlite::{params, Connection};

use crate::error::Error;
use crate::sql;

pub struct FileRevision {
    join: JoinHandle<()>,
    tx: Sender<Message>,
}

struct Message {
    path: OsString,
    revision: Vec<u8>,
    time: SystemTime,
    mark: Option<usize>,
    branches: Vec<Vec<u8>>,
}

impl FileRevision {
    pub(crate) fn new(conn: Connection) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();

        // SQLite isn't generally thread safe (most notably, statements can't
        // even move threads, although connections can as long as they're only
        // used from one thread at a time), and rusqlite doesn't provide any
        // async API as a result. To make it easier for async code to use this,
        // we'll hide all of this on a worker thread (a real thread, not a green
        // thread), and then handle inserts as messages going to that thread.
        let join = thread::spawn(move || {
            let mut branch_stmt = conn
                .prepare(
                    "REPLACE INTO file_revision_branches (file_revision, branch) VALUES (?, ?)",
                )
                .unwrap();
            let mut revision_stmt = conn
                .prepare(
                    "REPLACE INTO file_revisions (path, revision, time, mark) VALUES (?, ?, ?, ?)",
                )
                .unwrap();

            while let Ok(msg) = rx.recv() {
                let id = revision_stmt
                    .insert(params![
                        sql::os_str(msg.path.as_os_str()),
                        &msg.revision,
                        sql::time(&msg.time),
                        msg.mark,
                    ])
                    .unwrap();

                for branch in msg.branches.iter() {
                    branch_stmt.execute(params![id, branch]).unwrap();
                }
            }
        });

        Self { join, tx }
    }

    pub fn insert(
        &mut self,
        path: &OsStr,
        revision: &[u8],
        time: &SystemTime,
        mark: Option<usize>,
        branches: &[Vec<u8>],
    ) -> Result<(), Error> {
        Ok(self.tx.send(Message {
            path: path.to_os_string(),
            revision: revision.to_vec(),
            time: *time,
            mark,
            branches: branches.to_vec(),
        })?)
    }

    pub fn finalise(self) {
        drop(self.tx);

        if let Err(e) = self.join.join() {
            panic::resume_unwind(e);
        }
    }
}
