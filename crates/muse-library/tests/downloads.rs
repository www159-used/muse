// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs,
    path::Path,
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use muse_library::{AttachmentKind, ImportPhase, ImportReport, Library};
use rusqlite::Connection;
use tempfile::TempDir;

const MP3: &[u8] = include_bytes!("fixtures/silence.mp3");
const NCM: &[u8] = include_bytes!("fixtures/fixture.ncm");

fn download(library: &mut Library, source: &Path) -> ImportReport {
    library.import_downloads_with_progress(source, &AtomicBool::new(false), |_| {})
}

// Independent UTF-8 ID3v2.4 fixtures let these workflows exercise actual tag
// parsing, including missing fields; no production decoder constructs the data.
fn tagged_mp3(title: Option<&str>, artist: Option<&str>, album: &str) -> Vec<u8> {
    fn syncsafe(length: usize) -> [u8; 4] {
        assert!(length < 1 << 28);
        [
            ((length >> 21) & 0x7f) as u8,
            ((length >> 14) & 0x7f) as u8,
            ((length >> 7) & 0x7f) as u8,
            (length & 0x7f) as u8,
        ]
    }
    let mut frames = Vec::new();
    for (kind, text) in [(b"TIT2", title), (b"TPE1", artist), (b"TALB", Some(album))] {
        if let Some(text) = text {
            frames.extend_from_slice(kind);
            frames.extend_from_slice(&syncsafe(1 + text.len()));
            frames.extend_from_slice(&[0, 0, 3]); // Flags followed by UTF-8 encoding.
            frames.extend_from_slice(text.as_bytes());
        }
    }
    let mut audio = b"ID3\x04\x00\x00".to_vec();
    audio.extend_from_slice(&syncsafe(frames.len()));
    audio.extend(frames);
    audio.extend_from_slice(MP3);
    audio
}

#[test]
fn unicode_identity_trims_and_lowercases_tags_while_preserving_retained_audio() {
    let temporary = TempDir::new().unwrap();
    let mut library = Library::open(temporary.path().join("library.muse")).unwrap();
    let first = temporary.path().join("unrelated-name.mp3");
    let second = temporary.path().join("renamed.MP3");
    let original = tagged_mp3(Some("夜曲 Ä 한글"), Some("SÄNGER"), "First");
    let alternate = tagged_mp3(Some("  夜曲 ä 한글  "), Some("  sänger  "), "Second");
    fs::write(&first, &original).unwrap();
    fs::write(&second, &alternate).unwrap();
    assert_eq!(download(&mut library, &first).imported, 1);
    let report = download(&mut library, &second);
    assert_eq!((report.imported, report.duplicates), (1, 0));
    assert!(report.failed.is_empty());
    let tracks = library.list().unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "夜曲 Ä 한글");
    assert_eq!(tracks[0].artist.as_deref(), Some("SÄNGER"));
    assert_eq!(
        library.open_audio(tracks[0].id).unwrap().into_inner(),
        original
    );
    assert_eq!(
        library.open_audio(tracks[1].id).unwrap().into_inner(),
        alternate
    );
    assert_eq!(library.groups().unwrap().len(), 1);
    assert_eq!(report.collisions, 1);
    assert_eq!(fs::read(second).unwrap(), alternate);
}

#[test]
fn missing_tagged_title_or_artist_never_uses_filename_identity() {
    let temporary = TempDir::new().unwrap();
    let mut library = Library::open(temporary.path().join("library.muse")).unwrap();
    for (folder, title, artist) in [
        ("known", Some("Song"), Some("Artist")),
        ("missing-title", None, Some("Artist")),
        ("missing-artist", Some("Song"), None),
    ] {
        let directory = temporary.path().join(folder);
        fs::create_dir(&directory).unwrap();
        let source = directory.join("Song.mp3");
        let audio = tagged_mp3(title, artist, folder);
        fs::write(&source, &audio).unwrap();
        let report = download(&mut library, &source);
        assert_eq!(
            (report.imported, report.duplicates),
            (1, 0),
            "{folder}: {report:?}"
        );
        assert!(report.failed.is_empty());
        assert_eq!(fs::read(source).unwrap(), audio);
    }
    assert_eq!(library.list().unwrap().len(), 3);
}

