use std::{
    convert::TryInto,
    io::{self, Read},
    time::SystemTime,
};

use rusqlite::{blob::ZeroBlob, params, DatabaseName, OptionalExtension};

use crate::{error::Error, sql, ID};

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

    #[allow(clippy::too_many_arguments)]
    pub fn insert_file_revision_commit<I>(
        &mut self,
        path: &[u8],
        revision: &[u8],
        mark: Option<usize>,
        author: &[u8],
        message: &[u8],
        time: &SystemTime,
        branches: I,
    ) -> Result<ID, Error>
    where
        I: Iterator,
        I::Item: AsRef<[u8]>,
    {
        let id = self
            .conn
            .prepare_cached(
                "
                INSERT INTO
                    file_revision_commits
                (path, revision, mark, author, message, time)
                VALUES
                (?, ?, ?, ?, ?, ?)
                ",
            )?
            .insert(params![
                path,
                revision,
                mark,
                author,
                message,
                sql::time(time),
            ])?;

        let mut stmt = self.conn.prepare_cached(
            "
            INSERT INTO
                file_revision_commit_branches
            (file_revision_commit_id, branch)
            VALUES
            (?, ?)
            ",
        )?;
        for branch in branches {
            stmt.execute(params![id, branch.as_ref()])?;
        }

        Ok(id)
    }

    pub fn insert_patchset<I>(
        &mut self,
        mark: usize,
        branch: &[u8],
        time: &SystemTime,
        file_revision_commits: I,
    ) -> Result<ID, Error>
    where
        I: Iterator<Item = ID>,
    {
        let patchset_id = self
            .conn
            .prepare_cached("INSERT INTO patchsets (mark, branch, time) VALUES (?, ?, ?)")?
            .insert(params![mark, branch, sql::time(time)])?;

        let mut stmt = self.conn.prepare(
            "
            INSERT INTO
                file_revision_commit_patchsets
            (file_revision_commit_id, patchset_id)
            VALUES
            (?, ?)
            ",
        )?;
        for id in file_revision_commits {
            stmt.execute(params![id, patchset_id])?;
        }

        Ok(patchset_id)
    }

    pub fn insert_tag<I>(&mut self, tag: &[u8], file_revision_commits: I) -> Result<(), Error>
    where
        I: Iterator<Item = ID>,
    {
        let mut stmt = self
            .conn
            .prepare_cached("INSERT INTO tags (tag, file_revision_commit_id) VALUES (?, ?)")?;
        for id in file_revision_commits {
            stmt.execute(params![tag, id])?;
        }

        Ok(())
    }

    pub fn set_raw_marks<R: Read>(&mut self, mut reader: R, size: usize) -> Result<(), Error> {
        // Blobs can only be up to 2^31-1 bytes in size in SQLite, so rusqlite
        // sensibly requires an i32. However, we're pretty much always going to
        // think about lengths as usize outside of this function, so let's do
        // the conversion here.
        //
        // A possible enhancement would be to split the mark file across
        // multiple records if needed.
        let blob_size = match size.try_into() {
            Ok(size) => size,
            Err(_) => {
                return Err(Error::LargeMarkFile {
                    max: i32::MAX,
                    size,
                });
            }
        };

        let txn = self.conn.transaction()?;

        txn.execute("DELETE FROM marks", [])?;
        let row_id: i64 = txn.query_row(
            "INSERT INTO marks (raw) VALUES (?) RETURNING ROWID",
            [ZeroBlob(blob_size)],
            |row| row.get(0),
        )?;

        let mut blob = txn.blob_open(DatabaseName::Main, "marks", "raw", row_id, false)?;
        io::copy(&mut reader, &mut blob)?;
        drop(blob);

        Ok(txn.commit()?)
    }
}
