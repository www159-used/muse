// SPDX-License-Identifier: AGPL-3.0-only

use crate::support::{ImportWorld, Result};
use cucumber::when;

#[when(expr = "I import the folder {string}")]
#[when(expr = "I import the folder {string} again")]
fn import_folder(world: &mut ImportWorld, folder: String) -> Result<()> {
    world.import_folder(&folder)
}

#[when("I reopen the library")]
fn reopen_library(world: &mut ImportWorld) -> Result<()> {
    world.reopen_library()
}

#[when("I explicitly upgrade the legacy library")]
fn upgrade_legacy(world: &mut ImportWorld) -> Result<()> {
    world.upgrade_legacy()
}

#[when(expr = "I play the playlist {string}")]
fn play_playlist(world: &mut ImportWorld, name: String) -> Result<()> {
    world.play_playlist(&name)
}

#[when(expr = "I play the track from fixture {string}")]
fn play_fixture(world: &mut ImportWorld, fixture: String) -> Result<()> {
    world.play_fixture(&fixture)
}

#[when("I advance to the next track")]
fn advance(world: &mut ImportWorld) -> Result<()> {
    world.advance()
}

#[when("I go to the previous track")]
fn previous(world: &mut ImportWorld) -> Result<()> {
    world.previous()
}

#[when("I try to go back past the first track")]
fn try_previous(world: &mut ImportWorld) -> Result<()> {
    world.attempt_previous()
}

#[when("I pause playback")]
fn pause_playback(world: &mut ImportWorld) -> Result<()> {
    world.pause()
}

#[when("I resume playback")]
fn resume_playback(world: &mut ImportWorld) -> Result<()> {
    world.resume()
}

#[when(expr = "I queue the track from fixture {string} to play next once")]
fn queue_next(world: &mut ImportWorld, fixture: String) -> Result<()> {
    world.queue_fixture_next(&fixture)
}

#[when("I try to advance past the last track")]
fn try_advance(world: &mut ImportWorld) -> Result<()> {
    world.attempt_advance()
}

#[when("I cycle the playback mode")]
fn cycle_mode(world: &mut ImportWorld) -> Result<()> {
    world.cycle_mode()
}

#[when(expr = "I set the playback volume to {int}")]
fn set_volume(world: &mut ImportWorld, volume: usize) -> Result<()> {
    world.volume(&["volume", "set", &volume.to_string()])
}

#[when(expr = "I increase the playback volume by {int}")]
fn increase_volume(world: &mut ImportWorld, steps: usize) -> Result<()> {
    world.volume(&["volume", "inc", &steps.to_string()])
}

#[when(expr = "I decrease the playback volume by {int}")]
fn decrease_volume(world: &mut ImportWorld, steps: usize) -> Result<()> {
    world.volume(&["volume", "dec", &steps.to_string()])
}

#[when("I restart the playback service")]
fn restart_playback(world: &mut ImportWorld) -> Result<()> {
    world.restart_playback()
}

#[when("I shut down the playback service")]
fn shutdown_playback(world: &mut ImportWorld) -> Result<()> {
    world.shutdown_playback()
}
