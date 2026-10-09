// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    collections::HashSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use lofty::{
    config::ParseOptions,
    file::{AudioFile, FileType, TaggedFile, TaggedFileExt},
    mpeg::{Layer, MpegFile},
    probe::Probe,
    tag::{Accessor, ItemKey},
};
use serde_json::Value;
use walkdir::WalkDir;

use crate::{Attachment, AttachmentKind, Library, LibraryError};
use crate::{
    ncm::{Ncm, decode_metadata},
    storage::{Metadata, hash},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportIssue {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: Vec<ImportIssue>,
    pub attachment_issues: Vec<ImportIssue>,
    pub scan_issues: Vec<ImportIssue>,
    pub stopped: Option<String>,
    pub cancelled: bool,
}

/// The stage of an import, independent of keyboard or playback modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportPhase {
    Scanning,
    Importing,
    Finalizing,
    Completed,
    Cancelled,
    Stopped,
}

/// A file-level snapshot; totals remain unknown until discovery finishes.
///
/// Processed counts completed outcomes, including failed attempts, not bytes.
/// Scan and attachment issues do not count as additional processed audio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub discovered: usize,
    pub total: Option<usize>,
    pub processed: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub attachment_issues: usize,
    pub scan_issues: usize,
    pub current: Option<PathBuf>,
}

impl ImportProgress {
    fn publish(&mut self, report: &ImportReport, notify: &mut impl FnMut(Self)) {
        self.imported = report.imported;
        self.duplicates = report.duplicates;
        self.failed = report.failed.len();
        self.processed = self.imported + self.duplicates + self.failed;
        self.attachment_issues = report.attachment_issues.len();
        self.scan_issues = report.scan_issues.len();
        notify(self.clone());
    }
}

enum FileFailure {
    Source(String),
    Library(LibraryError),
}

#[derive(Clone, Copy)]
enum SourceSelection {
    All,
    Ncm,
}

impl SourceSelection {
    fn accepts(self, path: &Path) -> bool {
        match self {
            Self::All => supported(path),
            Self::Ncm => extension(path) == "ncm",
        }
    }

    fn unsupported_reason(self) -> &'static str {
        match self {
            Self::All => "only MP3, FLAC and NCM are supported",
            Self::Ncm => "only NCM source files can be imported",
        }
    }
}

impl From<LibraryError> for FileFailure {
    fn from(error: LibraryError) -> Self {
        Self::Library(error)
    }
}

impl Library {
    /// Recursively imports MP3, FLAC and NCM. The report preserves per-file failures.
    pub fn import(&mut self, path: impl AsRef<Path>) -> ImportReport {
        self.import_with_cancel(path, &AtomicBool::new(false))
    }

    /// Cancellation stops before the next file; a current song commits or rolls back.
    pub fn import_with_cancel(
        &mut self,
        path: impl AsRef<Path>,
        cancel: &AtomicBool,
    ) -> ImportReport {
        self.import_with_progress(path, cancel, |_| {})
    }

    /// Publishes scanning checkpoints and completed transaction outcomes.
    ///
    /// The callback runs synchronously on the caller's thread and should return
    /// promptly. It receives no audio bytes. Existing import methods use a no-op
    /// callback. Files discovered during scanning are attempted in that order;
    /// new files created afterwards belong to a later import.
    pub fn import_with_progress(
        &mut self,
        path: impl AsRef<Path>,
        cancel: &AtomicBool,
        notify: impl FnMut(ImportProgress),
    ) -> ImportReport {
        self.import_selected(path.as_ref(), cancel, SourceSelection::All, notify)
    }

    /// Imports only NCM sources, storing their unpacked MP3/FLAC audio.
    ///
    /// Folder discovery ignores ordinary MP3/FLAC before reading their audio and
    /// counts only NCM files, with case-insensitive extensions. A directly
    /// requested non-NCM file is reported as a source failure. Progress callbacks
    /// and cancellation follow the same contract as [`Self::import_with_progress`].
    pub fn import_ncm_with_progress(
        &mut self,
        path: impl AsRef<Path>,
        cancel: &AtomicBool,
        notify: impl FnMut(ImportProgress),
    ) -> ImportReport {
        self.import_selected(path.as_ref(), cancel, SourceSelection::Ncm, notify)
    }

