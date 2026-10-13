// SPDX-License-Identifier: AGPL-3.0-only
use muse_library::{Library, LibraryError};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
const NCM: &[u8] = include_bytes!("fixtures/fixture.ncm");
const MP3: &[u8] = include_bytes!("fixtures/silence.mp3");
const FLAC: &[u8] = include_bytes!("fixtures/tone.flac");
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn legacy(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("PRAGMA user_version=1; PRAGMA journal_mode=WAL;")
        .unwrap();
    db.execute("INSERT INTO audio VALUES(?1,?2)", params![hash(FLAC), FLAC])
        .unwrap();
    db.execute(
        "INSERT INTO tracks VALUES(42,'Fixture NCM','Fixture Artist','legacy','flac',123,321,?1)",
        [hash(FLAC)],
    )
    .unwrap();
    db.execute("INSERT INTO import_sources VALUES(?1,42)", [hash(NCM)])
        .unwrap();
    db.execute(
        "INSERT INTO attachments VALUES(42,'lyrics',?1,'text/plain',?2)",
        params![hash(b"legacy lyrics"), b"legacy lyrics"],
    )
    .unwrap();
    db
}
#[test]
fn upgrade_backs_up_wal_preserves_ids_and_recovers_suppressed_audio_idempotently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.muse");
    let db = legacy(&path);
    assert!(matches!(
        Library::open(&path),
        Err(LibraryError::UpgradeRequired)
    ));
    let backup = Library::upgrade(&path).unwrap().unwrap();
    let old = Connection::open(&backup).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        old.query_row("SELECT data FROM audio", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        FLAC
    );
    let restored = dir.path().join("old-copy.muse");
    Library::restore(&backup, &restored).unwrap();
    assert!(matches!(
        Library::open(&restored),
        Err(LibraryError::UpgradeRequired)
    ));
    let mut lib = Library::open(&path).unwrap();
    assert_eq!(lib.list().unwrap()[0].id, 42);
    assert_eq!(lib.open_audio(42).unwrap().into_inner(), FLAC);
    assert_eq!(lib.attachments(42).unwrap()[0].data, b"legacy lyrics");
    assert_eq!(
        db.query_row("SELECT verified FROM import_sources", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        lib.attachment_observations(42).unwrap()[0].attribution,
        "legacy"
    );
    assert_eq!(lib.import(dir.path().join("missing.ncm")).failed.len(), 1);
    let source = dir.path().join("original.ncm");
    fs::write(&source, NCM).unwrap();
    db.execute_batch("CREATE TRIGGER reject_observation BEFORE INSERT ON source_observations BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(lib.import(&source).stopped.is_some());
    assert_eq!(lib.list().unwrap().len(), 1);
    assert_eq!(
        db.query_row("SELECT verified FROM import_sources", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_observation").unwrap();
    assert_eq!(lib.import(&source).imported, 1);
    let recovered = lib.list().unwrap()[1].id;
    assert_eq!(lib.open_audio(recovered).unwrap().into_inner(), MP3);
    assert_eq!(lib.import(&source).duplicates, 1);
    assert_eq!(lib.list().unwrap().len(), 2);
    assert_eq!(lib.attachments(42).unwrap()[0].data, b"legacy lyrics");
    assert_eq!(fs::read(source).unwrap(), NCM);
    assert!(matches!(
        Library::upgrade(&path),
        Err(LibraryError::MaintenanceBusy)
    ));
    drop(lib);
    assert!(Library::upgrade(&path).unwrap().is_none());
}
#[test]
fn invalid_backup_aborts_before_migration_and_missing_reads_do_not_create() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.muse");
    let db = legacy(&path);
    db.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO import_sources VALUES('dangling',999)")
        .unwrap();
    assert!(Library::upgrade(&path).is_err());
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM tracks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let missing = dir.path().join("missing.muse");
    assert!(Library::open_existing(&missing).is_err());
    assert!(!missing.exists());
}

#[test]
fn legacy_plain_mp3_and_flac_sources_are_revalidated() {
    for (name, source_bytes, retained) in [("source.mp3", MP3, FLAC), ("source.flac", FLAC, MP3)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.muse");
        let db = legacy(&path);
        db.execute("DELETE FROM import_sources", []).unwrap();
        db.execute(
            "INSERT INTO import_sources VALUES(?1,42)",
            [hash(source_bytes)],
        )
        .unwrap();
        if retained == MP3 {
            db.execute("INSERT INTO audio VALUES(?1,?2)", params![hash(MP3), MP3])
                .unwrap();
            db.execute(
                "UPDATE tracks SET content_hash=?1,format='mp3' WHERE id=42",
                [hash(MP3)],
            )
            .unwrap();
            db.execute("DELETE FROM audio WHERE content_hash=?1", [hash(FLAC)])
                .unwrap();
        }
        Library::upgrade(&path).unwrap();
        let mut lib = Library::open(&path).unwrap();
        let source = dir.path().join(name);
        fs::write(&source, source_bytes).unwrap();
        assert_eq!(lib.import(&source).imported, 1);
        assert_eq!(lib.import(&source).duplicates, 1);
        assert_eq!(lib.open_audio(42).unwrap().into_inner(), retained);
        assert_eq!(
            lib.open_audio(lib.list().unwrap()[1].id)
                .unwrap()
                .into_inner(),
            source_bytes
        );
    }
}

#[test]
fn migration_failure_after_table_rebuild_rolls_back_and_keeps_the_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.muse");
    let db = legacy(&path);
    // A schema conflict fails after tracks have been rebuilt inside the transaction.
    db.execute_batch(
        "CREATE TABLE reserved(value); CREATE INDEX candidate_content ON reserved(value);",
    )
    .unwrap();
    assert!(Library::upgrade(&path).is_err());
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT id FROM tracks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    assert_eq!(
        db.query_row("SELECT data FROM audio", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        FLAC
    );
    assert_eq!(
        db.query_row("SELECT track_id FROM import_sources", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        42
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='library_descriptor'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let backups = fs::read_dir(dir.path())
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_name().to_string_lossy().contains(".pre-v2."))
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let restored = dir.path().join("restored.muse");
    Library::restore(backups[0].path(), &restored).unwrap();
    assert!(matches!(
        Library::open(restored),
        Err(LibraryError::UpgradeRequired)
    ));
    db.execute_batch("DROP INDEX candidate_content").unwrap();
    assert!(Library::upgrade(&path).unwrap().is_some());
}
