// SPDX-License-Identifier: AGPL-3.0-only

use crate::support::{ImportWorld, Result};
use cucumber::when;
use std::sync::atomic::AtomicBool;

#[when(expr = "I import the folder {string}")]
fn import_folder(world: &mut ImportWorld, folder: String) -> Result<()> {
    let path = world.source_path(&folder)?;
    if !path.is_dir() {
        return Err(format!("download folder does not exist: {folder}").into());
    }
    world.last_report = Some(world.library.import_downloads_with_progress(
        &path,
        &AtomicBool::new(false),
        |_| {},
    ));
    world.check_sources()
}

#[when("I reopen the library")]
fn reopen_library(world: &mut ImportWorld) -> Result<()> {
    world.reopen_library()
}

#[when("I explicitly upgrade the legacy library")]
fn upgrade_legacy(world: &mut ImportWorld) -> Result<()> {
    world.upgrade_legacy()
}
