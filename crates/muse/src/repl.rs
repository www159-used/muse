// SPDX-License-Identifier: AGPL-3.0-only
use crate::{
    command::{CommandLine, CursorMove, expand_path},
    import_job::ImportWorker,
    operations, service,
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
use muse_library::{Library, SongGroup};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::Path,
    time::Duration,
};

struct Raw;
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn split(input: &str) -> Result<Vec<OsString>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for ch in input.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                word.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            started = true;
        } else if ch.is_whitespace() {
            if started {
                words.push(OsString::from(&word));
                word.clear();
                started = false;
            }
        } else {
            word.push(ch);
            started = true;
        }
    }
    if quote.is_some() {
        return Err("unclosed quote".into());
    }
    if started {
        words.push(word.into());
    }
    Ok(words)
}

fn output(
    action: impl FnOnce() -> Result<operations::CommandOutput, String>,
) -> Result<(), String> {
    let result = action();
    let lines = match result {
        Ok(output) => [
            output.text,
            output.diagnostics,
            output
                .failure
                .map(|e| format!("muse: {e}"))
                .unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n"),
        Err(e) => format!("muse: {e}"),
    };
    if !lines.is_empty() {
        write!(io::stdout(), "{}\r\n", lines.replace('\n', "\r\n")).map_err(|e| e.to_string())?;
    }
    io::stdout().flush().map_err(|e| e.to_string())
}

pub fn run(path: &Path) -> Result<(), String> {
    let library = Library::open(path).map_err(|e| e.to_string())?;
    let descriptor = library.descriptor().map_err(|e| e.to_string())?;
    println!(
        "{} [netease] command mode. Type help; quit stops, detach keeps playback.",
        descriptor.name
    );
    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let _raw = Raw;
    let mut line = CommandLine::for_repl();
    let mut history = Vec::<String>::new();
    let mut history_index = 0;
    let mut choosing: Option<SongGroup> = None;
    let mut worker: Option<ImportWorker> = None;
    let mut redraw = true;
    let outcome = (|| -> Result<bool, String> {
        loop {
            if crate::ui::terminal_disconnected().map_err(|e| e.to_string())? {
                return Ok(false);
            }
            if let Some(job) = &worker {
                match job.result.try_recv() {
                    Ok(result) => {
                        worker.take();
                        output(|| {
                            write!(io::stdout(), "\r\n").map_err(|e| e.to_string())?;
                            Ok(operations::report_import(result?))
                        })?;
                        redraw = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        worker.take();
                        output(|| Err("import worker stopped".into()))?;
                        redraw = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if redraw {
                let prompt = if choosing.is_some() {
                    "candidate number (Esc cancels)> ".to_string()
                } else {
                    format!("{} [netease]> ", descriptor.name)
                };
                crossterm::execute!(
                    io::stdout(),
                    cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::CurrentLine)
                )
                .map_err(|e| e.to_string())?;
                write!(io::stdout(), "{prompt}{}", line.text()).map_err(|e| e.to_string())?;
                io::stdout().flush().map_err(|e| e.to_string())?;
                // Let the terminal account for wide/combining Unicode by printing the prefix.
                write!(io::stdout(), "\r{prompt}{}", &line.text()[..line.cursor()])
                    .map_err(|e| e.to_string())?;
                io::stdout().flush().map_err(|e| e.to_string())?;
                redraw = false;
            }
            if !event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
                continue;
            }
            let Event::Key(key) = event::read().map_err(|e| e.to_string())? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            redraw = true;
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && (key.code == KeyCode::Char('c')
                    || (key.code == KeyCode::Char('d') && line.text().is_empty()))
            {
                return Ok(false);
            }
            match key.code {
                KeyCode::Esc => {
                    choosing = None;
                    line.replace("");
                }
                KeyCode::Enter => {
                    let text = line.text().trim().to_string();
                    line.replace("");
                    write!(io::stdout(), "\r\n").map_err(|e| e.to_string())?;
                    if let Some(group) = choosing.take() {
                        if text.is_empty() {
                            continue;
                        }
                        let Some(index) = text
                            .parse::<usize>()
                            .ok()
                            .and_then(|n| n.checked_sub(1))
                            .filter(|n| *n < group.candidates.len())
                        else {
                            output(|| Err("choose a listed number or Esc".into()))?;
                            choosing = Some(group);
                            continue;
                        };
                        match operations::group(path, &group.token) {
                            Ok(current) if current == group => {
                                output(|| {
                                    operations::execute(
                                        path,
                                        &[
                                            "play".into(),
                                            group.candidates[index].id.to_string().into(),
                                        ],
                                    )
                                })?;
                            }
                            Ok(current) => {
                                output(|| {
                                    let mut choices = print_choices(&library, &current)?;
                                    choices.text = format!(
                                        "Candidates changed; select again.\n{}",
                                        choices.text
                                    );
                                    Ok(choices)
                                })?;
                                choosing = Some(current);
                            }
                            Err(e) => {
                                output(|| Err(e))?;
                            }
                        }
                        continue;
                    }
                    if text.is_empty() {
                        continue;
                    }
                    history.push(text.clone());
                    history_index = history.len();
                    let args = match split(&text) {
                        Ok(args) => args,
                        Err(e) => {
                            output(|| Err(e))?;
                            continue;
                        }
                    };
                    if args.len() == 1 && (args[0] == "quit" || args[0] == "q") {
                        return Ok(false);
                    }
                    if args.len() == 1 && args[0] == "detach" {
                        return Ok(true);
                    }
                    if args[0] == "import" && args.len() == 2 {
                        if worker.is_some() {
                            output(|| Err("an import is already running".into()))?;
                        } else {
                            worker = Some(ImportWorker::start(
                                path.to_owned(),
                                expand_path(&args[1].to_string_lossy()),
                            ));
                            output(|| Ok("Import started".to_string().into()))?;
                        }
                    } else if args[0] == "play"
                        && args.len() == 2
                        && args[1].to_string_lossy().starts_with("g:")
                    {
                        match operations::group(path, &args[1].to_string_lossy()) {
                            Ok(group) if group.candidates.len() > 1 => {
                                output(|| print_choices(&library, &group))?;
                                choosing = Some(group);
                            }
                            _ => output(|| operations::execute(path, &args))?,
                        }
                    } else {
                        output(|| operations::execute(path, &args))?;
                    }
                }
                KeyCode::Up if choosing.is_none() => {
                    history_index = history_index.saturating_sub(1);
                    if let Some(text) = history.get(history_index) {
                        line.replace(text);
                    }
                }
                KeyCode::Down if choosing.is_none() => {
                    history_index = (history_index + 1).min(history.len());
                    line.replace(history.get(history_index).map_or("", String::as_str));
                }
                KeyCode::Backspace => line.backspace(),
                KeyCode::Delete => line.delete(),
                KeyCode::Left => line.move_cursor(CursorMove::Left),
                KeyCode::Right => line.move_cursor(CursorMove::Right),
                KeyCode::Home => line.move_cursor(CursorMove::Start),
                KeyCode::End => line.move_cursor(CursorMove::End),
                KeyCode::Tab | KeyCode::BackTab if choosing.is_none() => {
                    if let Err(e) = line.complete(key.code == KeyCode::BackTab) {
                        output(|| Err(e.to_string()))?;
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    line.insert(ch)
                }
                _ => {}
            }
        }
    })();
    drop(worker); // Cancel at a file boundary and join before returning.
    let stopped = if !matches!(outcome, Ok(true)) {
        service::control_text(path, service::Command::Stop)
    } else {
        Ok(String::new())
    };
    let restored = terminal::disable_raw_mode().map_err(|e| e.to_string());
    if let Ok(text) = &stopped {
        let _ = writeln!(io::stdout(), "\n{text}");
    }
    stopped?;
    restored?;
    if crate::ui::terminal_disconnected().unwrap_or(false) {
        Ok(())
    } else {
        outcome.map(|_| ())
    }
}

fn print_choices(
    library: &Library,
    group: &SongGroup,
) -> Result<operations::CommandOutput, String> {
    Ok(operations::candidate_labels(library, group)?
        .iter()
        .enumerate()
        .map(|(i, label)| format!("{}. {}", i + 1, label))
        .collect::<Vec<_>>()
        .join("\n")
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_unicode_is_literal() {
        assert_eq!(
            split("import '/音乐/my files/$(echo x).ncm'").unwrap(),
            vec![
                OsString::from("import"),
                OsString::from("/音乐/my files/$(echo x).ncm")
            ]
        );
        assert!(split("show 'x").is_err());
    }
}
