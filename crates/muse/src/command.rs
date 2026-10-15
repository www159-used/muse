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
    Next,
    Pause,
    Play,
    Prev,
    Volume,
}

const COMMANDS: &[(&str, BuiltinCommand)] = &[
    ("import", BuiltinCommand::Import),
    ("q", BuiltinCommand::Quit),
    ("quit", BuiltinCommand::Quit),
    ("stop", BuiltinCommand::Stop),
    ("detach", BuiltinCommand::Detach),
    ("next", BuiltinCommand::Next),
    ("pause", BuiltinCommand::Pause),
    ("play", BuiltinCommand::Play),
    ("prev", BuiltinCommand::Prev),
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
        (matches!(
            command,
            Self::Import | Self::Playlist | Self::Volume | Self::Play
        ) || rest.is_empty())
        .then_some((command, rest))
    }
}

/// Marker that selects Rhai script input in a command prompt.
///
/// The TUI shows it as `:=` and the standalone REPL as `=`; routing itself only
/// looks for the leading `=`.
pub const SCRIPT_MARKER: char = '=';

/// How the command prompt dispatches submitted input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandForm {
    /// An ordinary shipped command form, including its arguments and numeric
    /// navigation.
    Command,
    /// Rhai source following the explicit [`SCRIPT_MARKER`].
    Script,
}

/// Classify submitted command input by its explicit script marker.
///
/// Only a leading `=` selects Rhai. Everything else keeps the ordinary command
/// path, so a call that merely resembles a command such as `play()` and unknown
/// input both report command errors instead of being evaluated as scripts, and a
/// script error never sends the same text back through command parsing.
pub fn classify(input: &str) -> CommandForm {
    if input.trim_start().starts_with(SCRIPT_MARKER) {
        CommandForm::Script
    } else {
        CommandForm::Command
    }
}

/// Return the Rhai body following the leading marker, trimmed.
///
/// A bare marker yields an empty body, which callers treat as no effect.
pub fn script_body(input: &str) -> Option<&str> {
    input
        .trim_start()
        .strip_prefix(SCRIPT_MARKER)
        .map(str::trim)
}

