// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use muse_library::{AttachmentKind, ImportPhase, Library, LibraryError};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const MP3: &[u8] = include_bytes!("fixtures/silence.mp3");
const FLAC: &[u8] = include_bytes!("fixtures/tone.flac");
const NCM: &[u8] = include_bytes!("fixtures/fixture.ncm");
const COVER: &[u8] = include_bytes!("fixtures/cover.png");

fn setup() -> (TempDir, Library) {
    let temporary = TempDir::new().unwrap();
    let library = Library::open(temporary.path().join("library.muse")).unwrap();
    (temporary, library)
}

fn count(path: &Path, table: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn restart_and_owned_audio_do_not_need_source_or_connection() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("无标签.mp3");
    fs::write(&path, MP3).unwrap();
    assert_eq!(library.import(&path).imported, 1);
    let track = library.list().unwrap().remove(0);
    assert_eq!(track.title, "无标签");
    assert_eq!(track.artist, None);
    assert_eq!(track.content_hash, format!("{:x}", Sha256::digest(MP3)));
    drop(library);
    fs::remove_file(path).unwrap();
    let reopened = Library::open(temporary.path().join("library.muse")).unwrap();
    let mut reader = reopened.open_audio(track.id).unwrap();
    drop(reopened);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, MP3);
}

#[test]
fn names_do_not_define_duplicates_and_changed_tags_are_distinct() {
    let (temporary, mut library) = setup();
    let first = temporary.path().join("first.mp3");
    fs::write(&first, MP3).unwrap();
    assert_eq!(library.import(&first).imported, 1);
    let renamed = temporary.path().join("renamed.mp3");
    fs::copy(&first, &renamed).unwrap();
    assert_eq!(library.import(&renamed).duplicates, 1);
    let mut tagged = MP3.to_vec();
    let mut id3v1 = [0u8; 128];
    id3v1[..3].copy_from_slice(b"TAG");
    id3v1[3..16].copy_from_slice(b"Changed title");
    tagged.extend(id3v1);
    fs::write(first, tagged).unwrap();
    assert_eq!(
        library.import(temporary.path().join("first.mp3")).imported,
        1
    );
    assert_eq!(library.list().unwrap().len(), 2);
    assert_eq!(count(&temporary.path().join("library.muse"), "audio"), 2);
}

#[test]
fn progress_counts_committed_outcomes_and_skips_existing_audio() {
    let (temporary, mut library) = setup();
    let database = temporary.path().join("library.muse");
    let root = temporary.path().join("sources");
    fs::create_dir(&root).unwrap();
    for (name, bytes) in [
        ("a.flac", FLAC),
        ("b.flac", FLAC),
        ("歌曲.mp3", MP3),
        ("broken.ncm", b"CTENFDAM".as_slice()),
        ("orphan.lrc", b"[00:00]unmatched".as_slice()),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
    let mut snapshots = Vec::new();
    let report = library.import_with_progress(&root, &AtomicBool::new(false), |progress| {
        // A separate connection can see every reported addition immediately.
        assert_eq!(count(&database, "tracks") as usize, progress.imported);
        assert_eq!(
            progress.processed,
            progress.imported + progress.duplicates + progress.failed
        );
        snapshots.push(progress);
    });
    assert!(snapshots.iter().any(|progress| {
        progress.phase == ImportPhase::Scanning
            && progress.discovered == 4
            && progress.total.is_none()
            && progress.processed == 0
    }));
    assert!(snapshots.iter().any(|progress| {
        progress.phase == ImportPhase::Importing
            && progress
                .current
                .as_ref()
                .is_some_and(|path| path.ends_with("歌曲.mp3"))
    }));
    assert!(snapshots.windows(2).all(|pair| {
        pair[0].processed <= pair[1].processed
            && pair[0].imported <= pair[1].imported
            && pair[0].duplicates <= pair[1].duplicates
            && pair[0].failed <= pair[1].failed
    }));
    let final_progress = snapshots.last().unwrap();
    assert_eq!(final_progress.phase, ImportPhase::Completed);
    assert_eq!(final_progress.total, Some(4));
    assert_eq!(final_progress.processed, 4);
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (2, 1, 1)
    );
    assert_eq!(final_progress.imported, report.imported);
    assert_eq!(final_progress.duplicates, report.duplicates);
    assert_eq!(final_progress.failed, report.failed.len());
    assert_eq!(final_progress.attachment_issues, 1);
    assert!(final_progress.current.is_none());

    let repeated = library.import(&root);
    assert_eq!(
        (
            repeated.imported,
            repeated.duplicates,
            repeated.failed.len()
        ),
        (0, 3, 1)
    );
    assert_eq!(count(&database, "tracks"), 2);
    assert_eq!(count(&database, "audio"), 2);
    assert_eq!(fs::read(root.join("歌曲.mp3")).unwrap(), MP3);
}

