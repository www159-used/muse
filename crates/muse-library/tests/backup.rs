// SPDX-License-Identifier: AGPL-3.0-only

use std::{fs, path::Path};

use muse_library::{Library, LibraryError};
use rusqlite::Connection;
use tempfile::TempDir;

const MP3: &[u8] = include_bytes!("fixtures/silence.mp3");
const FLAC: &[u8] = include_bytes!("fixtures/tone.flac");
const NCM: &[u8] = include_bytes!("fixtures/fixture.ncm");

fn assert_no_temporary_files(directory: &Path) {
    assert!(fs::read_dir(directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".muse-backup-")
    }));
}

#[test]
fn wal_snapshot_preserves_audio_attachments_sources_and_restores_independently() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let mut library = Library::open(&database).unwrap();
    let observer = Connection::open(&database).unwrap();
    observer
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    let sources = temporary.path().join("sources");
    fs::create_dir(&sources).unwrap();
    fs::write(sources.join("song.ncm"), NCM).unwrap();
    fs::write(sources.join("same.mp3"), MP3).unwrap();
    fs::write(sources.join("tone.flac"), FLAC).unwrap();
    fs::write(sources.join("song.lrc"), b"[00:00]fixture lyrics").unwrap();
    let report = library.import(&sources);
    assert_eq!((report.imported, report.duplicates), (2, 1), "{report:?}");
    assert!(report.failed.is_empty());
    assert!(
        fs::metadata(temporary.path().join("library.muse-wal"))
            .unwrap()
            .len()
            > 0
    );

    // Show that a main-file copy would omit these committed tracks.
    let raw_copy = temporary.path().join("raw.sqlite");
    fs::copy(&database, &raw_copy).unwrap();
    let count: i64 = Connection::open(&raw_copy)
        .unwrap()
        .query_row("SELECT count(*) FROM tracks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    let expected = library.list().unwrap();
    let attachments: Vec<_> = expected
        .iter()
        .map(|track| library.attachments(track.id).unwrap())
        .collect();
    assert_eq!(attachments[0].len(), 2);
    let backup = temporary.path().join("backup.muse");
    library.backup(&backup).unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !temporary
                .path()
                .join(format!("backup.muse{suffix}"))
                .exists()
        );
    }
    assert_no_temporary_files(temporary.path());
    let other = temporary.path().join("other");
    fs::create_dir(&other).unwrap();
    let portable = other.join("portable.muse");
    fs::rename(&backup, &portable).unwrap();
    drop(observer);
    drop(library);
    fs::remove_file(&database).unwrap();
    fs::remove_dir_all(&sources).unwrap();

    let restored = other.join("restored.muse");
    let before = fs::read(&portable).unwrap();
    Library::restore(&portable, &restored).unwrap();
    assert_eq!(fs::read(&portable).unwrap(), before);
    fs::remove_file(&portable).unwrap();
    let mut library = Library::open(&restored).unwrap();
    assert_eq!(library.list().unwrap(), expected);
    for (index, track) in expected.iter().enumerate() {
        assert_eq!(library.attachments(track.id).unwrap(), attachments[index]);
        assert_eq!(
            library.open_audio(track.id).unwrap().into_inner(),
            if track.format == "mp3" { MP3 } else { FLAC }
        );
    }
    let connection = Connection::open(&restored).unwrap();
    let sources: i64 = connection
        .query_row("SELECT count(*) FROM import_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sources, 3);
    fs::write(other.join("again.ncm"), NCM).unwrap();
    fs::write(other.join("again.mp3"), MP3).unwrap();
    fs::write(other.join("again.flac"), FLAC).unwrap();
    let report = library.import(&other);
    assert_eq!((report.imported, report.duplicates), (0, 3), "{report:?}");
    assert!(report.failed.is_empty());
    assert_no_temporary_files(&other);
}

#[test]
fn uncommitted_deletion_on_another_connection_is_not_backed_up() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let mut library = Library::open(&database).unwrap();
    let input = temporary.path().join("song.ncm");
    fs::write(&input, NCM).unwrap();
    assert_eq!(library.import(&input).imported, 1);
    let expected = library.list().unwrap();
    let writer = Connection::open(&database).unwrap();
    writer
        .execute_batch(
            "PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; DELETE FROM tracks; DELETE FROM audio;",
        )
        .unwrap();
    let backup = temporary.path().join("backup.muse");
    library.backup(&backup).unwrap();
    let copy = Library::open(&backup).unwrap();
    assert_eq!(copy.list().unwrap(), expected);
    assert_eq!(copy.attachments(expected[0].id).unwrap().len(), 1);
    assert_eq!(copy.open_audio(expected[0].id).unwrap().into_inner(), MP3);
    writer.execute_batch("COMMIT").unwrap();
    assert!(library.list().unwrap().is_empty());
}