/// A command line that is only a positive row number, used for navigation.
pub fn decimal_line(input: &str) -> Option<usize> {
    if input.is_empty() || !input.bytes().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    Some(input.bytes().fold(0usize, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'))
    }))
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
    Script(crate::scripting::completion::Edit),
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
            Some(CompletionTarget::Script(_)) => "Rhai",
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
        let (target, candidates) = if classify(&self.text) == CommandForm::Script {
            // REPL script execution arrives in phase 4; do not advertise it yet.
            if self.repl {
                return Ok(());
            }
            let Some((edit, suggestions)) =
                crate::scripting::completion::candidates(&self.text, self.cursor)
            else {
                return Ok(());
            };
            let candidates = suggestions
                .into_iter()
                .map(|suggestion| Candidate {
                    label: suggestion.label,
                    value: suggestion.insertion,
                    directory: false,
                })
                .collect();
            (CompletionTarget::Script(edit), candidates)
        } else if let Some(argument) = import_argument(&self.text, self.cursor) {
            let candidates = paths(&argument.path)?;
            (CompletionTarget::Paths(argument), candidates)
        } else if self.cursor == self.text.len()
            && !self.text.trim_start().contains(char::is_whitespace)
        {
            let name = self.text.trim_start();
            let prefix = self.text[..self.text.len() - name.len()].to_owned();
            let commands: Vec<&str> = if self.repl {
                vec![
                    "import", "list", "show", "play", "next", "pause", "prev", "stop", "status",
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
            CompletionTarget::Script(edit) => {
                (self.text, self.cursor) = edit.apply(value);
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

    #[test]
    fn only_marked_input_reaches_the_script_worker() {
        for input in [
            "",
            "   ",
            "play",
            "  quit  ",
            "q",
            "stop",
            "detach",
            "next",
            "pause",
            "prev",
            "import \"a b/song.ncm\"",
            "import ~/Music",
            "playlist new Focus",
            "volume set 50",
            "play 42",
            "23",
            "7",
            // A call that merely resembles a command stays ordinary input.
            "play()",
            "seek(90, unit::SEC)",
        ] {
            assert_eq!(
                classify(input),
                CommandForm::Command,
                "{input:?} must stay on the command path"
            );
        }
        for input in ["=play()", " =version()", "=23", "=seek(90, ", "=音", "="] {
            assert_eq!(
                classify(input),
                CommandForm::Script,
                "{input:?} must go to the script worker"
            );
        }
    }

    #[test]
    fn the_script_body_is_trimmed_and_empty_for_a_bare_marker() {
        assert_eq!(script_body(" =play()"), Some("play()"));
        assert_eq!(script_body("=seek(90, "), Some("seek(90,"));
        assert_eq!(script_body(" = 23 "), Some("23"));
        assert_eq!(script_body("="), Some(""));
        assert_eq!(script_body("play"), None);
        assert_eq!(script_body("play()"), None);
    }

    #[test]
    fn numeric_navigation_is_only_a_jump_target_for_plain_decimals() {
        assert_eq!(decimal_line("23"), Some(23));
        assert_eq!(decimal_line("007"), Some(7));
        for input in ["2 3", "2a", "-1", "1.5", "23G", "音"] {
            assert_eq!(decimal_line(input), None, "{input:?} is not a jump target");
            assert_eq!(classify(input), CommandForm::Command);
        }
    }

    #[test]
    fn unprefixed_function_syntax_never_selects_script_mode() {
        // `play()` is not the ordinary `play` command, and neither is unknown
        // input: both stay command errors instead of being evaluated as scripts.
        assert_eq!(BuiltinCommand::parse("play()"), None);
        assert_eq!(BuiltinCommand::parse("stop anything"), None);
        assert_eq!(classify("play()"), CommandForm::Command);
        assert_eq!(classify("stop anything"), CommandForm::Command);
        assert_eq!(
            BuiltinCommand::parse("play"),
            Some((BuiltinCommand::Play, ""))
        );
        assert_eq!(
            BuiltinCommand::parse("play 42"),
            Some((BuiltinCommand::Play, "42"))
        );
        assert_eq!(classify("play"), CommandForm::Command);
    }

    fn line(text: &str) -> CommandLine {
        CommandLine {
            text: text.to_owned(),
            cursor: text.len(),
            completion: None,
            repl: false,
        }
    }

    fn complete_script(input: &mut CommandLine, reverse: bool) {
        input
            .complete_using(reverse, |_| panic!("script symbols must not scan paths"))
            .unwrap();
    }

    #[test]
    fn script_completion_discovers_registered_functions_and_namespaces() {
        let mut input = line("  =  ");
        complete_script(&mut input, false);
        assert_eq!(input.text(), "  =  ");
        assert_eq!(input.completion_title(), "Rhai");
        let choices: Vec<_> = input.candidates().iter().map(|c| c.value.clone()).collect();
        assert!(choices.iter().any(|value| value == "unit::"));
        for function in crate::scripting::api::FUNCTIONS {
            assert!(choices.contains(&format!("{}()", function.name)));
        }
        assert!(!choices.iter().any(|value| value == "quit"));
        // Traverse every registered candidate twice, then reverse across the
        // wrap boundary. Adding host APIs must not require another name list.
        for expected in choices.iter().cycle().take(choices.len() * 2 + 1) {
            complete_script(&mut input, false);
            assert_eq!(input.text(), format!("  =  {expected}"));
        }
        complete_script(&mut input, true);
        assert_eq!(input.text(), format!("  =  {}", choices.last().unwrap()));
        input.move_cursor(CursorMove::Left);
        assert!(input.candidates().is_empty());
    }

    #[test]
    fn script_completion_uses_registered_units_and_stable_reverse_cycling() {
        let mut input = line("=unit::");
        complete_script(&mut input, false);
        let mut expected: Vec<_> = crate::scripting::api::CONSTANTS
            .iter()
            .map(|c| c.qualified_name())
            .collect();
        expected.sort();
        assert_eq!(
            input
                .candidates()
                .iter()
                .map(|c| c.label.clone())
                .collect::<Vec<_>>(),
            expected
        );
        complete_script(&mut input, true);
        assert_eq!(input.text(), "=unit::SEC");
        complete_script(&mut input, false);
        assert_eq!(input.text(), "=unit::HOUR");
        complete_script(&mut input, true);
        assert_eq!(input.text(), "=unit::SEC");
        input.insert(';');
        assert!(input.candidates().is_empty());
    }

    #[test]
    fn script_completion_preserves_unicode_prefix_suffix_and_existing_parentheses() {
        for (source, token, expected, cursor_prefix) in [
            (
                "  =let 名 = 1; ver; 名",
                "ver",
                "  =let 名 = 1; version(); 名",
                "  =let 名 = 1; version(",
            ),
            ("=ver (); 23", "ver", "=version (); 23", "=version ("),
            ("=version(); 23", "ver", "=version(); 23", "=version("),
            (
                "=fn f(x) { x }; f(unit::S); 23",
                "unit::S",
                "=fn f(x) { x }; f(unit::SEC); 23",
                "=fn f(x) { x }; f(unit::SEC",
            ),
        ] {
            let mut input = line(source);
            input.cursor = source.find(token).unwrap() + token.len();
            complete_script(&mut input, false);
            assert_eq!(input.text(), expected);
            assert_eq!(input.cursor(), cursor_prefix.len());
            assert!(input.text().is_char_boundary(input.cursor()));
        }
    }

    #[test]
    fn script_completion_leaves_literals_comments_and_unsupported_symbols_alone() {
        for source in [
            "=print(\"ver",
            "=print(\"a\\\"ver",
            "='v",
            "=`ver",
            "=// ver",
            "=/* ver",
            "=/* outer /* nested */ ver",
            "=object.ver",
            "=seek",
            "=unknown::S",
        ] {
            let mut input = line(source);
            complete_script(&mut input, false);
            assert_eq!(input.text(), source);
            assert!(input.candidates().is_empty());
        }
        let mut input = line("=/* nested /* comment */ done */ ver");
        complete_script(&mut input, false);
        assert_eq!(input.text(), "=/* nested /* comment */ done */ version()");
    }

    #[test]
    fn script_completion_does_not_leak_into_ordinary_or_repl_input() {
        let mut ordinary = line("ver");
        complete_script(&mut ordinary, false);
        assert_eq!(ordinary.text(), "ver");
        assert!(ordinary.candidates().is_empty());
        let mut repl = CommandLine::for_repl();
        repl.replace("=ver");
        complete_script(&mut repl, false);
        assert_eq!(repl.text(), "=ver");
        assert!(repl.candidates().is_empty());
        let mut input = line("=ver");
        input.move_cursor(CursorMove::Start);
        complete_script(&mut input, false);
        assert_eq!(input.text(), "=ver");
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
                "import", "q", "quit", "stop", "detach", "next", "pause", "play", "prev",
                "playlist", "volume",
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
        assert_eq!(
            BuiltinCommand::parse("pause"),
            Some((BuiltinCommand::Pause, ""))
        );
        assert_eq!(
            BuiltinCommand::parse(" play 12 "),
            Some((BuiltinCommand::Play, "12"))
        );
        assert_eq!(
            BuiltinCommand::parse("play"),
            Some((BuiltinCommand::Play, ""))
        );
        assert_eq!(
            BuiltinCommand::parse("prev"),
            Some((BuiltinCommand::Prev, ""))
        );
        assert_eq!(BuiltinCommand::parse("pause now"), None);
        assert_eq!(BuiltinCommand::parse("prev 1"), None);
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
