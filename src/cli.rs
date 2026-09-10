//! Command line handling.
//!
//! Hand-rolled rather than pulling in `clap`. There is exactly one subcommand,
//! it is machine-invoked by a hook rather than typed by a person, and `clap`
//! would add noticeable compile time and binary size for a five-line parse.

use crate::hooks::{self, Kind, Notification};
use anyhow::{Result, bail};
use std::path::PathBuf;

pub enum Command {
    /// Launch the TUI.
    Run,
    /// Report an agent lifecycle event to a running Houston, then exit.
    Notify {
        notification: Notification,
        socket: PathBuf,
    },
    /// Install the latest release over this binary.
    Update,
    /// Print usage and exit.
    Help,
    Version,
}

const USAGE: &str = "\
houston — a terminal workspace for a knowledge vault and coding agents

USAGE:
    houston                     start the workspace
    houston update              install the latest release over this one
    houston notify <event> --session <id> --socket <path>
    houston --help
    houston --version

NOTIFY
    Invoked by agent hooks, not by hand. Events: start, permission, stop.
";

/// Parses arguments. Takes an iterator so it can be tested without a process.
pub fn parse<I: IntoIterator<Item = String>>(arguments: I) -> Result<Command> {
    let mut arguments = arguments.into_iter();

    let Some(first) = arguments.next() else { return Ok(Command::Run) };

    match first.as_str() {
        "--help" | "-h" | "help" => return Ok(Command::Help),
        "--version" | "-V" => return Ok(Command::Version),
        "update" => return Ok(Command::Update),
        "notify" => {}
        other => bail!("unknown argument '{other}'\n\n{USAGE}"),
    }

    let Some(event) = arguments.next() else { bail!("notify needs an event\n\n{USAGE}") };
    let Some(kind) = Kind::parse(&event) else {
        bail!("unknown event '{event}' — expected start, permission or stop")
    };

    let mut session = None;
    let mut socket = None;

    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--session" => {
                let value = arguments.next().unwrap_or_default();
                session =
                    Some(value.parse::<u64>().map_err(|_| {
                        anyhow::anyhow!("--session expects a number, got '{value}'")
                    })?);
            }
            "--socket" => socket = arguments.next().map(PathBuf::from),
            other => bail!("unknown flag '{other}'\n\n{USAGE}"),
        }
    }

    let Some(session) = session else { bail!("notify needs --session") };
    let Some(socket) = socket else { bail!("notify needs --socket") };

    // Deliberately *not* reading the hook payload here. `parse` is pure: doing
    // I/O in it blocks every argument test on a stdin that never closes, and —
    // worse — it drains the pipe, so the read in `dispatch` that actually uses
    // the payload found nothing. Both bugs existed at once, which is why the
    // conversation id was always null.
    Ok(Command::Notify { notification: Notification { session, kind, conversation: None }, socket })
}

/// Runs everything that is not the TUI. `true` means we are done.
pub fn dispatch(command: &Command) -> bool {
    match command {
        Command::Run => false,
        Command::Help => {
            print!("{USAGE}");
            true
        }
        Command::Version => {
            println!("houston {}", env!("CARGO_PKG_VERSION"));
            true
        }
        Command::Update => {
            if let Err(error) = crate::update::run() {
                eprintln!("  {error}");
            }
            true
        }
        Command::Notify { notification, socket } => {
            // The agent pipes its hook payload in on stdin; `session_id` is
            // what `--resume` needs later. Best-effort: a hook that cannot
            // read its input still has a job to do.
            let notification = Notification {
                conversation: hooks::conversation_from_stdin(),
                ..notification.clone()
            };

            // A hook firing when Houston is not running is normal — the user
            // may have quit. Fail quietly rather than disrupting the agent.
            let _ = hooks::notify(socket, &notification);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(arguments: &[&str]) -> Result<Command> {
        parse(arguments.iter().map(|argument| (*argument).to_string()))
    }

    #[test]
    fn no_arguments_starts_the_workspace() {
        assert!(matches!(parse_args(&[]).unwrap(), Command::Run));
    }

    #[test]
    fn notify_parses_its_flags_in_any_order() {
        let command =
            parse_args(&["notify", "attention", "--socket", "/tmp/a.sock", "--session", "3"])
                .unwrap();

        let Command::Notify { notification, socket } = command else { panic!("expected notify") };
        assert_eq!(notification.session, 3);
        assert_eq!(notification.kind, Kind::Attention);
        assert_eq!(socket, PathBuf::from("/tmp/a.sock"));
    }

    #[test]
    fn notify_rejects_incomplete_invocations() {
        assert!(parse_args(&["notify"]).is_err(), "an event is required");
        assert!(parse_args(&["notify", "working"]).is_err(), "--session is required");
        assert!(
            parse_args(&["notify", "working", "--session", "1"]).is_err(),
            "--socket is required"
        );
        assert!(parse_args(&["notify", "wat", "--session", "1"]).is_err(), "unknown event");
        assert!(
            parse_args(&["notify", "working", "--session", "x", "--socket", "/s"]).is_err(),
            "a non-numeric session is an error, not a silent zero"
        );
    }

    #[test]
    fn unknown_arguments_are_an_error_not_a_silent_launch() {
        assert!(parse_args(&["--wat"]).is_err());
        assert!(parse_args(&["run"]).is_err());
    }

    #[test]
    fn help_and_version_are_recognised() {
        assert!(matches!(parse_args(&["--help"]).unwrap(), Command::Help));
        assert!(matches!(parse_args(&["-h"]).unwrap(), Command::Help));
        assert!(matches!(parse_args(&["--version"]).unwrap(), Command::Version));
    }
}
