// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    command::{BuiltinCommand, CommandLine, CursorMove, expand_path},
    playback::{View as PlaybackView, Worker as PlaybackWorker},
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muse_library::{ImportPhase, ImportProgress, ImportReport, Library, LibraryError, Track};
use muse_playback::{PlaybackState, SeekTarget};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

struct ImportWorker {
    source: PathBuf,
    progress: Arc<Mutex<Option<ImportProgress>>>,
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<ImportReport, String>>,
    thread: Option<JoinHandle<()>>,
}

impl ImportWorker {
    fn start(library: PathBuf, source: PathBuf) -> Self {
        let progress = Arc::new(Mutex::new(None));
        let worker_progress = Arc::clone(&progress);
        let worker_source = source.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, result) = mpsc::channel();
        let thread = thread::spawn(move || {
            let result = Library::open(library)
                .map(|mut library| {
                    library.import_ncm_with_progress(worker_source, &worker_cancel, |snapshot| {
                        *worker_progress
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(snapshot);
                    })
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        Self {
            source,
            progress,
            cancel,
            result,
            thread: Some(thread),
        }
    }

    fn snapshot(&self) -> Option<ImportProgress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for ImportWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct App {
    library: Library,
    library_path: PathBuf,
    tracks: Vec<Track>,
    selection: ListState,
    command: Option<CommandLine>,
    message: String,
    details: Vec<String>,
    details_focused: bool,
    details_scroll: (u16, u16),
    worker: Option<ImportWorker>,
    playback: Option<PlaybackWorker>,
    playback_view: PlaybackView,
    quitting: bool,
}

impl App {
    fn new(path: PathBuf) -> Result<Self, LibraryError> {
        let library = Library::open(&path)?;
        let tracks = library.list()?;
        let mut selection = ListState::default();
        if !tracks.is_empty() {
            selection.select(Some(0));
        }
        Ok(Self {
            library,
            library_path: path.clone(),
            tracks,
            selection,
            command: None,
            message:
                "Use :import <NCM file or folder> to import music; Enter plays the selected track"
                    .into(),
            details: Vec::new(),
            details_focused: false,
            details_scroll: (0, 0),
            worker: None,
            playback: Some(PlaybackWorker::start(path)),
            playback_view: PlaybackView::default(),
            quitting: false,
        })
    }

    fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release || self.quitting {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit();
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
        match key.code {
            KeyCode::Char(':') => self.command = Some(CommandLine::default()),
            KeyCode::Tab => {
                self.details_focused = !self.details_focused;
            }
            KeyCode::Char('j') | KeyCode::Down if self.details_focused => {
                let maximum =
                    u16::try_from(self.details.len().saturating_sub(1)).unwrap_or(u16::MAX);
                self.details_scroll.0 = self.details_scroll.0.saturating_add(1).min(maximum);
            }
            KeyCode::Char('k') | KeyCode::Up if self.details_focused => {
                self.details_scroll.0 = self.details_scroll.0.saturating_sub(1);
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
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
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

    fn play_selected(&mut self) {
        let Some(track) = self
            .selection
            .selected()
            .and_then(|index| self.tracks.get(index))
        else {
            self.message = "The library is empty; import music with :import first".into();
            return;
        };
        if let Some(playback) = &self.playback {
            match playback.load(track.id) {
                Ok(()) => self.message = format!("Playback requested: {}", track.title),
                Err(error) => self.message = error,
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
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.trim();
        let rest = match BuiltinCommand::parse(input) {
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
            self.message = "Usage: :import <NCM file or folder path>".into();
            return;
        }
        let source = expand_path(rest);
        self.message = "Import started".into();
        self.worker = Some(ImportWorker::start(self.library_path.clone(), source));
    }

    fn quit(&mut self) {
        self.quitting = true;
        // Release playback before waiting for the current import to finish.
        self.playback.take();
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
        let selected = self
            .selection
            .selected()
            .and_then(|index| self.tracks.get(index))
            .map(|track| track.id);
        self.tracks = self.library.list()?;
        self.selection.select(if self.tracks.is_empty() {
            None
        } else {
            Some(
                selected
                    .and_then(|id| self.tracks.iter().position(|track| track.id == id))
                    .unwrap_or(0),
            )
        });
        Ok(())
    }

    fn show_report(&mut self, report: ImportReport) {
        self.message = format!(
            "Import {}: added {}, skipped duplicates {}, failed {}, attachment/metadata issues {}, scan issues {}",
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
            report.scan_issues.len()
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
        frame.render_widget(
            Paragraph::new(format!(
                "muse | {} | {} tracks",
                self.library_path.display(),
                self.tracks.len()
            ))
            .style(Style::default().fg(Color::Cyan)),
            title,
        );
        let [songs, results] =
            Layout::vertical([Constraint::Percentage(65), Constraint::Percentage(35)]).areas(body);
        let songs_block = Block::default()
            .borders(Borders::ALL)
            .title("Library")
            .border_style(Style::default().fg(if self.details_focused {
                Color::DarkGray
            } else {
                Color::Cyan
            }));
        if self.tracks.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "The library is empty\nUse :import <NCM file or folder path> to import music",
                )
                .block(songs_block),
                songs,
            );
        } else {
            let items: Vec<ListItem> = self
                .tracks
                .iter()
                .map(|track| {
                    ListItem::new(Line::from(format!(
                        "{}{} — {} [{}]",
                        if self
                            .playback_view
                            .current
                            .is_some_and(|current| current.id == track.id)
                        {
                            "♪ "
                        } else {
                            ""
                        },
                        track.title,
                        track.artist.as_deref().unwrap_or("Unknown"),
                        track.format
                    )))
                })
                .collect();
            frame.render_stateful_widget(
                List::new(items)
                    .block(songs_block)
                    .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                    .highlight_symbol("› "),
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
            "No NCM files".to_owned()
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
                    "Added {} | Skipped duplicates {} | Failed {}",
                    progress.imported, progress.duplicates, progress.failed
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
        let (label, background, foreground) = if self.command.is_some() {
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
        let Some(input) = &self.command else { return };
        let prompt = format!(":{}", input.text());
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
        if let Some(error) = &self.playback_view.error {
            return format!("Playback error: {error}");
        }
        if self.playback_view.pending.is_some() {
            return "Loading track...".into();
        }
        let Some(status) = self.playback_view.status else {
            return "Not playing".into();
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
            .and_then(|current| self.tracks.iter().find(|track| track.id == current.id))
            .map(|track| track.title.as_str())
            .unwrap_or("");
        format!("{state} {title} {position}/{total}")
    }
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
        return format!("Scanning: {} NCM files found", progress.discovered);
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

impl Drop for App {
    fn drop(&mut self) {
        self.playback.take();
    }
}

pub fn run(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let mut app = App::new(path)?;
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init()?;
    loop {
        app.poll_import()?;
        app.poll_playback();
        if app.quitting && app.worker.is_none() {
            return Ok(());
        }
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            app.key(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
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
        for command in ["import", "q", "quit", "stop"] {
            assert!(rendered.contains(command));
        }
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.command.as_ref().unwrap().text(), "stop");
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
            "song_ignored.mp3",
            "song_ignored.flac",
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
        assert!(!rendered.contains("song_ignored"));
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert!(app.command.as_ref().unwrap().text().ends_with("song_b.NCM"));
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
        assert!(text.contains("NCM file or folder"));
        assert!(text.contains("Not playing"));
        assert!(!text.contains("Playing"));
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
        assert!(rendered.contains("Scanning: 3 NCM files found"));
        assert!(!rendered.contains('%'));
        {
            let mut latest = progress.lock().unwrap();
            let snapshot = latest.as_mut().unwrap();
            snapshot.phase = ImportPhase::Importing;
            snapshot.total = Some(0);
            snapshot.discovered = 0;
        }
        assert!(screen(&mut app, 100, 24).contains("No NCM files"));
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
        assert_eq!(progress.total, Some(200));
        assert_eq!(progress.processed, 200);
        assert_eq!(
            (progress.imported, progress.duplicates, progress.failed),
            (1, 199, 0)
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
    fn ncm_only_command_ignores_plain_folder_audio_and_rejects_direct_plain_sources() {
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
        assert_eq!(app.tracks.len(), 1);
        assert_eq!(app.tracks[0].format, "mp3");
        assert!(
            app.message
                .contains("added 1, skipped duplicates 0, failed 0")
        );
        for name in ["ordinary.mp3", "ordinary.flac"] {
            app.execute_command(&format!("import \"{}\"", plain.join(name).display()));
            wait(&mut app);
            assert_eq!(app.tracks.len(), 1);
            assert!(
                app.message
                    .contains("added 0, skipped duplicates 0, failed 1")
            );
            assert!(
                app.details
                    .iter()
                    .any(|detail| detail.contains("only NCM source files can be imported"))
            );
        }
        app.execute_command(&format!("import \"{}\"", plain.display()));
        wait(&mut app);
        assert_eq!(app.tracks.len(), 1);
        assert!(
            app.message
                .contains("added 0, skipped duplicates 0, failed 0")
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
}