#[test]
fn invalid_sources_fail_without_mutation_or_new_files() {
    let temporary = TempDir::new().unwrap();
    let source = temporary.path().join("source.muse");
    let destination = temporary.path().join("target.muse");
    assert!(Library::restore(&source, &destination).is_err());
    assert!(!source.exists());
    assert!(!destination.exists());

    for variant in [
        "foreign",
        "newer",
        "missing_table",
        "version_zero",
        "garbage",
        "truncated",
    ] {
        if variant == "foreign" {
            Connection::open(&source).unwrap().execute_batch("CREATE TABLE important(value TEXT); INSERT INTO important VALUES ('preserve');").unwrap();
        } else if variant == "garbage" {
            fs::write(&source, b"not a SQLite database").unwrap();
        } else {
            drop(Library::open(&source).unwrap());
            let connection = Connection::open(&source).unwrap();
            match variant {
                "newer" => connection.pragma_update(None, "user_version", 99).unwrap(),
                "missing_table" => connection.execute_batch("DROP TABLE audio").unwrap(),
                "version_zero" => connection.pragma_update(None, "user_version", 0).unwrap(),
                "truncated" => {}
                _ => unreachable!(),
            }
            drop(connection);
            if variant == "truncated" {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&source)
                    .unwrap()
                    .set_len(100)
                    .unwrap();
            }
        }
        let before = fs::read(&source).unwrap();
        let error = Library::restore(&source, &destination).unwrap_err();
        if variant == "foreign" {
            assert!(matches!(error, LibraryError::ForeignDatabase));
        } else if variant == "newer" {
            assert!(matches!(error, LibraryError::NewerSchema(99)));
        }
        assert_eq!(fs::read(&source).unwrap(), before, "{variant}");
        assert!(!destination.exists(), "{variant}");
        assert_no_temporary_files(temporary.path());
        fs::remove_file(&source).unwrap();
    }
}

#[test]
fn foreign_key_failure_after_copy_cleans_temporary_database_and_logs() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    drop(Library::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys=OFF; INSERT INTO import_sources(source_hash,track_id) VALUES ('broken', 999);",
        )
        .unwrap();
    drop(connection);
    let before = fs::read(&database).unwrap();
    let target = temporary.path().join("backup.muse");
    assert!(matches!(
        Library::restore(&database, &target),
        Err(LibraryError::InvalidBackup(_))
    ));
    assert_eq!(fs::read(&database).unwrap(), before);
    assert!(!target.exists());
    assert_no_temporary_files(temporary.path());
}

#[test]
fn sqlite_constraint_corruption_is_rejected_before_publication() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    drop(Library::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints=ON; INSERT INTO audio VALUES ('invalid', x'');",
        )
        .unwrap();
    drop(connection);
    let before = fs::read(&database).unwrap();
    let target = temporary.path().join("backup.muse");
    assert!(matches!(
        Library::restore(&database, &target),
        Err(LibraryError::InvalidBackup(_))
    ));
    assert_eq!(fs::read(&database).unwrap(), before);
    assert!(!target.exists());
    assert_no_temporary_files(temporary.path());
}

#[test]
fn existing_targets_sidecars_and_source_alias_are_never_overwritten() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let library = Library::open(&database).unwrap();
    let destination = temporary.path().join("backup.muse");
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let conflict = temporary.path().join(format!("backup.muse{suffix}"));
        fs::write(&conflict, b"keep existing data").unwrap();
        assert!(matches!(
            library.backup(&destination),
            Err(LibraryError::DestinationExists(_))
        ));
        assert_eq!(fs::read(&conflict).unwrap(), b"keep existing data");
        if !suffix.is_empty() {
            assert!(!destination.exists());
        }
        fs::remove_file(conflict).unwrap();
    }
    fs::create_dir(&destination).unwrap();
    assert!(matches!(
        library.backup(&destination),
        Err(LibraryError::DestinationExists(_))
    ));
    assert!(destination.is_dir());
    assert!(matches!(
        library.backup(&database),
        Err(LibraryError::DestinationExists(_))
    ));
    assert!(library.list().unwrap().is_empty());
    assert_no_temporary_files(temporary.path());
}

#[test]
fn empty_library_and_missing_destination_parent_are_handled() {
    let temporary = TempDir::new().unwrap();
    let library = Library::open(temporary.path().join("library.muse")).unwrap();
    let missing_parent = temporary.path().join("missing/backup.muse");
    assert!(library.backup(&missing_parent).is_err());
    assert!(!temporary.path().join("missing").exists());
    let destination = temporary.path().join("backup.muse");
    library.backup(&destination).unwrap();
    assert!(
        Library::open(destination)
            .unwrap()
            .list()
            .unwrap()
            .is_empty()
    );
    assert_no_temporary_files(temporary.path());
}

#[cfg(unix)]
#[test]
fn dangling_destination_symlinks_are_preserved() {
    use std::os::unix::fs::symlink;
    let temporary = TempDir::new().unwrap();
    let library = Library::open(temporary.path().join("library.muse")).unwrap();
    let destination = temporary.path().join("backup.muse");
    let absent = temporary.path().join("absent.muse");
    symlink(&absent, &destination).unwrap();
    assert!(matches!(
        library.backup(&destination),
        Err(LibraryError::DestinationExists(_))
    ));
    assert_eq!(fs::read_link(destination).unwrap(), absent);
    assert!(!absent.exists());
}
