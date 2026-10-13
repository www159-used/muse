// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    error::Error,
    io::{Stdout, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, mpsc::TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crate::{
    command::{BuiltinCommand, CommandLine, CursorMove, expand_path},
    playback::View as PlaybackView,
    search::{Direction, Search},
    service::Client as PlaybackWorker,
};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEvent, MouseEventKind,
};
use muse_library::{
    AttachmentKind, ImportPhase, ImportProgress, ImportReport, Library, LibraryError, SongGroup,
    Track,
};
use muse_playback::{PlaybackState, SeekTarget};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{
        Block, Borders, Clear, Gauge, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap,
    },
};

// Cursor restoration in ratatui's Drop writes an error to stderr on failure.
// Once all three standard streams lose their terminal, that reporting can panic.
struct TerminalOutput(Stdout);

impl Write for TerminalOutput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self.0.write(buffer) {
            Err(_) if terminal_disconnected().unwrap_or(false) => Ok(buffer.len()),
            result => result,
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.flush() {
            Err(_) if terminal_disconnected().unwrap_or(false) => Ok(()),
            result => result,
        }
    }
}

type UiTerminal = ratatui::Terminal<ratatui::backend::CrosstermBackend<TerminalOutput>>;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = crossterm::execute!(TerminalOutput(std::io::stdout()), DisableMouseCapture);
        let _ = ratatui::try_restore();
    }
}

use crate::import_job::ImportWorker;

struct SearchPrompt {
    input: CommandLine,
    direction: Direction,
    origin: ListState,
    origin_track: Option<i64>,
    details_focused: bool,
    preview: Option<Search>,
}

struct Chooser {
    group: SongGroup,
    labels: Vec<String>,
    selection: ListState,
}

#[derive(Clone)]
enum Browser {
    Library,
    Playlist { id: i64, name: String },
}

struct LyricLine {
    at: Option<Duration>,
    text: String,
}

struct App {
    library: Library,
    library_path: PathBuf,
    tracks: Vec<Track>,
    groups: Vec<SongGroup>,
    library_name: String,
    chooser: Option<Chooser>,
    refresh: Option<crate::refresh::Reader>,
    selection: ListState,
    command: Option<CommandLine>,
    search_prompt: Option<SearchPrompt>,
    search: Option<Search>,
    message: String,
    details: Vec<String>,
    details_focused: bool,
    details_scroll: (u16, u16),
    songs_area: Rect,
    results_area: Rect,
    browser: Browser,
    lyrics_open: bool,
    lyrics_track: Option<i64>,
    lyrics_lines: Vec<LyricLine>,
    lyrics_offset: usize,
    lyrics_following: bool,
    lyrics_paused_at: Option<Instant>,
    lyrics_rows: usize,
    lyrics_area: Rect,
    navigation_count: Option<usize>,
    pending_g: bool,
    worker: Option<ImportWorker>,
    playback: Option<PlaybackWorker>,
    playback_view: PlaybackView,
    quitting: bool,
    exit_error: Option<String>,
}

