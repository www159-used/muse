// SPDX-License-Identifier: AGPL-3.0-only

mod command;
mod playback;
mod ui;

use std::{ffi::OsString, io::IsTerminal, path::PathBuf, process::ExitCode};

enum Action {
    Help,
    Version,
    Run(PathBuf),
    Backup {
        source: PathBuf,
        destination: PathBuf,
    },
    Restore {
        source: PathBuf,
        destination: PathBuf,
    },
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
    let mut operation = None;
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
        } else {
            return Err(format!(
                "unknown argument: {}; use --help for usage",
                argument.to_string_lossy()
            ));
        }
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
                "muse {}\n\nUsage: muse [--library <file>] [--backup <copy> | --restore <copy>]\n\nDefault library: ./library.muse\n--backup <copy>  Back up an existing library to a new file; no terminal required\n--restore <copy> Restore a copy to a new library file; no terminal required\n--help, -h       Show help\n--version, -V    Show version\nBackup and restore never overwrite existing destinations.\n\nControls: j/k select, Enter play, Space pause/resume, Left/Right seek 10s, :stop stop.\n:import <NCM file or folder> imports NCM sources, :q or :quit quits.\nIn COMMAND, Tab completes command names and import paths; Shift-Tab cycles backward.\nImports NCM only; folder scans ignore ordinary MP3/FLAC. Plays unpacked library MP3/FLAC with a built-in backend. Playback stops at the end of the track.",
                env!("CARGO_PKG_VERSION")
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
