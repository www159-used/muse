// SPDX-License-Identifier: AGPL-3.0-only

//! Owned local music storage. Imports never modify source files.

mod backup;
pub mod netease;
mod storage;
mod upgrade;

pub use netease::{ImportIssue, ImportPhase, ImportProgress, ImportReport};
pub use storage::{
    ALL_LOCAL_PLAYLIST, Attachment, AttachmentKind, AttachmentObservation, Library,
    LibraryDescriptor, LibraryError, LibraryKind, Playlist, PlaylistKind, SCHEMA_VERSION,
    SongGroup, SourceObservation, Track,
};
