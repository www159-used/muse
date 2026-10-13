// SPDX-License-Identifier: AGPL-3.0-only

use crate::support::{ImportWorld, Result};
use cucumber::{gherkin::Step, then};
use muse_library::{AttachmentKind, Track};
use std::path::Path;

#[then(expr = "{int} track is added")]
#[then(expr = "{int} tracks are added")]
fn added_tracks(world: &mut ImportWorld, expected: usize) -> Result<()> {
    assert_eq!(world.report()?.imported, expected, "added track count");
    Ok(())
}

#[then(expr = "{int} duplicate is skipped")]
#[then(expr = "{int} duplicates are skipped")]
fn skipped_duplicates(world: &mut ImportWorld, expected: usize) -> Result<()> {
    assert_eq!(
        world.report()?.duplicates,
        expected,
        "skipped duplicate count"
    );
    Ok(())
}

#[then("the import finishes without errors")]
fn clean_import(world: &mut ImportWorld) -> Result<()> {
    let report = world.report()?;
    assert!(
        report.failed.is_empty(),
        "failed sources: {:?}",
        report.failed
    );
    assert!(
        report.scan_issues.is_empty(),
        "scan issues: {:?}",
        report.scan_issues
    );
    assert!(
        report.attachment_issues.is_empty(),
        "attachment issues: {:?}",
        report.attachment_issues
    );
    assert!(
        report.stopped.is_none(),
        "import stopped: {:?}",
        report.stopped
    );
    assert!(!report.cancelled, "import was cancelled");
    Ok(())
}

#[then(expr = "the library contains only {string} with audio {string}")]
fn owned_track(world: &mut ImportWorld, title: String, audio: String) -> Result<()> {
    let track = sole_track(world)?;
    assert_eq!(track.title, title, "track title");
    assert_eq!(track.format, audio_format(&audio)?, "stored audio format");
    assert_eq!(
        world.library.open_audio(track.id)?.into_inner(),
        ImportWorld::fixture(&audio)?,
        "stored audio bytes"
    );
    Ok(())
}

#[then("the library contains exactly these tracks:")]
fn owned_tracks(world: &mut ImportWorld, step: &Step) -> Result<()> {
    let table = step.table.as_ref().ok_or("expected a track table")?;
    let (headers, rows) = table.rows.split_first().ok_or("track table is empty")?;
    assert!(
        headers.len() == 2 && headers[0] == "title" && headers[1] == "audio",
        "expected title/audio table headers, got {headers:?}"
    );
    assert!(!rows.is_empty(), "expected at least one track row");
    let mut remaining = world
        .library
        .list()?
        .into_iter()
        .map(|track| {
            let bytes = world.library.open_audio(track.id)?.into_inner();
            Ok((track, bytes))
        })
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(remaining.len(), rows.len(), "library track count");
    for row in rows {
        let [title, audio] = row.as_slice() else {
            return Err("each track row needs a title and an audio fixture".into());
        };
        let expected_audio = ImportWorld::fixture(audio)?;
        let format = audio_format(audio)?;
        let index = remaining
            .iter()
            .position(|(track, bytes)| {
                track.title == *title && track.format == format && bytes == &expected_audio
            })
            .ok_or_else(|| {
                format!("no track matches title {title:?} and stored audio bytes from {audio:?}")
            })?;
        remaining.remove(index);
    }
    Ok(())
}

#[then(expr = "the imported track has artist {string}, album {string} and NetEase ID {int}")]
fn track_metadata(
    world: &mut ImportWorld,
    artist: String,
    album: String,
    netease_id: i64,
) -> Result<()> {
    let track = sole_track(world)?;
    assert_eq!(
        track.artist.as_deref(),
        Some(artist.as_str()),
        "track artist"
    );
    assert_eq!(track.album.as_deref(), Some(album.as_str()), "track album");
    assert_eq!(track.netease_id, Some(netease_id), "NetEase ID");
    Ok(())
}

#[then(expr = "the imported track has a PNG cover matching {string}")]
fn track_cover(world: &mut ImportWorld, fixture: String) -> Result<()> {
    let track = sole_track(world)?;
    let covers: Vec<_> = world
        .library
        .attachments(track.id)?
        .into_iter()
        .filter(|attachment| attachment.kind == AttachmentKind::Cover)
        .collect();
    assert_eq!(covers.len(), 1, "cover count");
    assert_eq!(covers[0].mime_type, "image/png", "cover MIME type");
    assert_eq!(
        covers[0].data,
        ImportWorld::fixture(&fixture)?,
        "cover bytes"
    );
    Ok(())
}

#[then("the imported track has these lyrics:")]
fn track_lyrics(world: &mut ImportWorld, step: &Step) -> Result<()> {
    let expected = step
        .docstring
        .as_deref()
        .ok_or("expected a lyrics docstring")?;
    let track = sole_track(world)?;
    let lyrics: Vec<_> = world
        .library
        .attachments(track.id)?
        .into_iter()
        .filter(|attachment| attachment.kind == AttachmentKind::Lyrics)
        .collect();
    assert_eq!(lyrics.len(), 1, "lyrics count");
    assert_eq!(lyrics[0].mime_type, "text/plain", "lyrics MIME type");
    assert_eq!(lyrics[0].data, expected.as_bytes(), "lyrics bytes");
    Ok(())
}

fn sole_track(world: &ImportWorld) -> Result<Track> {
    let mut tracks = world.library.list()?;
    assert_eq!(tracks.len(), 1, "library track count");
    Ok(tracks.remove(0))
}

fn audio_format(audio: &str) -> Result<&str> {
    Path::new(audio)
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| "audio fixture needs a format extension".into())
}

#[then(expr = "{int} collision is retained")]
#[then(expr = "{int} collisions are retained")]
fn retained_collisions(world: &mut ImportWorld, expected: usize) -> Result<()> {
    assert_eq!(world.report()?.collisions, expected);
    Ok(())
}
#[then(expr = "the library has {int} browsing group")]
fn browsing_groups(world: &mut ImportWorld, expected: usize) -> Result<()> {
    assert_eq!(world.library.groups()?.len(), expected);
    Ok(())
}
