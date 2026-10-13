-- SPDX-License-Identifier: AGPL-3.0-only
CREATE TABLE playlists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('all_local', 'user'))
);
CREATE UNIQUE INDEX playlists_one_all_local ON playlists(kind) WHERE kind = 'all_local';
CREATE TABLE playlist_items (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    PRIMARY KEY (playlist_id, position),
    UNIQUE (playlist_id, track_id)
);
INSERT INTO playlists(name, kind) VALUES ('全部本地', 'all_local');
PRAGMA user_version = 3;
