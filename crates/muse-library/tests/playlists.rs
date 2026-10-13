// SPDX-License-Identifier: AGPL-3.0-only

use muse_library::{ALL_LOCAL_PLAYLIST, Library, LibraryError, PlaylistKind, SCHEMA_VERSION};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::fs;

const MP3: &[u8] = include_bytes!("fixtures/silence.mp3");
const FLAC: &[u8] = include_bytes!("fixtures/tone.flac");

#[test]
fn new_library_has_an_empty_all_local_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.muse");
    let library = Library::open(&path).unwrap();
    assert_eq!(library.schema_version().unwrap(), SCHEMA_VERSION);
    let playlist = library.all_local().unwrap();
    assert_eq!(playlist.name, ALL_LOCAL_PLAYLIST);
    assert_eq!(playlist.kind, PlaylistKind::AllLocal);
    assert_eq!(playlist.len, 0);
    assert!(library.playlist_tracks(playlist.id).unwrap().is_empty());
    assert_eq!(library.playlists().unwrap(), vec![playlist]);
}

#[test]
fn user_playlists_keep_insertion_order_and_all_local_tracks_imports() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library.muse")).unwrap();
    let first = dir.path().join("first.mp3");
    let second = dir.path().join("second.flac");
    fs::write(&first, MP3).unwrap();
    fs::write(&second, FLAC).unwrap();
    assert_eq!(library.import(&first).imported, 1);
    assert_eq!(library.import(&second).imported, 1);
    let tracks = library.list().unwrap();
    let all = library.all_local().unwrap();
    assert_eq!(all.len, 2);
    assert_eq!(
        library
            .playlist_tracks(all.id)
            .unwrap()
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        tracks.iter().map(|track| track.id).collect::<Vec<_>>()
    );
    assert!(matches!(
        library.add_to_playlist(all.id, tracks[0].id),
        Err(LibraryError::AllLocalImmutable)
    ));
    assert!(matches!(
        library.create_playlist("  "),
        Err(LibraryError::EmptyPlaylistName)
    ));
    assert!(matches!(
        library.create_playlist(ALL_LOCAL_PLAYLIST),
        Err(LibraryError::PlaylistExists(_))
    ));
    let created = library.create_playlist(" Night Drive ").unwrap();
    assert_eq!(created.name, "Night Drive");
    assert_eq!(created.kind, PlaylistKind::User);
    assert!(matches!(
        library.create_playlist("Night Drive"),
        Err(LibraryError::PlaylistExists(_))
    ));
    assert!(library.add_to_playlist(created.id, tracks[1].id).unwrap());
    assert!(!library.add_to_playlist(created.id, tracks[1].id).unwrap());
    assert!(library.add_to_playlist(created.id, tracks[0].id).unwrap());
    assert_eq!(
        library
            .playlist_tracks(created.id)
            .unwrap()
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        vec![tracks[1].id, tracks[0].id]
    );
    assert!(library.remove(tracks[1].id).unwrap());
    let remaining = library.playlist(created.id).unwrap();
    assert_eq!(remaining.len, 1);
    assert_eq!(
        library.playlist_tracks(created.id).unwrap()[0].id,
        tracks[0].id
    );
    assert_eq!(library.all_local().unwrap().len, 1);
    assert!(matches!(
        library.playlist(999),
        Err(LibraryError::PlaylistNotFound(999))
    ));
}

#[test]
fn version_two_upgrade_keeps_audio_and_adds_all_local() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.muse");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch(include_str!("../src/schema-v2.sql"))
        .unwrap();
    db.execute(
        "INSERT INTO library_descriptor VALUES(1, 'kept', 'netease')",
        [],
    )
    .unwrap();
    let hash = format!("{:x}", Sha256::digest(FLAC));
    db.execute(
        "INSERT INTO audio VALUES(?1, ?2)",
        rusqlite::params![hash, FLAC],
    )
    .unwrap();
    db.execute(
        "INSERT INTO tracks(id, title, format, content_hash) VALUES(7, 'Kept', 'flac', ?1)",
        [hash],
    )
    .unwrap();
    drop(db);
    assert!(matches!(
        Library::open(&path),
        Err(LibraryError::UpgradeRequired)
    ));
    let backup = Library::upgrade(&path).unwrap().unwrap();
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".pre-v3.")
    );
    let old = Connection::open(&backup).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let library = Library::open(&path).unwrap();
    assert_eq!(library.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(library.descriptor().unwrap().name, "kept");
    assert_eq!(library.list().unwrap()[0].id, 7);
    assert_eq!(library.open_audio(7).unwrap().into_inner(), FLAC);
    let all = library.all_local().unwrap();
    assert_eq!(all.name, ALL_LOCAL_PLAYLIST);
    assert_eq!(library.playlist_tracks(all.id).unwrap()[0].id, 7);
    drop(library);
    assert!(Library::upgrade(&path).unwrap().is_none());
}