impl App {
    fn new(path: PathBuf) -> Result<Self, LibraryError> {
        let library = Library::open(&path)?;
        let groups = library.groups()?;
        let all_local = library.all_local()?;
        let tracks = library.playlist_tracks(all_local.id)?;
        let library_name = library.descriptor()?.name;
        let mut selection = ListState::default();
        if !tracks.is_empty() {
            selection.select(Some(0));
        }
        Ok(Self {
            library,
            library_path: path.clone(),
            tracks,
            groups,
            library_name,
            chooser: None,
            refresh: None,
            selection,
            command: None,
            search_prompt: None,
            search: None,
            message: "全部本地 is the current playlist. Enter plays inside it; r cycles mode; l opens lyrics. :import adds music.".into(),
            details: Vec::new(),
            details_focused: false,
            details_scroll: (0, 0),
            songs_area: Rect::default(),
            results_area: Rect::default(),
            browser: Browser::Playlist {
                id: all_local.id,
                name: all_local.name,
            },
            lyrics_open: false,
            lyrics_track: None,
            lyrics_lines: Vec::new(),
            lyrics_offset: 0,
            lyrics_following: true,
            lyrics_paused_at: None,
            lyrics_rows: 0,
            lyrics_area: Rect::default(),
            navigation_count: None,
            pending_g: false,
            worker: None,
            playback: None,
            playback_view: PlaybackView::default(),
            quitting: false,
            exit_error: None,
        })
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse),
            _ => {}
        }
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        if self.command.is_some()
            || self.search_prompt.is_some()
            || self.quitting
            || self.chooser.is_some()
        {
            return;
        }
        let delta = match mouse.kind {
            MouseEventKind::ScrollUp => -3,
            MouseEventKind::ScrollDown => 3,
            _ => return,
        };
        if self.lyrics_open {
            let pointer = Position::new(mouse.column, mouse.row);
            if self.lyrics_area.contains(pointer) {
                self.scroll_lyrics(delta);
            }
            return;
        }
        let pointer = Position::new(mouse.column, mouse.row);
        if self.songs_area.contains(pointer) {
            self.reset_navigation();
            self.scroll_songs(delta);
        } else if self.results_area.contains(pointer) && self.worker.is_none() {
            self.reset_navigation();
            self.scroll_details(delta);
        }
    }

    fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release || self.quitting {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit();
            return;
        }
        if self.chooser.is_some() {
            self.chooser_key(key);
            return;
        }
        if self.search_prompt.is_some() {
            self.search_key(key);
            return;
        }
        if let Some(command) = &mut self.command {
            match key.code {
                KeyCode::Esc => self.command = None,
                KeyCode::Backspace => {
                    command.backspace();
                }
                KeyCode::Delete => command.delete(),
                KeyCode::Left => command.move_cursor(CursorMove::Left),
                KeyCode::Right => command.move_cursor(CursorMove::Right),
                KeyCode::Home => command.move_cursor(CursorMove::Start),
                KeyCode::End => command.move_cursor(CursorMove::End),
                KeyCode::Tab | KeyCode::BackTab => {
                    let reverse =
                        key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
                    if let Err(error) = command.complete(reverse) {
                        self.message = format!("Cannot complete command: {error}");
                    }
                }
                KeyCode::Enter => {
                    let command = self.command.take().unwrap_or_default();
                    self.execute_command(command.text());
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    command.insert(character)
                }
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let count = self.navigation_count.unwrap_or(1);
            self.reset_navigation();
            if self.lyrics_open {
                let rows = isize::try_from(self.lyrics_rows.max(1)).unwrap_or(isize::MAX);
                let distance = rows.saturating_mul(isize::try_from(count).unwrap_or(isize::MAX));
                match key.code {
                    KeyCode::Char('d') | KeyCode::Char('f') => self.scroll_lyrics(distance),
                    KeyCode::Char('u') | KeyCode::Char('b') => self.scroll_lyrics(-distance),
                    _ => {}
                }
                return;
            }
            match key.code {
                KeyCode::Char('d') => self.page(1, true, count),
                KeyCode::Char('u') => self.page(-1, true, count),
                KeyCode::Char('f') => self.page(1, false, count),
                KeyCode::Char('b') => self.page(-1, false, count),
                _ => {}
            }
            return;
        }
        if key.modifiers.intersects(
            KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META,
        ) {
            self.reset_navigation();
            return;
        }
        if let KeyCode::Char(character) = key.code
            && character.is_ascii_digit()
        {
            if self.pending_g {
                self.navigation_count = None;
            }
            self.pending_g = false;
            self.navigation_count = Some(
                self.navigation_count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(usize::from(character as u8 - b'0')),
            );
            return;
        }
        if self.lyrics_open {
            self.refresh_lyric_follow();
        }
        let count = self.navigation_count.take();
        let pending_g = std::mem::take(&mut self.pending_g);
        let distance = isize::try_from(count.unwrap_or(1).max(1)).unwrap_or(isize::MAX);
        let code = if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Char('g') => KeyCode::Char('G'),
                KeyCode::Char('n') => KeyCode::Char('N'),
                code => code,
            }
        } else {
            key.code
        };
        match code {
            KeyCode::Esc if self.lyrics_open => self.close_lyrics(),
            KeyCode::Char('l') if self.lyrics_open => self.snap_or_close_lyrics(),
            KeyCode::Char('j') | KeyCode::Down if self.lyrics_open => self.scroll_lyrics(distance),
            KeyCode::Char('k') | KeyCode::Up if self.lyrics_open => self.scroll_lyrics(-distance),
            KeyCode::PageDown if self.lyrics_open => {
                self.scroll_lyrics(self.lyric_page(count.unwrap_or(1)));
            }
            KeyCode::PageUp if self.lyrics_open => {
                self.scroll_lyrics(-self.lyric_page(count.unwrap_or(1)));
            }
            KeyCode::Char('g') | KeyCode::Char('G') if self.lyrics_open => {}
            KeyCode::Char('g') => {
                if pending_g {
                    if let Some(line) = count {
                        self.jump_song(line);
                    } else {
                        self.jump_endpoint(false);
                    }
                } else {
                    self.navigation_count = count;
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                if let Some(line) = count {
                    self.jump_song(line);
                } else {
                    self.jump_endpoint(true);
                }
            }
            KeyCode::Char(':') => self.command = Some(CommandLine::default()),
            KeyCode::Char('/') => self.begin_search(Direction::Forward),
            KeyCode::Char('?') => self.begin_search(Direction::Backward),
            KeyCode::Char('n') => self.repeat_search(false, count.unwrap_or(1)),
            KeyCode::Char('N') => self.repeat_search(true, count.unwrap_or(1)),
            KeyCode::Tab => {
                self.details_focused = !self.details_focused;
            }
            KeyCode::PageDown => self.page(1, false, count.unwrap_or(1)),
            KeyCode::PageUp => self.page(-1, false, count.unwrap_or(1)),
            KeyCode::Char('j') | KeyCode::Down if self.details_focused => {
                self.scroll_details(distance);
            }
            KeyCode::Char('k') | KeyCode::Up if self.details_focused => {
                self.scroll_details(-distance);
            }
            KeyCode::Char('h') | KeyCode::Left if self.details_focused => {
                self.details_scroll.1 = self.details_scroll.1.saturating_sub(4);
            }
            KeyCode::Char('l') | KeyCode::Right if self.details_focused => {
                let maximum =
                    u16::try_from(self.details.iter().map(String::len).max().unwrap_or(0))
                        .unwrap_or(u16::MAX);
                self.details_scroll.1 = self.details_scroll.1.saturating_add(4).min(maximum);
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(distance),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-distance),
            KeyCode::Char('l') => self.open_lyrics(),
            KeyCode::Char('r') => self.cycle_repeat(),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char(' ') => {
                if self.playback_view.current.is_none() && self.playback_view.pending.is_none() {
                    self.play_selected();
                } else if let Some(playback) = &self.playback {
                    self.playback_message(playback.toggle());
                }
            }
            KeyCode::Left => {
                if let Some(playback) = &self.playback {
                    self.playback_message(
                        playback.seek(SeekTarget::Backward(Duration::from_secs(10))),
                    );
                }
            }
            KeyCode::Right => {
                if let Some(playback) = &self.playback {
                    self.playback_message(
                        playback.seek(SeekTarget::Forward(Duration::from_secs(10))),
                    );
                }
            }
            _ => {}
        }
    }

    fn begin_search(&mut self, direction: Direction) {
        self.reset_navigation();
        self.search_prompt = Some(SearchPrompt {
            input: CommandLine::default(),
            direction,
            origin: self.selection,
            origin_track: self
                .selection
                .selected()
                .and_then(|row| self.tracks.get(row))
                .map(|track| track.id),
            details_focused: self.details_focused,
            preview: None,
        });
        self.details_focused = false;
    }

    fn restore_search_origin(&mut self, prompt: &SearchPrompt) {
        self.selection = prompt.origin;
        let selected = prompt
            .origin_track
            .and_then(|id| self.tracks.iter().position(|track| track.id == id))
            .or(prompt.origin.selected());
        self.selection.select(selected);
        self.details_focused = prompt.details_focused;
        self.reveal_selection();
    }

    fn update_search_preview(&mut self) {
        let Some(mut prompt) = self.search_prompt.take() else {
            return;
        };
        self.restore_search_origin(&prompt);
        self.details_focused = false;
        prompt.preview = if prompt.input.text().is_empty() {
            None
        } else {
            Some(Search::new(
                prompt.input.text(),
                prompt.direction,
                &self.search_tracks(),
            ))
        };
        if let Some(target) = prompt
            .preview
            .as_ref()
            .and_then(|search| search.preview(self.selection.selected().unwrap_or(0)))
        {
            self.jump_song(target + 1);
        }
        self.search_prompt = Some(prompt);
    }

    fn search_key(&mut self, key: KeyEvent) {
        if key.modifiers.intersects(
            KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::HYPER
                | KeyModifiers::META,
        ) {
            return;
        }
        match key.code {
            KeyCode::Esc => {
                if let Some(prompt) = self.search_prompt.take() {
                    self.restore_search_origin(&prompt);
                }
            }
            KeyCode::Enter => self.confirm_search(),
            code => {
                let Some(prompt) = &mut self.search_prompt else {
                    return;
                };
                match code {
                    KeyCode::Char(character) => prompt.input.insert(character),
                    KeyCode::Backspace => {
                        prompt.input.backspace();
                    }
                    KeyCode::Delete => prompt.input.delete(),
                    KeyCode::Left => prompt.input.move_cursor(CursorMove::Left),
                    KeyCode::Right => prompt.input.move_cursor(CursorMove::Right),
                    KeyCode::Home => prompt.input.move_cursor(CursorMove::Start),
                    KeyCode::End => prompt.input.move_cursor(CursorMove::End),
                    _ => return,
                }
                if matches!(
                    code,
                    KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
                ) {
                    self.update_search_preview();
                }
            }
        }
    }

    fn confirm_search(&mut self) {
        let Some(prompt) = self.search_prompt.take() else {
            return;
        };
        if prompt.input.text().is_empty() {
            self.restore_search_origin(&prompt);
            self.search = self
                .search
                .as_ref()
                .map(|search| Search::new(&search.query, prompt.direction, &self.search_tracks()));
            self.repeat_search(false, 1);
        } else {
            self.search = prompt.preview;
            self.search_message();
        }
        self.details_focused = false;
    }

    fn repeat_search(&mut self, opposite: bool, count: usize) {
        if let Some(target) = self
            .search
            .as_ref()
            .and_then(|search| search.next(self.selection.selected().unwrap_or(0), opposite, count))
        {
            self.jump_song(target + 1);
        }
        self.search_message();
    }

    fn search_message(&mut self) {
        self.message = match &self.search {
            None => "No previous search".into(),
            Some(search) if search.preview(0).is_none() => {
                format!("Pattern not found: {}", search.query)
            }
            Some(search) => format!("Search {}", search.label(self.selection.selected())),
        };
    }

    fn active_search(&self) -> Option<&Search> {
        match &self.search_prompt {
            Some(prompt) => prompt.preview.as_ref(),
            None => self.search.as_ref(),
        }
    }

    fn play_selected(&mut self) {
        let Some(track) = self
            .selection
            .selected()
            .and_then(|index| self.tracks.get(index))
        else {
            self.message = "The library is empty; import music with :import first".into();
            return;
        };
        if let Some(group) = self.groups.iter().find(|g| g.candidates[0].id == track.id)
            && group.candidates.len() > 1
        {
            let labels = match crate::operations::candidate_labels(&self.library, group) {
                Ok(labels) => labels,
                Err(e) => {
                    self.message = e;
                    return;
                }
            };
            self.chooser = Some(Chooser {
                group: group.clone(),
                labels,
                selection: ListState::default().with_selected(Some(0)),
            });
            self.message = "Choose a candidate; Enter confirms, Esc cancels".into();
            return;
        }
        let id = track.id;
        self.load_candidate(id);
    }

    fn load_candidate(&mut self, id: i64) {
        if let Some(playback) = &self.playback {
            match playback.load(id) {
                Ok(()) => self.message = format!("Playback requested: candidate {id}"),
                Err(e) => self.message = e,
            }
        }
    }

    fn chooser_key(&mut self, key: KeyEvent) {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return;
        }
        let Some(mut chooser) = self.chooser.take() else {
            return;
        };
        let current = chooser.selection.selected().unwrap_or(0);
        match key.code {
            KeyCode::Esc => {
                self.message = "Selection cancelled".into();
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                chooser.selection.select(Some(current.saturating_sub(1)))
            }
            KeyCode::Down | KeyCode::Char('j') => chooser
                .selection
                .select(Some((current + 1).min(chooser.group.candidates.len() - 1))),
            KeyCode::Enter => {
                // Revalidate stable IDs, never silently substitute after a commit.
                match self.library.group(&chooser.group.token) {
                    Ok(group) if group == chooser.group => {
                        self.load_candidate(group.candidates[current].id);
                        return;
                    }
                    Ok(group) => {
                        chooser.labels =
                            match crate::operations::candidate_labels(&self.library, &group) {
                                Ok(labels) => labels,
                                Err(e) => {
                                    self.message = e;
                                    return;
                                }
                            };
                        chooser.group = group;
                        chooser.selection.select(Some(0));
                        self.message = "Candidates changed; review and confirm again".into();
                    }
                    Err(e) => {
                        self.message = e.to_string();
                        return;
                    }
                }
            }
            _ => {}
        }
        self.chooser = Some(chooser);
    }

    fn search_tracks(&self) -> Vec<Track> {
        self.tracks
            .iter()
            .map(|track| {
                let mut combined = track.clone();
                if let Some(group) = self.groups.iter().find(|g| g.candidates[0].id == track.id) {
                    // Only the search projection combines text; displayed metadata stays exact.
                    for other in group.candidates.iter().skip(1) {
                        combined.title.push_str(&format!(
                            "\n{}\n{}\n{}",
                            other.title,
                            other.artist.as_deref().unwrap_or(""),
                            other.album.as_deref().unwrap_or("")
                        ));
                    }
                }
                combined
            })
            .collect()
    }

    fn apply_groups(&mut self, groups: Vec<SongGroup>) {
        let old_row = self.selection.selected().unwrap_or(0);
        let selected = self.tracks.get(old_row).map(|t| t.id);
        self.tracks = groups.iter().map(|g| g.candidates[0].clone()).collect();
        self.groups = groups;
        self.selection.select(if self.tracks.is_empty() {
            None
        } else {
            Some(
                selected
                    .and_then(|id| {
                        self.groups
                            .iter()
                            .position(|g| g.candidates.iter().any(|t| t.id == id))
                    })
                    .unwrap_or(old_row.min(self.tracks.len() - 1)),
            )
        });
        self.reveal_selection();
        let search_tracks = self.search_tracks();
        if let Some(search) = &mut self.search {
            search.refresh(&search_tracks);
        }
        self.update_search_preview();
        // Keep the chooser's original membership until Enter revalidates it.
    }

    fn poll_refresh(&mut self) {
        let updates: Vec<_> = self
            .refresh
            .as_ref()
            .map(|r| r.updates.try_iter().collect())
            .unwrap_or_default();
        for update in updates {
            match update {
                Ok(groups) => match &self.browser {
                    Browser::Library if groups != self.groups => self.apply_groups(groups),
                    Browser::Library => {}
                    Browser::Playlist { .. } => {
                        self.groups = groups;
                        if let Err(error) = self.reload_browser() {
                            self.message = format!("Cannot refresh library; retrying: {error}");
                        }
                    }
                },
                Err(e) => self.message = format!("Cannot refresh library; retrying: {e}"),
            }
        }
    }

    fn playback_message(&mut self, result: Result<(), String>) {
        self.message = result
            .err()
            .unwrap_or_else(|| "Playback operation submitted".into());
    }

    fn poll_playback(&mut self) {
        if let Some(playback) = &self.playback {
            self.playback_view = playback.view();
            if self.quitting
                && let Some(result) = playback.completion()
            {
                self.exit_error = result.err();
                self.playback.take();
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if !self.tracks.is_empty() {
            let index = self
                .selection
                .selected()
                .unwrap_or(0)
                .saturating_add_signed(delta)
                .min(self.tracks.len() - 1);
            self.selection.select(Some(index));
        }
        self.reveal_selection();
    }

    /// Like agent board's reveal/clamp_view: keyboard movement changes the
    /// cursor; the camera follows only as far as needed to keep it visible.
    fn reveal_selection(&mut self) {
        if self.tracks.is_empty() {
            self.selection.select(None);
            return;
        }
        let selected = self
            .selection
            .selected()
            .unwrap_or(0)
            .min(self.tracks.len() - 1);
        self.selection.select(Some(selected));
        let height = usize::from(self.songs_area.height.max(1));
        let lower = selected.saturating_add(1).saturating_sub(height);
        let upper = selected.min(self.tracks.len().saturating_sub(height));
        let offset = self.selection.offset_mut();
        *offset = (*offset).clamp(lower, upper);
    }

    /// Agent board's wheel rule: move the camera, then move the selection by
    /// the distance the camera actually traveled, retaining its screen row.
    fn scroll_songs(&mut self, delta: isize) {
        if self.tracks.is_empty() || self.songs_area.height == 0 {
            return;
        }
        self.reveal_selection();
        let before = self.selection.offset();
        let maximum = self
            .tracks
            .len()
            .saturating_sub(usize::from(self.songs_area.height));
        let next = before.saturating_add_signed(delta).min(maximum);
        let selected = self.selection.selected().unwrap_or(0);
        self.selection.select(Some(if next >= before {
            selected.saturating_add(next - before)
        } else {
            selected.saturating_sub(before - next)
        }));
        *self.selection.offset_mut() = next;
    }

    fn page(&mut self, direction: isize, half: bool, count: usize) {
        let span = if self.details_focused {
            usize::from(self.results_area.height)
        } else {
            usize::from(self.songs_area.height).min(self.tracks.len())
        };
        let rows = if half {
            span / 2
        } else {
            span.saturating_sub(1)
        };
        let delta = isize::try_from(rows.max(1).saturating_mul(count.max(1))).unwrap_or(isize::MAX)
            * direction;
        if self.details_focused {
            self.scroll_details(delta);
        } else {
            self.move_selection(delta);
        }
    }

    fn details_maximum(&self) -> usize {
        self.details
            .iter()
            .map(|line| line.split('\n').count())
            .sum::<usize>()
            .saturating_sub(usize::from(self.results_area.height.max(1)))
            .min(usize::from(u16::MAX))
    }

    fn scroll_details(&mut self, delta: isize) {
        self.details_scroll.0 = u16::try_from(
            usize::from(self.details_scroll.0)
                .saturating_add_signed(delta)
                .min(self.details_maximum()),
        )
        .unwrap_or(u16::MAX);
    }

    fn reset_navigation(&mut self) {
        self.navigation_count = None;
        self.pending_g = false;
    }

    fn jump_song(&mut self, line: usize) {
        self.details_focused = false;
        self.selection.select(if self.tracks.is_empty() {
            None
        } else {
            Some(line.saturating_sub(1).min(self.tracks.len() - 1))
        });
        self.reveal_selection();
    }

    fn jump_endpoint(&mut self, last: bool) {
        if self.details_focused {
            self.details_scroll.0 = if last {
                u16::try_from(self.details_maximum()).unwrap_or(u16::MAX)
            } else {
                0
            };
        } else {
            self.jump_song(if last { self.tracks.len() } else { 1 });
        }
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.trim();
        self.reset_navigation();
        if let Some(line) = decimal_line(input) {
            self.jump_song(line);
            return;
        }
        let rest = match BuiltinCommand::parse(input) {
            Some((BuiltinCommand::Detach, _)) => {
                self.exit(true);
                return;
            }
            Some((BuiltinCommand::Quit, _)) => {
                self.quit();
                return;
            }
            Some((BuiltinCommand::Stop, _)) => {
                if let Some(playback) = &self.playback {
                    self.playback_message(playback.stop());
                }
                return;
            }
            Some((BuiltinCommand::Import, rest)) => rest,
            Some((BuiltinCommand::Playlist, rest)) => {
                self.playlist_command(rest);
                return;
            }
            Some((BuiltinCommand::Volume, rest)) => {
                self.volume_command(rest);
                return;
            }
            None => {
                self.message = format!("Unknown command: {input}");
                return;
            }
        };
        if self.worker.is_some() {
            self.message = "An import is already running".into();
            return;
        }
        let rest = rest.trim();
        let rest = if rest.len() >= 2
            && ((rest.starts_with('"') && rest.ends_with('"'))
                || (rest.starts_with('\'') && rest.ends_with('\'')))
        {
            &rest[1..rest.len() - 1]
        } else {
            rest
        };
        if rest.is_empty() {
            self.message = "Usage: :import <MP3, FLAC or NCM file or folder path>".into();
            return;
        }
        let source = expand_path(rest);
        self.message = "Import started".into();
        self.worker = Some(ImportWorker::start(self.library_path.clone(), source));
    }

    fn quit(&mut self) {
        self.exit(false);
    }

    fn exit(&mut self, detach: bool) {
        self.quitting = true;
        if detach {
            self.exit_error = None;
        }
        if let Some(playback) = &self.playback {
            playback.exit(detach);
        }
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Relaxed);
            self.message = "Finishing the current file import, then quitting...".into();
        }
    }

    fn poll_import(&mut self) -> Result<(), LibraryError> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        let result = match worker.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => Err("the import worker stopped unexpectedly".into()),
        };
        self.worker.take(); // Drop joins the finished worker and releases its connection.
        match result {
            Ok(report) => self.show_report(report),
            Err(error) => {
                self.message = format!("Cannot import: {error}");
                self.details = vec![error];
            }
        }
        if self.refresh.is_none() {
            self.reload_browser()?;
        }
        Ok(())
    }

    fn reload_browser(&mut self) -> Result<(), LibraryError> {
        match self.browser.clone() {
            Browser::Library => self.apply_groups(self.library.groups()?),
            Browser::Playlist { id, .. } => {
                self.groups = self.library.groups()?;
                match self.library.playlist_tracks(id) {
                    Ok(tracks) => self.replace_tracks(tracks),
                    Err(LibraryError::PlaylistNotFound(_)) => {
                        let all_local = self.library.all_local()?;
                        self.browser = Browser::Playlist {
                            id: all_local.id,
                            name: all_local.name,
                        };
                        let tracks = self.library.playlist_tracks(all_local.id)?;
                        self.replace_tracks(tracks);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    fn replace_tracks(&mut self, tracks: Vec<Track>) {
        let old_row = self.selection.selected().unwrap_or(0);
        let selected = self.tracks.get(old_row).map(|track| track.id);
        self.tracks = tracks;
        self.selection.select(if self.tracks.is_empty() {
            None
        } else {
            Some(
                selected
                    .and_then(|id| self.tracks.iter().position(|track| track.id == id))
                    .unwrap_or(old_row.min(self.tracks.len() - 1)),
            )
        });
        self.reveal_selection();
        let search_tracks = self.search_tracks();
        if let Some(search) = &mut self.search {
            search.refresh(&search_tracks);
        }
        self.update_search_preview();
    }

    fn cycle_repeat(&mut self) {
        if let Some(playback) = &self.playback {
            self.message = match playback.cycle_mode() {
                Ok(()) => {
                    let mode = playback.view().mode;
                    self.playback_view.mode = mode;
                    format!("Playback mode: {}", mode.label())
                }
                Err(error) => error,
            };
        } else {
            self.playback_view.mode = self.playback_view.mode.cycle();
            self.message = format!("Playback mode: {}", self.playback_view.mode.label());
        }
    }

    fn volume_command(&mut self, rest: &str) {
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let command = match crate::volume::parse(&tokens) {
            Ok(command) => command,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let args: Vec<std::ffi::OsString> = tokens.iter().copied().map(Into::into).collect();
        let (percent, message) = tui_volume_outcome(
            command,
            crate::operations::volume(&self.library_path, &args),
        );
        if let Some(percent) = percent {
            self.playback_view.volume = percent;
        }
        self.message = message;
    }

    fn playlist_command(&mut self, rest: &str) {
        let rest = rest.trim();
        if rest.is_empty() || rest == "list" {
            self.show_playlist_list();
            return;
        }
        if rest == "library" {
            self.browser = Browser::Library;
            match self.library.groups() {
                Ok(groups) => {
                    self.apply_groups(groups);
                    self.message =
                        "Showing the library. :playlist use <name> returns to a playlist.".into();
                }
                Err(error) => self.message = error.to_string(),
            }
            return;
        }
        if rest == "new" || rest.starts_with("new ") {
            let name = rest["new".len()..].trim();
            if name.is_empty() {
                self.message = "Usage: :playlist new <name>".into();
                return;
            }
            self.message = match self.library.create_playlist(name) {
                Ok(playlist) => format!("Created playlist {} ({})", playlist.name, playlist.id),
                Err(error) => error.to_string(),
            };
            return;
        }
        let (action, argument) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let argument = argument.trim();
        match action {
            "add" => self.add_to_named_playlist(argument),
            "use" => self.activate_playlist(argument, false),
            "play" => self.activate_playlist(argument, true),
            _ => {
                self.message =
                    "Usage: :playlist list | new <name> | add <playlist> [track] | use <playlist> | play <playlist> | library"
                        .into();
            }
        }
    }

    fn show_playlist_list(&mut self) {
        match self.library.playlists() {
            Ok(playlists) => {
                self.details = playlists
                    .iter()
                    .map(|playlist| {
                        let kind = match playlist.kind {
                            muse_library::PlaylistKind::AllLocal => "all-local",
                            muse_library::PlaylistKind::User => "user",
                        };
                        format!(
                            "{}\t{}\t{}\t{} tracks",
                            playlist.id, playlist.name, kind, playlist.len
                        )
                    })
                    .collect();
                self.details_scroll = (0, 0);
                self.message = "Playlists listed in the results panel".into();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn add_to_named_playlist(&mut self, argument: &str) {
        if argument.is_empty() {
            self.message = "Usage: :playlist add <playlist> [track-id]".into();
            return;
        }
        let (playlist, track) = split_playlist_target(argument);
        let track_id = if let Some(track) = track {
            match track.parse::<i64>() {
                Ok(id) if id > 0 => id,
                _ => {
                    self.message = "playlist add requires a positive track ID".into();
                    return;
                }
            }
        } else {
            match self
                .selection
                .selected()
                .and_then(|index| self.tracks.get(index))
                .map(|track| track.id)
            {
                Some(id) => id,
                None => {
                    self.message = "Select a track before adding it to a playlist".into();
                    return;
                }
            }
        };
        let playlist_id = match crate::operations::resolve_playlist(&self.library, playlist) {
            Ok(id) => id,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        self.message = match self.library.add_to_playlist(playlist_id, track_id) {
            Ok(true) => format!("Added track {track_id} to playlist {playlist_id}"),
            Ok(false) => format!("Track {track_id} is already in playlist {playlist_id}"),
            Err(error) => error.to_string(),
        };
    }

    fn activate_playlist(&mut self, token: &str, play: bool) {
        let (token, extra) = split_playlist_target(token);
        if token.is_empty() || extra.is_some() {
            self.message = "Usage: :playlist use <playlist> or :playlist play <playlist>".into();
            return;
        }
        let playlist_id = match crate::operations::resolve_playlist(&self.library, token) {
            Ok(id) => id,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let playlist = match self.library.playlist(playlist_id) {
            Ok(playlist) => playlist,
            Err(error) => {
                self.message = error.to_string();
                return;
            }
        };
        let tracks = match self.library.playlist_tracks(playlist.id) {
            Ok(tracks) => tracks,
            Err(error) => {
                self.message = error.to_string();
                return;
            }
        };
        self.browser = Browser::Playlist {
            id: playlist.id,
            name: playlist.name.clone(),
        };
        self.replace_tracks(tracks);
        if play {
            if let Some(playback) = &self.playback {
                self.playback_message(playback.play_playlist(playlist.id));
                return;
            }
        } else if let Some(playback) = &self.playback {
            self.playback_message(playback.use_playlist(playlist.id));
            return;
        }
        self.message = format!("Current playlist: {}", playlist.name);
    }

    fn show_report(&mut self, report: ImportReport) {
        self.message = format!(
            "Import {}: added {}, skipped duplicates {}, failed {}, attachment/metadata issues {}, scan issues {}, retained collisions {}",
            if report.cancelled {
                "cancelled"
            } else if report.stopped.is_some() {
                "stopped"
            } else {
                "completed"
            },
            report.imported,
            report.duplicates,
            report.failed.len(),
            report.attachment_issues.len(),
            report.scan_issues.len(),
            report.collisions
        );
        self.details = vec![self.message.clone()];
        if let Some(reason) = report.stopped {
            self.details.push(format!("Stopped: {reason}"));
        }
        for (label, issues) in [
            ("Failed", report.failed),
            ("Attachment/metadata", report.attachment_issues),
            ("Scan", report.scan_issues),
        ] {
            for issue in issues {
                self.details.push(format!(
                    "{label} {}: {}",
                    issue.path.display(),
                    issue.reason
                ));
            }
        }
        self.details_scroll = (0, 0);
    }

    fn draw(&mut self, frame: &mut Frame) {
        self.refresh_lyric_follow();
        let import = self
            .worker
            .as_ref()
            .map(|worker| (worker.source.clone(), worker.snapshot()));
        let [title, body, status, command] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        let list_name = match &self.browser {
            Browser::Library => "Library".to_owned(),
            Browser::Playlist { name, .. } => name.clone(),
        };
        if self.lyrics_open {
            self.draw_lyrics_top(frame, title);
            self.lyrics_area = body;
            self.songs_area = Rect::default();
            self.draw_lyrics(frame, body);
        } else {
            frame.render_widget(
                Paragraph::new(format!(
                    "muse | {} [local·NetEase] | {} | {} tracks",
                    self.library_name,
                    list_name,
                    self.tracks.len()
                ))
                .style(Style::default().fg(Color::Cyan)),
                title,
            );
            let main = body;
            let [songs, results] =
                Layout::vertical([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .areas(main);
            let library_title = self.active_search().map_or_else(
                || list_name.clone(),
                |search| format!("{list_name} | {}", search.label(self.selection.selected())),
            );
            let songs_block = Block::default()
                .borders(Borders::ALL)
                .title(library_title)
                .border_style(Style::default().fg(if self.details_focused {
                    Color::DarkGray
                } else {
                    Color::Cyan
                }));
            self.songs_area = songs_block.inner(songs);
            self.results_area = Block::bordered().inner(results);
            self.reveal_selection();
            self.scroll_details(0);
            if self.tracks.is_empty() {
                frame.render_widget(
                Paragraph::new(
                    "The library is empty\nUse :import <MP3, FLAC or NCM file or folder path> to import music",
                )
                .block(songs_block),
                songs,
            );
            } else {
                let number_width = self.tracks.len().to_string().len();
                let items: Vec<ListItem> =
                    self.tracks
                        .iter()
                        .enumerate()
                        .map(|(index, track)| {
                            ListItem::new(Line::from(format!(
                                "{:>number_width$} {}{} — {} [{}]",
                                index + 1,
                                if self.playback_view.current.is_some_and(|current| current.id
                                    == track.id
                                    || self.groups.iter().any(|g| g.candidates[0].id == track.id
                                        && g.candidates.iter().any(|t| t.id == current.id)))
                                {
                                    "♪ "
                                } else {
                                    ""
                                },
                                track.title,
                                track.artist.as_deref().unwrap_or("Unknown"),
                                self.groups
                                    .iter()
                                    .find(|g| g.candidates[0].id == track.id)
                                    .map_or_else(
                                        || track.format.clone(),
                                        |g| format!(
                                            "{}; {} candidates",
                                            track.format,
                                            g.candidates.len()
                                        )
                                    )
                            )))
                            .style(
                                if self
                                    .active_search()
                                    .is_some_and(|search| search.contains(index))
                                {
                                    Style::default()
                                        .fg(Color::Yellow)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default()
                                },
                            )
                        })
                        .collect();
                frame.render_stateful_widget(
                    List::new(items)
                        .block(songs_block)
                        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                        .highlight_symbol("› ")
                        .highlight_spacing(HighlightSpacing::Always),
                    songs,
                    &mut self.selection,
                );
            }
            if let Some((source, progress)) = &import {
                self.draw_import(frame, results, source, progress.as_ref());
            } else {
                let results_block = Block::default()
                    .borders(Borders::ALL)
                    .title("Import results | Tab focus, j/k scroll, h/l horizontal")
                    .border_style(Style::default().fg(if self.details_focused {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }));
                frame.render_widget(
                    Paragraph::new(self.details.join("\n"))
                        .block(results_block)
                        .scroll(self.details_scroll),
                    results,
                );
            }
        }
        let playback_status = self.playback_line();
        let status_text = match &import {
            Some((_, progress)) => format!(
                "{} | {playback_status} | {}",
                import_summary(progress.as_ref()),
                self.message
            ),
            None => format!("{playback_status} | {}", self.message),
        };
        frame.render_widget(
            Paragraph::new(status_text).style(Style::default().fg(Color::Yellow)),
            status,
        );
        self.draw_command(frame, command);
        if let Some(chooser) = &mut self.chooser {
            let area = frame.area();
            let inset = if area.width > 8 && area.height > 6 {
                2
            } else {
                0
            };
            let area = Rect::new(
                area.x + inset,
                area.y + inset,
                area.width.saturating_sub(inset * 2),
                area.height.saturating_sub(inset * 2),
            );
            frame.render_widget(Clear, area);
            let items = chooser
                .labels
                .iter()
                .map(|label| ListItem::new(label.as_str()))
                .collect::<Vec<_>>();
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::bordered().title("Choose candidate | Enter confirm | Esc cancel"))
                    .highlight_style(Style::default().bg(Color::Blue))
                    .highlight_symbol("› "),
                area,
                &mut chooser.selection,
            );
        }
    }

    fn draw_import(
        &self,
        frame: &mut Frame,
        area: Rect,
        source: &Path,
        progress: Option<&ImportProgress>,
    ) {
        let stage = if self.quitting {
            "Finishing current file"
        } else {
            match progress.map(|progress| progress.phase) {
                None => "Preparing",
                Some(ImportPhase::Scanning) => "Scanning",
                Some(ImportPhase::Importing) => "Importing",
                Some(ImportPhase::Finalizing) => "Finalizing attachments",
                Some(ImportPhase::Completed) => "Completed",
                Some(ImportPhase::Cancelled) => "Cancelled",
                Some(ImportPhase::Stopped) => "Stopped",
            }
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Import progress | {stage}"))
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [bar, counts, current, root] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);
        let ratio = progress.map_or(0.0, import_ratio);
        let label = if progress.is_some_and(|progress| progress.total == Some(0)) {
            "No supported audio files".to_owned()
        } else {
            import_summary(progress)
        };
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(ratio)
                .label(label),
            bar,
        );
        let counters = progress.map_or_else(
            || "Added 0 | Skipped duplicates 0 | Failed 0".to_owned(),
            |progress| {
                format!(
                    "Added {} | Skipped duplicates {} | Failed {} | Retained collisions {}",
                    progress.imported, progress.duplicates, progress.failed, progress.collisions
                )
            },
        );
        frame.render_widget(Paragraph::new(counters), counts);
        let filename = progress
            .and_then(|progress| progress.current.as_ref())
            .and_then(|path| path.file_name())
            .map(|name| display_path(Path::new(name)))
            .unwrap_or_default();
        frame.render_widget(Paragraph::new(format!("Current: {filename}")), current);
        frame.render_widget(
            Paragraph::new(format!("Source: {}", display_path(source))).wrap(Wrap { trim: false }),
            root,
        );
    }

    fn draw_command(&self, frame: &mut Frame, area: Rect) {
        let [badge, input_area] =
            Layout::horizontal([Constraint::Length(9), Constraint::Min(0)]).areas(area);
        let (label, background, foreground) =
            if self.command.is_some() || self.search_prompt.is_some() {
                (" COMMAND ", Color::Yellow, Color::Black)
            } else {
                (" NORMAL  ", Color::Blue, Color::White)
            };
        frame.render_widget(
            Paragraph::new(label).style(
                Style::default()
                    .bg(background)
                    .fg(foreground)
                    .add_modifier(Modifier::BOLD),
            ),
            badge,
        );
        let (marker, input) = if let Some(search) = &self.search_prompt {
            (search.direction.marker(), &search.input)
        } else if let Some(command) = &self.command {
            (':', command)
        } else {
            return;
        };
        let prompt = format!("{marker}{}", input.text());
        let cursor_width = Line::raw(&prompt[..1 + input.cursor()]).width();
        let scroll = cursor_width.saturating_sub(usize::from(input_area.width.saturating_sub(1)));
        let scroll = u16::try_from(scroll).unwrap_or(u16::MAX);
        frame.render_widget(Paragraph::new(prompt).scroll((0, scroll)), input_area);
        if input_area.width > 0 && input_area.height > 0 {
            let offset = cursor_width
                .saturating_sub(usize::from(scroll))
                .min(usize::from(input_area.width - 1)) as u16;
            frame.set_cursor_position((input_area.x + offset, input_area.y));
        }
        if self.search_prompt.is_some() {
            return;
        }
        let candidates = input.candidates();
        let height = (candidates.len() + 2)
            .min(8)
            .min(usize::from(area.y.saturating_sub(frame.area().y))) as u16;
        if candidates.is_empty() || height < 3 || input_area.width < 4 {
            return;
        }
        let width = candidates
            .iter()
            .map(|candidate| Line::raw(&candidate.label).width() + 4)
            .max()
            .unwrap_or(4)
            .clamp(20, 70)
            .min(usize::from(input_area.width)) as u16;
        let popup = Rect::new(input_area.x, area.y - height, width, height);
        frame.render_widget(Clear, popup);
        let items = candidates
            .iter()
            .map(|candidate| ListItem::new(candidate.label.as_str()));
        let mut selection = ListState::default().with_selected(input.selected());
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(input.completion_title()),
                )
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                .highlight_symbol("› "),
            popup,
            &mut selection,
        );
    }

    fn playback_line(&self) -> String {
        let volume = format!("vol: {}%", self.playback_view.volume);
        let playlist = if self.playback_view.playlist_name.is_empty() {
            match &self.browser {
                Browser::Playlist { name, .. } => name.clone(),
                Browser::Library => muse_library::ALL_LOCAL_PLAYLIST.to_owned(),
            }
        } else {
            self.playback_view.playlist_name.clone()
        };
        let mode = self.playback_view.mode.label();
        if let Some(error) = &self.playback_view.error {
            return format!("Playback error: {error} | {playlist} | {mode} | {volume}");
        }
        if self.playback_view.pending.is_some() {
            return format!("Loading track... | {playlist} | {mode} | {volume}");
        }
        let Some(status) = self.playback_view.status else {
            return format!("Not playing | {playlist} | {mode} | {volume}");
        };
        let state = match status.state {
            PlaybackState::Empty => "Empty",
            PlaybackState::Ready => "Stopped",
            PlaybackState::Playing => "Playing",
            PlaybackState::Paused => "Paused",
            PlaybackState::Finished => "Finished",
        };
        let position = time_label(status.position);
        let total = status
            .duration
            .map(time_label)
            .unwrap_or_else(|| "Unknown".into());
        let title = self
            .playback_view
            .current
            .and_then(|current| {
                self.groups
                    .iter()
                    .flat_map(|g| g.candidates.iter())
                    .chain(self.tracks.iter())
                    .find(|track| track.id == current.id)
            })
            .map(|track| track.title.as_str())
            .unwrap_or("");
        format!("{state} {title} {position}/{total} | {playlist} | {mode} | {volume}")
    }

    fn draw_lyrics_top(&self, frame: &mut Frame, area: Rect) {
        let (title, artist) = self.current_track().map_or(("", "Unknown"), |track| {
            (
                track.title.as_str(),
                track.artist.as_deref().unwrap_or("Unknown"),
            )
        });
        let (elapsed, total) = self.playback_view.status.map_or_else(
            || ("0:00".to_owned(), "--:--".to_owned()),
            |status| {
                (
                    time_label(status.position),
                    status
                        .duration
                        .map(time_label)
                        .unwrap_or_else(|| "--:--".into()),
                )
            },
        );
        let timing = format!("{elapsed} / {total}");
        let name = if title.is_empty() {
            "Not playing".to_owned()
        } else {
            format!("{title} - {artist}")
        };
        let timing_width = u16::try_from(timing.len()).unwrap_or(u16::MAX);
        let [left, right] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(timing_width.saturating_add(1)),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(name).style(Style::default().fg(Color::Cyan)),
            left,
        );
        frame.render_widget(
            Paragraph::new(timing)
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::Cyan)),
            right,
        );
    }

    fn draw_lyrics(&mut self, frame: &mut Frame, area: Rect) {
        self.sync_lyrics();
        self.lyrics_rows = usize::from(area.height);
        if self.playback_view.current.is_none() || self.lyrics_lines.is_empty() {
            self.draw_lyrics_empty(frame, area);
            return;
        }
        let height = self.lyrics_rows.max(1);
        let len = self.lyrics_lines.len();
        if self.lyrics_following {
            if let Some(active) = self.active_lyric() {
                self.lyrics_offset = centered_lyric_offset(active, height, len);
            }
        } else {
            self.lyrics_offset = self.lyrics_offset.min(len.saturating_sub(height));
        }
        let active = self.active_lyric();
        let lines: Vec<Line> = self
            .lyrics_lines
            .iter()
            .enumerate()
            .skip(self.lyrics_offset)
            .take(height)
            .map(|(index, line)| {
                let style = if Some(index) == active {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(line.text.as_str()).style(style)
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }

    fn draw_lyrics_empty(&self, frame: &mut Frame, area: Rect) {
        let pad = area.height.saturating_sub(2) / 2;
        let [_, title, helper, _] = Layout::vertical([
            Constraint::Length(pad),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new("暂无歌词")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::White)),
            title,
        );
        frame.render_widget(
            Paragraph::new("可放同名 .lrc 后 :import")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            helper,
        );
    }

    fn sync_lyrics(&mut self) {
        let id = self.playback_view.current.map(|current| current.id);
        if id == self.lyrics_track {
            return;
        }
        self.lyrics_track = id;
        self.lyrics_lines = id
            .and_then(|id| self.library.attachments(id).ok())
            .and_then(|attachments| {
                attachments
                    .into_iter()
                    .find(|attachment| attachment.kind == AttachmentKind::Lyrics)
            })
            .map(|attachment| parse_lrc(&String::from_utf8_lossy(&attachment.data)))
            .unwrap_or_default();
        self.lyrics_offset = 0;
        self.lyrics_following = true;
        self.lyrics_paused_at = None;
    }

    fn open_lyrics(&mut self) {
        self.lyrics_open = true;
        self.lyrics_following = true;
        self.lyrics_paused_at = None;
        self.lyrics_track = Some(-1);
        self.message = "Lyrics".into();
    }

    fn close_lyrics(&mut self) {
        self.lyrics_open = false;
        self.lyrics_following = true;
        self.lyrics_paused_at = None;
        self.message = "Lyrics hidden".into();
    }

    fn snap_or_close_lyrics(&mut self) {
        if self.lyrics_following {
            self.close_lyrics();
        } else {
            self.lyrics_following = true;
            self.lyrics_paused_at = None;
            self.message = "Lyrics".into();
        }
    }

    fn lyric_page(&self, count: usize) -> isize {
        let rows = isize::try_from(self.lyrics_rows.max(1)).unwrap_or(isize::MAX);
        rows.saturating_mul(isize::try_from(count.max(1)).unwrap_or(isize::MAX))
    }

    fn scroll_lyrics(&mut self, delta: isize) {
        if self.lyrics_lines.is_empty() || delta == 0 {
            return;
        }
        self.pause_lyric_follow();
        let height = self.lyrics_rows.max(1);
        let max = self.lyrics_lines.len().saturating_sub(height);
        if delta < 0 {
            self.lyrics_offset = self.lyrics_offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.lyrics_offset = self
                .lyrics_offset
                .saturating_add(usize::try_from(delta).unwrap_or(usize::MAX));
        }
        self.lyrics_offset = self.lyrics_offset.min(max);
    }

    fn pause_lyric_follow(&mut self) {
        if self.lyrics_following {
            let height = self.lyrics_rows.max(1);
            if let Some(active) = self.active_lyric() {
                self.lyrics_offset = centered_lyric_offset(active, height, self.lyrics_lines.len());
            }
        }
        self.lyrics_following = false;
        self.lyrics_paused_at = Some(Instant::now());
    }

    fn refresh_lyric_follow(&mut self) {
        let Some(paused) = self.lyrics_paused_at else {
            return;
        };
        if paused.elapsed() >= Duration::from_secs(3) {
            self.lyrics_following = true;
            self.lyrics_paused_at = None;
        }
    }

    fn current_track(&self) -> Option<&Track> {
        self.playback_view.current.and_then(|current| {
            self.groups
                .iter()
                .flat_map(|group| group.candidates.iter())
                .chain(self.tracks.iter())
                .find(|track| track.id == current.id)
        })
    }

    fn active_lyric(&self) -> Option<usize> {
        let position = self.playback_view.status?.position;
        self.lyrics_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.at.is_some_and(|at| at <= position))
            .map(|(index, _)| index)
            .next_back()
    }
}

/// TUI success flash. The status bar already shows `vol: N%`, so the message
/// line must not repeat the CLI `Volume:` / `Gain:` observation.
fn volume_success_flash(command: crate::volume::VolumeCommand) -> String {
    match command {
        crate::volume::VolumeCommand::Query => "ok".into(),
        crate::volume::VolumeCommand::Set(value) => format!("vol set {value}"),
        crate::volume::VolumeCommand::Inc(step) => format!("vol inc {step}"),
        crate::volume::VolumeCommand::Dec(step) => format!("vol dec {step}"),
    }
}

fn ui_volume_percent(observation: &str) -> Option<u8> {
    observation
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("Volume: "))
        .and_then(|line| line.strip_suffix('%'))
        .and_then(|line| line.parse().ok())
}

fn tui_volume_outcome(
    command: crate::volume::VolumeCommand,
    result: Result<crate::operations::CommandOutput, String>,
) -> (Option<u8>, String) {
    match result {
        Ok(output) => (
            ui_volume_percent(&output.text),
            volume_success_flash(command),
        ),
        Err(error) => (None, error),
    }
}

fn centered_lyric_offset(selected: usize, height: usize, len: usize) -> usize {
    let height = height.max(1);
    selected
        .saturating_sub(height / 2)
        .min(len.saturating_sub(height))
}

fn split_playlist_target(argument: &str) -> (&str, Option<&str>) {
    if let Some((name, track)) = argument.rsplit_once(char::is_whitespace)
        && track.parse::<i64>().ok().is_some_and(|id| id > 0)
    {
        (name.trim(), Some(track))
    } else {
        (argument.trim(), None)
    }
}

fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut offset_ms = 0_i64;
    let mut lines = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(value) = line
            .strip_prefix("[offset:")
            .and_then(|value| value.strip_suffix(']'))
        {
            if let Ok(parsed) = value.trim().parse::<i64>() {
                offset_ms = parsed;
            }
            continue;
        }
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(end) = rest.strip_prefix('[').and_then(|body| body.find(']')) {
            let tag = &rest[1..end + 1];
            let Some(millis) = timestamp_millis(tag) else {
                break;
            };
            times.push(millis);
            rest = &rest[end + 2..];
        }
        let text = rest.trim();
        if times.is_empty() {
            if !text.is_empty() && !line.starts_with('[') {
                lines.push(LyricLine {
                    at: None,
                    text: text.to_owned(),
                });
            }
            continue;
        }
        for millis in times {
            let adjusted = millis + offset_ms;
            let at = (adjusted > 0).then(|| Duration::from_millis(adjusted as u64));
            lines.push(LyricLine {
                at,
                text: text.to_owned(),
            });
        }
    }
    lines.sort_by(|left, right| match (left.at, right.at) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => left.cmp(&right),
    });
    lines
}

fn timestamp_millis(tag: &str) -> Option<i64> {
    let (minutes, seconds) = tag.split_once(':')?;
    if !minutes.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let minutes = minutes.parse::<i64>().ok()?;
    let seconds = seconds.parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(minutes * 60_000 + (seconds * 1000.0).round() as i64)
}

/// Compose modern Hangul in display labels; file access keeps the original path.
fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        let current = u32::from(character);
        let composed = output.chars().next_back().and_then(|previous| {
            let previous = u32::from(previous);
            if (0x1100..0x1113).contains(&previous) && (0x1161..0x1176).contains(&current) {
                // Leading consonant + vowel -> an LV syllable.
                Some(0xac00 + ((previous - 0x1100) * 21 + current - 0x1161) * 28)
            } else if (0xac00..0xd7a4).contains(&previous)
                && (previous - 0xac00).is_multiple_of(28)
                && (0x11a8..0x11c3).contains(&current)
            {
                // An LV syllable + trailing consonant -> an LVT syllable.
                Some(previous + current - 0x11a7)
            } else {
                None
            }
        });
        if let Some(composed) = composed {
            output.pop();
            output
                .push(char::from_u32(composed).expect("composed Hangul is a valid Unicode scalar"));
        } else {
            output.push(character);
        }
    }
    output
}