#[test]
fn progress_cancellation_preserves_only_completed_work() {
    for phase in [ImportPhase::Scanning, ImportPhase::Importing] {
        let (temporary, mut library) = setup();
        let root = temporary.path().join("sources");
        fs::create_dir(&root).unwrap();
        for (name, bytes) in [("a.mp3", MP3), ("b.flac", FLAC), ("c.ncm", NCM)] {
            fs::write(root.join(name), bytes).unwrap();
        }
        let cancel = AtomicBool::new(false);
        let mut snapshots = Vec::new();
        let report = library.import_with_progress(&root, &cancel, |progress| {
            if progress.phase == phase
                && ((phase == ImportPhase::Scanning && progress.discovered == 2)
                    || (phase == ImportPhase::Importing && progress.processed == 1))
            {
                cancel.store(true, Ordering::Relaxed);
            }
            snapshots.push(progress);
        });
        assert!(report.cancelled);
        let final_progress = snapshots.last().unwrap();
        assert_eq!(final_progress.phase, ImportPhase::Cancelled);
        assert!(final_progress.current.is_none());
        if phase == ImportPhase::Scanning {
            assert_eq!(final_progress.total, None);
            assert_eq!(final_progress.processed, 0);
            assert_eq!(final_progress.discovered, 2);
        } else {
            assert_eq!(final_progress.total, Some(3));
            assert_eq!(final_progress.processed, 1);
        }
        assert_eq!(final_progress.processed, report.imported);
        assert_eq!(library.list().unwrap().len(), report.imported);
        assert!(report.failed.is_empty());
        assert_eq!(report.duplicates, 0);
    }
}

#[test]
fn progress_always_finishes_for_single_empty_and_invalid_sources() {
    let (temporary, mut library) = setup();
    let song = temporary.path().join("song.mp3");
    let empty = temporary.path().join("empty");
    let unsupported = temporary.path().join("unsupported.txt");
    fs::write(&song, MP3).unwrap();
    fs::create_dir(&empty).unwrap();
    fs::write(&unsupported, b"not audio").unwrap();
    for (source, total, processed, failed) in [
        (song, Some(1), 1, 0),
        (empty, Some(0), 0, 0),
        (temporary.path().join("missing"), None, 1, 1),
        (unsupported, Some(1), 1, 1),
    ] {
        let mut snapshots = Vec::new();
        let report = library.import_with_progress(source, &AtomicBool::new(false), |progress| {
            snapshots.push(progress);
        });
        assert_eq!(snapshots.first().unwrap().phase, ImportPhase::Scanning);
        let final_progress = snapshots.last().unwrap();
        assert_eq!(final_progress.phase, ImportPhase::Completed);
        assert_eq!(final_progress.total, total);
        assert_eq!(final_progress.processed, processed);
        assert_eq!(final_progress.failed, failed);
        assert_eq!(final_progress.imported, report.imported);
        assert_eq!(final_progress.failed, report.failed.len());
        assert!(final_progress.current.is_none());
    }
}

#[test]
fn malformed_ncm_lengths_never_create_success_records() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("broken.ncm");
    for length in [0, 7, 10, 13, 20, 50, 100, 200, 300] {
        fs::write(&path, &NCM[..length]).unwrap();
        assert_eq!(library.import(&path).failed.len(), 1, "prefix {length}");
    }
    let mut invalid = NCM.to_vec();
    invalid[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&path, invalid).unwrap();
    assert_eq!(library.import(&path).failed.len(), 1);
    assert_eq!(
        count(&temporary.path().join("library.muse"), "import_sources"),
        0
    );
    fs::write(&path, NCM).unwrap();
    assert_eq!(library.import(&path).imported, 1);
}

