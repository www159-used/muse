// SPDX-License-Identifier: AGPL-3.0-only

use std::{error::Error, fmt, sync::Arc};

/// The operation that failed in the native playback backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackErrorKind {
    NoSource,
    UnsupportedFormat,
    UnsupportedSeek,
    Input,
    Decode,
    Output,
    Worker,
}

/// An error whose cause can be retained and reported by subsequent status calls.
#[derive(Clone, Debug)]
pub struct PlaybackError {
    kind: PlaybackErrorKind,
    cause: Arc<dyn Error + Send + Sync>,
}

impl PlaybackError {
    pub(crate) fn new(kind: PlaybackErrorKind, cause: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind,
            cause: Arc::new(cause),
        }
    }

    pub(crate) fn message(kind: PlaybackErrorKind, message: impl Into<String>) -> Self {
        Self::new(kind, std::io::Error::other(message.into()))
    }

    pub fn kind(&self) -> PlaybackErrorKind {
        self.kind
    }
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.cause)
    }
}

impl Error for PlaybackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