fn import_ratio(progress: &ImportProgress) -> f64 {
    progress
        .total
        .filter(|&total| total > 0)
        .map_or(0.0, |total| {
            (progress.processed as f64 / total as f64).clamp(0.0, 1.0)
        })
}

fn import_summary(progress: Option<&ImportProgress>) -> String {
    let Some(progress) = progress else {
        return "Preparing import".into();
    };
    if progress.phase == ImportPhase::Scanning {
        return format!("Scanning: {} audio files found", progress.discovered);
    }
    let label = match progress.phase {
        ImportPhase::Scanning | ImportPhase::Importing => "Import",
        ImportPhase::Finalizing => "Finalizing",
        ImportPhase::Completed => "Import completed",
        ImportPhase::Cancelled => "Import cancelled",
        ImportPhase::Stopped => "Import stopped",
    };
    let total = progress
        .total
        .map_or_else(|| "?".to_owned(), |total| total.to_string());
    if progress.phase == ImportPhase::Importing && progress.total.is_some_and(|total| total > 0) {
        format!(
            "{label} {}/{total} ({}%)",
            progress.processed,
            (import_ratio(progress) * 100.0) as usize
        )
    } else {
        format!("{label} {}/{total}", progress.processed)
    }
}

fn time_label(duration: Duration) -> String {
    format!("{}:{:02}", duration.as_secs() / 60, duration.as_secs() % 60)
}