#[test]
fn duplicate_fills_missing_lyrics_and_preserves_existing_attachment() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("song.mp3");
    fs::write(&path, MP3).unwrap();
    assert_eq!(library.import(&path).imported, 1);
    fs::write(path.with_extension("lrc"), b"[00:00]original").unwrap();
    let result = library.import(&path);
    assert_eq!((result.imported, result.duplicates), (0, 1));
    let track = library.list().unwrap().remove(0);
    fs::write(path.with_extension("lrc"), b"[00:00]changed").unwrap();
    assert_eq!(library.import(&path).duplicates, 1);
    let attachments = library.attachments(track.id).unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].kind, AttachmentKind::Lyrics);
    assert_eq!(attachments[0].data, b"[00:00]original");
}

#[test]
fn attachment_read_failures_preserve_new_and_duplicate_classification() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("song.mp3");
    fs::write(&path, MP3).unwrap();
    fs::create_dir(path.with_extension("lrc")).unwrap();
    let new = library.import(&path);
    assert_eq!((new.imported, new.duplicates, new.failed.len()), (1, 0, 0));
    assert_eq!(new.attachment_issues.len(), 1);
    let duplicate = library.import(&path);
    assert_eq!(
        (
            duplicate.imported,
            duplicate.duplicates,
            duplicate.failed.len()
        ),
        (0, 1, 0)
    );
    assert_eq!(duplicate.attachment_issues.len(), 1);
}

#[test]
fn deleting_all_source_records_allows_both_original_inputs_to_return() {
    let (temporary, mut library) = setup();
    let ncm = temporary.path().join("song.ncm");
    let mp3 = temporary.path().join("song.mp3");
    fs::write(&ncm, NCM).unwrap();
    fs::write(&mp3, MP3).unwrap();
    assert_eq!(library.import(&ncm).imported, 1);
    assert_eq!(library.import(&mp3).duplicates, 1);
    let id = library.list().unwrap()[0].id;
    assert!(library.remove(id).unwrap());
    assert!(!library.remove(id).unwrap());
    let database = temporary.path().join("library.muse");
    for table in ["tracks", "audio", "attachments", "import_sources"] {
        assert_eq!(count(&database, table), 0);
    }
    assert!(matches!(
        library.open_audio(id),
        Err(LibraryError::TrackNotFound(_))
    ));
    assert_eq!(library.import(&mp3).imported, 1);
    assert_eq!(library.import(&ncm).duplicates, 1);
    assert_eq!(fs::read(ncm).unwrap(), NCM);
}

#[test]
fn attachment_write_failure_rolls_back_song_audio_and_source_marker() {
    let (temporary, mut library) = setup();
    let database = temporary.path().join("library.muse");
    let connection = Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_attachment BEFORE INSERT ON attachments BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;").unwrap();
    let path = temporary.path().join("song.ncm");
    fs::write(&path, NCM).unwrap();
    let result = library.import(&path);
    assert_eq!(result.failed.len(), 1);
    assert!(result.stopped.is_some());
    for table in ["tracks", "audio", "attachments", "import_sources"] {
        assert_eq!(count(&database, table), 0);
    }
    connection
        .execute_batch("DROP TRIGGER reject_attachment")
        .unwrap();
    assert_eq!(library.import(path).imported, 1);
}

#[test]
fn library_failure_preserves_earlier_commits_and_stops_remaining_files() {
    let (temporary, mut library) = setup();
    let database = temporary.path().join("library.muse");
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER reject_track BEFORE INSERT ON tracks WHEN NEW.title = 'b' BEGIN SELECT RAISE(ABORT, 'simulated full disk'); END;").unwrap();
    let root = temporary.path().join("sources");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.mp3"), MP3).unwrap();
    fs::write(root.join("b.flac"), FLAC).unwrap();
    fs::write(root.join("c.ncm"), NCM).unwrap();
    let mut snapshots = Vec::new();
    let report = library.import_with_progress(&root, &AtomicBool::new(false), |progress| {
        snapshots.push(progress);
    });
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (1, 0, 1)
    );
    assert!(report.stopped.is_some());
    let final_progress = snapshots.last().unwrap();
    assert_eq!(final_progress.phase, ImportPhase::Stopped);
    assert_eq!(final_progress.total, Some(3));
    assert_eq!(final_progress.processed, 2);
    assert_eq!(final_progress.imported, 1);
    assert_eq!(final_progress.failed, 1);
    assert!(final_progress.current.is_none());
    assert_eq!(count(&database, "audio"), 1);
    assert_eq!(count(&database, "import_sources"), 1);
    assert_eq!(library.list().unwrap()[0].title, "a");
}

