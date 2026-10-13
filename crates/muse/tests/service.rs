// SPDX-License-Identifier: AGPL-3.0-only

#[allow(dead_code)]
#[path = "../src/playback.rs"]
mod playback;
#[allow(dead_code, unused_imports)]
#[path = "../src/service/mod.rs"]
mod service;
#[path = "support/service.rs"]
mod support;

use serde_json::json;
use std::{
    fs, thread,
    time::{Duration, Instant},
};
use support::{Session, cli, success};

#[test]
#[ignore = "child process entry point used by integration tests"]
fn service_child() {
    support::child();
}

#[test]
fn cli_controls_one_session_and_shutdown_releases_it() {
    let mut session = Session::new();
    session.play();
    thread::sleep(Duration::from_millis(200));
    assert!(session.status().contains("State: Playing"));
    success(cli(&session.directory, &["pause"]));
    let paused = session.status();
    assert!(paused.contains("State: Paused"));
    assert!(paused.contains(&format!("Track: {}", session.id)));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(paused, session.status());
    success(cli(&session.directory, &["pause"]));
    assert_eq!(paused, session.status());
    success(cli(&session.directory, &["play"]));
    assert!(session.status().contains("State: Playing"));
    success(cli(&session.directory, &["stop"]));
    assert!(
        session
            .status()
            .contains("State: Stopped\nPosition: 0.000s")
    );
    success(cli(&session.directory, &["stop"]));
    assert!(session.child.try_wait().unwrap().is_none());
    success(cli(&session.directory, &["shutdown"]));
    assert!(session.status().contains("No playback service"));
    assert!(session.child.wait().unwrap().success());
}

#[test]
fn stale_seek_and_failed_load_preserve_the_new_track() {
    let session = Session::new();
    session.play();
    let first = session.rpc(json!("Status"), json!(null))["result"]["Ok"]["current"].clone();
    session.play();
    let seek = session.rpc(json!({"Seek": {"direction": 1, "millis": 600000}}), first);
    assert!(seek["result"]["Err"].as_str().unwrap().contains("stale"));
    assert!(
        !cli(&session.directory, &["play", "999999"])
            .status
            .success()
    );
    assert!(session.status().contains("State: Playing"));
    success(cli(&session.directory, &["stop"]));
    assert!(session.status().contains("State: Stopped"));
}