fn decimal_line(input: &str) -> Option<usize> {
    if input.is_empty() || !input.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    Some(input.bytes().fold(0usize, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'))
    }))
}

impl Drop for App {
    fn drop(&mut self) {
        self.playback.take();
    }
}

#[cfg(unix)]
pub(crate) fn terminal_disconnected() -> std::io::Result<bool> {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    let stdin = std::io::stdin();
    // macOS only reports a terminal hangup when read readiness is requested.
    let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
    match poll(&mut fds, Some(&Timespec::default())) {
        Ok(_) => Ok(fds[0]
            .revents()
            .intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL)),
        Err(rustix::io::Errno::INTR) => Ok(false),
        // The pty slave is gone. Reporting that through the same terminal panics.
        Err(rustix::io::Errno::NXIO) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(unix))]
pub(crate) fn terminal_disconnected() -> std::io::Result<bool> {
    Ok(false)
}

pub fn run(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let mut app = App::new(path.clone())?;
    app.refresh = Some(crate::refresh::Reader::start(path.clone()));
    match crate::service::Remote::attach(&path).and_then(PlaybackWorker::start) {
        Ok(playback) => {
            app.playback_view = playback.view();
            app.playback = Some(playback);
        }
        Err(error) => {
            app.playback_view.error = Some(error.clone());
            app.exit_error = Some(format!("playback stop cannot be confirmed: {error}"));
            app.message = error;
        }
    }
    let _restore = RestoreTerminal;
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(TerminalOutput(std::io::stdout()));
    let mut terminal = UiTerminal::new(backend)?;
    match run_loop(&mut app, &mut terminal) {
        // A hangup can arrive while drawing or waiting for input, after the
        // check at the start of the loop. Finish cleanup in that case too.
        Err(_) if terminal_disconnected()? => {
            app.quit();
            finish_exit(&mut app)
        }
        Err(error) => {
            if !app.quitting {
                app.quit();
            }
            let cleanup = finish_exit(&mut app);
            cleanup.and(Err(error))
        }
        Ok(()) => Ok(()),
    }
}

