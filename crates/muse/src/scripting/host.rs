// SPDX-License-Identifier: AGPL-3.0-only

//! Shared handle for script calls into the existing playback client.

use rhai::{EvalAltResult, Position};
use std::sync::{Arc, Mutex};

use crate::service::{Client, Command};

#[derive(Clone, Default)]
pub struct Host {
    client: Arc<Mutex<Option<Arc<Client>>>>,
}

impl Host {
    pub fn attach(&self, client: Arc<Client>) {
        *self
            .client
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(client);
    }

    pub fn execute(&self, command: Command) -> Result<(), Box<EvalAltResult>> {
        let client = self
            .client
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .ok_or_else(|| runtime_error("playback service is unavailable"))?;
        client.call(command).map(|_| ()).map_err(runtime_error)
    }
}

fn runtime_error(error: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(error.into().into(), Position::NONE).into()
}