#[test]
fn empty_observation_and_invalid_commands_have_no_side_effects() {
    let directory = tempfile::TempDir::new().unwrap();
    for args in [vec!["status"], vec!["stop"], vec!["shutdown"]] {
        assert!(success(cli(&directory, &args)).contains("No playback service"));
    }
    for args in [
        vec!["play"],
        vec!["pause"],
        vec!["play", "12"],
        vec!["play", "0"],
        vec!["stop", "extra"],
        vec!["pause", "--backup", "copy.muse"],
    ] {
        assert!(!cli(&directory, &args).status.success(), "{args:?}");
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn concurrent_launches_converge_and_stale_service_can_restart() {
    let mut session = Session::new();
    let path = session.directory.path().join("library.muse");
    let endpoint = session.endpoint();
    let mut challengers: Vec<_> = (0..4)
        .map(|_| support::launch(&session.directory, &path))
        .collect();
    for challenger in &mut challengers {
        assert!(challenger.wait().unwrap().success());
    }
    assert_eq!(session.endpoint(), endpoint);
    session.child.kill().unwrap();
    session.child.wait().unwrap();
    assert!(session.status().contains("No playback service"));
    session.child = support::launch(&session.directory, &path);
    support::wait_ready(&session.directory);
    assert_ne!(session.endpoint()["token"], endpoint["token"]);
    assert!(session.status().contains("State: Empty"));
}

#[cfg(unix)]
#[test]
fn symlink_alias_controls_the_same_library_and_other_library_is_isolated() {
    let session = Session::new();
    session.play();
    std::os::unix::fs::symlink(
        session.directory.path().join("library.muse"),
        session.directory.path().join("alias.muse"),
    )
    .unwrap();
    success(cli(
        &session.directory,
        &["--library", "alias.muse", "pause"],
    ));
    assert!(session.status().contains("State: Paused"));
    let other = Session::new();
    other.play();
    success(cli(&session.directory, &["stop"]));
    assert!(other.status().contains("State: Playing"));
}

#[test]
fn stop_after_pending_load_is_a_session_operation() {
    let session = Session::new();
    let mut command = support::command(&session.directory);
    let mut load = command
        .args(["play", &session.id.to_string()])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !session.status().contains("Loading:") {
        assert!(Instant::now() < deadline, "load never became pending");
        thread::sleep(Duration::from_millis(5));
    }
    success(cli(&session.directory, &["stop"]));
    assert!(load.wait().unwrap().success());
    thread::sleep(Duration::from_millis(100));
    assert!(session.status().contains("State: Stopped"));
}

#[test]
fn production_launchers_race_without_creating_multiple_services() {
    let directory = tempfile::TempDir::new().unwrap();
    drop(muse_library::Library::open(directory.path().join("library.muse")).unwrap());
    // An invalid ID exercises automatic startup without touching an audio device.
    let mut launchers: Vec<_> = (0..4)
        .map(|_| {
            support::command(&directory)
                .args(["play", "999999"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut launchers {
        assert!(!child.wait().unwrap().success());
    }
    let status = success(cli(&directory, &["status"]));
    assert!(status.contains("State: Empty"), "{status}");
    let endpoint_count = fs::read_dir(directory.path().join("runtime"))
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
        .count();
    assert_eq!(endpoint_count, 1);
    success(cli(&directory, &["shutdown"]));
    assert!(success(cli(&directory, &["status"])).contains("No playback service"));
}

#[test]
fn unauthenticated_oversized_and_incompatible_requests_preserve_playback() {
    use std::{
        io::{Read, Write},
        net::{Ipv4Addr, TcpStream},
    };
    let session = Session::new();
    session.play();
    let endpoint = session.endpoint();
    let address = (
        Ipv4Addr::LOCALHOST,
        endpoint["port"].as_u64().unwrap() as u16,
    );
    for (version, token) in [(1, "incorrect"), (999, endpoint["token"].as_str().unwrap())] {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let data = serde_json::to_vec(
            &json!({"version":version,"token":token,"id":2,"command":"Stop","expected":null}),
        )
        .unwrap();
        stream
            .write_all(&(data.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&data).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        assert!(String::from_utf8_lossy(&response).contains("unauthenticated"));
    }
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let mut byte = [0];
    assert_eq!(stream.read(&mut byte).unwrap(), 0);
    assert!(session.status().contains("State: Playing"));

    let record = fs::read_dir(session.directory.path().join("runtime"))
        .unwrap()
        .map(Result::unwrap)
        .find(|e| e.path().extension().is_some_and(|s| s == "json"))
        .unwrap()
        .path();
    let original = fs::read(&record).unwrap();
    let mut incompatible = endpoint;
    incompatible["version"] = json!(999);
    fs::write(&record, serde_json::to_vec(&incompatible).unwrap()).unwrap();
    let output = cli(&session.directory, &["stop"]);
    fs::write(&record, original).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("incompatible"));
    assert!(session.status().contains("State: Playing"));
}

#[test]
fn library_queries_do_not_create_or_start_services_and_ambiguous_play_is_read_only() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["list", "--json"],
        vec!["show", "1", "--json"],
        vec!["library", "info"],
    ] {
        assert!(!cli(&directory, &args).status.success());
        assert!(!directory.path().join("library.muse").exists());
        assert!(!directory.path().join("runtime").exists());
    }
    let source = directory.path().join("音乐 file.ncm");
    fs::write(
        &source,
        include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
    )
    .unwrap();
    success(cli(&directory, &["import", source.to_str().unwrap()]));
    assert!(!directory.path().join("runtime").exists());
    let listed: serde_json::Value =
        serde_json::from_str(&success(cli(&directory, &["list", "--json"]))).unwrap();
    assert_eq!(listed[0]["token"], "g:1");
    let details: serde_json::Value =
        serde_json::from_str(&success(cli(&directory, &["show", "g:1", "--json"]))).unwrap();
    assert_eq!(details["candidates"][0]["sources"][0]["container"], "ncm");
    assert!(!directory.path().join("runtime").exists());

    let session = Session::new();
    session.play();
    let source = session.directory.path().join("first.ncm");
    fs::write(
        &source,
        include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
    )
    .unwrap();
    success(cli(
        &session.directory,
        &["import", source.to_str().unwrap()],
    ));
    let alternative = session.directory.path().join("second.ncm");
    fs::write(
        &alternative,
        include_bytes!("../../muse-e2e/fixtures/flac.ncm"),
    )
    .unwrap();
    success(cli(
        &session.directory,
        &["import", alternative.to_str().unwrap()],
    ));
    let before = session.rpc(json!("Status"), json!(null))["result"]["Ok"]["current"].clone();
    let rejected = cli(&session.directory, &["play", "g:1"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("multiple candidates"));
    assert_eq!(
        session.rpc(json!("Status"), json!(null))["result"]["Ok"]["current"],
        before
    );
    assert!(
        !cli(&session.directory, &["library", "upgrade"])
            .status
            .success()
    );
    let candidates: serde_json::Value = serde_json::from_str(&success(cli(
        &session.directory,
        &["list", "--candidates", "--json"],
    )))
    .unwrap();
    let id = candidates.as_array().unwrap().last().unwrap()["id"]
        .as_i64()
        .unwrap();
    success(cli(&session.directory, &["play", &id.to_string()]));
    assert!(session.status().contains(&format!("Track: {id}")));
}

#[test]
fn playlist_commands_play_next_and_cycle_mode() {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("library.muse");
    let mut library = muse_library::Library::open(&path).unwrap();
    let mp3 = directory.path().join("song.mp3");
    let flac = directory.path().join("song.flac");
    fs::write(
        &mp3,
        include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
    )
    .unwrap();
    fs::write(
        &flac,
        include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
    )
    .unwrap();
    assert_eq!(library.import(&mp3).imported, 1);
    assert_eq!(library.import(&flac).imported, 1);
    let ids: Vec<_> = library
        .list()
        .unwrap()
        .into_iter()
        .map(|track| track.id)
        .collect();
    drop(library);
    let mut child = support::launch(&directory, &path);
    struct Stop<'a>(&'a mut std::process::Child);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _stop = Stop(&mut child);
    support::wait_ready(&directory);
    let listed = success(cli(&directory, &["playlist", "list"]));
    assert!(listed.contains("全部本地"));
    assert!(listed.contains("all-local"));
    success(cli(&directory, &["playlist", "create", "Night", "Drive"]));
    success(cli(
        &directory,
        &["playlist", "add", "Night Drive", &ids[1].to_string()],
    ));
    success(cli(
        &directory,
        &["playlist", "add", "Night Drive", &ids[0].to_string()],
    ));
    let played = success(cli(&directory, &["playlist", "play", "Night Drive"]));
    assert!(played.contains("Playback command completed"));
    let status = success(cli(&directory, &["status"]));
    assert!(status.contains("Playlist: Night Drive"));
    assert!(status.contains("Mode: sequential"));
    assert!(status.contains(&format!("Track: {}", ids[1])));
    assert!(status.contains("Index: 1/2"));
    success(cli(&directory, &["next"]));
    let advanced = success(cli(&directory, &["status"]));
    assert!(advanced.contains(&format!("Track: {}", ids[0])));
    assert!(advanced.contains("Index: 2/2"));
    assert!(!cli(&directory, &["next"]).status.success());
    assert!(success(cli(&directory, &["status"])).contains(&format!("Track: {}", ids[0])));
    success(cli(&directory, &["mode"]));
    assert!(success(cli(&directory, &["status"])).contains("Mode: list loop"));
    success(cli(&directory, &["mode", "single"]));
    assert!(success(cli(&directory, &["status"])).contains("Mode: single loop"));
    success(cli(&directory, &["mode", "sequential"]));
    assert!(success(cli(&directory, &["status"])).contains("Mode: sequential"));
    success(cli(&directory, &["shutdown"]));
}
