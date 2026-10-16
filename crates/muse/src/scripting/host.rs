// SPDX-License-Identifier: AGPL-3.0-only

//! Shared handle for script calls into the existing playback client.

use rhai::{EvalAltResult, Position};
use std::sync::{Arc, Mutex};

use crate::playback::View;
use crate::service::{Client, Command};

#[derive(Clone, Default)]
pub struct Host {
    client: Arc<Mutex<Option<Arc<Client>>>>,
    confirmed_seek: Arc<Mutex<Option<View>>>,
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
        let view = client.call(command).map_err(runtime_error)?;
        if matches!(command, Command::Seek { .. } | Command::SeekPercent(_)) {
            *self
                .confirmed_seek
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(view);
        } else if matches!(command, Command::Stop) {
            self.take_confirmed_seek();
        }
        Ok(())
    }

    pub fn take_confirmed_seek(&self) -> Option<View> {
        self.confirmed_seek
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

pub(super) fn runtime_error(error: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(error.into().into(), Position::NONE).into()
}
