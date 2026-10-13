// SPDX-License-Identifier: AGPL-3.0-only
use crate::service;
use muse_library::{Library, PlaylistKind, SCHEMA_VERSION, SongGroup, Track};
use serde_json::json;
use std::{ffi::OsString, path::Path, sync::atomic::AtomicBool};

pub const HELP: &str = "list [--search text] [--candidates] [--json]\nshow <candidate-id|g:anchor> [--json]\nimport <file-or-folder>\nplaylist list | playlist create <name> | playlist add <playlist> <track-id> | playlist play <playlist>\nplay [candidate-id|g:anchor] | next | mode [sequential|list|single] | pause | stop | status | shutdown\nlibrary info | library upgrade\nhelp | quit (q) | detach\nPlayback stays inside the current playlist. The default playlist is 全部本地.\nCandidate IDs are stable; g: tokens identify current browsing groups.\nMultiple candidates require selection. Quit stops playback; detach keeps it.";

#[derive(Default)]
pub struct CommandOutput {
    pub text: String,
    pub diagnostics: String,
    pub failure: Option<String>,
}
impl From<String> for CommandOutput {
    fn from(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
}
impl CommandOutput {
    pub fn print(self) -> Result<(), String> {
        if !self.text.is_empty() {
            println!("{}", self.text);
        }
        if !self.diagnostics.is_empty() {
            eprintln!("{}", self.diagnostics);
        }
        self.failure.map_or(Ok(()), Err)
    }
}

pub fn matches(track: &Track, query: &str) -> bool {
    let query = query.to_lowercase();
    [
        Some(track.title.as_str()),
        track.artist.as_deref(),
        track.album.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|s| s.to_lowercase().contains(&query))
}

pub fn describe(track: &Track) -> String {
    format!(
        "{}  {} — {} | {} | {} | {} ms | NetEase {}",
        track.id,
        track.title,
        track.artist.as_deref().unwrap_or("Unknown"),
        track.format,
        track.album.as_deref().unwrap_or("Unknown album"),
        track
            .duration_ms
            .map_or("unknown".into(), |n| n.to_string()),
        track.netease_id.map_or("unknown".into(), |n| n.to_string())
    )
}

pub fn group(path: &Path, token: &str) -> Result<SongGroup, String> {
    Library::open_existing(path)
        .and_then(|l| l.group(token))
        .map_err(|e| e.to_string())
}

pub fn execute(path: &Path, args: &[OsString]) -> Result<CommandOutput, String> {
    let name = args
        .first()
        .and_then(|a| a.to_str())
        .ok_or("missing command")?;
    let rest = &args[1..];
    match name {
        "help" if rest.is_empty() => Ok(HELP.to_string().into()),
        "import" if rest.len() == 1 => {
            let mut library = Library::open(path).map_err(|e| e.to_string())?;
            let report = library.import_downloads_with_progress(
                Path::new(&rest[0]),
                &AtomicBool::new(false),
                |_| {},
            );
            Ok(report_import(report))
        }
        "play" if rest.len() <= 1 => {
            let command = if let Some(target) = rest.first() {
                let target = target.to_str().ok_or("invalid play target")?;
                let id = if target.starts_with("g:") {
                    let group = group(path, target)?;
                    if group.candidates.len() != 1 {
                        return Err(format!(
                            "{target} has multiple candidates; use play <candidate-id>:\n{}",
                            group
                                .candidates
                                .iter()
                                .map(describe)
                                .collect::<Vec<_>>()
                                .join("\n")
                        ));
                    }
                    group.candidates[0].id
                } else {
                    target
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or("play requires a positive candidate ID or g: token")?
                };
                service::Command::Load(id)
            } else {
                service::Command::Play
            };
            service::control_text(path, command).map(Into::into)
        }
        "pause" | "stop" | "status" | "shutdown" if rest.is_empty() => service::control_text(
            path,
            match name {
                "pause" => service::Command::Pause,
                "stop" => service::Command::Stop,
                "shutdown" => service::Command::Shutdown,
                _ => service::Command::Status,
            },
        )
        .map(Into::into),
        "library" if rest == [OsString::from("upgrade")] => {
            if service::Remote::find(path)?.is_some() {
                return Err("stop the playback service with shutdown and close clients before library upgrade".into());
            }
            Ok(match Library::upgrade(path).map_err(|e| e.to_string())? {
                Some(backup) => format!(
                    "Upgraded to version {SCHEMA_VERSION}. Pre-upgrade backup: {}",
                    backup.display()
                ),
                None => format!("Library is already version {SCHEMA_VERSION}"),
            }
            .into())
        }
        "library" if rest == [OsString::from("info")] => {
            let lib = Library::open_existing(path).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string_pretty(&json!({"descriptor":lib.descriptor().map_err(|e|e.to_string())?,"schema_version":lib.schema_version().map_err(|e|e.to_string())?})).map_err(|e|e.to_string())?.into())
        }
        "playlist" => playlist_command(path, rest),
        "list" | "show" => query(path, name, rest),
        _ => Err(format!("invalid command or arguments: {name}; use help")),
    }
}

