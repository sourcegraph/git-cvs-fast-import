//! Helpers for types that don't natively implement ToSql.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::ToSql;

pub(crate) fn from_time(time: &SystemTime) -> impl ToSql {
    time.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

pub(crate) fn into_time(timestamp: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(timestamp)
}
