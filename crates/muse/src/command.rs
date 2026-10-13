// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs, io,
    path::{MAIN_SEPARATOR, Path, PathBuf},
};

pub enum CursorMove {
    Left,
    Right,
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinCommand {
    Import,
    Detach,
    Quit,
    Stop,
    Playlist,
    NextOnce,
    Volume,
}

const COMMANDS: &[(&str, BuiltinCommand)] = &[
    ("import", BuiltinCommand::Import),
    ("q", BuiltinCommand::Quit),
    ("quit", BuiltinCommand::Quit),
    ("stop", BuiltinCommand::Stop),
    ("detach", BuiltinCommand::Detach),
    ("next", BuiltinCommand::NextOnce),
    ("playlist", BuiltinCommand::Playlist),
    ("volume", BuiltinCommand::Volume),
];

impl BuiltinCommand {
    pub fn parse(input: &str) -> Option<(Self, &str)> {
        let input = input.trim();
        let split = input.find(char::is_whitespace).unwrap_or(input.len());
        let (name, rest) = input.split_at(split);
        let rest = rest.trim();
        let command = COMMANDS
            .iter()
            .find(|(registered, _)| *registered == name)?
            .1;
        (matches!(command, Self::Import | Self::Playlist | Self::Volume) || rest.is_empty())
            .then_some((command, rest))
    }
}

#[derive(Default)]
pub struct CommandLine {
    text: String,
    cursor: usize,
    completion: Option<Completion>,
    repl: bool,
}

pub struct Candidate {
    pub label: String,
    value: String,
    directory: bool,
}

struct Completion {
    target: CompletionTarget,
    candidates: Vec<Candidate>,
    selected: Option<usize>,
}

enum CompletionTarget {
    Commands { prefix: String },
    Paths(PathArgument),
}

struct PathArgument {
    prefix: String,
    path: String,
    quote: Option<char>,
}

impl CommandLine {
    pub fn for_repl() -> Self {
        Self {
            repl: true,
            ..Self::default()
        }
    }
    pub fn replace(&mut self, text: &str) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.completion = None;
    }
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn candidates(&self) -> &[Candidate] {
        self.completion
            .as_ref()
            .map_or(&[], |completion| &completion.candidates)
    }

    pub fn selected(&self) -> Option<usize> {
        self.completion
            .as_ref()
            .and_then(|completion| completion.selected)
    }

