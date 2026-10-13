-- SPDX-License-Identifier: AGPL-3.0-only
CREATE TABLE library_descriptor (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    name TEXT NOT NULL,
    kind TEXT NOT NULL
);
CREATE TABLE tracks_v2 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL, artist TEXT, album TEXT,
    format TEXT NOT NULL CHECK(format IN ('mp3','flac')),
    duration_ms INTEGER, netease_id INTEGER,
    content_hash TEXT NOT NULL REFERENCES audio(content_hash),
    tagged_title TEXT, tagged_artist TEXT
);
INSERT INTO tracks_v2(id,title,artist,album,format,duration_ms,netease_id,content_hash)
SELECT id,title,artist,album,format,duration_ms,netease_id,content_hash FROM tracks;
DROP TABLE tracks;
ALTER TABLE tracks_v2 RENAME TO tracks;
CREATE INDEX candidate_content ON tracks(content_hash, netease_id);
CREATE INDEX candidate_platform ON tracks(netease_id);
ALTER TABLE import_sources ADD COLUMN verified INTEGER NOT NULL DEFAULT 0;
CREATE TABLE source_observations (
    source_hash TEXT NOT NULL REFERENCES import_sources(source_hash) ON DELETE CASCADE,
    path TEXT NOT NULL, container TEXT NOT NULL,
    title TEXT NOT NULL, artist TEXT, album TEXT, netease_id INTEGER,
    content_hash TEXT NOT NULL REFERENCES audio(content_hash),
    PRIMARY KEY(source_hash,path)
);
CREATE TABLE attachment_observations (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    source_hash TEXT,
    kind TEXT NOT NULL, content_hash TEXT NOT NULL, mime_type TEXT NOT NULL,
    data BLOB NOT NULL CHECK(length(data)>0),
    attribution TEXT NOT NULL,
    UNIQUE(track_id,source_hash,kind,content_hash)
);
INSERT INTO attachment_observations
SELECT track_id,NULL,kind,content_hash,mime_type,data,'legacy' FROM attachments;
PRAGMA user_version = 2;