fn playlist_command(path: &Path, args: &[OsString]) -> Result<CommandOutput, String> {
    let mut args = args.iter();
    let sub = args
        .next()
        .and_then(|arg| arg.to_str())
        .ok_or("playlist requires list, create, add, or play")?;
    match sub {
        "list" if args.next().is_none() => {
            let library = Library::open_existing(path).map_err(|error| error.to_string())?;
            let lines = library
                .playlists()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|playlist| {
                    let kind = match playlist.kind {
                        PlaylistKind::AllLocal => "all-local",
                        PlaylistKind::User => "user",
                    };
                    format!(
                        "{}\t{}\t{}\t{} tracks",
                        playlist.id, playlist.name, kind, playlist.len
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(lines.into())
        }
        "create" => {
            let name = args.map(|arg| arg.to_string_lossy()).collect::<Vec<_>>();
            if name.is_empty() {
                return Err("playlist create requires a name".into());
            }
            let mut library = Library::open(path).map_err(|error| error.to_string())?;
            let playlist = library
                .create_playlist(&name.join(" "))
                .map_err(|error| error.to_string())?;
            Ok(format!("Created playlist {} ({})", playlist.name, playlist.id).into())
        }
        "add" => {
            let playlist = args
                .next()
                .and_then(|arg| arg.to_str())
                .ok_or("playlist add requires a playlist and a track ID")?;
            let track = args
                .next()
                .and_then(|arg| arg.to_str())
                .ok_or("playlist add requires a track ID")?;
            if args.next().is_some() {
                return Err("playlist add accepts a playlist and a track ID".into());
            }
            let track_id = track
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or("playlist add requires a positive track ID")?;
            let mut library = Library::open(path).map_err(|error| error.to_string())?;
            let playlist_id = resolve_playlist(&library, playlist)?;
            let added = library
                .add_to_playlist(playlist_id, track_id)
                .map_err(|error| error.to_string())?;
            Ok(if added {
                format!("Added track {track_id} to playlist {playlist_id}")
            } else {
                format!("Track {track_id} is already in playlist {playlist_id}")
            }
            .into())
        }
        "play" => {
            let playlist = args
                .next()
                .and_then(|arg| arg.to_str())
                .ok_or("playlist play requires a playlist")?;
            if args.next().is_some() {
                return Err("playlist play accepts one playlist".into());
            }
            let playlist_id = {
                let library = Library::open_existing(path).map_err(|error| error.to_string())?;
                resolve_playlist(&library, playlist)?
            };
            service::control_text(path, service::Command::PlayPlaylist(playlist_id)).map(Into::into)
        }
        _ => Err("playlist requires list, create, add, or play".into()),
    }
}

pub fn resolve_playlist(library: &Library, token: &str) -> Result<i64, String> {
    if let Ok(id) = token.parse::<i64>() {
        return library
            .playlist(id)
            .map(|playlist| playlist.id)
            .map_err(|error| error.to_string());
    }
    library
        .playlists()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|playlist| playlist.name == token)
        .map(|playlist| playlist.id)
        .ok_or_else(|| format!("playlist {token} does not exist"))
}

fn query(path: &Path, name: &str, args: &[OsString]) -> Result<CommandOutput, String> {
    let mut json_output = false;
    let mut candidates = false;
    let mut search = None;
    let mut target = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--json") if !json_output => json_output = true,
            Some("--candidates") if name == "list" && !candidates => candidates = true,
            Some("--search") if name == "list" && search.is_none() => {
                search = Some(
                    args.next()
                        .and_then(|s| s.to_str())
                        .ok_or("--search requires text")?,
                )
            }
            Some(value) if name == "show" && !value.starts_with('-') && target.is_none() => {
                target = Some(value)
            }
            _ => return Err("invalid query argument; use help".into()),
        }
    }
    let lib = Library::open_existing(path).map_err(|e| e.to_string())?;
    let mut output = Vec::new();
    if name == "show" {
        let target = target.ok_or("show requires a candidate ID or g: token")?;
        let tracks = if target.starts_with("g:") {
            lib.group(target).map_err(|e| e.to_string())?.candidates
        } else {
            vec![
                lib.track(target.parse().map_err(|_| "invalid candidate ID")?)
                    .map_err(|e| e.to_string())?,
            ]
        };
        let details=tracks.iter().map(|t| Ok(json!({"candidate":t,"sources":lib.observations(t.id).map_err(|e|e.to_string())?,"attachments":lib.attachment_observations(t.id).map_err(|e|e.to_string())?}))).collect::<Result<Vec<_>,String>>()?;
        if json_output {
            output.push(json!({"target":target,"candidates":details}).to_string());
        } else {
            for (track, detail) in tracks.iter().zip(details) {
                output.push(format!(
                    "{}\n  Sources: {}\n  Attachments: {}",
                    describe(track),
                    detail["sources"],
                    detail["attachments"]
                ));
            }
        }
    } else {
        let mut groups = lib.groups().map_err(|e| e.to_string())?;
        if let Some(search) = search {
            groups.retain(|g| g.candidates.iter().any(|t| matches(t, search)));
        }
        if candidates {
            let tracks: Vec<_> = groups.iter().flat_map(|g| &g.candidates).collect();
            if json_output {
                output.push(json!(tracks).to_string());
            } else {
                for t in tracks {
                    output.push(describe(t));
                }
            }
        } else if json_output {
            output.push(json!(groups).to_string());
        } else {
            for g in groups {
                output.push(format!(
                    "{}  {} — {} [{} candidates]",
                    g.token,
                    g.candidates[0].title,
                    g.candidates[0].artist.as_deref().unwrap_or("Unknown"),
                    g.candidates.len()
                ));
            }
        }
    }
    Ok(output.join("\n").into())
}