    fn import_selected(
        &mut self,
        path: &Path,
        cancel: &AtomicBool,
        selection: SourceSelection,
        mut notify: impl FnMut(ImportProgress),
    ) -> ImportReport {
        let mut report = ImportReport::default();
        let mut progress = ImportProgress {
            phase: ImportPhase::Scanning,
            discovered: 0,
            total: None,
            processed: 0,
            imported: 0,
            duplicates: 0,
            failed: 0,
            attachment_issues: 0,
            scan_issues: 0,
            current: None,
        };
        progress.publish(&report, &mut notify);
        self.run_import(
            path,
            cancel,
            selection,
            &mut report,
            &mut progress,
            &mut notify,
        );
        progress.phase = if report.cancelled {
            ImportPhase::Cancelled
        } else if report.stopped.is_some() {
            ImportPhase::Stopped
        } else {
            ImportPhase::Completed
        };
        progress.current = None;
        progress.publish(&report, &mut notify);
        report
    }

    fn run_import(
        &mut self,
        path: &Path,
        cancel: &AtomicBool,
        selection: SourceSelection,
        report: &mut ImportReport,
        progress: &mut ImportProgress,
        notify: &mut impl FnMut(ImportProgress),
    ) {
        let mut associated = HashSet::new();
        let canonical = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) => {
                report.failed.push(ImportIssue {
                    path: path.into(),
                    reason: error.to_string(),
                });
                return;
            }
        };
        if canonical.is_file() {
            progress.phase = ImportPhase::Importing;
            progress.discovered = usize::from(selection.accepts(&canonical));
            progress.total = Some(1);
            progress.current = Some(canonical.clone());
            progress.publish(report, notify);
            if selection.accepts(&canonical) {
                let root = canonical.parent().unwrap_or(Path::new("."));
                self.process_file(&canonical, root, cancel, report, &mut associated);
            } else {
                report.failed.push(ImportIssue {
                    path: path.into(),
                    reason: selection.unsupported_reason().into(),
                });
            }
            progress.publish(report, notify);
            return;
        }
        if !canonical.is_dir() {
            report.failed.push(ImportIssue {
                path: path.into(),
                reason: "the import source is not a regular file or directory".into(),
            });
            return;
        }
        let mut discovered = Vec::new();
        let mut audio_files = Vec::new();
        for entry in WalkDir::new(&canonical)
            .follow_links(false)
            .sort_by_file_name()
        {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return;
            }
            match entry {
                Ok(entry) => {
                    progress.current = Some(entry.path().to_path_buf());
                    if entry.file_type().is_file() {
                        if selection.accepts(entry.path()) {
                            audio_files.push(entry.path().to_path_buf());
                        } else if sidecar_candidate(entry.path()) {
                            discovered.push(entry.path().to_path_buf());
                        }
                    }
                }
                Err(error) => {
                    progress.current = error.path().map(Path::to_path_buf);
                    report.scan_issues.push(ImportIssue {
                        path: error.path().unwrap_or(&canonical).into(),
                        reason: error.to_string(),
                    });
                }
            }
            progress.discovered = audio_files.len();
            progress.publish(report, notify);
        }
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return;
        }
        progress.phase = ImportPhase::Importing;
        progress.total = Some(audio_files.len());
        progress.current = None;
        progress.publish(report, notify);
        for path in audio_files {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            progress.current = Some(path.clone());
            progress.publish(report, notify);
            let keep_running =
                self.process_file(&path, &canonical, cancel, report, &mut associated);
            progress.publish(report, notify);
            if !keep_running {
                break;
            }
        }
        if !report.cancelled && report.stopped.is_none() {
            progress.phase = ImportPhase::Finalizing;
            progress.current = None;
            progress.publish(report, notify);
            for path in discovered
                .into_iter()
                .filter(|path| !associated.contains(path))
            {
                if cancel.load(Ordering::Relaxed) {
                    report.cancelled = true;
                    break;
                }
                progress.current = Some(path.clone());
                report.attachment_issues.push(ImportIssue {
                    path,
                    reason: "the attachment could not be associated with an imported track".into(),
                });
                progress.publish(report, notify);
            }
        }
    }

    fn process_file(
        &mut self,
        path: &Path,
        root: &Path,
        cancel: &AtomicBool,
        report: &mut ImportReport,
        associated: &mut HashSet<PathBuf>,
    ) -> bool {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return false;
        }
        match self.import_file(path, root, &mut report.attachment_issues, associated) {
            Ok(true) => report.imported += 1,
            Ok(false) => report.duplicates += 1,
            Err(FileFailure::Source(reason)) => report.failed.push(ImportIssue {
                path: path.into(),
                reason,
            }),
            Err(FileFailure::Library(error)) => {
                let reason = error.to_string();
                report.failed.push(ImportIssue {
                    path: path.into(),
                    reason: reason.clone(),
                });
                report.stopped = Some(reason);
                return false;
            }
        }
        true
    }

    fn import_file(
        &mut self,
        path: &Path,
        root: &Path,
        issues: &mut Vec<ImportIssue>,
        associated: &mut HashSet<PathBuf>,
    ) -> Result<bool, FileFailure> {
        // This same owned snapshot supplies both the hash and all decoding/tag reads.
        let input = fs::read(path).map_err(|error| FileFailure::Source(error.to_string()))?;
        let source_hash = hash(&input);
        let existing = self.source_track(&source_hash)?;
        let mut attachments = Vec::new();
        let (mut metadata, audio) = if extension(path) == "ncm" {
            let container = Ncm::parse(&input).map_err(FileFailure::Source)?;
            if let Some(reason) = &container.metadata_error {
                issues.push(ImportIssue {
                    path: path.into(),
                    reason: reason.clone(),
                });
            }
            add_cover(container.cover, path, &mut attachments, issues);
            let (mut metadata, audio) = if let Some(track) = &existing {
                (
                    Metadata {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        album: track.album.clone(),
                        format: track.format.clone(),
                        duration_ms: track.duration_ms,
                        netease_id: track.netease_id,
                    },
                    None,
                )
            } else {
                let decoded = container.decode_audio();
                let (metadata, embedded) =
                    read_audio(&decoded, path, issues).map_err(FileFailure::Source)?;
                attachments.extend(embedded);
                (metadata, Some(decoded))
            };
            if let Some(value) = &container.metadata {
                apply_netease_metadata(&mut metadata, value);
                if let Some(lyrics) = value
                    .get("lyric")
                    .or_else(|| value.get("lyrics"))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    attachments.insert(
                        0,
                        Attachment {
                            kind: AttachmentKind::Lyrics,
                            mime_type: "text/plain".into(),
                            data: lyrics.as_bytes().into(),
                        },
                    );
                }
            }
            (metadata, audio)
        } else {
            let (metadata, embedded) =
                read_audio(&input, path, issues).map_err(FileFailure::Source)?;
            attachments.extend(embedded);
            (
                metadata,
                if existing.is_some() {
                    None
                } else {
                    Some(input)
                },
            )
        };
        if metadata.title.trim().is_empty() {
            metadata.title = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        sidecars(
            path,
            root,
            metadata.netease_id,
            &mut attachments,
            issues,
            associated,
        );
        let content_hash = match &existing {
            Some(track) => track.content_hash.clone(),
            None => hash(audio.as_deref().ok_or(LibraryError::MissingAudio)?),
        };
        Ok(self.store(
            &source_hash,
            &content_hash,
            &metadata,
            audio.as_deref(),
            &attachments,
        )?)
    }
}