fn finish_exit(app: &mut App) -> Result<(), Box<dyn Error>> {
    while app.playback.is_some() || app.worker.is_some() {
        app.poll_playback();
        app.poll_import()?;
        thread::sleep(Duration::from_millis(10));
    }
    app.exit_error.take().map_or(Ok(()), |e| Err(e.into()))
}

fn run_loop(app: &mut App, terminal: &mut UiTerminal) -> Result<(), Box<dyn Error>> {
    loop {
        if terminal_disconnected()? {
            app.quit();
            return finish_exit(app);
        }
        app.poll_import()?;
        app.poll_playback();
        app.poll_refresh();
        if app.quitting && app.worker.is_none() && app.playback.is_none() {
            return app.exit_error.take().map_or(Ok(()), |e| Err(e.into()));
        }
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(100))? {
            app.event(event::read()?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};
    use tempfile::TempDir;

    fn wait(app: &mut App) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.worker.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "background import did not finish"
            );
            app.poll_import().unwrap();
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn type_command(app: &mut App, command: &str) {
        app.key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        for character in command.chars() {
            app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|row| {
                let mut text = String::new();
                let mut cells = row.iter();
                while let Some(cell) = cells.next() {
                    text.push_str(cell.symbol());
                    for _ in 1..Line::raw(cell.symbol()).width() {
                        cells.next();
                    }
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn navigation_app(path: PathBuf, count: usize) -> App {
        let mut app = App::new(path).unwrap();
        app.tracks = (1..=count)
            .map(|number| Track {
                id: i64::try_from(number + 100).unwrap(),
                title: format!("Track {number:03}"),
                artist: Some("Artist".into()),
                album: None,
                format: "mp3".into(),
                duration_ms: None,
                netease_id: None,
                content_hash: format!("{number:064x}"),
            })
            .collect();
        app.selection.select((count > 0).then_some(0));
        app
    }

    fn normal_keys(app: &mut App, keys: &str) {
        for character in keys.chars() {
            app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    fn wheel(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
        app.event(Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    #[test]
    fn song_search_previews_metadata_and_repeats_without_playback() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.tracks[22].artist = Some("MOON ARTIST".into());
        app.tracks[40].album = Some("Moon Album".into());
        app.tracks[60].title = "moonlight".into();
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "/moon");
        assert_eq!(app.selection.selected(), Some(22));
        let preview = screen(&mut app, 100, 24);
        assert!(preview.contains("全部本地 | /moon | 1/3 matches"));
        assert!(preview.contains("› 23 Track 023"));
        assert!(preview.lines().last().unwrap().starts_with(" COMMAND "));
        assert!(preview.lines().last().unwrap().contains("/moon"));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(22));
        assert!(
            screen(&mut app, 100, 24)
                .lines()
                .last()
                .unwrap()
                .starts_with(" NORMAL ")
        );
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(40));
        assert!(screen(&mut app, 100, 24).contains("› 41 Track 041"));
        app.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::SHIFT));
        assert_eq!(app.selection.selected(), Some(22));
        normal_keys(&mut app, "3n");
        assert_eq!(app.selection.selected(), Some(22));
        assert_eq!(app.tracks.len(), 80);
        assert!(app.worker.is_none());
        assert!(app.playback_view.pending.is_none());
    }

    #[test]
    fn song_search_cancellation_restores_camera_focus_and_previous_query() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25).map(|row| format!("Result {row:03}")).collect();
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "/Track 001");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        normal_keys(&mut app, "23G");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "G");
        screen(&mut app, 100, 24);
        let origin = app.selection;
        let results_scroll = app.details_scroll;
        normal_keys(&mut app, "99g/Track 080");
        assert_eq!(app.selection.selected(), Some(79));
        assert!(!app.details_focused);
        assert!(screen(&mut app, 100, 24).contains("› 80 Track 080"));
        normal_keys(&mut app, "x");
        assert_eq!(app.selection, origin);
        assert!(screen(&mut app, 100, 24).contains("0/0 matches"));
        app.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(79));
        for _ in 0.."Track 080".len() {
            app.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        assert_eq!(app.selection, origin);
        normal_keys(&mut app, "Track 080");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.selection, origin);
        assert!(app.details_focused);
        assert_eq!(app.details_scroll, results_scroll);
        assert_eq!(app.search.as_ref().unwrap().query, "Track 001");
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(0));
        assert!(!app.details_focused);
        assert!(app.navigation_count.is_none());
        assert!(!app.pending_g);
    }

    #[test]
    fn song_search_backward_wraps_and_empty_prompts_repeat_in_the_new_direction() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        for row in [2, 20, 70] {
            app.tracks[row].album = Some("Match".into());
        }
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "40G?match");
        assert_eq!(app.selection.selected(), Some(20));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(2));
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(70));
        normal_keys(&mut app, "N");
        assert_eq!(app.selection.selected(), Some(2));
        normal_keys(&mut app, "2n");
        assert_eq!(app.selection.selected(), Some(20));
        // Oversized NORMAL input saturates; repeating is bounded by match count.
        normal_keys(&mut app, &format!("{}n", "9".repeat(100)));
        assert_eq!(app.selection.selected(), Some(20));
        normal_keys(&mut app, "/");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(70));
        assert!(screen(&mut app, 100, 24).contains("全部本地 | /match | 3/3 matches"));
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(2));
        normal_keys(&mut app, "?");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(70));
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(20));
        normal_keys(&mut app, "21G?match");
        assert_eq!(app.selection.selected(), Some(20)); // Preview includes the origin.
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        normal_keys(&mut app, "gg?match");
        assert_eq!(app.selection.selected(), Some(70));
    }

    #[test]
    fn song_search_edits_unicode_and_treats_punctuation_as_literal_text() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 50);
        app.tracks[30].title = "夜曲 한글 [A.*] İ".into();
        app.tracks[5].title = "夜曲 한글 ABC".into();
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "/夜曲 한글 [a.*]");
        assert_eq!(app.selection.selected(), Some(30));
        for code in [KeyCode::Left, KeyCode::Char('歌'), KeyCode::Backspace] {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        }
        assert_eq!(
            app.search_prompt.as_ref().unwrap().input.text(),
            "夜曲 한글 [a.*]"
        );
        for code in [
            KeyCode::Home,
            KeyCode::Delete,
            KeyCode::Char('夜'),
            KeyCode::End,
        ] {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        }
        assert_eq!(
            app.search_prompt.as_ref().unwrap().input.text(),
            "夜曲 한글 [a.*]"
        );
        assert!(screen(&mut app, 100, 24).contains("/夜曲 한글 [a.*]"));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        normal_keys(&mut app, "/i\u{307}");
        assert_eq!(app.selection.selected(), Some(30));
        assert!(screen(&mut app, 100, 24).contains("1/1 matches"));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        normal_keys(&mut app, "/a.*");
        assert_eq!(app.selection.selected(), Some(30));
        assert!(screen(&mut app, 100, 24).contains("1/1 matches"));
    }

    #[test]
    fn song_search_input_is_isolated_from_commands_navigation_and_playback() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "23G/23nN :import stop q");
        let origin = app.selection;
        for code in [
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::PageDown,
            KeyCode::Down,
        ] {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        }
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        app.key(KeyEvent::new_with_kind(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection, origin);
        assert_eq!(
            app.search_prompt.as_ref().unwrap().input.text(),
            "23nN :import stop q"
        );
        assert!(
            app.search_prompt
                .as_ref()
                .unwrap()
                .input
                .candidates()
                .is_empty()
        );
        assert!(app.command.is_none());
        assert!(app.worker.is_none());
        assert!(!app.quitting);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.message, "Pattern not found: 23nN :import stop q");
        app.poll_playback();
        assert!(app.playback_view.pending.is_none());
        assert!(app.playback_view.current.is_none());
        normal_keys(&mut app, "j");
        assert_eq!(app.selection.selected(), Some(23));
        normal_keys(&mut app, "/");
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quitting);
    }

    #[test]
    fn song_search_is_safe_for_empty_libraries_single_matches_and_small_views() {
        let temporary = TempDir::new().unwrap();
        for count in [0, 1] {
            let mut app = navigation_app(
                temporary.path().join(format!("library-{count}.muse")),
                count,
            );
            if let Some(track) = app.tracks.first_mut() {
                track.artist = None;
            }
            screen(&mut app, 100, 24);
            normal_keys(&mut app, "nN/");
            app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(app.message, "No previous search");
            normal_keys(&mut app, "/Unknown");
            app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(app.message, "Pattern not found: Unknown");
            normal_keys(&mut app, "3nN?");
            app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(app.message, "Pattern not found: Unknown");
            assert_eq!(app.selection.selected(), (count > 0).then_some(0));
            normal_keys(&mut app, "/Track");
            app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            normal_keys(&mut app, &format!("{}nN", "9".repeat(100)));
            assert_eq!(app.selection.selected(), (count > 0).then_some(0));
            normal_keys(&mut app, &format!("/{}", "歌曲 한글 ".repeat(30)));
            for (width, height) in [(16, 4), (10, 3), (1, 1)] {
                screen(&mut app, width, height);
                app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                app.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
            }
            let rendered = screen(&mut app, 100, 24);
            assert!(rendered.lines().last().unwrap().starts_with(" COMMAND "));
            app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert!(
                screen(&mut app, 100, 24)
                    .lines()
                    .last()
                    .unwrap()
                    .starts_with(" NORMAL ")
            );
            assert!(app.worker.is_none());
        }
    }

    #[test]
    fn song_search_highlights_matching_rows_and_keeps_absolute_numbers() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 20);
        app.tracks[2].title = "Moon One".into();
        app.tracks[5].album = Some("Moon Album".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        normal_keys(&mut app, "/moon");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let row_text = |row| {
            buffer
                .content()
                .chunks(100)
                .nth(row)
                .unwrap()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        assert!(row_text(4).contains("›  3 Moon One"));
        assert_eq!(buffer[(6, 4)].bg, Color::Blue);
        assert!(row_text(7).contains("6 Track 006"));
        assert_eq!(buffer[(6, 7)].fg, Color::Yellow);
        assert!(buffer[(6, 7)].modifier.contains(Modifier::BOLD));
        assert_ne!(buffer[(6, 6)].fg, Color::Yellow);
        normal_keys(&mut app, "j");
        assert!(screen(&mut app, 100, 24).contains("全部本地 | /moon | 0/2 matches"));
        normal_keys(&mut app, "n");
        assert!(screen(&mut app, 100, 24).contains("全部本地 | /moon | 2/2 matches"));
        assert_eq!(app.tracks.len(), 20);
    }

    #[test]
    fn song_search_refreshes_after_background_import_and_restores_the_original_track_id() {
        let temporary = TempDir::new().unwrap();
        let database = temporary.path().join("library.muse");
        let source = temporary.path().join("fixture.ncm");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        let mut probe = Library::open(temporary.path().join("probe.muse")).unwrap();
        assert_eq!(probe.import(&source).imported, 1);
        let query = probe.list().unwrap()[0].title.clone();
        drop(probe);

        let first = temporary.path().join("before.flac");
        std::fs::write(
            &first,
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        let origin_source = temporary.path().join("Origin.mp3");
        let mut audio = include_bytes!("../../muse-library/tests/fixtures/silence.mp3").to_vec();
        let mut tag = [0; 128];
        tag[..3].copy_from_slice(b"TAG");
        tag[3..9].copy_from_slice(b"Origin");
        audio.extend_from_slice(&tag);
        std::fs::write(&origin_source, audio).unwrap();
        let mut library = Library::open(&database).unwrap();
        assert_eq!(library.import(&first).imported, 1);
        assert_eq!(library.import(&origin_source).imported, 1);
        let rows = library.list().unwrap();
        let before_id = rows[0].id;
        let origin_id = rows[1].id;
        drop(library);

        let mut app = App::new(database.clone()).unwrap();
        screen(&mut app, 100, 24);
        normal_keys(&mut app, &format!("2G/{query}"));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.message.starts_with("Pattern not found:"));
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        type_command(&mut app, &format!("import \"{}\"", source.display()));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.worker.is_some());
        normal_keys(&mut app, &format!("/{query}"));
        assert!(screen(&mut app, 100, 24).contains("0/0 matches"));
        // A second public library connection removes an earlier row while import
        // runs. Restoring by an old numeric index would select the new song.
        let mut external = Library::open(&database).unwrap();
        assert!(external.remove(before_id).unwrap());
        drop(external);
        wait(&mut app);
        assert_eq!(app.tracks.len(), 2);
        assert_eq!(app.tracks[0].id, origin_id);
        assert_eq!(app.selection.selected(), Some(1));
        assert!(screen(&mut app, 100, 24).contains("1/1 matches"));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(0));
        assert!(app.details_focused);
        normal_keys(&mut app, "n");
        assert_eq!(app.selection.selected(), Some(1));
        assert_eq!(app.tracks[1].title, query);
        assert!(!app.details_focused);
        assert!(screen(&mut app, 100, 24).contains("1/1 matches"));
        assert!(app.playback_view.pending.is_none());
    }

    #[test]
    fn vim_wheel_moves_camera_and_selection_together_without_drift() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "23G");
        let original = screen(&mut app, 100, 24);
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection.selected(), Some(25));
        assert_eq!(app.selection.offset(), 14);
        let scrolled = screen(&mut app, 100, 24);
        assert!(scrolled.contains("Track 015"));
        assert!(scrolled.contains("› 26 Track 026"));
        assert!(!scrolled.contains("Track 014"));

        wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
        assert_eq!(app.selection.selected(), Some(22));
        assert_eq!(app.selection.offset(), 11);
        assert_eq!(screen(&mut app, 100, 24), original);
        for _ in 0..20 {
            wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
            screen(&mut app, 100, 24);
            wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
            assert_eq!(screen(&mut app, 100, 24), original);
        }

        normal_keys(&mut app, "k");
        assert_eq!(app.selection.selected(), Some(21));
        assert_eq!(app.selection.offset(), 11);
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection.selected(), Some(24));
        assert_eq!(app.selection.offset(), 14);
        assert!(screen(&mut app, 100, 24).contains("› 25 Track 025"));
    }

    #[test]
    fn vim_wheel_stops_with_camera_at_edges_and_survives_resizing() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "78G2k");
        assert_eq!(app.selection.selected(), Some(75));
        assert_eq!(app.selection.offset(), 66);
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection.selected(), Some(77));
        assert_eq!(app.selection.offset(), 68);
        for _ in 0..30 {
            wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        }
        assert_eq!(app.selection.selected(), Some(77));
        assert_eq!(app.selection.offset(), 68);
        assert!(screen(&mut app, 100, 24).contains("› 78 Track 078"));
        for _ in 0..30 {
            wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
        }
        assert_eq!(app.selection.selected(), Some(9));
        assert_eq!(app.selection.offset(), 0);
        wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
        assert_eq!(app.selection.selected(), Some(9));
        assert_eq!(app.selection.offset(), 0);

        normal_keys(&mut app, "78G");
        screen(&mut app, 100, 14);
        assert_eq!(app.selection.offset(), 73);
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection.selected(), Some(79));
        assert_eq!(app.selection.offset(), 75);
        let enlarged = screen(&mut app, 100, 34);
        assert_eq!(app.selection.offset(), 62);
        assert!(enlarged.contains("Track 063"));
        wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
        assert_eq!(app.selection.selected(), Some(76));
        assert_eq!(app.selection.offset(), 59);
        assert!(screen(&mut app, 100, 34).contains("› 77 Track 077"));
        screen(&mut app, 1, 1);
        wheel(&mut app, MouseEventKind::ScrollDown, 0, 0);
        assert_eq!(app.selection.selected(), Some(76));
        assert!(screen(&mut app, 100, 24).contains("› 77 Track 077"));

        for count in [0, 1, 5] {
            let mut app =
                navigation_app(temporary.path().join(format!("short-{count}.muse")), count);
            screen(&mut app, 100, 24);
            wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
            wheel(&mut app, MouseEventKind::ScrollUp, 5, 5);
            assert_eq!(app.selection.selected(), (count > 0).then_some(0));
            assert_eq!(app.selection.offset(), 0);
        }
    }

    #[test]
    fn vim_wheel_targets_the_pointer_panel_and_respects_modes() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "23G");
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 17);
        assert_eq!(app.details_scroll.0, 3);
        assert_eq!(app.selection.selected(), Some(22));
        assert_eq!(app.selection.offset(), 11);
        assert!(!app.details_focused);
        assert!(screen(&mut app, 100, 24).contains("Result 004"));
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(app.selection.selected(), Some(25));
        assert_eq!(app.selection.offset(), 14);
        assert_eq!(app.details_scroll.0, 3);
        assert!(app.details_focused);

        type_command(&mut app, "im");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        wheel(&mut app, MouseEventKind::ScrollUp, 5, 17);
        assert_eq!(app.command.as_ref().unwrap().text(), "import ");
        assert_eq!(app.selection.selected(), Some(25));
        assert_eq!(app.selection.offset(), 14);
        assert_eq!(app.details_scroll.0, 3);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "99g");
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        normal_keys(&mut app, "j");
        assert_eq!(app.selection.selected(), Some(29));
        assert_eq!(app.selection.offset(), 18);

        for (kind, column, row) in [
            (MouseEventKind::ScrollDown, 5, 0),
            (MouseEventKind::ScrollUp, 5, 1),
            (MouseEventKind::ScrollDown, 0, 5),
            (MouseEventKind::ScrollDown, 5, 15),
            (MouseEventKind::ScrollDown, 5, 22),
            (MouseEventKind::ScrollDown, 5, 23),
            (MouseEventKind::ScrollDown, 100, 5),
            (MouseEventKind::Moved, 5, 5),
            (MouseEventKind::ScrollLeft, 5, 5),
        ] {
            wheel(&mut app, kind, column, row);
        }
        assert_eq!(app.selection.selected(), Some(29));
        assert_eq!(app.selection.offset(), 18);
        assert_eq!(app.details_scroll, (3, 0));
        assert!(app.worker.is_none());
        assert!(app.playback_view.pending.is_none());
        app.quit();
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 5);
        wheel(&mut app, MouseEventKind::ScrollUp, 5, 17);
        assert_eq!(app.selection.selected(), Some(29));
        assert_eq!(app.selection.offset(), 18);
        assert_eq!(app.details_scroll, (3, 0));
    }

    #[test]
    fn vim_paging_reveals_selection_and_adapts_to_resize() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        let initial = screen(&mut app, 100, 24);
        assert_eq!(
            initial
                .lines()
                .filter(|line| line.contains("Track "))
                .count(),
            12
        );
        for (key, selected, first_visible) in [
            ('d', 6, "Track 001"),
            ('u', 0, "Track 001"),
            ('f', 11, "Track 001"),
            ('b', 0, "Track 001"),
        ] {
            app.key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL));
            assert_eq!(app.selection.selected(), Some(selected), "Ctrl-{key}");
            let rendered = screen(&mut app, 100, 24);
            assert!(
                rendered
                    .lines()
                    .find(|line| line.contains("Track "))
                    .unwrap()
                    .contains(first_visible)
            );
        }
        screen(&mut app, 100, 14);
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(2));
        assert!(
            screen(&mut app, 100, 14)
                .lines()
                .any(|line| line.contains("› ") && line.contains("Track 003"))
        );
        app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(6));
        assert_eq!(app.selection.offset(), 2);
        app.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(2));
        assert_eq!(app.selection.offset(), 2);
    }

    #[test]
    fn vim_selection_and_camera_follow_agent_board_keyboard_rules() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        screen(&mut app, 100, 24);
        normal_keys(&mut app, "3j23G");
        assert_eq!(app.selection.selected(), Some(22));
        assert_eq!(app.selection.offset(), 11);
        let jumped = screen(&mut app, 100, 24);
        assert!(jumped.contains("Track 012"));
        assert!(jumped.contains("› 23 Track 023"));
        assert!(!jumped.contains("Track 011"));
        assert_eq!(app.selection.offset(), 11);

        normal_keys(&mut app, "k15G");
        assert_eq!(app.selection.selected(), Some(14));
        assert_eq!(app.selection.offset(), 11);
        let visible_jump = screen(&mut app, 100, 24);
        assert!(visible_jump.contains("Track 012"));
        assert!(visible_jump.contains("› 15 Track 015"));

        type_command(&mut app, "5");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(4));
        assert_eq!(app.selection.offset(), 4);
        assert!(screen(&mut app, 100, 24).contains("›  5 Track 005"));

        normal_keys(&mut app, "G");
        assert_eq!(app.selection.selected(), Some(79));
        assert_eq!(app.selection.offset(), 68);
        app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(73));
        assert_eq!(app.selection.offset(), 68);
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(79));
        assert_eq!(app.selection.offset(), 68);

        normal_keys(&mut app, "gg23G");
        let before_resize = screen(&mut app, 100, 24);
        assert_eq!(screen(&mut app, 100, 24), before_resize);
        screen(&mut app, 100, 14);
        assert_eq!(app.selection.selected(), Some(22));
        assert_eq!(app.selection.offset(), 18);
        let enlarged = screen(&mut app, 100, 24);
        assert_eq!(app.selection.offset(), 18);
        assert!(enlarged.contains("Track 019"));
        assert!(enlarged.contains("› 23 Track 023"));
    }

    #[test]
    fn vim_page_counts_apply_once_to_the_focused_panel_and_saturate() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        screen(&mut app, 100, 24);

        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(12));
        assert_eq!(app.selection.offset(), 1);
        assert!(screen(&mut app, 100, 24).contains("› 13 Track 013"));
        app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        normal_keys(&mut app, "j");
        assert_eq!(app.selection.selected(), Some(7));
        assert_eq!(app.selection.offset(), 1);

        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(29));
        assert_eq!(app.selection.offset(), 18);
        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(7));
        assert_eq!(app.selection.offset(), 7);
        normal_keys(&mut app, "3");
        app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(40));
        assert_eq!(app.selection.offset(), 29);
        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(18));
        assert_eq!(app.selection.offset(), 18);

        let enormous = "9".repeat(100);
        normal_keys(&mut app, &enormous);
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(79));
        assert_eq!(app.selection.offset(), 68);
        normal_keys(&mut app, &enormous);
        app.key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(0));
        assert_eq!(app.selection.offset(), 0);

        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "3");
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.details_scroll.0, 6);
        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(app.details_scroll.0, 14);
        normal_keys(&mut app, "2");
        app.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.details_scroll.0, 6);
        normal_keys(&mut app, "5");
        app.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(app.details_scroll.0, 4);
        assert_eq!(app.selection.selected(), Some(0));
        assert!(screen(&mut app, 100, 24).contains("Result 005"));
    }

    #[test]
    fn vim_paging_uses_existing_rows_in_short_views() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 5);
        screen(&mut app, 100, 24);
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(2));
        assert_eq!(app.selection.offset(), 0);
        normal_keys(&mut app, "gg");
        app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(4));
        assert_eq!(app.selection.offset(), 0);
        app.key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert_eq!(app.selection.selected(), Some(0));
        assert_eq!(app.selection.offset(), 0);
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("› 1 Track 001"));
        assert!(rendered.contains("Track 005"));
    }

    #[test]
    fn vim_paging_clamps_focused_panels_and_respects_command_mode() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        screen(&mut app, 100, 24);
        for _ in 0..20 {
            app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        assert_eq!(app.selection.selected(), Some(79));
        assert_eq!(app.selection.offset(), 68);
        assert!(screen(&mut app, 100, 24).contains("Track 080"));
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.details_scroll.0, 2);
        assert!(screen(&mut app, 100, 24).contains("Result 003"));
        for _ in 0..20 {
            app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        }
        assert_eq!(app.details_scroll.0, 20);
        assert_eq!(app.selection.selected(), Some(79));
        let bottom = screen(&mut app, 100, 24);
        assert!(bottom.contains("Result 025"));
        assert!(!bottom.contains("Result 020"));
        type_command(&mut app, "import ./downloads");
        app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        app.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "import ./downloads");
        assert_eq!(app.details_scroll.0, 20);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        for _ in 0..20 {
            app.key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        }
        assert_eq!(app.details_scroll.0, 0);
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        for _ in 0..20 {
            app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        }
        assert_eq!(app.selection.selected(), Some(0));
        assert_eq!(app.selection.offset(), 0);
        app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        let mut released = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        released.kind = KeyEventKind::Release;
        app.key(released);
        assert_eq!(app.selection.selected(), Some(0));
        assert!(app.worker.is_none());
        assert!(app.playback_view.pending.is_none());
    }

    #[test]
    fn vim_line_numbers_and_jumps_keep_the_target_visible() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 120);
        assert!(screen(&mut app, 100, 24).contains("›   1 Track 001"));
        normal_keys(&mut app, "23G");
        assert_eq!(app.selection.selected(), Some(22));
        assert!(screen(&mut app, 100, 24).contains("›  23 Track 023"));
        type_command(&mut app, "75");
        assert_eq!(app.selection.selected(), Some(22));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(74));
        assert!(screen(&mut app, 100, 24).contains("›  75 Track 075"));
        normal_keys(&mut app, "G");
        assert_eq!(app.selection.selected(), Some(119));
        assert!(screen(&mut app, 100, 24).contains("› 120 Track 120"));
        normal_keys(&mut app, "gg");
        assert_eq!(app.selection.selected(), Some(0));
        assert!(screen(&mut app, 100, 24).contains("›   1 Track 001"));
        normal_keys(&mut app, "3j2k");
        assert_eq!(app.selection.selected(), Some(1));
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "G");
        assert_eq!(app.details_scroll.0, 20);
        assert_eq!(app.selection.selected(), Some(1));
        normal_keys(&mut app, "gg");
        assert_eq!(app.details_scroll.0, 0);
        normal_keys(&mut app, "12G");
        assert!(!app.details_focused);
        assert_eq!(app.selection.selected(), Some(11));
        assert!(screen(&mut app, 100, 24).contains("›  12 Track 012"));
    }

    #[test]
    fn vim_navigation_resizing_fills_the_last_viewport() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        screen(&mut app, 100, 14);
        normal_keys(&mut app, "G");
        screen(&mut app, 100, 14);
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "G");
        assert_eq!(app.details_scroll.0, 23);
        let enlarged = screen(&mut app, 100, 34);
        assert_eq!(
            enlarged
                .lines()
                .filter(|line| line.contains("Track "))
                .count(),
            18
        );
        assert!(enlarged.contains("Track 063"));
        assert!(enlarged.contains("Track 080"));
        assert!(enlarged.contains("Result 017"));
        assert!(enlarged.contains("Result 025"));
        assert_eq!(app.details_scroll.0, 16);
    }

    #[test]
    fn vim_navigation_prefixes_reset_and_oversized_targets_clamp() {
        let temporary = TempDir::new().unwrap();
        let mut app = navigation_app(temporary.path().join("library.muse"), 80);
        app.details = (1..=25)
            .map(|number| format!("Result {number:03}"))
            .collect();
        screen(&mut app, 100, 24);
        let enormous = "9".repeat(100);
        normal_keys(&mut app, &format!("{enormous}G"));
        assert_eq!(app.selection.selected(), Some(79));
        type_command(&mut app, &enormous);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(79));
        type_command(&mut app, "0");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selection.selected(), Some(0));
        normal_keys(&mut app, "23");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        normal_keys(&mut app, "jgxg");
        assert_eq!(app.selection.selected(), Some(1));
        type_command(&mut app, "23G");
        app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(app.command.as_ref().unwrap().text(), "23G");
        assert_eq!(app.selection.selected(), Some(1));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        normal_keys(&mut app, "j99");
        type_command(&mut app, "im");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "import ");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        normal_keys(&mut app, "j99");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "j");
        assert_eq!(app.details_scroll.0, 1);
        assert_eq!(app.selection.selected(), Some(3));
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        normal_keys(&mut app, "5");
        app.key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::ALT));
        normal_keys(&mut app, "j");
        assert_eq!(app.selection.selected(), Some(4));
        app.key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT));
        assert_eq!(app.selection.selected(), Some(79));
        assert!(app.worker.is_none());
        assert!(app.playback_view.pending.is_none());
    }

    #[test]
    fn vim_navigation_is_safe_for_empty_short_and_tiny_views() {
        let temporary = TempDir::new().unwrap();
        for count in [0, 1, 2] {
            let mut app = navigation_app(
                temporary.path().join(format!("library-{count}.muse")),
                count,
            );
            screen(&mut app, 1, 1);
            for character in ['d', 'u', 'f', 'b'] {
                app.key(KeyEvent::new(
                    KeyCode::Char(character),
                    KeyModifiers::CONTROL,
                ));
            }
            normal_keys(&mut app, "999999999999999999999999Ggg");
            type_command(&mut app, "999999999999999999999999");
            app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert_eq!(app.selection.selected(), count.checked_sub(1));
            app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
            normal_keys(&mut app, "Ggg");
            app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
            assert_eq!(app.details_scroll, (0, 0));
            let rendered = screen(&mut app, 100, 24);
            if count == 0 {
                assert!(rendered.contains("The library is empty"));
            } else {
                assert!(rendered.contains(&format!("Track {count:03}")));
                assert_eq!(app.selection.offset(), 0);
            }
            assert!(rendered.lines().last().unwrap().starts_with(" NORMAL "));
            assert!(app.worker.is_none());
            assert!(app.playback_view.pending.is_none());
        }
    }

    #[test]
    fn completed_unicode_path_imports_and_mode_badge_tracks_submission_and_cancellation() {
        let temporary = TempDir::new().unwrap();
        let source = temporary.path().join("歌曲 source.ncm");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let initial = screen(&mut app, 100, 24);
        assert!(initial.lines().last().unwrap().starts_with(" NORMAL "));
        assert!(!initial.lines().last().unwrap().contains("j/k"));
        type_command(&mut app, "im");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "import ");
        assert!(app.worker.is_none());
        for character in format!(
            "\"{}{}歌\"",
            temporary.path().display(),
            std::path::MAIN_SEPARATOR
        )
        .chars()
        {
            app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let expected = format!("import \"{}\"", source.display());
        assert_eq!(app.command.as_ref().unwrap().text(), expected);
        assert!(!app.details_focused);
        assert!(
            screen(&mut app, 100, 24)
                .lines()
                .last()
                .unwrap()
                .starts_with(" COMMAND ")
        );
        app.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), expected);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.command.is_none());
        assert!(app.worker.is_some());
        assert!(
            screen(&mut app, 100, 24)
                .lines()
                .last()
                .unwrap()
                .starts_with(" NORMAL ")
        );
        wait(&mut app);
        assert_eq!(app.tracks.len(), 1);
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.details_focused);
        type_command(&mut app, "import ignored");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.command.is_none());
        assert!(app.worker.is_none());
        assert!(
            screen(&mut app, 100, 24)
                .lines()
                .last()
                .unwrap()
                .starts_with(" NORMAL ")
        );
    }

    #[test]
    fn command_popup_cycles_without_execution_and_quit_requires_submission() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        type_command(&mut app, "");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("Commands"));
        assert!(!rendered.contains("Paths"));
        assert!(rendered.lines().last().unwrap().starts_with(" COMMAND "));
        for command in ["import", "q", "quit", "stop", "detach", "playlist"] {
            assert!(rendered.contains(command), "{rendered}");
        }
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.command.as_ref().unwrap().text(), "volume");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "import");
        for (width, height) in [(16, 4), (1, 1)] {
            screen(&mut app, width, height);
        }
        assert!(app.worker.is_none());
        assert!(!app.quitting);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.command.is_none());
        assert!(
            screen(&mut app, 100, 24)
                .lines()
                .last()
                .unwrap()
                .starts_with(" NORMAL ")
        );

        type_command(&mut app, "q");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().candidates().len(), 2);
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.command.as_ref().unwrap().text(), "quit");
        assert!(!app.quitting);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.quitting);

        type_command(&mut app, "st");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "stop");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.command.is_none());
        assert!(!app.message.contains("Unknown command"));

        type_command(&mut app, "qu");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command.as_ref().unwrap().text(), "quit");
        assert!(!app.quitting);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.quitting);
        assert!(app.command.is_none());
    }

    #[test]
    fn candidates_render_cycle_and_fit_small_terminals() {
        let temporary = TempDir::new().unwrap();
        for name in [
            "song_a.ncm",
            "song_b.NCM",
            "song_c.MP3",
            "song_d.FLAC",
            "song_ignored.ogg",
        ] {
            std::fs::write(temporary.path().join(name), []).unwrap();
        }
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        type_command(
            &mut app,
            &format!("import {}/so", temporary.path().display()),
        );
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("Paths"));
        assert!(rendered.contains("song_a.ncm"));
        assert!(rendered.contains("song_b.NCM"));
        assert!(rendered.contains("song_c.MP3"));
        assert!(rendered.contains("song_d.FLAC"));
        assert!(!rendered.contains("song_ignored"));
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert!(
            app.command
                .as_ref()
                .unwrap()
                .text()
                .ends_with("song_d.FLAC")
        );
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.command.as_ref().unwrap().text().ends_with("song_a.ncm"));
        for (width, height) in [(16, 4), (1, 1)] {
            screen(&mut app, width, height);
        }
        app.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!screen(&mut app, 100, 24).contains("Paths"));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.details_focused);
    }

    #[test]
    fn empty_screen_guides_import_and_does_not_claim_playback() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("The library is empty"));
        assert!(text.contains(":import"));
        assert!(text.contains("MP3, FLAC or NCM file or folder"));
        assert!(text.contains("Not playing"));
        assert!(text.contains("vol: 100%"));
        assert!(!text.contains("Gain:"));
        assert!(!text.contains("Playing"));
    }

    #[test]
    fn tui_volume_success_flash_keeps_volume_on_the_status_bar() {
        use crate::volume::{VolumeCommand, format_observation};

        let observation = format_observation(50);
        assert_eq!(observation, "Volume: 50%\nGain: 0.2500");
        for (command, flash) in [
            (VolumeCommand::Query, "ok"),
            (VolumeCommand::Set(50), "vol set 50"),
            (VolumeCommand::Inc(5), "vol inc 5"),
            (VolumeCommand::Dec(5), "vol dec 5"),
        ] {
            let (percent, message) = tui_volume_outcome(command, Ok(observation.clone().into()));
            assert_eq!(percent, Some(50));
            assert_eq!(message, flash);
            assert!(!message.contains("Volume:"), "{message}");
            assert!(!message.contains("Gain:"), "{message}");
        }
        let clamped =
            tui_volume_outcome(VolumeCommand::Inc(60), Ok(format_observation(100).into()));
        assert_eq!(clamped, (Some(100), "vol inc 60".into()));
        let error = tui_volume_outcome(
            VolumeCommand::Set(50),
            Err("volume set 999 is out of range; use an integer from 0 to 100".into()),
        );
        assert_eq!(error.0, None);
        assert!(error.1.contains("out of range"), "{}", error.1);

        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let (percent, message) = tui_volume_outcome(VolumeCommand::Set(50), Ok(observation.into()));
        app.playback_view.volume = percent.unwrap();
        app.message = message;
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("vol: 50%"), "{rendered}");
        assert!(rendered.contains("vol set 50"), "{rendered}");
        assert!(!rendered.contains("Volume:"), "{rendered}");
        assert!(!rendered.contains("Gain:"), "{rendered}");
    }

    #[test]
    fn status_bar_shows_the_ui_volume_and_offline_volume_commands_fail() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        app.playback_view.volume = 37;
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("vol: 37%"), "{rendered}");
        assert!(!rendered.contains("Gain:"), "{rendered}");
        app.execute_command("volume");
        assert!(
            app.message.contains("no playback service running"),
            "{}",
            app.message
        );
        app.execute_command("volume set 999");
        assert!(app.message.contains("out of range"), "{}", app.message);
        app.execute_command("volume inc abc");
        assert!(app.message.contains("abc"), "{}", app.message);
        assert_eq!(app.playback_view.volume, 37);
    }

    #[test]
    fn live_progress_keeps_counts_visible_and_modes_independent() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let source = temporary.path().join("歌曲.ncm");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        assert_eq!(app.library.import(&source).imported, 1);
        let progress = Arc::new(Mutex::new(Some(ImportProgress {
            phase: ImportPhase::Importing,
            discovered: 3,
            total: Some(3),
            processed: 1,
            imported: 1,
            duplicates: 0,
            collisions: 0,
            failed: 0,
            attachment_issues: 0,
            scan_issues: 0,
            current: Some(source),
        })));
        let (sender, result) = mpsc::channel();
        app.worker = Some(ImportWorker {
            source: temporary.path().join("long Unicode 音乐 path/".repeat(20)),
            progress: Arc::clone(&progress),
            cancel: Arc::new(AtomicBool::new(false)),
            result,
            thread: None,
        });
        app.message = "Long previous status ".repeat(20);
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("Import progress | Importing"));
        assert!(rendered.contains("Added 1 | Skipped duplicates 0 | Failed 0"));
        assert!(rendered.contains("Current: 歌曲.ncm"), "{rendered}");
        let status = rendered.lines().nth(22).unwrap();
        assert!(status.starts_with("Import 1/3 (33%) | Not playing"));
        assert!(rendered.lines().last().unwrap().starts_with(" NORMAL "));
        app.details = (1..=25)
            .map(|number| format!("Previous result {number}"))
            .collect();
        wheel(&mut app, MouseEventKind::ScrollDown, 5, 17);
        assert_eq!(app.details_scroll, (0, 0));
        assert_eq!(screen(&mut app, 100, 24), rendered);
        type_command(&mut app, "import ignored");
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("Import 1/3 (33%)"));
        assert!(rendered.lines().last().unwrap().starts_with(" COMMAND "));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        for (width, height) in [(16, 4), (1, 1)] {
            screen(&mut app, width, height);
        }
        {
            let mut latest = progress.lock().unwrap();
            let snapshot = latest.as_mut().unwrap();
            snapshot.phase = ImportPhase::Scanning;
            snapshot.total = None;
            snapshot.processed = 0;
            snapshot.imported = 0;
        }
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains("Scanning: 3 audio files found"));
        assert!(
            !rendered.contains("%)"),
            "scanning must not invent an import percentage: {rendered}"
        );
        assert!(rendered.contains("vol: 100%"));
        {
            let mut latest = progress.lock().unwrap();
            let snapshot = latest.as_mut().unwrap();
            snapshot.phase = ImportPhase::Importing;
            snapshot.total = Some(0);
            snapshot.discovered = 0;
        }
        assert!(screen(&mut app, 100, 24).contains("No supported audio files"));
        sender
            .send(Ok(ImportReport {
                imported: 1,
                cancelled: true,
                ..ImportReport::default()
            }))
            .unwrap();
        app.poll_import().unwrap();
        let rendered = screen(&mut app, 100, 24);
        assert!(!rendered.contains("Import progress"));
        assert!(rendered.contains("Import cancelled: added 1, skipped duplicates 0, failed 0"));
        assert!(rendered.lines().last().unwrap().starts_with(" NORMAL "));
        assert_eq!(app.tracks.len(), 1);
    }

    #[test]
    fn import_progress_displays_decomposed_korean_names_as_complete_syllables() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        for (original, displayed) in [
            (
                "EXID - \u{1103}\u{1165}\u{11af}\u{1103}\u{1165}\u{11af}\u{1103}\u{1165}\u{11af}.ncm",
                "EXID - 덜덜덜.ncm",
            ),
            (
                "Trouble Maker - 내일은 없어.ncm",
                "Trouble Maker - 내일은 없어.ncm",
            ),
            ("가\u{11a8}.ncm", "각.ncm"),
            ("Cafe\u{301} - ㄱㅏ.ncm", "Cafe\u{301} - ㄱㅏ.ncm"),
        ] {
            let source = PathBuf::from(original);
            let progress = ImportProgress {
                phase: ImportPhase::Importing,
                discovered: 1,
                total: Some(1),
                processed: 0,
                imported: 0,
                duplicates: 0,
                collisions: 0,
                failed: 0,
                attachment_issues: 0,
                scan_issues: 0,
                current: Some(source.clone()),
            };
            let (_sender, result) = mpsc::channel();
            app.worker = Some(ImportWorker {
                source: source.clone(),
                progress: Arc::new(Mutex::new(Some(progress))),
                cancel: Arc::new(AtomicBool::new(false)),
                result,
                thread: None,
            });
            let rendered = screen(&mut app, 100, 24);
            assert!(
                rendered.contains(&format!("Current: {displayed}")),
                "{rendered}"
            );
            assert!(rendered.contains(&format!("Source: {displayed}")));
            assert_eq!(
                app.worker.as_ref().unwrap().snapshot().unwrap().current,
                Some(source.clone())
            );
            assert_eq!(app.worker.as_ref().unwrap().source, source);
        }
    }

    #[test]
    fn worker_final_progress_agrees_with_real_import_report() {
        let temporary = TempDir::new().unwrap();
        let database = temporary.path().join("library.muse");
        drop(Library::open(&database).unwrap());
        let source = temporary.path().join("sources");
        std::fs::create_dir(&source).unwrap();
        for index in 0..200 {
            std::fs::write(
                source.join(format!("song-{index:03}.ncm")),
                include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
            )
            .unwrap();
            std::fs::write(
                source.join(format!("song-{index:03}.mp3")),
                include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
            )
            .unwrap();
        }
        let worker = ImportWorker::start(database.clone(), source);
        let report = worker
            .result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let progress = worker.snapshot().unwrap();
        assert_eq!(progress.phase, ImportPhase::Completed);
        assert_eq!(progress.total, Some(400));
        assert_eq!(progress.processed, 400);
        assert_eq!(
            (progress.imported, progress.duplicates, progress.failed),
            (1, 399, 0)
        );
        assert_eq!(progress.imported, report.imported);
        assert_eq!(progress.duplicates, report.duplicates);
        assert_eq!(progress.failed, report.failed.len());
        assert_eq!(Library::open(database).unwrap().list().unwrap().len(), 1);
    }

    #[test]
    fn quoted_import_runs_in_background_and_populates_real_library() {
        let temporary = TempDir::new().unwrap();
        let source = temporary.path().join("歌曲 source.ncm");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        app.execute_command(&format!("import \"{}\"", source.display()));
        assert!(app.worker.is_some());
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.details_focused);
        wait(&mut app);
        assert_eq!(app.tracks.len(), 1);
        assert!(app.message.contains("added 1"));
        let source: Box<dyn muse_playback::AudioSource> =
            Box::new(app.library.open_audio(app.tracks[0].id).unwrap());
        drop(app);
        drop(source);
    }

    #[test]
    fn download_command_includes_plain_audio_and_supports_direct_sources() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("downloads");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("歌曲.NCM"),
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        let plain = root.join("plain");
        std::fs::create_dir(&plain).unwrap();
        std::fs::write(
            plain.join("ordinary.mp3"),
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        std::fs::write(
            plain.join("ordinary.flac"),
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        app.execute_command(&format!("import \"{}\"", root.display()));
        wait(&mut app);
        assert_eq!(app.tracks.len(), 2);
        assert_eq!(app.tracks[0].format, "mp3");
        assert!(
            app.message
                .contains("added 2, skipped duplicates 1, failed 0")
        );
        for name in ["ordinary.mp3", "ordinary.flac"] {
            app.execute_command(&format!("import \"{}\"", plain.join(name).display()));
            wait(&mut app);
            assert_eq!(app.tracks.len(), 2);
            assert!(
                app.message
                    .contains("added 0, skipped duplicates 1, failed 0")
            );
        }
        app.execute_command(&format!("import \"{}\"", plain.display()));
        wait(&mut app);
        assert_eq!(app.tracks.len(), 2);
        assert!(
            app.message
                .contains("added 0, skipped duplicates 2, failed 0")
        );
        let unsupported = root.join("notes.txt");
        std::fs::write(&unsupported, "not audio").unwrap();
        app.execute_command(&format!("import \"{}\"", unsupported.display()));
        wait(&mut app);
        assert!(
            app.message
                .contains("added 0, skipped duplicates 0, failed 1")
        );
        assert!(
            app.details
                .iter()
                .any(|line| line.contains("only MP3, FLAC and NCM are supported"))
        );
    }

    #[test]
    fn quit_cancels_worker_and_leaves_only_complete_songs() {
        let temporary = TempDir::new().unwrap();
        let source = temporary.path().join("song.ncm");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/fixture.ncm"),
        )
        .unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        app.execute_command(&format!("import {}", source.display()));
        app.execute_command("q");
        assert!(app.quitting);
        wait(&mut app);
        assert!(app.tracks.len() <= 1);
        if let Some(track) = app.tracks.first() {
            assert_eq!(
                app.library.open_audio(track.id).unwrap().into_inner(),
                include_bytes!("../../muse-library/tests/fixtures/silence.mp3")
            );
        }
    }
    #[test]
    fn group_refresh_preserves_input_searches_alternatives_and_revalidates_chooser() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("library.muse");
        let mut app = App::new(path.clone()).unwrap();
        let mut writer = Library::open(&path).unwrap();
        let first = temp.path().join("first.ncm");
        std::fs::write(
            &first,
            include_bytes!("../../muse-e2e/fixtures/standard.ncm"),
        )
        .unwrap();
        writer.import(&first);
        app.apply_groups(writer.groups().unwrap());
        type_command(&mut app, "import 'unfinished path");
        let command = app.command.as_ref().unwrap().text().to_string();
        let second = temp.path().join("second.mp3");
        std::fs::write(
            &second,
            include_bytes!("../../muse-e2e/fixtures/same-song.mp3"),
        )
        .unwrap();
        writer.import(&second);
        app.apply_groups(writer.groups().unwrap());
        assert_eq!(app.command.as_ref().unwrap().text(), command);
        assert_eq!(app.groups.len(), 1);
        assert_eq!(app.groups[0].candidates.len(), 2);
        app.command = None;
        app.play_selected();
        assert!(app.chooser.is_some());
        for (w, h) in [(100, 24), (12, 4), (1, 1)] {
            screen(&mut app, w, h);
        }
        let third = temp.path().join("third.ncm");
        std::fs::write(&third, include_bytes!("../../muse-e2e/fixtures/flac.ncm")).unwrap();
        writer.import(&third);
        app.apply_groups(writer.groups().unwrap());
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.message.contains("confirm again"));
        assert_eq!(app.chooser.as_ref().unwrap().group.candidates.len(), 3);
        let selected = app.chooser.as_ref().unwrap().group.candidates[0].id;
        writer.remove(selected).unwrap();
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.chooser.is_none());
        assert!(app.message.contains("does not exist"));
        app.apply_groups(writer.groups().unwrap());
        assert_eq!(app.selection.selected(), Some(0));
        // Only an alternative carries this album; the displayed representative stays unchanged.
        app.groups[0].candidates[1].album = Some("Alternate needle".into());
        let query = Search::new("needle", Direction::Forward, &app.search_tracks());
        assert!(query.contains(0));
        assert!(app.playback_view.current.is_none());
    }

    #[test]
    fn repeat_key_cycles_modes_and_lyrics_follow_playback_position() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        assert!(screen(&mut app, 100, 24).contains("sequential"));
        app.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(app.message.contains("list loop"));
        assert!(screen(&mut app, 100, 24).contains("list loop"));
        app.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(screen(&mut app, 100, 24).contains("single loop"));
        app.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(screen(&mut app, 100, 24).contains("sequential"));
        let source = temporary.path().join("song.mp3");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        std::fs::write(
            source.with_extension("lrc"),
            b"[00:00.00]First line\n[00:10.00]Second line\n",
        )
        .unwrap();
        assert_eq!(app.library.import(&source).imported, 1);
        app.reload_browser().unwrap();
        let id = app.tracks[0].id;
        app.playback_view.current = Some(crate::playback::Current { id, generation: 1 });
        app.playback_view.status = Some(muse_playback::PlaybackStatus {
            state: PlaybackState::Playing,
            position: Duration::from_secs(11),
            duration: Some(Duration::from_secs(30)),
        });
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let shown = screen(&mut app, 100, 24);
        assert!(shown.contains("Second line"));
        assert!(!shown.contains("[00:"));
        assert!(!shown.contains("Import results"));
        assert!(app.lyrics_following);
        app.playback_view.status.as_mut().unwrap().position = Duration::from_secs(1);
        let earlier = screen(&mut app, 100, 24);
        assert!(earlier.contains("First line"));
        assert!(!earlier.contains("[00:00]"));
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(!app.lyrics_open);
        app.playback_view.current = None;
        app.lyrics_track = Some(-1);
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let empty = screen(&mut app, 100, 24);
        assert!(empty.contains("暂无歌词"));
        assert!(empty.contains("可放同名 .lrc 后 :import"));
        assert!(!app.message.to_lowercase().contains("error"));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.lyrics_open);
    }

    #[test]
    fn lyric_scroll_pauses_follow_until_l_or_three_seconds() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let source = temporary.path().join("song.mp3");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        let lyrics: String = (0..40)
            .map(|second| format!("[00:{second:02}.00]Line {second}\n"))
            .collect();
        std::fs::write(source.with_extension("lrc"), lyrics).unwrap();
        assert_eq!(app.library.import(&source).imported, 1);
        app.reload_browser().unwrap();
        let id = app.tracks[0].id;
        app.playback_view.current = Some(crate::playback::Current { id, generation: 1 });
        app.playback_view.status = Some(muse_playback::PlaybackStatus {
            state: PlaybackState::Playing,
            position: Duration::from_secs(20),
            duration: Some(Duration::from_secs(60)),
        });
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let shown = screen(&mut app, 80, 24);
        assert!(shown.lines().next().unwrap().contains("0:20 / 1:00"));
        let centered = lyric_row(&shown, "Line 20");
        app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(!app.lyrics_following);
        assert_ne!(lyric_row(&screen(&mut app, 80, 24), "Line 20"), centered);
        app.lyrics_paused_at = Some(
            Instant::now()
                .checked_sub(Duration::from_secs(3))
                .expect("monotonic clock"),
        );
        let resumed = screen(&mut app, 80, 24);
        assert!(app.lyrics_following);
        assert_eq!(lyric_row(&resumed, "Line 20"), centered);
        app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(app.lyrics_open);
        assert!(app.lyrics_following);
        assert_eq!(lyric_row(&screen(&mut app, 80, 24), "Line 20"), centered);
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(!app.lyrics_open);
    }

    #[test]
    fn user_playlist_can_be_created_from_the_library_and_played_as_a_list() {
        let temporary = TempDir::new().unwrap();
        let mut app = App::new(temporary.path().join("library.muse")).unwrap();
        let first = temporary.path().join("first.mp3");
        let second = temporary.path().join("second.flac");
        std::fs::write(
            &first,
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        std::fs::write(
            &second,
            include_bytes!("../../muse-library/tests/fixtures/tone.flac"),
        )
        .unwrap();
        assert_eq!(app.library.import(&first).imported, 1);
        assert_eq!(app.library.import(&second).imported, 1);
        app.reload_browser().unwrap();
        assert_eq!(app.tracks.len(), 2);
        assert!(screen(&mut app, 120, 24).contains("全部本地"));
        app.execute_command("playlist new Night Drive");
        assert!(app.message.contains("Night Drive"));
        let second_id = app.tracks[1].id;
        app.selection.select(Some(1));
        app.execute_command("playlist add Night Drive");
        app.selection.select(Some(0));
        app.execute_command("playlist add Night Drive");
        app.execute_command("playlist use Night Drive");
        assert_eq!(app.tracks.len(), 2);
        assert_eq!(app.tracks[0].id, second_id);
        assert!(screen(&mut app, 120, 24).contains("Night Drive"));
        app.execute_command("playlist library");
        assert!(screen(&mut app, 120, 24).contains("Library"));
    }

    fn lyric_row(screen: &str, text: &str) -> usize {
        screen
            .lines()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("missing lyric {text}"))
    }

    #[test]
    fn lyric_timestamps_highlight_the_current_line() {
        let lines = parse_lrc("[offset:1000]\n[00:01.00][00:05.50]Hold\nplain\n[ti:title]\n");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "plain");
        assert!(lines[0].at.is_none());
        assert_eq!(lines[1].at, Some(Duration::from_millis(2_000)));
        assert_eq!(lines[2].at, Some(Duration::from_millis(6_500)));
    }
}
