//! Agent state, driven by Claude Code hooks rather than by reading the screen.
//!
//! Chloe got this right and we copy the design: the agent CLI is configured to
//! run `houston notify …` at known lifecycle points, which connects to a Unix
//! socket we listen on. Screen-scraping a TUI to guess whether an agent is
//! waiting is guesswork; a hook is a fact.
//!
//! Where we differ: Chloe's `generate_files` **overwrites**
//! `.claude/settings.local.json` wholesale, so any hand-written settings in a
//! project are destroyed the first time you point it at that directory. We
//! merge, and we never touch a key we did not write.

use crate::session::SessionId;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    thread,
};

/// What an agent just did, named by *meaning* rather than by which hook fired.
///
/// Several hooks map onto each of these, and the mapping is the interesting
/// part — see [`EVENTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The agent is doing something and does not need you.
    Working,
    /// The agent cannot continue without you.
    Attention,
    /// The agent finished its turn and is waiting for your next message.
    Idle,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Self> {
        // `start`, `permission`, `stop` and `end` are older spellings, kept so
        // a settings file written by a previous version keeps reporting until
        // it is rewritten on next launch.
        match text {
            "working" | "start" => Some(Self::Working),
            "attention" | "permission" => Some(Self::Attention),
            "idle" | "stop" | "end" => Some(Self::Idle),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Attention => "attention",
            Self::Idle => "idle",
        }
    }
}

/// Which Claude Code hooks Houston listens to, and what each one means.
///
/// The two that matter, and that a first version got wrong:
///
/// - **`Stop` means idle, not working.** It fires when the agent *finishes* its
///   turn. Mapping it to "working" leaves the board claiming an agent is busy
///   from the moment it goes quiet until you next type — which looks exactly
///   like the board getting stuck.
/// - **`PostToolUse` is what clears an attention state.** Nothing fires when
///   you approve a permission prompt, so without it a session stays on "needs
///   you" for the whole of the work it was asking permission to do.
pub const EVENTS: [(&str, Kind); 4] = [
    ("UserPromptSubmit", Kind::Working),
    ("PostToolUse", Kind::Working),
    ("PermissionRequest", Kind::Attention),
    ("Notification", Kind::Attention),
];

/// Hooks that end a turn. Kept separate only because `Stop` is the one whose
/// meaning is easiest to get backwards.
pub const END_EVENTS: [(&str, Kind); 1] = [("Stop", Kind::Idle)];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub session: u64,
    pub kind: Kind,
    /// The agent's own conversation id, lifted from the hook payload.
    #[serde(default)]
    pub conversation: Option<String>,
}

/// Reads `session_id` from a hook payload on stdin.
///
/// Claude Code passes JSON to every hook — `session_id`, `cwd`,
/// `transcript_path`, `hook_event_name`. The id is what
/// `claude --resume <id>` needs, so capturing it here is what makes a
/// restored session a continuation rather than a fresh start.
///
/// Anything unreadable yields `None`: a hook that fails is worse than a
/// session that reopens without its history.
#[must_use]
pub fn conversation_from_stdin() -> Option<String> {
    conversation_from(std::io::stdin().lock())
}

/// Reads `session_id` from a hook payload.
///
/// Takes a reader rather than touching stdin directly, so the parsing can be
/// tested — the version that only read stdin was untestable, and shipped
/// broken because of it.
#[must_use]
pub fn conversation_from(mut reader: impl std::io::Read) -> Option<String> {
    let mut payload = String::new();
    reader.read_to_string(&mut payload).ok()?;

    let value: serde_json::Value = serde_json::from_str(&payload).ok()?;
    let id = value.get("session_id")?.as_str()?.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// Houston's own directory, created on demand.
pub fn state_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    let directory = Path::new(&home).join(".houston");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    Ok(directory)
}

/// The socket this process listens on.
///
/// Keyed by pid so two Houston instances do not fight over one socket, and so
/// a stale socket from a crashed run never captures a live one's hooks.
pub fn socket_path() -> Result<PathBuf> {
    Ok(state_dir()?.join(format!("houston-{}.sock", std::process::id())))
}

/// Listens for hook notifications, handing each to `on_event`.
///
/// The listener owns its socket file and removes it on drop.
pub struct Listener {
    path: PathBuf,
}

impl Listener {
    pub fn start(sender: tokio::sync::mpsc::UnboundedSender<Notification>) -> Result<Self> {
        Self::start_at(socket_path()?, sender)
    }

    /// Listens on an explicit path. Split out so tests use their own socket
    /// rather than racing each other on the process-wide one.
    pub fn start_at(
        path: PathBuf,
        sender: tokio::sync::mpsc::UnboundedSender<Notification>,
    ) -> Result<Self> {
        // A leftover file from a crashed run would make `bind` fail.
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path)
            .with_context(|| format!("could not listen on {}", path.display()))?;

