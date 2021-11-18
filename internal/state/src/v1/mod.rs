//! v1 contains the data types for the original v1 state format: `bincode`
//! requires that data types be exactly the same for deserialisation.

use std::{io::Read, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::{sync::RwLock, task};

use crate::{Error, Manager};

pub(crate) mod file_revision;
pub(crate) mod patchset;
pub(crate) mod tag;

#[derive(Deserialize, Serialize)]
struct Ser {
    version: u8,
    file_revisions: Vec<u8>,
    patchsets: Vec<u8>,
    tags: Vec<u8>,
    raw_marks: Vec<u8>,
}

pub async fn deserialize_from<R>(reader: R) -> Result<Manager, Error>
where
    R: Read,
{
    let ser: Ser = bincode::deserialize_from(reader)?;

    if ser.version != 1 {
        return Err(Error::UnknownSerialisationVersion(ser.version));
    }

    let file_revisions = ser.file_revisions;
    let patchsets = ser.patchsets;
    let tags = ser.tags;
    let raw_marks = ser.raw_marks;

    // Note that we deserialise into the v1 data types here.
    let (file_revisions, patchsets, tags, raw_marks) = tokio::try_join!(
        task::spawn(async move { bincode::deserialize::<file_revision::Store>(&file_revisions) }),
        task::spawn(async move { bincode::deserialize::<patchset::Store>(&patchsets) }),
        task::spawn(async move { bincode::deserialize::<tag::Store>(&tags) }),
        task::spawn(async move { bincode::deserialize(&raw_marks) }),
    )
    .unwrap();

    // Now we can use .into() to convert the v1 data types to v2.
    Ok(Manager {
        file_revisions: Arc::new(RwLock::new(file_revisions?.into())),
        patchsets: Arc::new(RwLock::new(patchsets?.into())),
        tags: Arc::new(RwLock::new(tags?.into())),
        raw_marks: Arc::new(RwLock::new(raw_marks?)),
    })
}
