use std::io::{self, Read};

use rusqlite::{DatabaseName, OptionalExtension};

use crate::error::Error;

#[derive(Debug)]
pub struct Connection {
    pub(crate) conn: rusqlite::Connection,
}

impl Connection {
    pub(crate) fn new(conn: rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn get_raw_marks(&mut self) -> Result<Option<impl Read + '_>, Error> {
        Ok(
            if let Some(row_id) = self
                .conn
                .query_row::<i64, _, _>("SELECT ROWID FROM marks", [], |row| row.get(0))
                .optional()?
            {
                Some(
                    self.conn
                        .blob_open(DatabaseName::Main, "marks", "raw", row_id, true)?,
                )
            } else {
                None
            },
        )
    }

    pub fn set_raw_marks<R: Read>(&mut self, mut reader: R) -> Result<(), Error> {
        let txn = self.conn.transaction()?;

        txn.execute("DELETE FROM marks", [])?;
        let row_id: i64 = txn.query_row(
            "INSERT INTO marks (raw) VALUES ('') RETURNING ROWID",
            [],
            |row| row.get(0),
        )?;

        let mut blob = txn.blob_open(DatabaseName::Main, "marks", "raw", row_id, false)?;
        io::copy(&mut reader, &mut blob)?;
        drop(blob);

        Ok(txn.commit()?)
    }
}
