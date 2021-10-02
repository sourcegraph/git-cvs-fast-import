use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

mod embedded {
    refinery::embed_migrations!("./src/migrations");
}

mod connection;
pub use connection::Connection;

mod error;
pub use error::Error;

mod sql;

mod types;
pub use types::*;

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

    fn open_connection(&self) -> rusqlite::Result<rusqlite::Connection> {
        rusqlite::Connection::open(self.path.as_path())
    }
}