fn read_audio(
    bytes: &[u8],
    path: &Path,
    issues: &mut Vec<ImportIssue>,
) -> Result<(Metadata, Vec<Attachment>), String> {
    let probe = Probe::new(Cursor::new(bytes))
        .guess_file_type()
        .map_err(|error| error.to_string())?;
    let tagged: TaggedFile = if probe.file_type() == Some(FileType::Mpeg) {
        let file = MpegFile::read_from(&mut Cursor::new(bytes), ParseOptions::default())
            .map_err(|error| format!("cannot read valid audio: {error}"))?;
        if file.properties().layer() != &Layer::Layer3 {
            return Err("MPEG audio is not MP3 (Layer III)".into());
        }
        file.into()
    } else {
        probe
            .read()
            .map_err(|error| format!("cannot read valid audio: {error}"))?
    };
    let format = match tagged.file_type() {
        FileType::Mpeg => "mp3",
        FileType::Flac => "flac",
        _ => return Err("audio content is neither MP3 nor FLAC".into()),
    };
    if tagged.properties().sample_rate().unwrap_or(0) == 0 {
        return Err("audio has no valid sample rate".into());
    }
    let duration = tagged.properties().duration();
    let mut metadata = Metadata {
        format: format.into(),
        duration_ms: if duration.is_zero() {
            None
        } else {
            i64::try_from(duration.as_millis()).ok()
        },
        ..Metadata::default()
    };
    let mut attachments = Vec::new();
    for tag in tagged.tags() {
        if metadata.title.is_empty() {
            metadata.title = tag
                .title()
                .map(|text| text.into_owned())
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_default();
        }
        if metadata.artist.is_none() {
            metadata.artist = tag
                .artist()
                .map(|text| text.into_owned())
                .filter(|text| !text.trim().is_empty());
        }
        if metadata.album.is_none() {
            metadata.album = tag
                .album()
                .map(|text| text.into_owned())
                .filter(|text| !text.trim().is_empty());
        }
        if metadata.netease_id.is_none() {
            for item in tag.items() {
                if let Some(text) = item
                    .value()
                    .text()
                    .filter(|text| text.starts_with("163 key(Don't modify):"))
                    && let Ok(value) = decode_metadata(text.as_bytes())
                {
                    metadata.netease_id = value
                        .get("musicId")
                        .and_then(Value::as_i64)
                        .filter(|id| *id > 0);
                }
            }
        }
        for picture in tag.pictures() {
            add_cover(picture.data(), path, &mut attachments, issues);
        }
        for lyrics in tag
            .get_strings(ItemKey::Lyrics)
            .filter(|text| !text.is_empty())
        {
            attachments.push(Attachment {
                kind: AttachmentKind::Lyrics,
                mime_type: "text/plain".into(),
                data: lyrics.as_bytes().into(),
            });
        }
    }
    Ok((metadata, attachments))
}

