## 1. Native backend

- [x] 1.1 Add native dependencies, errors, owned MP3/FLAC decoding and actual time seeking.
- [x] 1.2 Implement NativePlayback, bounded PCM, six controls, persistent faults and cleanup.

## 2. Application and build integration

- [x] 2.1 Connect background library reads, confirmed track identity, status, play/pause, seeking, stop and quit.
- [x] 2.2 Update platform build dependencies, interface-only checks, README and design/progress records.

## 3. Verification

- [x] 3.1 Verify real MP3/FLAC PCM, pause, seeking, replay, boundaries, buffering, rejected replacements, persistent failures and cleanup.
- [x] 3.2 Verify terminal controls, stale requests, playback during import and shutdown; run workspace checks and strict OpenSpec validation, and record physical-device results separately.

## 4. Workspace layout and English messages

- [x] 4.1 Move the application and integration tests to crates/muse, keep a virtual root, and update dependencies, fixture paths and release version lookup.
- [x] 4.2 Use English help, labels, reports, application/library errors and README; preserve Unicode paths and metadata.
- [x] 4.3 Verify default application selection, English errors, relocated tests, packaging and release preview.
