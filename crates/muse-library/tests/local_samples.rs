// SPDX-License-Identifier: AGPL-3.0-only

use std::{fs, path::PathBuf};

use muse_library::Library;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// Opt-in smoke check: copies three real files into a temporary source directory.
/// It never imports into an existing library or changes the supplied source folder.
#[test]
#[ignore = "set MUSE_SAMPLE_ROOT to a folder containing MP3, FLAC and NCM"]
fn import_real_local_samples() {
    let root =
        PathBuf::from(std::env::var_os("MUSE_SAMPLE_ROOT").expect("MUSE_SAMPLE_ROOT is required"));
    let temporary = TempDir::new().unwrap();
    let copies = temporary.path().join("sources");
    fs::create_dir(&copies).unwrap();
    let mut originals = Vec::new();
    for extension in ["mp3", "flac", "ncm"] {
        let mut paths = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|suffix| suffix == extension))
            .collect::<Vec<_>>();
        paths.sort_by_key(|path| path.metadata().unwrap().len());
        let source = paths.first().expect("sample format is missing");
        let bytes = fs::read(source).unwrap();
        originals.push((source.clone(), Sha256::digest(&bytes)));
        fs::write(copies.join(format!("sample.{extension}")), bytes).unwrap();
    }
    let database = temporary.path().join("library.muse");
    let mut library = Library::open(&database).unwrap();
    let first = library.import(&copies);
    assert_eq!(first.imported + first.duplicates, 3, "{first:?}");
    assert!(first.failed.is_empty(), "{first:?}");
    let second = library.import(&copies);
    assert_eq!(
        (second.imported, second.duplicates, second.failed.len()),
        (0, 3, 0),
        "{second:?}"
    );
    drop(library);
    fs::remove_dir_all(copies).unwrap();
    let library = Library::open(&database).unwrap();
    for track in library.list().unwrap() {
        let reader = library.open_audio(track.id).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(reader.get_ref())),
            track.content_hash
        );
        assert!(!track.title.is_empty());
    }
    for (source, original_hash) in originals {
        assert_eq!(Sha256::digest(fs::read(source).unwrap()), original_hash);
    }
}