fn apply_netease_metadata(metadata: &mut Metadata, value: &Value) {
    if let Some(title) = value
        .get("musicName")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        metadata.title = title.into();
    }
    if let Some(album) = value
        .get("album")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        metadata.album = Some(album.into());
    }
    if let Some(artists) = value.get("artist").and_then(Value::as_array) {
        let artists = artists
            .iter()
            .filter_map(|artist| artist.get(0).and_then(Value::as_str))
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if !artists.is_empty() {
            metadata.artist = Some(artists);
        }
    }
    if let Some(id) = value
        .get("musicId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
    {
        metadata.netease_id = Some(id);
    }
    if let Some(duration) = value
        .get("duration")
        .and_then(Value::as_i64)
        .filter(|duration| *duration > 0)
    {
        metadata.duration_ms = Some(duration);
    }
}

fn sidecars(
    path: &Path,
    root: &Path,
    netease_id: Option<i64>,
    attachments: &mut Vec<Attachment>,
    issues: &mut Vec<ImportIssue>,
    associated: &mut HashSet<PathBuf>,
) {
    let lyrics_path = path.with_extension("lrc");
    associated.insert(lyrics_path.clone());
    if let Some(data) = read_sidecar(&lyrics_path, root, issues) {
        if !data.is_empty() {
            attachments.push(Attachment {
                kind: AttachmentKind::Lyrics,
                mime_type: "text/plain".into(),
                data,
            });
        } else {
            issues.push(ImportIssue {
                path: lyrics_path,
                reason: "the lyrics file is empty".into(),
            });
        }
    }
    if let Some(id) = netease_id {
        let parent = path.parent().unwrap_or(root);
        let mut candidates = vec![parent.join("meta").join(format!("track-{id}.jpg"))];
        if parent != root {
            candidates.push(root.join("meta").join(format!("track-{id}.jpg")));
        }
        for candidate in candidates {
            associated.insert(candidate.clone());
            if let Some(data) = read_sidecar(&candidate, root, issues) {
                add_cover(&data, &candidate, attachments, issues);
            }
        }
    }
}