    pub fn completion_title(&self) -> &'static str {
        match self
            .completion
            .as_ref()
            .map(|completion| &completion.target)
        {
            Some(CompletionTarget::Commands { .. }) => "Commands",
            _ => "Paths",
        }
    }

    pub fn insert(&mut self, character: char) {
        self.completion = None;
        self.text.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    pub fn backspace(&mut self) {
        self.completion = None;
        if let Some((previous, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.text.drain(previous..self.cursor);
            self.cursor = previous;
        }
    }

    pub fn delete(&mut self) {
        self.completion = None;
        if let Some(character) = self.text[self.cursor..].chars().next() {
            self.text
                .drain(self.cursor..self.cursor + character.len_utf8());
        }
    }

    pub fn move_cursor(&mut self, direction: CursorMove) {
        self.completion = None;
        self.cursor = match direction {
            CursorMove::Left => self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index),
            CursorMove::Right => self.text[self.cursor..]
                .chars()
                .next()
                .map_or(self.cursor, |character| self.cursor + character.len_utf8()),
            CursorMove::Start => 0,
            CursorMove::End => self.text.len(),
        };
    }

    pub fn complete(&mut self, reverse: bool) -> io::Result<()> {
        self.complete_using(reverse, |path| {
            find_candidates(path, &std::env::current_dir()?, home_dir().as_deref())
        })
    }

    #[cfg(test)]
    fn complete_in(&mut self, cwd: &Path, home: Option<&Path>, reverse: bool) -> io::Result<()> {
        self.complete_using(reverse, |path| find_candidates(path, cwd, home))
    }

    fn complete_using(
        &mut self,
        reverse: bool,
        paths: impl FnOnce(&str) -> io::Result<Vec<Candidate>>,
    ) -> io::Result<()> {
        if let Some(mut completion) = self.completion.take() {
            let count = completion.candidates.len();
            let selected = match (completion.selected, reverse) {
                (None, false) => 0,
                (None, true) => count - 1,
                (Some(0), true) => count - 1,
                (Some(index), true) => index - 1,
                (Some(index), false) => (index + 1) % count,
            };
            self.apply(&completion.target, &completion.candidates[selected].value);
            completion.selected = Some(selected);
            self.completion = Some(completion);
            return Ok(());
        }
        let (target, candidates) = if let Some(argument) = import_argument(&self.text, self.cursor)
        {
            let candidates = paths(&argument.path)?;
            (CompletionTarget::Paths(argument), candidates)
        } else if self.cursor == self.text.len()
            && !self.text.trim_start().contains(char::is_whitespace)
        {
            let name = self.text.trim_start();
            let prefix = self.text[..self.text.len() - name.len()].to_owned();
            let commands: Vec<&str> = if self.repl {
                vec![
                    "import", "list", "show", "play", "next", "pause", "stop", "status",
                    "shutdown", "mode", "playlist", "library", "help", "q", "quit", "detach",
                    "volume",
                ]
            } else {
                COMMANDS.iter().map(|(name, _)| *name).collect()
            };
            let candidates = commands
                .iter()
                .filter(|command| command.starts_with(name))
                .map(|command| Candidate {
                    label: (*command).to_owned(),
                    value: (*command).to_owned(),
                    directory: false,
                })
                .collect();
            (CompletionTarget::Commands { prefix }, candidates)
        } else {
            return Ok(());
        };
        match candidates.len() {
            0 => {}
            1 => {
                self.apply(&target, &candidates[0].value);
                if matches!(target, CompletionTarget::Commands { .. })
                    && matches!(
                        BuiltinCommand::parse(self.text()).map(|(command, _)| command),
                        Some(BuiltinCommand::Import | BuiltinCommand::Playlist)
                    )
                {
                    self.insert(' ');
                }
            }
            count => {
                let selected = reverse.then_some(count - 1);
                let value = if reverse {
                    candidates[count - 1].value.clone()
                } else {
                    common_prefix(&candidates)
                };
                self.apply(&target, &value);
                self.completion = Some(Completion {
                    target,
                    candidates,
                    selected,
                });
            }
        }
        Ok(())
    }

    fn apply(&mut self, target: &CompletionTarget, value: &str) {
        let argument = match target {
            CompletionTarget::Commands { prefix } => {
                self.text = format!("{prefix}{value}");
                self.cursor = self.text.len();
                return;
            }
            CompletionTarget::Paths(argument) => argument,
        };
        let path = value;
        let quote = argument.quote.or_else(|| {
            (path.contains(char::is_whitespace) || path.starts_with(['\'', '"'])).then_some('"')
        });
        self.text = match quote {
            Some(quote) => format!("{}{quote}{path}{quote}", argument.prefix),
            None => format!("{}{path}", argument.prefix),
        };
        self.cursor = self.text.len() - quote.map_or(0, char::len_utf8);
    }
}

fn import_argument(input: &str, cursor: usize) -> Option<PathArgument> {
    let rest = input.trim_start().strip_prefix("import")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let raw = rest.trim_start();
    let mut prefix = input[..input.len() - raw.len()].to_owned();
    if rest.is_empty() {
        prefix.push(' ');
    }
    let quote = raw
        .chars()
        .next()
        .filter(|character| matches!(character, '\'' | '"'));
    let path = if let Some(quote) = quote {
        let inner = &raw[quote.len_utf8()..];
        inner.strip_suffix(quote).unwrap_or(inner)
    } else {
        raw
    };
    let path_end = input.len()
        - if quote.is_some_and(|quote| raw.len() > 1 && raw.ends_with(quote)) {
            1
        } else {
            0
        };
    if cursor != input.len() && cursor != path_end {
        return None;
    }
    Some(PathArgument {
        prefix,
        path: path.to_owned(),
        quote,
    })
}

