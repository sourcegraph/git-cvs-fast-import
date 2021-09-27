use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

mod embedded {
    refinery::embed_migrations!("./src/migrations");
}

mod connection;
mod sql;
pub use connection::Connection;

mod error;
pub use error::Error;

mod inserters;
pub use inserters::{
    file_revision::FileRevision as FileRevisionInserter, patchset::PatchSet as PatchSetInserter,
    tag::Tag as TagInserter,
};

#[derive(Debug, Clone)]
pub struct Store {
    path: Arc<PathBuf>,
}

impl Store {
    pub fn new<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let store = Self {
            path: Arc::new(path.as_ref().to_path_buf()),
        };

        // Apply the migrations now so we don't have to do it on each new
        // connection.
        embedded::migrations::runner().run(&mut store.open_connection()?)?;

        Ok(store)
    }

    pub fn connection(&self) -> Result<Connection, Error> {
        Ok(Connection::new(self.open_connection()?))
    }

    pub fn file_revision_inserter(&self) -> Result<FileRevisionInserter, Error> {
        Ok(FileRevisionInserter::new(self.open_connection()?))
    }

    pub fn patchset_inserter(&self) -> Result<PatchSetInserter, Error> {
        Ok(PatchSetInserter::new(self.open_connection()?))
    }

    pub fn tag_inserter(&self) -> Result<TagInserter, Error> {
        Ok(TagInserter::new(self.open_connection()?))
    }

    fn open_connection(&self) -> rusqlite::Result<rusqlite::Connection> {
        rusqlite::Connection::open(self.path.as_path())
    }
}