fn read_sidecar(path: &Path, root: &Path, issues: &mut Vec<ImportIssue>) -> Option<Vec<u8>> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            issues.push(ImportIssue {
                path: path.into(),
                reason: error.to_string(),
            });
            return None;
        }
    };
    if !canonical.starts_with(root) {
        issues.push(ImportIssue {
            path: path.into(),
            reason: "the attachment points outside the import scope".into(),
        });
        return None;
    }
    match fs::read(&canonical) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            issues.push(ImportIssue {
                path: path.into(),
                reason: error.to_string(),
            });
            None
        }
    }
}

fn add_cover(
    bytes: &[u8],
    path: &Path,
    attachments: &mut Vec<Attachment>,
    issues: &mut Vec<ImportIssue>,
) {
    if bytes.is_empty() {
        return;
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        issues.push(ImportIssue {
            path: path.into(),
            reason: "the cover is not a recognized PNG or JPEG image".into(),
        });
        return;
    };
    attachments.push(Attachment {
        kind: AttachmentKind::Cover,
        mime_type: mime.into(),
        data: bytes.into(),
    });
}

fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}

fn supported(path: &Path) -> bool {
    matches!(extension(path).as_str(), "mp3" | "flac" | "ncm")
}

fn sidecar_candidate(path: &Path) -> bool {
    extension(path) == "lrc"
        || (extension(path) == "jpg"
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "meta")
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.strip_prefix("track-"))
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        sync::mpsc::{self, Sender},
        thread,
        time::Duration,
    };

    thread_local! {
        static BUSY_NOTICE: RefCell<Option<Sender<()>>> = const { RefCell::new(None) };
    }

    fn notify_busy(attempt: i32) -> bool {
        if attempt == 0 {
            BUSY_NOTICE.with(|notice| {
                if let Some(sender) = notice.borrow_mut().take() {
                    let _ = sender.send(());
                }
            });
        }
        thread::sleep(Duration::from_millis(1));
        attempt < 2000
    }

    #[test]
    fn file_replacement_while_waiting_for_commit_cannot_change_saved_snapshot() {
        let temporary = tempfile::TempDir::new().unwrap();
        let database = temporary.path().join("library.muse");
        let source = temporary.path().join("song.mp3");
        let original = include_bytes!("../tests/fixtures/silence.mp3");
        fs::write(&source, original).unwrap();
        let mut library = Library::open(&database).unwrap();
        let lock = rusqlite::Connection::open(&database).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let (sender, receiver) = mpsc::channel();
        let worker_path = source.clone();
        let worker = thread::spawn(move || {
            BUSY_NOTICE.with(|notice| *notice.borrow_mut() = Some(sender));
            library.connection.busy_handler(Some(notify_busy)).unwrap();
            library.import(worker_path)
        });
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut replacement = original.to_vec();
        let mut tag = [0u8; 128];
        tag[..3].copy_from_slice(b"TAG");
        replacement.extend(tag);
        fs::write(&source, &replacement).unwrap();
        lock.execute_batch("ROLLBACK").unwrap();
        assert_eq!(worker.join().unwrap().imported, 1);
        let mut library = Library::open(&database).unwrap();
        let track = library.list().unwrap().remove(0);
        assert_eq!(library.open_audio(track.id).unwrap().into_inner(), original);
        let source_hash: String = library
            .connection
            .query_row("SELECT source_hash FROM import_sources", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(source_hash, hash(original));
        assert_ne!(source_hash, hash(&replacement));
        assert_eq!(library.import(&source).imported, 1);
    }
}