#[test]
fn concurrent_downloads_retain_two_candidates_and_source_records() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let first_audio = tagged_mp3(Some("Song"), Some("Artist"), "First");
    let second_audio = tagged_mp3(Some("Song"), Some("Artist"), "Second");
    let sources = [
        temporary.path().join("a.mp3"),
        temporary.path().join("b.mp3"),
    ];
    fs::write(&sources[0], &first_audio).unwrap();
    fs::write(&sources[1], &second_audio).unwrap();
    let libraries = [
        Library::open(&database).unwrap(),
        Library::open(&database).unwrap(),
    ];
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = libraries
        .into_iter()
        .zip(sources)
        .map(|(mut library, source)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                download(&mut library, &source)
            })
        })
        .collect();
    let reports: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        reports.iter().map(|report| report.imported).sum::<usize>(),
        2
    );
    assert_eq!(
        reports
            .iter()
            .map(|report| report.duplicates)
            .sum::<usize>(),
        0
    );
    assert!(reports.iter().all(|report| report.failed.is_empty()));
    let library = Library::open(&database).unwrap();
    let tracks = library.list().unwrap();
    assert_eq!(tracks.len(), 2);
    let retained = library.open_audio(tracks[0].id).unwrap().into_inner();
    assert!(retained == first_audio || retained == second_audio);
    let count: i64 = Connection::open(database)
        .unwrap()
        .query_row("SELECT count(*) FROM import_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn backup_restore_and_deletion_preserve_then_release_metadata_duplicates() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let mut library = Library::open(&database).unwrap();
    let plain = temporary.path().join("first.mp3");
    let container = temporary.path().join("later.ncm");
    let original = tagged_mp3(Some("Fixture NCM"), Some("Fixture Artist"), "Album");
    fs::write(&plain, &original).unwrap();
    fs::write(&container, NCM).unwrap();
    assert_eq!(download(&mut library, &plain).imported, 1);
    let backup = temporary.path().join("backup.muse");
    library.backup(&backup).unwrap();
    let restored = temporary.path().join("restored.muse");
    Library::restore(&backup, &restored).unwrap();
    let mut copy = Library::open(restored).unwrap();
    assert_eq!(download(&mut copy, &container).imported, 1);
    let track = copy.list().unwrap().remove(0);
    assert_eq!(copy.open_audio(track.id).unwrap().into_inner(), original);
    assert!(copy.attachments(track.id).unwrap().is_empty());
    let alternative = copy.list().unwrap().remove(1);
    assert!(
        copy.attachments(alternative.id)
            .unwrap()
            .iter()
            .any(|a| a.kind == AttachmentKind::Cover)
    );
    assert!(copy.remove(track.id).unwrap());
    assert_eq!(download(&mut copy, &container).duplicates, 1);
    assert_eq!(download(&mut copy, &plain).imported, 1);
    let track = copy.list().unwrap().remove(0);
    assert_eq!(copy.open_audio(track.id).unwrap().into_inner(), MP3);
    assert_eq!(fs::read(plain).unwrap(), original);
    assert_eq!(fs::read(container).unwrap(), NCM);
}

