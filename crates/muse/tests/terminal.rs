// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(unix)]

use std::{
    fs::File,
    io::{Read, Write},
    os::unix::ffi::OsStrExt,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    io::{FdFlags, fcntl_setfd},
    pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt},
    termios::{Winsize, tcsetwinsize},
};
use tempfile::TempDir;

#[allow(dead_code)]
#[path = "../src/playback.rs"]
mod playback;
#[allow(dead_code, unused_imports)]
#[path = "../src/service/mod.rs"]
mod service;
#[allow(dead_code)]
#[path = "support/service.rs"]
mod support;

#[test]
#[ignore = "child process entry point used by terminal integration tests"]
fn service_child() {
    support::child();
}

struct Player(Child, std::path::PathBuf, bool);

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        if self.2 {
            let _ = Command::new(env!("CARGO_BIN_EXE_muse"))
                .current_dir(&self.1)
                .env("MUSE_RUNTIME_DIR", self.1.join("runtime"))
                .arg("shutdown")
                .output();
        }
    }
}

fn start(directory: &TempDir, stderr_in_terminal: bool) -> (Player, File) {
    start_mode(directory, stderr_in_terminal, &[])
}
fn start_mode(directory: &TempDir, stderr_in_terminal: bool, args: &[&str]) -> (Player, File) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
    fcntl_setfd(&master, FdFlags::CLOEXEC).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let name = ptsname(&master, Vec::new()).unwrap();
    let slave = File::options()
        .read(true)
        .write(true)
        .open(std::ffi::OsStr::from_bytes(name.to_bytes()))
        .unwrap();
    tcsetwinsize(
        &slave,
        Winsize {
            ws_row: 24,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .unwrap();
    let stderr = if stderr_in_terminal {
        Stdio::from(slave.try_clone().unwrap())
    } else {
        Stdio::piped()
    };
    let child = Command::new(env!("CARGO_BIN_EXE_muse"))
        .args(args)
        .current_dir(directory.path())
        .env("MUSE_RUNTIME_DIR", directory.path().join("runtime"))
        .stdin(slave.try_clone().unwrap())
        .stdout(slave)
        .stderr(stderr)
        .spawn()
        .unwrap();
    (
        Player(child, directory.path().to_owned(), true),
        File::from(master),
    )
}

fn wait_for_screen(player: &mut Player, terminal: &mut File, text: &str) -> Vec<u8> {
    assert!(player.0.try_wait().unwrap().is_none(), "muse exited early");
    read_until(terminal, text)
}

fn read_until(terminal: &mut File, text: &str) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        let mut fds = [PollFd::new(terminal, PollFlags::IN)];
        let timeout = Timespec::try_from(Duration::from_millis(50)).unwrap();
        if poll(&mut fds, Some(&timeout)).unwrap() > 0 {
            let mut buffer = [0; 8192];
            let count = terminal.read(&mut buffer).unwrap();
            output.extend_from_slice(&buffer[..count]);
            if String::from_utf8_lossy(&output).contains(text) {
                return output;
            }
        }
    }
    panic!(
        "muse did not display {text:?}: {}",
        String::from_utf8_lossy(&output)
    );
}