        thread::Builder::new()
            .name("houston-hooks".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if let Some(notification) = read_notification(stream)
                        && sender.send(notification).is_err()
                    {
                        // The app has gone away.
                        break;
                    }
                }
            })
            .context("could not start the hook listener")?;

        Ok(Self { path })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn read_notification(stream: UnixStream) -> Option<Notification> {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

/// Sends one notification, used by the `houston notify` subcommand.
pub fn notify(socket: &Path, notification: &Notification) -> Result<()> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("no Houston listening at {}", socket.display()))?;
    let mut payload = serde_json::to_string(notification)?;
    payload.push('\n');
    stream.write_all(payload.as_bytes())?;
    Ok(())
}

/// Adds Houston's hooks to a project's `.claude/settings.local.json`.
///
/// Merges into whatever is already there. Existing hooks are preserved, and
/// every key we do not own is left untouched — this file is frequently
/// hand-edited, and clobbering it is Chloe's one genuinely destructive bug.
pub fn install(project: &Path, session: SessionId, socket: &Path) -> Result<PathBuf> {
    let directory = project.join(".claude");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let path = directory.join("settings.local.json");

    let mut settings: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(existing) => serde_json::from_str(&existing).with_context(|| {
            format!("{} is not valid JSON — refusing to overwrite it", path.display())
        })?,
        Err(_) => serde_json::json!({}),
    };

    if !settings.is_object() {
        anyhow::bail!("{} is not a JSON object — refusing to overwrite it", path.display());
    }

    let executable = std::env::current_exe().context("could not find the houston binary")?;
    let socket = socket.display();
    let id = session.0;

    let command = |kind: Kind| {
        format!(
            "{} notify {} --session {id} --socket {socket}",
            executable.display(),
            kind.as_str()
        )
    };

    let hooks = settings
        .as_object_mut()
        .expect("checked above")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    for (event, kind) in EVENTS.into_iter().chain(END_EVENTS) {
        let Some(hooks) = hooks.as_object_mut() else { continue };
        let entry = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
        let Some(list) = entry.as_array_mut() else { continue };

        // Drop any Houston hook from a previous session before adding ours, so
        // repeated launches do not stack up dead notifiers.
        list.retain(|group| !is_houston_hook(group));

        list.push(serde_json::json!({
            "hooks": [{ "type": "command", "command": command(kind) }]
        }));
    }

    std::fs::write(&path, serde_json::to_string_pretty(&settings)?)
        .with_context(|| format!("could not write {}", path.display()))?;

    Ok(path)
}

