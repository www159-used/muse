// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs, io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use rusqlite::{
    Connection, OpenFlags, Transaction,
    backup::{Backup, StepResult},
};
use tempfile::{Builder, NamedTempFile};

use crate::{
    Library, LibraryError,
    storage::{APPLICATION_ID, SCHEMA_VERSION},
};

impl Library {
    /// Saves a committed snapshot, including WAL contents, to a new file.
    ///
    /// Audio, attachments and import records are preserved. The destination and
    /// its SQLite sidecars must not exist. Other WAL connections can keep writing.
    pub fn backup(&self, destination: impl AsRef<Path>) -> Result<(), LibraryError> {
        copy_to_new(&self.connection, destination.as_ref())
    }

    /// Copies an existing library or backup to a new library path.
    ///
    /// The source is opened read-only and is neither created nor upgraded.
    /// Foreign, unsupported or invalid databases are rejected. This can also
    /// back up a library without opening it through the normal writable API.
    pub fn restore(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<(), LibraryError> {
        let source_path = source.as_ref();
        if !source_path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "library does not exist",
            )
            .into());
        }
        let _ownership = crate::upgrade::ownership(source_path, false)?;
        let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.busy_timeout(Duration::from_secs(5))?;
        copy_to_new(&source, destination.as_ref())
    }
}

pub(crate) fn copy_to_new(source: &Connection, destination: &Path) -> Result<(), LibraryError> {
    ensure_new_destination(destination)?;
    let snapshot = source.unchecked_transaction()?;
    // Force a read before querying header pragmas or copying pages. The transaction
    // pins this committed WAL snapshot even when another connection later writes.
    snapshot.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
        row.get::<_, i64>(0)
    })?;
    check_schema(&snapshot)?;
    publish_snapshot(snapshot, destination)
}

fn publish_snapshot(snapshot: Transaction<'_>, destination: &Path) -> Result<(), LibraryError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = Builder::new().prefix(".muse-backup-").tempdir_in(parent)?;
    let file = NamedTempFile::new_in(directory.path())?;
    let mut output = Connection::open(file.path())?;
    {
        let backup = Backup::new(&snapshot, &mut output)?;
        let mut blocked_since = None;
        loop {
            match backup.step(128)? {
                StepResult::Done => break,
                StepResult::More => blocked_since = None,
                StepResult::Busy | StepResult::Locked => {
                    let since = blocked_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= Duration::from_secs(5) {
                        return Err(LibraryError::BackupBusy);
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                _ => {
                    return Err(LibraryError::InvalidBackup(
                        "unknown SQLite backup state".into(),
                    ));
                }
            }
        }
    }
    snapshot.rollback()?; // Release the WAL snapshot before validating the output.
    check_schema(&output)?;
    check_integrity(&output)?;
    output.pragma_update(None, "journal_mode", "DELETE")?;
    output.close().map_err(|(_, error)| error)?;
    file.as_file().sync_all()?;
    ensure_new_destination(destination)?;
    file.persist_noclobber(destination).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            LibraryError::DestinationExists(destination.to_path_buf())
        } else {
            LibraryError::Io(error.error)
        }
    })?;
    Ok(())
}