fn wait_for_exit(player: &mut Player) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = player.0.try_wait().unwrap() {
            let mut stderr = String::new();
            if let Some(mut pipe) = player.0.stderr.take() {
                pipe.read_to_string(&mut stderr).unwrap();
            }
            assert!(status.success(), "muse failed: {status}: {stderr}");
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "muse kept running after terminal hangup or quit"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn terminal_hangup_during_event_poll_exits_without_spinning() {
    terminal_hangup(false);
}

#[test]
fn terminal_hangup_with_stderr_on_the_same_terminal_exits_cleanly() {
    terminal_hangup(true);
}

fn terminal_hangup(stderr_in_terminal: bool) {
    let directory = TempDir::new().unwrap();
    let (mut player, mut terminal) = start(&directory, stderr_in_terminal);
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    // Close while the UI is waiting inside the event library, rather than drawing.
    thread::sleep(Duration::from_millis(20));
    drop(terminal);
    wait_for_exit(&mut player);
    let library = muse_library::Library::open(directory.path().join("library.muse")).unwrap();
    assert!(library.list().unwrap().is_empty());
}

#[test]
fn connected_terminal_still_accepts_commands_and_quits() {
    let directory = TempDir::new().unwrap();
    let (mut player, mut terminal) = start(&directory, true);
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    thread::sleep(Duration::from_millis(150));
    assert!(player.0.try_wait().unwrap().is_none());
    terminal.write_all(b":").unwrap();
    wait_for_screen(&mut player, &mut terminal, "COMMAND");
    terminal.write_all(b"q\r").unwrap();
    wait_for_exit(&mut player);
}

fn seed_library(directory: &TempDir) {
    let mut library = muse_library::Library::open(directory.path().join("library.muse")).unwrap();
    for letter in b'A'..=b'T' {
        let title = char::from(letter).to_string().repeat(8);
        let mut audio = include_bytes!("../../muse-library/tests/fixtures/silence.mp3").to_vec();
        let mut tag = [0_u8; 128];
        tag[..3].copy_from_slice(b"TAG");
        tag[3..11].copy_from_slice(title.as_bytes());
        if matches!(letter, b'C' | b'R') {
            tag[63..69].copy_from_slice(b"Needle");
        }
        audio.extend_from_slice(&tag);
        let source = directory.path().join(format!("{title}.mp3"));
        std::fs::write(&source, audio).unwrap();
        assert_eq!(library.import(&source).imported, 1);
    }
}

#[test]
fn song_search_accepts_real_input_and_repeats_without_starting_playback() {
    let directory = TempDir::new().unwrap();
    seed_library(&directory);
    let (mut player, mut terminal) = start(&directory, true);
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    terminal.write_all(b"/").unwrap();
    wait_for_screen(&mut player, &mut terminal, "COMMAND");
    terminal.write_all(b"Needle").unwrap();
    wait_for_screen(&mut player, &mut terminal, "1/2 matches");
    terminal.write_all(b"\r").unwrap();
    let confirmed = wait_for_screen(&mut player, &mut terminal, "NORMAL");
    let confirmed = String::from_utf8_lossy(&confirmed);
    // Ratatui writes cell diffs, so the status message can be split by cursor
    // escapes. The renderer tests assert its complete text and match position.
    assert!(!confirmed.contains("Loading track"));
    terminal.write_all(b"n").unwrap();
    let repeated = wait_for_screen(&mut player, &mut terminal, "RRRRRRRR");
    assert!(!String::from_utf8_lossy(&repeated).contains("Loading track"));
    terminal.write_all(b":q\r").unwrap();
    wait_for_exit(&mut player);
    let library = muse_library::Library::open(directory.path().join("library.muse")).unwrap();
    assert_eq!(library.list().unwrap().len(), 20);
}

#[test]
fn wheel_events_scroll_the_real_ui_and_mouse_capture_is_restored() {
    let directory = TempDir::new().unwrap();
    seed_library(&directory);

    let (mut player, mut terminal) = start(&directory, true);
    let startup = wait_for_screen(&mut player, &mut terminal, "NORMAL");
    let startup = String::from_utf8_lossy(&startup);
    assert!(startup.contains("\x1b[?1006h"));
    assert!(startup.contains("LLLLLLLL"));
    // SGR reports use one-based coordinates: column/row 6 is inside Library.
    terminal.write_all(b"\x1b[<65;6;6M").unwrap();
    wait_for_screen(&mut player, &mut terminal, "OOOOOOOO");
    terminal.write_all(b"\x1b[<64;6;6M").unwrap();
    wait_for_screen(&mut player, &mut terminal, "LLLLLLLL");
    terminal.write_all(b":q\r").unwrap();
    let restored = read_until(&mut terminal, "\x1b[?1049l");
    let restored = String::from_utf8_lossy(&restored);
    assert!(restored.contains("\x1b[?1006l"));
    assert!(restored.contains("\x1b[?1000l"));
    wait_for_exit(&mut player);
}

#[test]
fn detach_reconnect_and_quit_control_the_same_playback_session() {
    let session = support::Session::new();
    session.play();
    let before = session.rpc(serde_json::json!("Status"), serde_json::json!(null));
    let (mut player, mut terminal) = start(&session.directory, true);
    player.2 = false; // The Session owns service cleanup across multiple terminal attachments.
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    terminal.write_all(b":det\t").unwrap();
    thread::sleep(Duration::from_millis(50));
    assert!(player.0.try_wait().unwrap().is_none());
    terminal.write_all(b"\r").unwrap();
    let restored = read_until(&mut terminal, "\x1b[?1049l");
    assert!(String::from_utf8_lossy(&restored).contains("\x1b[?1006l"));
    wait_for_exit(&mut player);
    drop(terminal);
    assert!(session.status().contains("State: Playing"));
    let detached = session.rpc(serde_json::json!("Status"), serde_json::json!(null));
    assert_eq!(
        before["result"]["Ok"]["current"],
        detached["result"]["Ok"]["current"]
    );
    assert_ne!(
        before["result"]["Ok"]["status"]["position"],
        detached["result"]["Ok"]["status"]["position"]
    );

    support::success(support::cli(&session.directory, &["pause"]));
    let paused = session.status();
    let (mut second, mut terminal) = start(&session.directory, true);
    second.2 = false;
    wait_for_screen(&mut second, &mut terminal, "Paused");
    terminal.write_all(b":detach\r").unwrap();
    wait_for_exit(&mut second);
    assert_eq!(paused, session.status());

    support::success(support::cli(&session.directory, &["play"]));
    let (mut third, mut terminal) = start(&session.directory, true);
    third.2 = false;
    wait_for_screen(&mut third, &mut terminal, "NORMAL");
    terminal.write_all(b":q\r").unwrap();
    wait_for_exit(&mut third);
    assert!(session.status().contains("State: Stopped"));
}

#[test]
fn terminal_hangup_stops_but_forced_client_loss_preserves_playback() {
    let session = support::Session::new();
    session.play();
    let (mut player, mut terminal) = start(&session.directory, false);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    drop(terminal);
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Stopped"));

    session.play();
    let (mut player, mut terminal) = start(&session.directory, true);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    player.0.kill().unwrap();
    player.0.wait().unwrap();
    drop(terminal);
    assert!(session.status().contains("State: Playing"));
}

#[test]
fn detach_during_import_finishes_a_transaction_and_preserves_playback() {
    let session = support::Session::new();
    session.play();
    let sources = session.directory.path().join("imports");
    std::fs::create_dir(&sources).unwrap();
    for index in 0..100 {
        std::fs::write(
            sources.join(format!("{index}.ncm")),
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
    }
    let (mut player, mut terminal) = start(&session.directory, true);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    terminal.write_all(b":import imports\r:detach\r").unwrap();
    read_until(&mut terminal, "\x1b[?1049l");
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Playing"));
    // Reopen the actual database and read every committed audio source after cleanup.
    let library =
        muse_library::Library::open(session.directory.path().join("library.muse")).unwrap();
    for track in library.list().unwrap() {
        assert!(
            !library
                .open_audio(track.id)
                .unwrap()
                .into_inner()
                .is_empty()
        );
    }
    assert_eq!(std::fs::read_dir(sources).unwrap().count(), 100);
}

#[test]
fn service_shutdown_is_visible_and_detach_still_restores_the_terminal() {
    let session = support::Session::new();
    session.play();
    let (mut player, mut terminal) = start(&session.directory, true);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    support::success(support::cli(&session.directory, &["shutdown"]));
    wait_for_screen(&mut player, &mut terminal, "unavailable");
    terminal.write_all(b":detach\r").unwrap();
    let output = read_until(&mut terminal, "\x1b[?1049l");
    assert!(String::from_utf8_lossy(&output).contains("\x1b[?1006l"));
    wait_for_exit(&mut player);
}

fn add_variant(session: &support::Session, filename: &str, bytes: &[u8]) {
    let path = session.directory.path().join(filename);
    std::fs::write(&path, bytes).unwrap();
    support::success(support::cli(
        &session.directory,
        &["import", path.to_str().unwrap()],
    ));
}

#[test]
fn external_cli_import_updates_tui_and_candidate_confirmation_alone_changes_playback() {
    let session = support::Session::new();
    session.play();
    add_variant(
        &session,
        "first.ncm",
        include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
    );
    let (mut player, mut terminal) = start(&session.directory, true);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    // While command input is active, a different client commits an alternative.
    terminal.write_all(b":").unwrap();
    wait_for_screen(&mut player, &mut terminal, "COMMAND");
    terminal.write_all(b"sto").unwrap();
    add_variant(
        &session,
        "other.ncm",
        include_bytes!("../../muse-e2e/fixtures/flac.ncm"),
    );
    // Polling does not discard the command. Escape then Enter must open a picker.
    thread::sleep(Duration::from_millis(700));
    terminal.write_all(b"\x1b").unwrap();
    wait_for_screen(&mut player, &mut terminal, "NORMAL");
    terminal.write_all(b"\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "Choose candidate");
    let before = session.rpc(serde_json::json!("Status"), serde_json::json!(null))["result"]["Ok"]
        ["current"]
        .clone();
    terminal.write_all(b"j").unwrap();
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        session.rpc(serde_json::json!("Status"), serde_json::json!(null))["result"]["Ok"]["current"],
        before
    );
    terminal.write_all(b"\x1b").unwrap();
    wait_for_screen(&mut player, &mut terminal, "Selection cancell");
    assert_eq!(
        session.rpc(serde_json::json!("Status"), serde_json::json!(null))["result"]["Ok"]["current"],
        before
    );
    terminal.write_all(b"\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "Choose candidate");
    terminal.write_all(b"j\r").unwrap();
    // Keep consuming terminal output so a full PTY cannot block the UI before Enter.
    wait_for_screen(&mut player, &mut terminal, "candidate 2");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if session.status().contains("Track: 2") {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(30));
    }
    terminal.write_all(b":detach\r").unwrap();
    read_until(&mut terminal, "\x1b[?1049l");
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Playing"));
}

#[test]
fn command_mode_handles_history_completion_quoted_import_selection_and_detach() {
    let session = support::Session::new();
    session.play();
    add_variant(
        &session,
        "first.ncm",
        include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
    );
    let source = session.directory.path().join("版本 two.ncm");
    std::fs::write(&source, include_bytes!("../../muse-e2e/fixtures/flac.ncm")).unwrap();
    let (mut player, mut terminal) = start_mode(&session.directory, true, &["command"]);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "[netease]> ");
    terminal
        .write_all("import '版本 two.ncm'\r".as_bytes())
        .unwrap();
    wait_for_screen(&mut player, &mut terminal, "Import: added 1");
    terminal.write_all(b"lis\t\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "2 candidates");
    terminal.write_all(b"\x1b[A\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "2 candidates");
    terminal.write_all(b"play g:1\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "candidate number");
    terminal.write_all(b"\x1b").unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(session.status().contains("Track: 1"));
    terminal.write_all(b"play g:1\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "candidate number");
    terminal.write_all(b"2\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "Playback command completed");
    assert!(session.status().contains("Track: 2"));
    terminal.write_all(b"detach\r").unwrap();
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Playing"));
    let (mut player, mut terminal) = start_mode(&session.directory, true, &["command"]);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "[netease]> ");
    terminal.write_all(b"\x04").unwrap();
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Stopped"));
}

#[test]
fn command_mode_import_detach_and_control_c_finish_cleanly() {
    let session = support::Session::new();
    session.play();
    let downloads = session.directory.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    for i in 0..100 {
        std::fs::write(
            downloads.join(format!("{i}.ncm")),
            include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
        )
        .unwrap();
    }
    let (mut player, mut terminal) = start_mode(&session.directory, true, &["command"]);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "[netease]> ");
    terminal.write_all(b"import downloads\r").unwrap();
    wait_for_screen(&mut player, &mut terminal, "Import started");
    terminal.write_all(b"detach\r").unwrap();
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Playing"));
    let library =
        muse_library::Library::open_existing(session.directory.path().join("library.muse"))
            .unwrap();
    assert_eq!(library.list().unwrap().len(), 1);
    assert_eq!(
        library.open_audio(session.id).unwrap().into_inner(),
        include_bytes!("../../muse-library/tests/fixtures/silence.mp3")
    );
    let (mut player, mut terminal) = start_mode(&session.directory, true, &["command"]);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "[netease]> ");
    terminal.write_all(b"\x03").unwrap();
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Stopped"));
    session.play();
    let (mut player, mut terminal) = start_mode(&session.directory, true, &["command"]);
    player.2 = false;
    wait_for_screen(&mut player, &mut terminal, "[netease]> ");
    drop(terminal);
    wait_for_exit(&mut player);
    assert!(session.status().contains("State: Stopped"));
}
