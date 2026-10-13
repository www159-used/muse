// SPDX-License-Identifier: AGPL-3.0-only

mod command;
mod import_job;
mod operations;
mod playback;
mod refresh;
mod repl;
mod search;
mod service;
mod ui;
mod volume;

use std::{ffi::OsString, io::IsTerminal, path::PathBuf, process::ExitCode};

enum Action {
    Help,
    Version,
    Run(PathBuf),
    Service(PathBuf),
    Library(PathBuf, Vec<OsString>),
    Control(PathBuf, service::Command),
    Volume(PathBuf, Vec<OsString>),
    Backup {
        source: PathBuf,
        destination: PathBuf,
    },
    Restore {
        source: PathBuf,
        destination: PathBuf,
    },
}

fn parse_mode(name: &str) -> Result<playback::RepeatMode, String> {
    match name {
        "sequential" => Ok(playback::RepeatMode::Sequential),
        "list" | "loop" => Ok(playback::RepeatMode::List),
        "single" | "track" => Ok(playback::RepeatMode::Track),
        _ => Err("mode must be sequential, list, or single".into()),
    }
}

fn path_argument(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &str,
) -> Result<PathBuf, String> {
    arguments
        .next()
        .filter(|path| !path.is_empty() && !path.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a file path"))
}

fn arguments(mut arguments: impl Iterator<Item = OsString>) -> Result<Action, String> {
    let mut library = PathBuf::from("library.muse");
    let mut filtered = Vec::new();
    while let Some(arg) = arguments.next() {
        if arg == "--library" {
            library = path_argument(&mut arguments, "--library")?;
        } else {
            filtered.push(arg);
        }
    }
    if filtered.first().is_some_and(|a| {
        matches!(
            a.to_str(),
            Some("list" | "show" | "import" | "library" | "command" | "playlist")
        )
    }) || (filtered.first().is_some_and(|a| a == "play")
        && filtered
            .get(1)
            .is_some_and(|a| a.to_string_lossy().starts_with("g:")))
    {
        return Ok(Action::Library(library, filtered));
    }
    let mut arguments = filtered.into_iter();
    let mut operation = None;
    let mut control = None;
    let mut internal_service = false;
    let mut arguments = arguments.by_ref().peekable();
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(Action::Help);
        } else if argument == "--version" || argument == "-V" {
            return Ok(Action::Version);
        } else if argument == "--library" {
            library = path_argument(&mut arguments, "--library")?;
        } else if argument == "--backup" || argument == "--restore" {
            if operation.is_some() {
                return Err("specify --backup or --restore once; they cannot be combined".into());
            }
            let path = path_argument(&mut arguments, &argument.to_string_lossy())?;
            operation = Some((argument == "--backup", path));
        } else if argument == "--internal-service" {
            if internal_service {
                return Err("specify one operation".into());
            }
            internal_service = true;
        } else if argument == "volume" {
            if operation.is_some() || control.is_some() || internal_service {
                return Err(
                    "maintenance, playback and service operations cannot be combined".into(),
                );
            }
            return Ok(Action::Volume(library, arguments.collect()));
        } else if matches!(
            argument.to_str(),
            Some("play" | "pause" | "stop" | "status" | "shutdown" | "next" | "mode")
        ) {
            if control.is_some() {
                return Err("specify one playback command".into());
            }
            control = Some(match argument.to_str().unwrap() {
                "next" => service::Command::Next,
                "mode" => {
                    if arguments
                        .peek()
                        .is_some_and(|arg| !arg.to_string_lossy().starts_with('-'))
                    {
                        service::Command::SetMode(parse_mode(
                            &arguments.next().unwrap().to_string_lossy(),
                        )?)
                    } else {
                        service::Command::CycleMode
                    }
                }
                "play" => {
                    if arguments
                        .peek()
                        .is_some_and(|arg| !arg.to_string_lossy().starts_with('-'))
                    {
                        let id = arguments
                            .next()
                            .unwrap()
                            .to_str()
                            .and_then(|s| s.parse::<i64>().ok())
                            .filter(|id| *id > 0)
                            .ok_or("play requires a positive database track ID")?;
                        service::Command::Load(id)
                    } else {
                        service::Command::Play
                    }
                }
                "pause" => service::Command::Pause,
                "stop" => service::Command::Stop,
                "shutdown" => service::Command::Shutdown,
                _ => service::Command::Status,
            });
        } else {
            return Err(format!(
                "unknown argument: {}; use --help for usage",
                argument.to_string_lossy()
            ));
        }
    }
    if usize::from(operation.is_some())
        + usize::from(control.is_some())
        + usize::from(internal_service)
        > 1
    {
        return Err("maintenance, playback and service operations cannot be combined".into());
    }
    if internal_service {
        return Ok(Action::Service(library));
    }
    if let Some(command) = control {
        return Ok(Action::Control(library, command));
    }
    Ok(match operation {
        Some((true, destination)) => Action::Backup {
            source: library,
            destination,
        },
        Some((false, source)) => Action::Restore {
            source,
            destination: library,
        },
        None => Action::Run(library),
    })
}