/// Whether a hook group is one Houston wrote.
fn is_houston_hook(group: &serde_json::Value) -> bool {
    group.get("hooks").and_then(|hooks| hooks.as_array()).is_some_and(|hooks| {
        hooks.iter().any(|hook| {
            hook.get("command")
                .and_then(|command| command.as_str())
                .is_some_and(|command| command.contains("houston") && command.contains("notify"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("houston-hooks-{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_hook_payload_yields_its_session_id() {
        let payload = br#"{"session_id":"abc-123","cwd":"/tmp","hook_event_name":"Stop"}"#;
        assert_eq!(conversation_from(&payload[..]).as_deref(), Some("abc-123"));
    }

    #[test]
    fn anything_unreadable_yields_nothing_rather_than_failing_the_hook() {
        assert!(conversation_from(&b""[..]).is_none());
        assert!(conversation_from(&b"not json"[..]).is_none());
        assert!(conversation_from(&br#"{"cwd":"/tmp"}"#[..]).is_none());
        assert!(conversation_from(&br#"{"session_id":""}"#[..]).is_none());
        assert!(conversation_from(&br#"{"session_id":42}"#[..]).is_none());
    }

    #[test]
    fn event_names_round_trip() {
        for kind in [Kind::Working, Kind::Attention, Kind::Idle] {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse("nonsense"), None);
    }

    /// A settings file written by an older Houston keeps reporting until it is
    /// rewritten, so its spellings have to keep working.
    #[test]
    fn older_event_spellings_are_still_understood() {
        assert_eq!(Kind::parse("start"), Some(Kind::Working));
        assert_eq!(Kind::parse("permission"), Some(Kind::Attention));
        assert_eq!(Kind::parse("stop"), Some(Kind::Idle));
        assert_eq!(Kind::parse("end"), Some(Kind::Idle));
    }

    /// The bug this whole mapping exists to prevent: `Stop` fires when a turn
    /// *ends*, so treating it as "working" pins the board on busy from the
    /// moment the agent goes quiet.
    #[test]
    fn stop_means_idle_not_working() {
        let stop = END_EVENTS.iter().find(|(name, _)| *name == "Stop").unwrap();
        assert_eq!(stop.1, Kind::Idle);
    }

    /// Nothing fires when a permission is approved, so without `PostToolUse`
    /// a session stays on "needs you" for the whole of the work it asked about.
    #[test]
    fn something_clears_an_attention_state() {
        let clears: Vec<&str> = EVENTS
            .iter()
            .filter(|(_, kind)| *kind == Kind::Working)
            .map(|(name, _)| *name)
            .collect();

        assert!(clears.contains(&"PostToolUse"), "got {clears:?}");
    }

    #[test]
    fn installing_into_a_fresh_project_writes_all_three_hooks() {
        let root = scratch("fresh");
        let path = install(&root, SessionId(7), Path::new("/tmp/x.sock")).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let hooks = &settings["hooks"];

        for (event, _) in EVENTS.into_iter().chain(END_EVENTS) {
            assert!(hooks[event].is_array(), "{event} should be wired");
        }
        let command = hooks["Stop"][0]["hooks"][0]["command"].as_str().unwrap();
        assert!(command.contains("--session 7"));
        assert!(command.contains("/tmp/x.sock"));

        fs::remove_dir_all(&root).ok();
    }

    /// The bug we are explicitly not repeating from Chloe.
    #[test]
    fn installing_preserves_existing_settings() {
        let root = scratch("preserve");
        let claude = root.join(".claude");
        fs::create_dir_all(&claude).unwrap();
        fs::write(
            claude.join("settings.local.json"),
            r#"{
              "permissions": { "allow": ["Bash(npm test)"] },
              "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "my-own-script" }] }]
              }
            }"#,
        )
        .unwrap();

        let path = install(&root, SessionId(1), Path::new("/tmp/x.sock")).unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert_eq!(
            settings["permissions"]["allow"][0], "Bash(npm test)",
            "unrelated settings must survive"
        );

        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "the user's own hook is kept alongside ours");
        assert_eq!(stop[0]["hooks"][0]["command"], "my-own-script");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reinstalling_replaces_our_hook_rather_than_stacking_them() {
        let root = scratch("restack");

        install(&root, SessionId(1), Path::new("/tmp/a.sock")).unwrap();
        let path = install(&root, SessionId(2), Path::new("/tmp/b.sock")).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let stop = settings["hooks"]["Stop"].as_array().unwrap();

        assert_eq!(stop.len(), 1, "the stale notifier from session 1 is gone");
        assert!(stop[0]["hooks"][0]["command"].as_str().unwrap().contains("--session 2"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn malformed_settings_are_refused_not_overwritten() {
        let root = scratch("malformed");
        let claude = root.join(".claude");
        fs::create_dir_all(&claude).unwrap();
        let settings = claude.join("settings.local.json");
        fs::write(&settings, "{ this is not json").unwrap();

        assert!(install(&root, SessionId(1), Path::new("/tmp/x.sock")).is_err());
        assert_eq!(
            fs::read_to_string(&settings).unwrap(),
            "{ this is not json",
            "a file we cannot parse must be left exactly as it was"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// The whole ingress path, for real: a listener on a socket, a `notify`
    /// call from outside, and the event coming out the other end. Unit-testing
    /// only the JSON would not prove the socket is wired up.
    #[tokio::test]
    async fn a_notification_travels_over_the_socket() {
        let socket = std::env::temp_dir().join("houston-test-ingress.sock");
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let _listener = Listener::start_at(socket.clone(), sender).unwrap();

        notify(
            &socket,
            &Notification {
                session: 12,
                kind: Kind::Attention,
                conversation: Some("conv-7".to_string()),
            },
        )
        .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .expect("the notification should arrive promptly")
            .expect("the channel should stay open");

        assert_eq!(event.session, 12);
        assert_eq!(event.kind, Kind::Attention);
        assert_eq!(
            event.conversation.as_deref(),
            Some("conv-7"),
            "the conversation id must survive the socket, or resume cannot work"
        );
    }

    #[test]
    fn notifying_a_socket_nobody_is_listening_on_is_an_error_not_a_panic() {
        // Normal when Houston has been quit but an agent is still running.
        let result = notify(
            Path::new("/tmp/houston-definitely-not-listening.sock"),
            &Notification { session: 1, kind: Kind::Idle, conversation: None },
        );
        assert!(result.is_err());
    }

    #[test]
    fn notifications_round_trip_as_json() {
        let notification = Notification { session: 42, kind: Kind::Attention, conversation: None };
        let encoded = serde_json::to_string(&notification).unwrap();
        let decoded: Notification = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.session, 42);
        assert_eq!(decoded.kind, Kind::Attention);
    }
}
