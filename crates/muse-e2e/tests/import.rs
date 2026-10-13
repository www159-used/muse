// SPDX-License-Identifier: AGPL-3.0-only

#[allow(dead_code, unused_imports)]
#[path = "../../muse/src/playback.rs"]
mod playback;
#[allow(dead_code, unused_imports)]
#[path = "../../muse/src/service/mod.rs"]
mod service;
#[allow(dead_code, unused_imports)]
#[path = "../../muse/src/volume.rs"]
mod volume;

mod steps;
mod support;

use cucumber::{Parser as _, World as _};
use futures::{StreamExt, executor::block_on};
use std::path::Path;
use support::ImportWorld;

fn main() {
    if std::env::var_os("MUSE_E2E_CLOCK").is_some() {
        let path = std::env::var("MUSE_TEST_CHILD_LIBRARY").expect("clock service library path");
        service::run_with(Path::new(&path), |path| {
            playback::Worker::start_with(path.to_owned(), || Ok(support::ClockPlayer::default()))
        })
        .expect("clock playback service");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios");
    let features = block_on(
        cucumber::parser::Basic::default()
            .parse(&root, Default::default())
            .collect::<Vec<_>>(),
    );
    let features = features
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("could not parse import stories");
    let scenarios: usize = features.iter().map(|feature| feature.scenarios.len()).sum();
    assert!(scenarios > 0, "no import scenarios found");
    if std::env::args().skip(1).eq(["--list"]) {
        for feature in &features {
            for scenario in &feature.scenarios {
                println!(
                    "{}:{} — {}",
                    feature.path.as_ref().unwrap().display(),
                    scenario.position.line,
                    scenario.name
                );
            }
        }
        return;
    }
    block_on(
        ImportWorld::cucumber()
            .fail_on_skipped_with(|_, _, _| true)
            .run_and_exit(root),
    );
}
