// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use rusqlite_migration::{M, Migrations};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub(crate) const APPLICATION_ID: i64 = 0x4d55_5345;
pub(crate) const SCHEMA_VERSION: i64 = 1;
const TRACK_COLUMNS: &str =
    "id, title, artist, album, format, duration_ms, netease_id, content_hash";

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("library schema migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("this SQLite file is not a muse library")]
    ForeignDatabase,
    #[error("library version {0} is newer than supported version {SCHEMA_VERSION}")]
    NewerSchema(i64),
    #[error("track {0} does not exist")]
    TrackNotFound(i64),
    #[error("the import is missing complete audio for the new track")]
    MissingAudio,
    #[error("the destination or its SQLite journal already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("library copy validation failed: {0}")]
    InvalidBackup(String),
    #[error("database lock timeout; backup did not complete")]
    BackupBusy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub format: String,
    pub duration_ms: Option<i64>,
    pub netease_id: Option<i64>,
    pub content_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Cover,
    Lyrics,
}

impl AttachmentKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Lyrics => "lyrics",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct Metadata {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub format: String,
    pub duration_ms: Option<i64>,
    pub netease_id: Option<i64>,
}

/// A connection to one library. Open separate connections for background imports.
pub struct Library {
    pub(crate) connection: Connection,
}

impl Library {
    /// Opens or creates a library, refusing foreign or newer databases before writes.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LibraryError> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let application_id: i64 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let table_count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if application_id != APPLICATION_ID
            && !(application_id == 0 && version == 0 && table_count == 0)
        {
            return Err(LibraryError::ForeignDatabase);
        }
        if version > SCHEMA_VERSION {
            return Err(LibraryError::NewerSchema(version));
        }
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Migrations::new(vec![M::up(include_str!("schema.sql"))]).to_latest(&mut connection)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(Self { connection })
    }

    /// Lists metadata in stable import order, without fetching audio or attachments.
    pub fn list(&self) -> Result<Vec<Track>, LibraryError> {
        let mut statement = self
            .connection
            .prepare(&format!("SELECT {TRACK_COLUMNS} FROM tracks ORDER BY id"))?;
        Ok(statement
            .query_map([], track_from_row)?
            .collect::<Result<_, _>>()?)
    }

    /// Returns an owned reader that remains valid after the library is closed.
    ///
    /// This first implementation copies one complete encoded song into memory.
    pub fn open_audio(&self, track_id: i64) -> Result<Cursor<Vec<u8>>, LibraryError> {
        let bytes = self
            .connection
            .query_row(
                "SELECT audio.data FROM audio JOIN tracks USING(content_hash) WHERE tracks.id = ?1",
                [track_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or(LibraryError::TrackNotFound(track_id))?;
        Ok(Cursor::new(bytes))
    }

    pub fn attachments(&self, track_id: i64) -> Result<Vec<Attachment>, LibraryError> {
        self.track(track_id)?;
        let mut statement = self.connection.prepare(
            "SELECT kind, mime_type, data FROM attachments WHERE track_id = ?1 ORDER BY kind",
        )?;
        Ok(statement
            .query_map([track_id], |row| {
                let kind: String = row.get(0)?;
                Ok(Attachment {
                    kind: if kind == "cover" {
                        AttachmentKind::Cover
                    } else {
                        AttachmentKind::Lyrics
                    },
                    mime_type: row.get(1)?,
                    data: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    /// Atomically removes a song and all its library-owned content and source records.
    /// Returns false if it was already absent. Source files are never touched.
    pub fn remove(&mut self, track_id: i64) -> Result<bool, LibraryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let hash: Option<String> = transaction
            .query_row(
                "SELECT content_hash FROM tracks WHERE id = ?1",
                [track_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(hash) = hash else { return Ok(false) };
        transaction.execute("DELETE FROM tracks WHERE id = ?1", [track_id])?;
        transaction.execute(
            "DELETE FROM audio WHERE content_hash = ?1 AND NOT EXISTS (SELECT 1 FROM tracks WHERE content_hash = ?1)",
            [hash],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn track(&self, id: i64) -> Result<Track, LibraryError> {
        self.connection
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                [id],
                track_from_row,
            )
            .optional()?
            .ok_or(LibraryError::TrackNotFound(id))
    }

    pub(crate) fn source_track(&self, source_hash: &str) -> Result<Option<Track>, LibraryError> {
        let id = self
            .connection
            .query_row(
                "SELECT track_id FROM import_sources WHERE source_hash = ?1",
                [source_hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        id.map(|id| self.track(id)).transpose()
    }

    pub(crate) fn store(
        &mut self,
        source_hash: &str,
        content_hash: &str,
        metadata: &Metadata,
        audio: Option<&[u8]>,
        attachments: &[Attachment],
    ) -> Result<bool, LibraryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT track_id FROM import_sources WHERE source_hash = ?1",
                [source_hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .or(transaction
                .query_row(
                    "SELECT id FROM tracks WHERE content_hash = ?1",
                    [content_hash],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?);
        let (track_id, added) = if let Some(id) = existing {
            (id, false)
        } else {
            let audio = audio.ok_or(LibraryError::MissingAudio)?;
            transaction.execute(
                "INSERT INTO audio(content_hash, data) VALUES (?1, ?2)",
                params![content_hash, audio],
            )?;
            transaction.execute(
                "INSERT INTO tracks(title, artist, album, format, duration_ms, netease_id, content_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![metadata.title, metadata.artist, metadata.album, metadata.format, metadata.duration_ms, metadata.netease_id, content_hash],
            )?;
            (transaction.last_insert_rowid(), true)
        };
        transaction.execute(
            "INSERT INTO import_sources(source_hash, track_id) VALUES (?1, ?2) ON CONFLICT(source_hash) DO NOTHING",
            params![source_hash, track_id],
        )?;
        for attachment in attachments {
            if attachment.data.is_empty() {
                continue;
            }
            transaction.execute(
                "INSERT INTO attachments(track_id, kind, content_hash, mime_type, data) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(track_id, kind) DO NOTHING",
                params![track_id, attachment.kind.as_str(), hash(&attachment.data), attachment.mime_type, attachment.data],
            )?;
        }
        transaction.commit()?;
        Ok(added)
    }
}

fn track_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        title: row.get(1)?,
        artist: row.get(2)?,
        album: row.get(3)?,
        format: row.get(4)?,
        duration_ms: row.get(5)?,
        netease_id: row.get(6)?,
        content_hash: row.get(7)?,
    })
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
