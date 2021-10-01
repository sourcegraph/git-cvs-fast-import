//! Helpers for types that don't natively implement ToSql.

use std::{
    ffi::OsStr,
    os::unix::prelude::OsStrExt,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::ToSql;

pub(crate) fn os_str(os: &OsStr) -> impl ToSql + '_ {
    os.as_bytes()
}

pub(crate) fn time(time: &SystemTime) -> impl ToSql {
    time.duration_since(UNIX_EPOCH).unwrap().as_secs()
}
