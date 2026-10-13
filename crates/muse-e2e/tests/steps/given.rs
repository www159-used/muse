// SPDX-License-Identifier: AGPL-3.0-only

use crate::support::{ImportWorld, Result};
use cucumber::{gherkin::Step, given};

#[given(expr = "{string} is a copy of the NCM fixture {string}")]
fn downloaded_ncm(world: &mut ImportWorld, path: String, fixture: String) -> Result<()> {
    world.copy_ncm(&path, &fixture)
}

#[given(expr = "{string} is a copy of the fixture {string}")]
fn source_file(world: &mut ImportWorld, path: String, fixture: String) -> Result<()> {
    world.copy_fixture(&path, &fixture)
}

#[given(expr = "{string} is an unreadable or corrupt audio file")]
fn corrupt_audio(world: &mut ImportWorld, path: String) -> Result<()> {
    world.write_corrupt(&path)
}

#[given(expr = "{string} is replaced with a copy of the fixture {string}")]
fn replace_fixture(world: &mut ImportWorld, path: String, fixture: String) -> Result<()> {
    world.replace_fixture(&path, &fixture)
}

#[given(expr = "an empty folder {string}")]
fn empty_folder(world: &mut ImportWorld, folder: String) -> Result<()> {
    world.empty_folder(&folder)
}

#[given(expr = "a previous import failed on {string}")]
fn previous_failure(world: &mut ImportWorld, path: String) -> Result<()> {
    world.fail_import_on(&path)
}

#[given(expr = "the text file {string} contains:")]
fn text_file(world: &mut ImportWorld, path: String, step: &Step) -> Result<()> {
    let text = step
        .docstring
        .as_deref()
        .ok_or("expected a text docstring")?;
    world.write_source(&path, text.as_bytes().to_vec())
}

#[given(expr = "a legacy library suppressed the source {string} by tagged name")]
fn legacy_library(world: &mut ImportWorld, source: String) -> Result<()> {
    world.prepare_legacy(&source)
}
