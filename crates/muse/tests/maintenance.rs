// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

use muse_library::Library;
use tempfile::TempDir;

fn run(directory: &Path, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_muse"))
        .current_dir(directory)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn backup_and_restore_work_without_tty_and_with_spaces_in_paths() {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("library.muse");
    let mut library = Library::open(&path).unwrap();
    let input = temporary.path().join("song.ncm");
    fs::write(
        &input,
        include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
    )
    .unwrap();
    assert_eq!(library.import(&input).imported, 1);
    let expected = library.list().unwrap();
    let output = run(temporary.path(), ["--backup", "音乐备份 copy.muse"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Backup completed"));
    let output = run(
        temporary.path(),
        [
            "--library",
            "恢复 new.muse",
            "--restore",
            "音乐备份 copy.muse",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Restore completed"));
    drop(library);
    fs::remove_file(&path).unwrap();
    fs::remove_file(temporary.path().join("音乐备份 copy.muse")).unwrap();
    let mut restored = Library::open(temporary.path().join("恢复 new.muse")).unwrap();
    assert_eq!(restored.list().unwrap(), expected);
    assert_eq!(
        restored.open_audio(expected[0].id).unwrap().into_inner(),
        include_bytes!("../../muse-library/tests/fixtures/silence.mp3")
    );
    assert_eq!(restored.attachments(expected[0].id).unwrap().len(), 1);
    assert_eq!(restored.import(input).duplicates, 1);
}

#[test]
fn bad_arguments_and_missing_sources_do_not_initialize_a_library() {
    let temporary = TempDir::new().unwrap();
    for args in [
        vec!["--backup"],
        vec!["--restore"],
        vec!["--backup", ""],
        vec!["--library", "--backup", "copy.muse"],
        vec!["--backup", "copy.muse", "--restore", "source.muse"],
        vec!["--backup", "copy.muse", "--unknown"],
        vec!["--backup", "copy.muse"],
        vec!["--restore", "source.muse"],
    ] {
        let output = run(temporary.path(), &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.starts_with("muse: "), "{error}");
        assert!(
            !error
                .chars()
                .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)),
            "{error}"
        );
        assert_eq!(
            fs::read_dir(temporary.path()).unwrap().count(),
            0,
            "{args:?}"
        );
    }
    let output = run(temporary.path(), ["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("--backup"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("--restore"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: muse"));
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[test]
fn failed_cli_operations_preserve_existing_files() {
    let temporary = TempDir::new().unwrap();
    drop(Library::open(temporary.path().join("library.muse")).unwrap());
    let source = fs::read(temporary.path().join("library.muse")).unwrap();
    fs::write(temporary.path().join("copy.muse"), b"keep existing").unwrap();
    let output = run(temporary.path(), ["--backup", "copy.muse"]);
    assert!(!output.status.success());
    assert_eq!(
        fs::read(temporary.path().join("copy.muse")).unwrap(),
        b"keep existing"
    );
    assert_eq!(
        fs::read(temporary.path().join("library.muse")).unwrap(),
        source
    );
    let output = run(
        temporary.path(),
        ["--library", "new.muse", "--restore", "copy.muse"],
    );
    assert!(!output.status.success());
    assert!(!temporary.path().join("new.muse").exists());
    assert_eq!(
        fs::read(temporary.path().join("copy.muse")).unwrap(),
        b"keep existing"
    );
    assert!(fs::read_dir(temporary.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".muse-backup-")
    }));
}