#[test]
fn failed_delete_preserves_all_associations() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("song.ncm");
    fs::write(&path, NCM).unwrap();
    assert_eq!(library.import(&path).imported, 1);
    let database = temporary.path().join("library.muse");
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON tracks BEGIN SELECT RAISE(ABORT, 'simulated delete failure'); END;").unwrap();
    let id = library.list().unwrap()[0].id;
    assert!(library.remove(id).is_err());
    for table in ["tracks", "audio", "attachments", "import_sources"] {
        assert_eq!(count(&database, table), 1);
    }
    assert_eq!(library.open_audio(id).unwrap().into_inner(), MP3);
}

#[test]
fn newer_and_foreign_databases_are_refused_without_modification() {
    let (temporary, library) = setup();
    drop(library);
    let database = temporary.path().join("library.muse");
    let connection = Connection::open(&database).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    drop(connection);
    let before = fs::read(&database).unwrap();
    assert!(matches!(
        Library::open(&database),
        Err(LibraryError::NewerSchema(99))
    ));
    assert_eq!(before, fs::read(&database).unwrap());
    let foreign = temporary.path().join("other.sqlite");
    Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE important(data TEXT); INSERT INTO important VALUES ('keep');")
        .unwrap();
    let before = fs::read(&foreign).unwrap();
    assert!(matches!(
        Library::open(&foreign),
        Err(LibraryError::ForeignDatabase)
    ));
    assert_eq!(before, fs::read(foreign).unwrap());
}

#[test]
fn cancellation_before_processing_does_not_create_song_records() {
    let (temporary, mut library) = setup();
    let path = temporary.path().join("song.mp3");
    fs::write(&path, MP3).unwrap();
    let report = library.import_with_cancel(&path, &AtomicBool::new(true));
    assert!(report.cancelled);
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (0, 0, 0)
    );
    assert!(library.list().unwrap().is_empty());
}

#[test]
fn unmatched_attachments_are_reported_without_failing_audio() {
    let (temporary, mut library) = setup();
    let sources = temporary.path().join("sources");
    fs::create_dir_all(sources.join("meta")).unwrap();
    fs::write(sources.join("song.mp3"), MP3).unwrap();
    fs::write(sources.join("other.lrc"), b"unmatched").unwrap();
    fs::write(sources.join("meta/track-999.jpg"), COVER).unwrap();
    let report = library.import(sources);
    assert_eq!((report.imported, report.failed.len()), (1, 0));
    assert_eq!(report.attachment_issues.len(), 2);
}

#[cfg(unix)]
#[test]
fn external_symlink_attachments_are_not_read() {
    use std::os::unix::fs::symlink;
    let (temporary, mut library) = setup();
    let sources = temporary.path().join("sources");
    fs::create_dir(&sources).unwrap();
    fs::write(sources.join("song.mp3"), MP3).unwrap();
    let external = temporary.path().join("outside.lrc");
    fs::write(&external, b"outside selected directory").unwrap();
    symlink(external, sources.join("song.lrc")).unwrap();
    let report = library.import(sources);
    assert_eq!((report.imported, report.failed.len()), (1, 0));
    assert_eq!(report.attachment_issues.len(), 1);
    assert!(
        library
            .attachments(library.list().unwrap()[0].id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn conflicting_attachment_observations_survive_without_replacing_primary() {
    let (temporary, mut library) = setup();
    let source = temporary.path().join("song.mp3");
    let lyrics = temporary.path().join("song.lrc");
    fs::write(&source, MP3).unwrap();
    fs::write(&lyrics, b"first lyrics").unwrap();
    assert_eq!(library.import(&source).imported, 1);
    fs::write(&lyrics, b"second lyrics").unwrap();
    assert_eq!(library.import(&source).duplicates, 1);
    let id = library.list().unwrap()[0].id;
    assert_eq!(library.attachments(id).unwrap()[0].data, b"first lyrics");
    let observations = library.attachment_observations(id).unwrap();
    assert_eq!(observations.len(), 2);
    assert!(observations.iter().all(|o| o.attribution == "source"));
}