fn ensure_new_destination(destination: &Path) -> Result<(), LibraryError> {
    if destination.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the destination must be a file path",
        )
        .into());
    }
    let mut paths = vec![destination.to_path_buf()];
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = destination.as_os_str().to_os_string();
        sidecar.push(suffix);
        paths.push(PathBuf::from(sidecar));
    }
    for path in paths {
        match fs::symlink_metadata(&path) {
            Ok(_) => return Err(LibraryError::DestinationExists(path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(crate) fn check_schema(connection: &Connection) -> Result<(), LibraryError> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(LibraryError::ForeignDatabase);
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(LibraryError::NewerSchema(version));
    }
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Err(LibraryError::InvalidBackup(format!(
            "unsupported schema version {version}"
        )));
    }
    if version >= 2 {
        connection.prepare("SELECT name,kind FROM library_descriptor WHERE singleton=1")?;
        connection.prepare("SELECT tagged_title,tagged_artist FROM tracks LIMIT 0")?;
        connection.prepare("SELECT verified FROM import_sources LIMIT 0")?;
        connection.prepare("SELECT source_hash,path,container,title,artist,album,netease_id,content_hash FROM source_observations LIMIT 0")?;
        connection.prepare("SELECT track_id,source_hash,kind,content_hash,mime_type,data,attribution FROM attachment_observations LIMIT 0")?;
    }
    if version >= 3 {
        connection.prepare("SELECT id, name, kind FROM playlists LIMIT 0")?;
        connection.prepare("SELECT playlist_id, position, track_id FROM playlist_items LIMIT 0")?;
    }
    for query in [
        "SELECT id, title, artist, album, format, duration_ms, netease_id, content_hash FROM tracks LIMIT 0",
        "SELECT content_hash, data FROM audio LIMIT 0",
        "SELECT source_hash, track_id FROM import_sources LIMIT 0",
        "SELECT track_id, kind, content_hash, mime_type, data FROM attachments LIMIT 0",
    ] {
        connection.prepare(query)?;
    }
    Ok(())
}

pub(crate) fn check_integrity(connection: &Connection) -> Result<(), LibraryError> {
    let mut statement = connection.prepare("PRAGMA integrity_check")?;
    let mut rows = statement.query([])?;
    let mut ok = false;
    while let Some(row) = rows.next()? {
        let result: String = row.get(0)?;
        if result != "ok" {
            return Err(LibraryError::InvalidBackup(result));
        }
        ok = true;
    }
    if !ok {
        return Err(LibraryError::InvalidBackup(
            "the integrity check returned no results".into(),
        ));
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    if let Some(row) = statement.query([])?.next()? {
        let table: String = row.get(0)?;
        return Err(LibraryError::InvalidBackup(format!(
            "{table} contains a foreign key violation"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_snapshot_excludes_a_later_committed_song_and_deletion() {
        let temporary = tempfile::TempDir::new().unwrap();
        let database = temporary.path().join("library.muse");
        let mut library = Library::open(&database).unwrap();
        let mp3 = temporary.path().join("first.mp3");
        // More than one 128-page backup step, using valid repeated silent frames.
        let audio = include_bytes!("../tests/fixtures/silence.mp3").repeat(1024);
        fs::write(&mp3, &audio).unwrap();
        assert_eq!(library.import(&mp3).imported, 1);
        let expected = library.list().unwrap();
        let snapshot = library.connection.unchecked_transaction().unwrap();
        snapshot
            .query_row("SELECT count(*) FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        let mut writer = Library::open(&database).unwrap();
        assert!(writer.remove(expected[0].id).unwrap());
        let flac = temporary.path().join("later.flac");
        fs::write(&flac, include_bytes!("../tests/fixtures/tone.flac")).unwrap();
        assert_eq!(writer.import(&flac).imported, 1);
        let destination = temporary.path().join("snapshot.muse");
        publish_snapshot(snapshot, &destination).unwrap();
        let copy = Library::open(destination).unwrap();
        assert_eq!(copy.list().unwrap(), expected);
        assert_eq!(copy.open_audio(expected[0].id).unwrap().into_inner(), audio);
        assert_eq!(writer.list().unwrap()[0].format, "flac");
    }

    #[test]
    fn destination_created_after_precheck_survives_publication_failure() {
        let temporary = tempfile::TempDir::new().unwrap();
        let library = Library::open(temporary.path().join("library.muse")).unwrap();
        let destination = temporary.path().join("competing.muse");
        ensure_new_destination(&destination).unwrap();
        let snapshot = library.connection.unchecked_transaction().unwrap();
        snapshot
            .query_row("SELECT count(*) FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        fs::write(&destination, b"another writer's file").unwrap();
        assert!(matches!(
            publish_snapshot(snapshot, &destination),
            Err(LibraryError::DestinationExists(_))
        ));
        assert_eq!(fs::read(destination).unwrap(), b"another writer's file");
        assert!(fs::read_dir(temporary.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".muse-backup-")
        }));
    }
}
