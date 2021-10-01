use std::{
    ffi::{OsStr, OsString},
    panic,
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use rusqlite::{params, Connection};

use crate::{error::Error, sql};

pub struct Tag {
    join: JoinHandle<()>,
    tx: Sender<Message>,
}

struct Message {
    tag: Vec<u8>,
    path: OsString,
    revision: Vec<u8>,
}

impl Tag {
    pub(crate) fn new(conn: Connection) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();

        let join = thread::spawn(move || {
            let mut stmt = conn
                .prepare("REPLACE INTO tags (tag, file, revision) VALUES (?, ?, ?)")
                .unwrap();

            while let Ok(msg) = rx.recv() {
                stmt.execute(params![
                    &msg.tag,
                    sql::os_str(msg.path.as_os_str()),
                    &msg.revision
                ])
                .unwrap();
            }
        });

        Self { join, tx }
    }

    pub fn insert(&mut self, tag: &[u8], path: &OsStr, revision: &[u8]) -> Result<(), Error> {
        Ok(self.tx.send(Message {
            tag: tag.to_vec(),
            path: path.to_os_string(),
            revision: revision.to_vec(),
        })?)
    }

    pub fn finalise(self) {
        drop(self.tx);

        if let Err(e) = self.join.join() {
            panic::resume_unwind(e);
        }
    }
}