pub fn report_import(report: muse_library::ImportReport) -> CommandOutput {
    let mut diagnostics = format!(
        "Import: added {}, retained collisions {}, exact duplicates {}, failed {}",
        report.imported,
        report.collisions,
        report.duplicates,
        report.failed.len()
    );
    for issue in report
        .failed
        .iter()
        .chain(&report.attachment_issues)
        .chain(&report.scan_issues)
    {
        diagnostics.push_str(&format!("\n{}: {}", issue.path.display(), issue.reason));
    }
    let failure = report.stopped.or_else(|| {
        if report.failed.is_empty() {
            None
        } else {
            Some("some sources could not be imported".into())
        }
    });
    CommandOutput {
        text: String::new(),
        diagnostics,
        failure,
    }
}

pub fn candidate_labels(library: &Library, group: &SongGroup) -> Result<Vec<String>, String> {
    group
        .candidates
        .iter()
        .map(|track| {
            let sources = library.observations(track.id).map_err(|e| e.to_string())?;
            let labels = sources
                .iter()
                .map(|s| format!("{} source: {}", s.container.to_uppercase(), s.path))
                .collect::<Vec<_>>()
                .join("; ");
            Ok(format!(
                "{}\n   {}",
                describe(track),
                if labels.is_empty() {
                    "Source unknown (legacy)"
                } else {
                    &labels
                }
            ))
        })
        .collect()
}
