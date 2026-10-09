-- SPDX-License-Identifier: AGPL-3.0-only
PRAGMA application_id = 1297437509;

CREATE TABLE audio (
    content_hash TEXT PRIMARY KEY NOT NULL,
    data BLOB NOT NULL CHECK (length(data) > 0)
);
CREATE TABLE tracks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    artist TEXT,
    album TEXT,
    format TEXT NOT NULL CHECK (format IN ('mp3', 'flac')),
    duration_ms INTEGER,
    netease_id INTEGER,
    content_hash TEXT NOT NULL UNIQUE REFERENCES audio(content_hash)
);
CREATE TABLE import_sources (
    source_hash TEXT PRIMARY KEY NOT NULL,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE
);
CREATE TABLE attachments (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('cover', 'lyrics')),
    content_hash TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    data BLOB NOT NULL CHECK (length(data) > 0),
    PRIMARY KEY (track_id, kind)
);