fn copy_library(source: PathBuf, destination: PathBuf, operation: &str) -> Result<(), String> {
    muse_library::Library::restore(&source, &destination).map_err(|error| {
        format!(
            "{operation} failed ({} -> {}): {error}",
            source.display(),
            destination.display()
        )
    })?;
    println!("{operation} completed: {}", destination.display());
    Ok(())
}

fn main() -> ExitCode {
    let result = match arguments(std::env::args_os().skip(1)) {
        Ok(Action::Help) => {
            println!(
                "muse {}\n\nUsage: muse [--library <file>] [command | <operation>]\nDefault library: ./library.muse (local NetEase)\n\n{}\n\n--backup <copy>  Save a validated snapshot to a new file\n--restore <copy> Restore a v1/v2/v3 copy to a new library file\n--help, -h       Show help\n--version, -V    Show version\n\nWithout an operation, opens the TUI on the 全部本地 playlist. Enter plays the selected track inside the current playlist.\nr cycles sequential, list loop, and single loop. l opens full-screen lyrics; Esc closes them. While lyrics are scrolled away, l returns to the current line. Esc cancels a chooser.\nSpace pauses/resumes; Left/Right seeks 10s. :import imports local NCM/MP3/FLAC.\n:playlist creates or plays a playlist. :volume shows or changes application gain (set, inc, dec); it does not change the system volume. :q stops shared playback and exits; :detach keeps playback.\n\nDifferent audio is retained; equal tagged names group candidates for selection. No network API is used.\nVersion-1 and version-2 libraries require library upgrade after closing clients and shutting down the old service.\nUpgrade creates a verified pre-upgrade backup; reimport original files to recover previously skipped audio.",
                env!("CARGO_PKG_VERSION"),
                operations::HELP
            );
            Ok(())
        }
        Ok(Action::Version) => {
            println!("muse {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Ok(Action::Backup {
            source,
            destination,
        }) => copy_library(source, destination, "Backup"),
        Ok(Action::Restore {
            source,
            destination,
        }) => copy_library(source, destination, "Restore"),
        Ok(Action::Library(path, args)) => {
            if args[0] == "command" {
                if args.len() != 1 {
                    Err("command takes no arguments".into())
                } else if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                    repl::run(&path)
                } else {
                    Err("command mode requires an interactive terminal".into())
                }
            } else {
                operations::execute(&path, &args).and_then(operations::CommandOutput::print)
            }
        }
        Ok(Action::Service(path)) => service::run(&path),
        Ok(Action::Control(path, command)) => service::control(&path, command),
        Ok(Action::Volume(path, args)) => {
            operations::volume(&path, &args).and_then(operations::CommandOutput::print)
        }
        Ok(Action::Run(path)) => {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                ui::run(path).map_err(|error| error.to_string())
            } else {
                Err("muse requires an interactive terminal; use --help for usage".into())
            }
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("muse: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_arguments_accept_spaces_and_require_a_value() {
        let action = arguments(
            [
                OsString::from("--library"),
                OsString::from("a b/library.muse"),
            ]
            .into_iter(),
        )
        .unwrap();
        assert!(
            matches!(action, Action::Run(path) if path == std::path::Path::new("a b/library.muse"))
        );
        assert!(arguments([OsString::from("--library")].into_iter()).is_err());
        assert!(arguments([OsString::from("--unknown")].into_iter()).is_err());
    }

    #[test]
    fn maintenance_arguments_select_source_and_destination_and_reject_conflicts() {
        let parse = |args: &[&str]| arguments(args.iter().map(OsString::from));
        assert!(matches!(parse(&["--backup", "a b.muse"]).unwrap(),
            Action::Backup { source, destination } if source == std::path::Path::new("library.muse") && destination == std::path::Path::new("a b.muse")));
        assert!(
            matches!(parse(&["--restore", "backup.muse", "--library", "new.muse"]).unwrap(),
            Action::Restore { source, destination } if source == std::path::Path::new("backup.muse") && destination == std::path::Path::new("new.muse"))
        );
        for args in [
            vec!["--backup"],
            vec!["--restore", ""],
            vec!["--library", "--backup", "copy.muse"],
            vec!["--backup", "--restore", "copy.muse"],
            vec!["--backup", "a.muse", "--restore", "b.muse"],
            vec!["--backup", "a.muse", "--backup", "b.muse"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
