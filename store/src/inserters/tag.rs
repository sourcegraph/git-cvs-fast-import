use std::{
    ffi::{OsStr, OsString},
    sync::mpsc::{self, Sender},
    thread,
};

use rusqlite::{params, Connection};

use crate::{error::Error, sql};

pub struct Tag {
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

        thread::spawn(move || {
            let mut stmt = conn
                .prepare("INSERT INTO tags (tag, file, revision) VALUES (?, ?, ?)")
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

        Self { tx }
    }

    pub fn insert(&mut self, tag: &[u8], path: &OsStr, revision: &[u8]) -> Result<(), Error> {
        Ok(self.tx.send(Message {
            tag: tag.to_vec(),
            path: path.to_os_string(),
            revision: revision.to_vec(),
        })?)
    }
}
