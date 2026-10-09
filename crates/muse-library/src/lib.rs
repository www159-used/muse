// SPDX-License-Identifier: AGPL-3.0-only

//! Owned local music storage. Imports never modify source files.

mod backup;
mod import;
mod ncm;
mod storage;

pub use import::{ImportIssue, ImportPhase, ImportProgress, ImportReport};
pub use storage::{Attachment, AttachmentKind, Library, LibraryError, Track};
