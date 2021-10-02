use std::{
    convert::TryInto,
    io::{self, Read},
    time::SystemTime,
};

use rusqlite::{blob::ZeroBlob, params, DatabaseName, OptionalExtension};

use crate::{error::Error, sql, FileRevisionCommit, PatchSet, ID};

#[derive(Debug)]
pub struct Connection {
    pub(crate) conn: rusqlite::Connection,
}

impl Connection {
    pub(crate) fn new(conn: rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn get_file_revisions<F, E>(&mut self, mut f: F) -> Result<(), Box<dyn std::error::Error>>
    where
        E: std::error::Error + 'static,
        F: FnMut(FileRevisionCommit) -> Result<(), E>,
    {
        let mut file_revision_stmt = self.conn.prepare_cached(
            "
            SELECT
                id,
                path,
                revision,
                mark,
                author,
                message,
                time
            FROM
                file_revision_commits
            ",
        )?;

        let mut branch_stmt = self.conn.prepare_cached(
            "
            SELECT
                branch
            FROM
                file_revision_commit_branches
            WHERE
                file_revision_commit_id = ?
            ",
        )?;

        let mut rows = file_revision_stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id = row.get(0)?;
            let branches: Result<Vec<Vec<u8>>, rusqlite::Error> =
                branch_stmt.query_map([id], |row| row.get(0))?.collect();

            f(FileRevisionCommit {
                id,
                path: row.get(1)?,
                revision: row.get(2)?,
                mark: row.get(3)?,
                author: row.get(4)?,
                message: row.get(5)?,
                time: sql::into_time(row.get(6)?),
                branches: branches?,
            })?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_file_revision_commit<I>(
        &mut self,
        path: &[u8],
        revision: &[u8],
        mark: Option<usize>,
        author: &str,
        message: &str,
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
                sql::from_time(time),
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

    pub fn get_patchsets<F, E>(&mut self, mut f: F) -> Result<(), Box<dyn std::error::Error>>
    where
        E: std::error::Error + 'static,
        F: FnMut(PatchSet) -> Result<(), E>,
    {
        let mut patchset_stmt = self.conn.prepare_cached(
            "
            SELECT
                id,
                mark,
                branch,
                time
            FROM
                patchsets
          ",
        )?;

        let mut file_revision_stmt = self.conn.prepare_cached(
            "
            SELECT
                file_revision_commit_id
            FROM
                file_revision_commit_patchsets
            WHERE
                patchset_id = ?
            ",
        )?;

        let mut rows = patchset_stmt.query([])?;
        while let Some(row) = rows.next()? {
            let patchset_id = row.get(0)?;
            let file_revisions: Result<Vec<ID>, rusqlite::Error> = file_revision_stmt
                .query_map([patchset_id], |row| row.get(0))?
                .collect();

            f(PatchSet {
                id: patchset_id,
                mark: row.get(1)?,
                branch: row.get(2)?,
                time: sql::into_time(row.get(3)?),
                file_revisions: file_revisions?,
            })?;
        }

        Ok(())
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
            .insert(params![mark, branch, sql::from_time(time)])?;

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

    pub fn get_tags<F, E>(&mut self, mut f: F) -> Result<(), Box<dyn std::error::Error>>
    where
        E: std::error::Error + 'static,
        F: FnMut(Vec<u8>, Vec<ID>) -> Result<(), E>,
    {
        let mut stmt = self.conn.prepare_cached(
            "
        SELECT
            id,
            tag,
            file_revision_commit_id
        FROM
            tags
        ORDER BY
            tag
        ",
        )?;

        let mut current_tag: Option<(Vec<u8>, Vec<ID>)> = None;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let tag_name = row.get(1)?;
            let id = row.get(2)?;

            match current_tag.take() {
                Some((current_tag_name, mut ids)) if current_tag_name == tag_name => {
                    ids.push(id);
                    current_tag = Some((current_tag_name, ids));
                }
                Some((current_tag_name, ids)) => {
                    f(current_tag_name, ids)?;
                    current_tag = Some((tag_name, vec![id]));
                }
                None => {
                    current_tag = Some((tag_name, vec![id]));
                }
            }
        }

        if let Some((tag_name, ids)) = current_tag.take() {
            f(tag_name, ids)?;
        }

        Ok(())
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