fn find_candidates(path: &str, cwd: &Path, home: Option<&Path>) -> io::Result<Vec<Candidate>> {
    let separator = path
        .chars()
        .rev()
        .find(|&character| std::path::is_separator(character))
        .unwrap_or(MAIN_SEPARATOR);
    let path = if matches!(path, "." | "..") || (path == "~" && home.is_some()) {
        format!("{path}{separator}")
    } else {
        path.to_owned()
    };
    let split = path
        .rfind(std::path::is_separator)
        .map_or(0, |index| index + 1);
    let (parent, prefix) = path.split_at(split);
    let parent_path = expand_with_home(parent, home);
    let directory = if parent_path.is_absolute() {
        parent_path
    } else {
        cwd.join(parent_path)
    };
    let mut candidates = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
            continue;
        }
        let Ok(metadata) = entry.path().metadata() else {
            continue;
        };
        let directory = metadata.is_dir();
        let audio = Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["ncm", "mp3", "flac"]
                    .iter()
                    .any(|format| extension.eq_ignore_ascii_case(format))
            });
        if !(directory || metadata.is_file() && audio) {
            continue;
        }
        let suffix = if directory {
            separator.to_string()
        } else {
            String::new()
        };
        candidates.push(Candidate {
            label: format!("{name}{suffix}"),
            value: format!("{parent}{name}{suffix}"),
            directory,
        });
    }
    candidates.sort_by(|left, right| {
        right
            .directory
            .cmp(&left.directory)
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(candidates)
}

fn common_prefix(candidates: &[Candidate]) -> String {
    let mut prefix = candidates[0].value.clone();
    for candidate in &candidates[1..] {
        prefix = prefix
            .chars()
            .zip(candidate.value.chars())
            .take_while(|(left, right)| left == right)
            .map(|(character, _)| character)
            .collect();
    }
    prefix
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expand_with_home(path: &str, home: Option<&Path>) -> PathBuf {
    if let Some(home) = home {
        if path == "~" {
            return home.to_owned();
        }
        if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

pub fn expand_path(path: &str) -> PathBuf {
    expand_with_home(path, home_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn line(text: &str) -> CommandLine {
        CommandLine {
            text: text.to_owned(),
            cursor: text.len(),
            completion: None,
            repl: false,
        }
    }

    #[test]
    fn unique_names_complete_without_scanning_and_import_hands_off_to_paths() {
        for (text, expected) in [
            ("im", "import "),
            ("  im", "  import "),
            ("st", "stop"),
            ("qu", "quit"),
            ("quit", "quit"),
        ] {
            for reverse in [false, true] {
                let mut input = line(text);
                input
                    .complete_using(reverse, |_| panic!("command names must not scan paths"))
                    .unwrap();
                assert_eq!(input.text(), expected);
                assert_eq!(input.cursor(), expected.len());
                assert!(input.candidates().is_empty());
                assert!(BuiltinCommand::parse(input.text()).is_some());
            }
        }
        let temporary = TempDir::new().unwrap();
        fs::write(temporary.path().join("track.NCM"), []).unwrap();
        fs::write(temporary.path().join("track.mp3"), []).unwrap();
        let mut input = line("im");
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(input.text(), "import ");
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(input.text(), "import track.");
        assert_eq!(input.candidates().len(), 2);
    }

    #[test]
    fn command_candidates_cycle_aliases_and_invalidate_after_edits() {
        let mut input = line("  ");
        let complete = |input: &mut CommandLine, reverse| {
            input
                .complete_using(reverse, |_| panic!("command cycling must not scan paths"))
                .unwrap();
        };
        complete(&mut input, false);
        assert_eq!(input.text(), "  ");
        assert_eq!(input.completion_title(), "Commands");
        assert_eq!(
            input
                .candidates()
                .iter()
                .map(|candidate| candidate.label.as_str())
                .collect::<Vec<_>>(),
            [
                "import", "q", "quit", "stop", "detach", "next", "playlist", "volume",
            ]
        );
        assert_eq!(input.selected(), None);
        for (reverse, expected) in [
            (false, "import"),
            (false, "q"),
            (true, "import"),
            (true, "volume"),
            (false, "import"),
        ] {
            complete(&mut input, reverse);
            assert_eq!(input.text(), format!("  {expected}"));
            assert!(BuiltinCommand::parse(input.text()).is_some());
        }
        input.insert(' ');
        assert!(input.candidates().is_empty());
        let mut alias = line("q");
        complete(&mut alias, false);
        assert_eq!(alias.text(), "q");
        assert_eq!(alias.candidates().len(), 2);
        complete(&mut alias, true);
        assert_eq!(alias.text(), "quit");
        complete(&mut alias, false);
        assert_eq!(alias.text(), "q");
        alias.move_cursor(CursorMove::Start);
        assert!(alias.candidates().is_empty());
        let mut reverse = line("");
        complete(&mut reverse, true);
        assert_eq!(reverse.text(), "volume");
    }

    #[test]
    fn unsupported_name_contexts_preserve_text_cursor_and_dispatch_boundaries() {
        for text in [
            "unknown",
            "Import",
            "imported path",
            "stop anything",
            "q anything",
            "qu ",
            "音",
        ] {
            let mut input = line(text);
            let cursor = input.cursor();
            input
                .complete_using(false, |_| panic!("not an import path"))
                .unwrap();
            assert_eq!(input.text(), text);
            assert_eq!(input.cursor(), cursor);
            assert!(input.candidates().is_empty());
            assert_eq!(BuiltinCommand::parse(text), None);
        }
        let mut input = line("im");
        input.move_cursor(CursorMove::Left);
        input
            .complete_using(false, |_| panic!("not an import path"))
            .unwrap();
        assert_eq!((input.text(), input.cursor()), ("im", 1));
        assert_eq!(
            BuiltinCommand::parse("  quit  "),
            Some((BuiltinCommand::Quit, ""))
        );
        assert_eq!(BuiltinCommand::parse("q"), Some((BuiltinCommand::Quit, "")));
        assert_eq!(
            BuiltinCommand::parse(" stop "),
            Some((BuiltinCommand::Stop, ""))
        );
        assert_eq!(
            BuiltinCommand::parse("import \"音 乐/file.ncm\""),
            Some((BuiltinCommand::Import, "\"音 乐/file.ncm\""))
        );
    }

    #[test]
    fn directory_completion_descends_and_filters_unsupported_files() {
        let temporary = TempDir::new().unwrap();
        fs::create_dir_all(temporary.path().join("Music/sub")).unwrap();
        fs::write(temporary.path().join("Music/cover.png"), []).unwrap();
        fs::write(temporary.path().join("Music/sub/tone.NCM"), []).unwrap();
        fs::write(temporary.path().join("Music/sub/tone.mp3"), []).unwrap();
        fs::write(temporary.path().join("Music/sub/tone.flac"), []).unwrap();
        let mut input = line("import Mu");
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(input.text(), format!("import Music{MAIN_SEPARATOR}"));
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(
            input.text(),
            format!("import Music{MAIN_SEPARATOR}sub{MAIN_SEPARATOR}")
        );
        input.complete_in(temporary.path(), None, false).unwrap();
        assert!(input.text().ends_with("tone."));
        assert_eq!(input.candidates().len(), 3);
        let mut absolute = line(&format!(
            "import {}{MAIN_SEPARATOR}Mu",
            temporary.path().display()
        ));
        absolute.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(
            absolute.text(),
            format!(
                "import {}{MAIN_SEPARATOR}Music{MAIN_SEPARATOR}",
                temporary.path().display()
            )
        );
    }

    #[test]
    fn common_prefix_and_forward_reverse_cycles_wrap() {
        let temporary = TempDir::new().unwrap();
        for name in [
            "song_a.ncm",
            "song_b.MP3",
            "song_c.FLAC",
            "song_ignored.ogg",
            "song_ignored.wav",
        ] {
            fs::write(temporary.path().join(name), []).unwrap();
        }
        let mut input = line("import so");
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(input.text(), "import song_");
        assert_eq!(input.candidates().len(), 3);
        for (reverse, expected) in [
            (false, "song_a.ncm"),
            (false, "song_b.MP3"),
            (true, "song_a.ncm"),
            (true, "song_c.FLAC"),
            (false, "song_a.ncm"),
        ] {
            input.complete_in(temporary.path(), None, reverse).unwrap();
            assert_eq!(input.text(), format!("import {expected}"));
        }
        input.move_cursor(CursorMove::Left);
        assert!(input.candidates().is_empty());
        let previous = input.text().to_owned();
        input.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(input.text(), previous);
        let mut reverse = line("import so");
        reverse.complete_in(temporary.path(), None, true).unwrap();
        assert_eq!(reverse.text(), "import song_c.FLAC");
    }

    #[test]
    fn home_and_quoted_unicode_paths_remain_editable_inside_quotes() {
        let temporary = TempDir::new().unwrap();
        fs::create_dir(temporary.path().join("音 乐")).unwrap();
        fs::write(temporary.path().join("音 乐/歌曲.ncm"), []).unwrap();
        let mut input = line("import \"~/音\"");
        input
            .complete_in(temporary.path(), Some(temporary.path()), false)
            .unwrap();
        assert_eq!(input.text(), "import \"~/音 乐/\"");
        input.insert('歌');
        assert_eq!(input.text(), "import \"~/音 乐/歌\"");
        input
            .complete_in(temporary.path(), Some(temporary.path()), false)
            .unwrap();
        assert_eq!(input.text(), "import \"~/音 乐/歌曲.ncm\"");
        input.backspace();
        input.insert('m');
        assert_eq!(input.text(), "import \"~/音 乐/歌曲.ncm\"");
        assert_eq!(input.cursor(), input.text().len() - 1);
        assert_eq!(
            expand_with_home("~/音 乐/歌曲.ncm", Some(temporary.path())),
            temporary.path().join("音 乐/歌曲.ncm")
        );
        let mut bare = line("import ~/音");
        bare.complete_in(temporary.path(), Some(temporary.path()), false)
            .unwrap();
        assert_eq!(bare.text(), "import \"~/音 乐/\"");
    }

    #[test]
    fn no_matches_and_scan_errors_preserve_input_and_other_commands_do_nothing() {
        let temporary = TempDir::new().unwrap();
        for text in ["import missing", "imported path", "q", "stop"] {
            let mut input = line(text);
            input.complete_in(temporary.path(), None, false).unwrap();
            assert_eq!(input.text(), text);
        }
        let mut input = line("import missing/");
        assert!(input.complete_in(temporary.path(), None, false).is_err());
        assert_eq!(input.text(), "import missing/");
        fs::write(temporary.path().join("visible.ncm"), []).unwrap();
        fs::write(temporary.path().join(".secret.ncm"), []).unwrap();
        let mut empty = line("import");
        empty.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(empty.text(), "import visible.ncm");
        let mut hidden = line("import .s");
        hidden.complete_in(temporary.path(), None, false).unwrap();
        assert_eq!(hidden.text(), "import .secret.ncm");
    }

    #[test]
    fn cursor_edits_preserve_unicode_boundaries() {
        let mut input = line("ab界d");
        input.move_cursor(CursorMove::Left);
        input.backspace();
        assert_eq!(input.text(), "abd");
        input.insert('音');
        input.delete();
        assert_eq!(input.text(), "ab音");
        input.move_cursor(CursorMove::Start);
        input.delete();
        input.move_cursor(CursorMove::End);
        input.move_cursor(CursorMove::Right);
        input.insert('乐');
        assert_eq!(input.text(), "b音乐");
        assert_eq!(input.cursor(), input.text().len());
    }
}
