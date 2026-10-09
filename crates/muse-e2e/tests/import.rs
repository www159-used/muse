// SPDX-License-Identifier: AGPL-3.0-only

mod steps;
mod support;

use cucumber::{Parser as _, World as _};
use futures::{StreamExt, executor::block_on};
use std::path::Path;
use support::ImportWorld;

fn main() {
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
