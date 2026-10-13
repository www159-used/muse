// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Library, LibraryError};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn default_name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn ownership(path: &Path, exclusive: bool) -> Result<File, LibraryError> {
    let canonical = if path.exists() {
        path.canonicalize()?
    } else {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        parent.canonicalize()?.join(
            path.file_name()
                .ok_or_else(|| std::io::Error::other("library path requires a filename"))?,
        )
    };
    let mut lock = canonical.into_os_string();
    lock.push(".maintenance.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock)?;
    if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    }
    .map_err(|_| LibraryError::MaintenanceBusy)?;
    Ok(file)
}

impl Library {
    /// Requires exclusive ownership; old clients must also be closed by the caller.
    /// Returns the verified pre-upgrade backup, or None for an already current file.
    pub fn upgrade(path: impl AsRef<Path>) -> Result<Option<PathBuf>, LibraryError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "library does not exist",
            )
            .into());
        }
        let _ownership = ownership(path, true)?;
        let mut connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        crate::backup::check_schema(&connection)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version == 2 {
            return Ok(None);
        }
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let mut name = path.as_os_str().to_os_string();
        name.push(format!(".pre-v2.{suffix}.muse"));
        let backup = PathBuf::from(name);
        crate::backup::copy_to_new(&connection, &backup)?;
        connection.pragma_update(None, "foreign_keys", "OFF")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        tx.execute_batch(include_str!("schema-v2.sql"))?;
        tx.execute(
            "INSERT INTO library_descriptor VALUES(1,?1,'netease')",
            [default_name(path)],
        )?;
        crate::backup::check_integrity(&tx)?;
        tx.commit()?;
        Ok(Some(backup))
    }
}