#[test]
fn metadata_duplicate_attachment_failure_rolls_back_and_allows_retry() {
    let temporary = TempDir::new().unwrap();
    let database = temporary.path().join("library.muse");
    let mut library = Library::open(&database).unwrap();
    let plain = temporary.path().join("first.mp3");
    let container = temporary.path().join("later.ncm");
    let original = tagged_mp3(Some("Fixture NCM"), Some("Fixture Artist"), "Album");
    fs::write(&plain, &original).unwrap();
    fs::write(&container, NCM).unwrap();
    assert_eq!(download(&mut library, &plain).imported, 1);
    let connection = Connection::open(database).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_cover BEFORE INSERT ON attachments BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
    let report = download(&mut library, &container);
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (0, 0, 1)
    );
    assert!(report.stopped.is_some());
    let count: i64 = connection
        .query_row("SELECT count(*) FROM import_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let track = library.list().unwrap().remove(0);
    assert!(library.attachments(track.id).unwrap().is_empty());
    assert_eq!(library.open_audio(track.id).unwrap().into_inner(), original);
    connection
        .execute_batch("DROP TRIGGER reject_cover")
        .unwrap();
    let report = download(&mut library, &container);
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (1, 0, 0)
    );
    assert!(report.stopped.is_none());
    assert!(library.attachments(track.id).unwrap().is_empty());
    assert_eq!(
        library
            .attachments(library.list().unwrap()[1].id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn invalid_ncm_does_not_reserve_identity_and_progress_counts_ordinary_audio() {
    let temporary = TempDir::new().unwrap();
    let source = temporary.path().join("downloads");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("Song.ncm"), b"invalid container").unwrap();
    fs::write(
        source.join("Song.mp3"),
        tagged_mp3(Some("Song"), Some("Artist"), "Album"),
    )
    .unwrap();
    fs::write(source.join("notes.txt"), "not audio").unwrap();
    let mut library = Library::open(temporary.path().join("library.muse")).unwrap();
    let mut snapshots = Vec::new();
    let report =
        library.import_downloads_with_progress(&source, &AtomicBool::new(false), |snapshot| {
            snapshots.push(snapshot)
        });
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (1, 0, 1)
    );
    assert!(report.stopped.is_none());
    assert_eq!(library.list().unwrap().len(), 1);
    let final_progress = snapshots.last().unwrap();
    assert_eq!(final_progress.phase, ImportPhase::Completed);
    assert_eq!(final_progress.total, Some(2));
    assert_eq!(final_progress.processed, 2);
    assert!(snapshots.iter().any(|snapshot| {
        snapshot
            .current
            .as_ref()
            .is_some_and(|path| path.ends_with("Song.mp3"))
    }));
}

#[test]
fn download_cancellation_keeps_only_committed_songs() {
    let temporary = TempDir::new().unwrap();
    let source = temporary.path().join("downloads");
    fs::create_dir(&source).unwrap();
    let first = tagged_mp3(Some("First"), Some("Artist"), "Album");
    fs::write(source.join("a.mp3"), &first).unwrap();
    fs::write(
        source.join("b.mp3"),
        tagged_mp3(Some("Second"), Some("Artist"), "Album"),
    )
    .unwrap();
    let mut library = Library::open(temporary.path().join("library.muse")).unwrap();
    let cancel = AtomicBool::new(false);
    let report = library.import_downloads_with_progress(&source, &cancel, |snapshot| {
        if snapshot.processed == 1 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(report.cancelled);
    assert_eq!(
        (report.imported, report.duplicates, report.failed.len()),
        (1, 0, 0)
    );
    let tracks = library.list().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        library.open_audio(tracks[0].id).unwrap().into_inner(),
        first
    );
}

#[test]
fn content_only_and_ncm_only_apis_preserve_versions_and_old_duplicates() {
    let temporary = TempDir::new().unwrap();
    let mut library = Library::open(temporary.path().join("library.muse")).unwrap();
    let plain = temporary.path().join("first.mp3");
    let container = temporary.path().join("later.ncm");
    let alternate = temporary.path().join("alternate.mp3");
    fs::write(
        &plain,
        tagged_mp3(Some("Fixture NCM"), Some("Fixture Artist"), "Album"),
    )
    .unwrap();
    fs::write(&container, NCM).unwrap();
    fs::write(
        &alternate,
        tagged_mp3(Some("Fixture NCM"), Some("Fixture Artist"), "Alternate"),
    )
    .unwrap();
    assert_eq!(library.import(&plain).imported, 1);
    assert_eq!(
        library
            .import_ncm_with_progress(&container, &AtomicBool::new(false), |_| {})
            .imported,
        1
    );
    let before = library.list().unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(download(&mut library, &alternate).imported, 1);
    assert_eq!(&library.list().unwrap()[..2], &before);
    assert_eq!(library.groups().unwrap()[0].candidates.len(), 3);
}
