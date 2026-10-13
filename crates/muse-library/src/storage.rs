// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub(crate) const APPLICATION_ID: i64 = 0x4d55_5345;
pub(crate) const SCHEMA_VERSION: i64 = 2;
const TRACK_COLUMNS: &str =
    "id, title, artist, album, format, duration_ms, netease_id, content_hash";

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("this SQLite file is not a muse library")]
    ForeignDatabase,
    #[error("library version {0} is newer than supported version {SCHEMA_VERSION}")]
    NewerSchema(i64),
    #[error(
        "library version 1 requires an offline upgrade: close clients, then run muse library upgrade"
    )]
    UpgradeRequired,
    #[error("unsupported library type: {0}")]
    UnsupportedType(String),
    #[error("library is in use or undergoing maintenance; close its clients and service first")]
    MaintenanceBusy,
    #[error("group {0} does not exist; list current groups again")]
    GroupNotFound(String),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryKind {
    Netease,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryDescriptor {
    pub name: String,
    pub kind: LibraryKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceObservation {
    pub source_hash: String,
    pub path: String,
    pub container: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub netease_id: Option<i64>,
    pub content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttachmentObservation {
    pub source_hash: Option<String>,
    pub kind: String,
    pub content_hash: String,
    pub mime_type: String,
    pub attribution: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SongGroup {
    pub token: String,
    pub candidates: Vec<Track>,
}

/// A connection to one typed library. Maintenance ownership lasts until drop.
pub struct Library {
    pub(crate) connection: Connection,
    pub(crate) _ownership: std::fs::File,
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LibraryError> {
        Self::connect(path.as_ref(), false)
    }

    /// Read operations never create a missing database or migrate its schema.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, LibraryError> {
        Self::connect(path.as_ref(), true)
    }

    fn connect(path: &Path, readonly: bool) -> Result<Self, LibraryError> {
        if readonly && !path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "library does not exist",
            )
            .into());
        }
        let ownership = crate::upgrade::ownership(path, false)?;
        let mut connection = if readonly {
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
        } else {
            Connection::open(path)?
        };
        connection.busy_timeout(Duration::from_secs(5))?;
        let application_id: i64 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if application_id != APPLICATION_ID
            && !(application_id == 0 && version == 0 && count == 0 && !readonly)
        {
            return Err(LibraryError::ForeignDatabase);
        }
        if version > SCHEMA_VERSION {
            return Err(LibraryError::NewerSchema(version));
        }
        if version == 1 {
            return Err(LibraryError::UpgradeRequired);
        }
        if version == 0 && !readonly {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Another creator may have completed while this connection waited.
            let current: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if current == 0 {
                tx.execute_batch(include_str!("schema.sql"))?;
                tx.execute_batch(include_str!("schema-v2.sql"))?;
                tx.execute(
                    "INSERT INTO library_descriptor VALUES(1,?1,'netease')",
                    [crate::upgrade::default_name(path)],
                )?;
            }
            tx.commit()?;
        }
        let library = Self {
            connection,
            _ownership: ownership,
        };
        library.descriptor()?; // Validate type before changing connection/database settings.
        library
            .connection
            .pragma_update(None, "foreign_keys", "ON")?;
        if !readonly {
            library
                .connection
                .pragma_update(None, "journal_mode", "WAL")?;
        }
        Ok(library)
    }

    pub fn descriptor(&self) -> Result<LibraryDescriptor, LibraryError> {
        let (name, kind): (String, String) = self.connection.query_row(
            "SELECT name,kind FROM library_descriptor WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let kind = match kind.as_str() {
            "netease" => LibraryKind::Netease,
            _ => return Err(LibraryError::UnsupportedType(kind)),
        };
        Ok(LibraryDescriptor { name, kind })
    }

    pub fn data_version(&self) -> Result<i64, LibraryError> {
        Ok(self
            .connection
            .pragma_query_value(None, "data_version", |r| r.get(0))?)
    }

    pub fn observations(&self, id: i64) -> Result<Vec<SourceObservation>, LibraryError> {
        self.track(id)?;
        let mut stmt = self.connection.prepare("SELECT o.source_hash,path,container,title,artist,album,netease_id,content_hash FROM source_observations o JOIN import_sources s USING(source_hash) WHERE s.track_id=?1 ORDER BY path")?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(SourceObservation {
                    source_hash: r.get(0)?,
                    path: r.get(1)?,
                    container: r.get(2)?,
                    title: r.get(3)?,
                    artist: r.get(4)?,
                    album: r.get(5)?,
                    netease_id: r.get(6)?,
                    content_hash: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    pub fn attachment_observations(
        &self,
        id: i64,
    ) -> Result<Vec<AttachmentObservation>, LibraryError> {
        self.track(id)?;
        let mut stmt=self.connection.prepare("SELECT source_hash,kind,content_hash,mime_type,attribution FROM attachment_observations WHERE track_id=?1 ORDER BY kind,source_hash,content_hash")?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(AttachmentObservation {
                    source_hash: r.get(0)?,
                    kind: r.get(1)?,
                    content_hash: r.get(2)?,
                    mime_type: r.get(3)?,
                    attribution: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?)
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

    pub fn track(&self, id: i64) -> Result<Track, LibraryError> {
        self.connection
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                [id],
                track_from_row,
            )
            .optional()?
            .ok_or(LibraryError::TrackNotFound(id))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn store(
        &mut self,
        source_hash: &str,
        content_hash: &str,
        metadata: &Metadata,
        audio: Option<&[u8]>,
        attachments: &[Attachment],
        source: (&Path, &str),
        identity: Option<(&str, &str)>,
    ) -> Result<(bool, bool), LibraryError> {
        let (path, observed_title) = source;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let verified: Option<i64> = tx
            .query_row(
                "SELECT track_id FROM import_sources WHERE source_hash=?1 AND verified=1",
                [source_hash],
                |r| r.get(0),
            )
            .optional()?;
        let same: Vec<(i64, Option<i64>)> = {
            let mut stmt =
                tx.prepare("SELECT id,netease_id FROM tracks WHERE content_hash=?1 ORDER BY id")?;
            stmt.query_map([content_hash], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let compatible = if let Some(platform) = metadata.netease_id {
            same.iter()
                .find(|(_, p)| *p == Some(platform))
                .map(|(id, _)| *id)
                .or_else(|| {
                    if same.iter().all(|(_, p)| p.is_none()) && same.len() == 1 {
                        Some(same[0].0)
                    } else {
                        None
                    }
                })
        } else if same.len() == 1 {
            Some(same[0].0)
        } else {
            same.iter().find(|(_, p)| p.is_none()).map(|(id, _)| *id)
        };
        let existing = verified.or(compatible);
        let mut collision = false;
        if existing.is_none() {
            let mut stmt =
                tx.prepare("SELECT netease_id,tagged_title,tagged_artist FROM tracks")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?;
            for row in rows {
                let (id, title, artist) = row?;
                if (metadata.netease_id.is_some() && id == metadata.netease_id)
                    || identity.is_some_and(|key| {
                        song_identity(title.as_deref().unwrap_or(""), artist.as_deref())
                            .is_some_and(|k| k.0 == key.0 && k.1 == key.1)
                    })
                {
                    collision = true;
                    break;
                }
            }
        }
        let id = if let Some(id) = existing {
            if verified.is_none() && metadata.netease_id.is_some() {
                tx.execute(
                    "UPDATE tracks SET netease_id=?1 WHERE id=?2 AND netease_id IS NULL",
                    params![metadata.netease_id, id],
                )?;
            }
            id
        } else {
            tx.execute(
                "INSERT INTO audio(content_hash,data) VALUES(?1,?2) ON CONFLICT DO NOTHING",
                params![content_hash, audio.ok_or(LibraryError::MissingAudio)?],
            )?;
            tx.execute("INSERT INTO tracks(title,artist,album,format,duration_ms,netease_id,content_hash,tagged_title,tagged_artist) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![metadata.title,metadata.artist,metadata.album,metadata.format,metadata.duration_ms,metadata.netease_id,content_hash,identity.map(|k|k.0),identity.map(|k|k.1)])?;
            tx.last_insert_rowid()
        };
        // Revalidation repairs old mappings only after complete decoding succeeded.
        tx.execute("INSERT INTO import_sources(source_hash,track_id,verified) VALUES(?1,?2,1) ON CONFLICT(source_hash) DO UPDATE SET track_id=excluded.track_id,verified=1",params![source_hash,id])?;
        tx.execute("INSERT INTO source_observations(source_hash,path,container,title,artist,album,netease_id,content_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT DO NOTHING",params![source_hash,path.to_string_lossy(),path.extension().unwrap_or_default().to_string_lossy().to_lowercase(),observed_title,metadata.artist,metadata.album,metadata.netease_id,content_hash])?;
        // Legacy representatives gain only verified real-tag grouping evidence.
        tx.execute("UPDATE tracks SET tagged_title=COALESCE(tagged_title,?1),tagged_artist=COALESCE(tagged_artist,?2) WHERE id=?3",params![identity.map(|k|k.0),identity.map(|k|k.1),id])?;
        for attachment in attachments {
            if attachment.data.is_empty() {
                continue;
            }
            let digest = hash(&attachment.data);
            tx.execute("INSERT INTO attachments(track_id,kind,content_hash,mime_type,data) VALUES(?1,?2,?3,?4,?5) ON CONFLICT DO NOTHING",params![id,attachment.kind.as_str(),digest,attachment.mime_type,attachment.data])?;
            tx.execute("INSERT INTO attachment_observations(track_id,source_hash,kind,content_hash,mime_type,data,attribution) VALUES(?1,?2,?3,?4,?5,?6,'source') ON CONFLICT DO NOTHING",params![id,source_hash,attachment.kind.as_str(),digest,attachment.mime_type,attachment.data])?;
        }
        tx.commit()?;
        Ok((existing.is_none(), collision))
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

pub(crate) fn song_identity(title: &str, artist: Option<&str>) -> Option<(String, String)> {
    let title = title.trim();
    let artist = artist?.trim();
    if title.is_empty() || artist.is_empty() {
        None
    } else {
        Some((title.to_lowercase(), artist.to_lowercase()))
    }
}

#[cfg(test)]
mod candidate_tests {
    use super::*;
    fn add(
        library: &mut Library,
        source: &str,
        bytes: &[u8],
        platform: Option<i64>,
        title: &str,
    ) -> (bool, bool) {
        library
            .store(
                source,
                &hash(bytes),
                &Metadata {
                    title: title.into(),
                    artist: Some("artist".into()),
                    format: "mp3".into(),
                    netease_id: platform,
                    ..Metadata::default()
                },
                Some(bytes),
                &[],
                (Path::new(source), title),
                Some((title, "artist")),
            )
            .unwrap()
    }
    #[test]
    fn identities_share_bytes_without_merging_and_plain_audio_cannot_bridge_them() {
        for order in [[None, Some(1), Some(2)], [Some(2), Some(1), None]] {
            let dir = tempfile::tempdir().unwrap();
            let mut lib = Library::open(dir.path().join("typed.muse")).unwrap();
            for (i, id) in order.into_iter().enumerate() {
                add(&mut lib, &format!("{i}.ncm"), b"same audio", id, "song");
            }
            let tracks = lib.list().unwrap();
            assert!(tracks.iter().any(|t| t.netease_id == Some(1)));
            assert!(tracks.iter().any(|t| t.netease_id == Some(2)));
            assert_eq!(
                lib.connection
                    .query_row("SELECT count(*) FROM audio", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(lib.groups().unwrap().len(), 1);
            assert!(!add(&mut lib, "0.ncm", b"same audio", order[0], "song").0);
            assert_eq!(lib.list().unwrap(), tracks);
        }
    }
    #[test]
    fn platform_group_uses_oldest_tags_and_removed_anchor_never_reappears() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("typed.muse")).unwrap();
        add(&mut lib, "a", b"one", Some(1), "first");
        assert_eq!(
            add(&mut lib, "b", b"two", Some(1), "alternate"),
            (true, true)
        );
        add(&mut lib, "c", b"three", Some(2), "alternate");
        let groups = lib.groups().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].candidates.len(), 2);
        let token = groups[0].token.clone();
        lib.remove(groups[0].candidates[0].id).unwrap();
        assert!(lib.group(&token).is_err());
        for t in lib.list().unwrap() {
            lib.remove(t.id).unwrap();
        }
        add(&mut lib, "d", b"four", None, "new");
        assert!(lib.list().unwrap()[0].id > 3);
    }
    #[test]
    fn descriptor_survives_rename_and_unknown_type_is_rejected_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("我的收藏.muse");
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.descriptor().unwrap().name, "我的收藏");
        drop(lib);
        let renamed = dir.path().join("other.muse");
        std::fs::rename(&path, &renamed).unwrap();
        let lib = Library::open(&renamed).unwrap();
        assert_eq!(lib.descriptor().unwrap().name, "我的收藏");
        lib.connection
            .execute("UPDATE library_descriptor SET kind='future'", [])
            .unwrap();
        drop(lib);
        assert!(
            matches!(Library::open(&renamed),Err(LibraryError::UnsupportedType(t)) if t=="future")
        );
    }
}
