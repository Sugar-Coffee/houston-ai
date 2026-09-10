//! Application state.

use crate::{
    config::Config,
    editor::Editor,
    form::{Field, Form},
    pty::Size,
    session::{Focus, Sessions},
    ui::{Theme, theme},
    vault::{Browser, Vault, browser::Mode as VaultMode},
    worktree::Worktree,
};
use std::{collections::VecDeque, path::PathBuf};

/// The top-level views, rendered as the tab strip.
///
/// Mirrors `docs/roadmap.md`: Sessions (Phase 2), Vault (Phase 4),
/// Board (Phase 6), Settings (Phase 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Sessions,
    Vault,
    Board,
    Worktrees,
    Settings,
}

impl Tab {
    pub const ALL: [Self; 5] =
        [Self::Sessions, Self::Vault, Self::Board, Self::Worktrees, Self::Settings];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::Vault => "Vault",
            Self::Board => "Board",
            Self::Worktrees => "Worktrees",
            Self::Settings => "Settings",
        }
    }
}

/// Where keystrokes are going right now.
///
/// An explicit enum rather than a chain of booleans: every new text field —
/// rename, the session picker, the editor — otherwise has to be remembered in
/// three separate predicates, and forgetting one means `q` quits the app while
/// someone is typing a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFocus {
    /// Keys are Houston's commands.
    Commands,
    /// Keys belong to an attached child process.
    Session,
    /// Keys fill a text field Houston owns.
    Text,
    /// Keys drive a modal overlay: move the selection, accept, cancel.
    Overlay,
    /// Keys belong to the editor, which owns its own modality.
    Editor,
    /// Keys drive a form: move between fields, edit one, accept or cancel.
    Form,
    /// Keys drive a copy cursor over a session's scrollback.
    Copy,
}

/// A modal chooser: pick one of the running sessions.
///
/// Used to send a note into a session when more than one is running. Ordered
/// exactly as the Sessions sidebar, so the list you see is the list you know.
#[derive(Debug, Clone)]
pub struct Picker {
    pub prompt: String,
    /// Sent to the chosen session, as a paste rather than keystrokes.
    pub payload: String,
    pub selected: usize,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "App is the single place application state lives; grouping these \
              into sub-structs would add indirection without adding meaning"
)]
pub struct App {
    pub tab: Tab,
    pub sessions: Sessions,
    /// `None` when the vault could not be opened. The view says how to fix it.
    pub browser: Option<Browser>,
    pub config: Config,
    /// Where `config` is persisted. A field rather than a lookup so tests can
    /// point it somewhere harmless.
    pub config_path: PathBuf,
    /// Why the vault could not be opened, if it could not.
    pub vault_error: Option<String>,
    /// Where new sessions start. The directory Houston was launched from.
    pub cwd: PathBuf,
    pub should_quit: bool,
    /// Set whenever state changes in a way that needs a redraw. The render loop
    /// is frame-budgeted, so this coalesces bursts of events into one draw.
    pub dirty: bool,
    /// Shown in the footer when an action cannot be carried out.
    pub notice: Option<String>,
    /// `q` has been pressed once and is waiting for confirmation.
    pub quit_armed: bool,
    /// The name being typed for the selected session. `None` when not renaming.
    pub renaming: Option<String>,
    /// The open modal chooser, if any.
    pub picker: Option<Picker>,
    /// The open editor, if any. Replaces the reader on the Vault view.
    pub editor: Option<Editor>,
    /// The open modal form, if any.
    pub form: Option<Form>,
    /// What the open form will do when accepted.
    pub form_purpose: FormPurpose,
    /// The Settings view, which *is* a form — a menu of fields you move
    /// through and press Return to edit.
    pub settings: Form,
    /// What a scan of the installed fonts found, for the Settings hint.
    ///
    /// Cached. Answering it means reading font files until one matches, which
    /// is fine once and absurd per frame.
    pub font_detection: crate::fonts::Detection,
    /// A font install running on another thread, if one is.
    ///
    /// On a thread because a download is seconds and the frame tick is
    /// milliseconds. Blocking the loop would freeze the whole app on what is
    /// meant to be a convenience.
    pub font_install: Option<std::sync::mpsc::Receiver<anyhow::Result<std::path::PathBuf>>>,
    /// The last left-button press: when, where, and how many in a row.
    ///
    /// Click counting has to happen here because a terminal reports three
    /// separate presses for a triple-click and leaves the counting to you.
    pub last_click: Option<(std::time::Instant, u16, u16, u8)>,
    /// A mouse selection is in progress, so drags extend it.
    pub dragging: bool,
    /// The session whose scrollback is being selected, if any.
    ///
    /// The id rather than a flag: copy mode lives in that session's own VT
    /// state, so leaving it has to reach the same session even if the
    /// selection has moved on since.
    pub copying: Option<crate::session::SessionId>,
    /// A newer release, once the background check has found one.
    pub update_available: Option<String>,
    /// A pending question. Nothing destructive happens while this is set.
    pub confirm: Option<Confirm>,
    /// The open theme picker, if any.
    pub theme_picker: Option<ThemePicker>,
    /// The open diff, if any. Captured once on open; see [`crate::diff::View`].
    pub diff: Option<crate::diff::View>,
    /// The worktree manager's contents, loaded when it opens.
    pub worktrees: Option<Vec<Worktree>>,
    pub worktree_selected: usize,
    /// The last few input events, newest last.
    ///
    /// Always recorded, shown only when asked for. Whether the terminal is
    /// reporting the mouse at all is otherwise unanswerable from inside the
    /// app — and guessing at it cost a whole round of work.
    pub input_log: VecDeque<String>,
    pub show_inspector: bool,
    /// Where the remembered session list is persisted.
    pub state_path: PathBuf,
    /// The theme in force. Resolved once, not per frame.
    pub theme: Theme,
    /// Every theme available, for the Settings row.
    pub themes: Vec<theme::Named>,
}

/// How many input events the inspector remembers.
const INPUT_LOG: usize = 14;

/// What accepting the open form does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPurpose {
    None,
    /// Start a session with the form's settings.
    NewSession,
    /// Land the named worktree: commit, push, open a PR, remove.
    Land(String),
    /// Create a note or folder in the vault.
    NewVaultEntry,
    /// Rename or move whatever is selected in the vault.
    RenameVaultEntry,
}

/// A question standing between you and something irreversible.
///
/// **Replaces the shift-variant idiom.** Destructive actions used to hide
/// behind a second, capitalised key — `D` to remove a worktree with
/// uncommitted work in it, `S` to save over a file that changed on disk. That
/// has two problems. You have to already know the capital exists, so the first
/// time you meet the situation the app tells you no and stops; and the safety
/// of it rests on your shift finger rather than on your having read what is at
/// stake.
///
/// A confirmation says what will be lost and asks. It works the first time,
/// and it is the same shape everywhere.
#[derive(Debug, Clone)]
pub struct Confirm {
    /// The question, phrased so that "yes" is unambiguous.
    pub question: String,
    /// What is at stake, in the user's terms.
    pub detail: String,
    pub action: Pending,
}

/// What happens if the answer is yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// Remove the named worktree, uncommitted work and all.
    RemoveWorktree(String),
    /// Save the open note over a file that has changed on disk.
    OverwriteNote,
    /// Delete the vault path, and everything under it if it is a folder.
    RemoveVaultEntry(std::path::PathBuf),
}

/// The theme picker's state.
///
/// `original` is what to go back to on Esc. Held here rather than read from
/// config on cancel because the preview does *not* touch config — a theme you
/// scrolled past should not survive a crash as your setting.
#[derive(Debug, Clone)]
pub struct ThemePicker {
    pub selected: usize,
    pub original: Option<String>,
}

/// Field labels, so the form and the code that reads it cannot drift apart.
pub mod fields {
    pub const NAME: &str = "Name";
    pub const DIRECTORY: &str = "Directory";
    pub const WORKTREE: &str = "Worktree";
    pub const WORKTREE_NAME: &str = "Worktree name";
    pub const VAULT: &str = "Vault folder";
    pub const AGENT_DIRECTORY: &str = "New sessions start in";
    pub const THEME: &str = "Theme";
    pub const CREATE: &str = "Create";
    pub const MOUSE: &str = "Capture the mouse";
    pub const POWERLINE: &str = "Powerline separators";
    pub const INSTALL_FONT: &str = "Install a powerline font";
    pub const FOLDER: &str = "Folder";
    pub const MESSAGE: &str = "Commit message";
    pub const PUSH: &str = "Push to origin";
    pub const PULL_REQUEST: &str = "Open a pull request";
    pub const REMOVE: &str = "Remove the worktree";
    pub const LAND: &str = "Land";
}

impl App {
    pub fn new() -> Self {
        // **A test must never read the developer's own config.** `config_path`
        // and `state_path` below were already pointed somewhere harmless, but
        // the *load* was not, so the suite quietly depended on a file outside
        // the repository. It went unnoticed until the theme in
        // `~/.houston/config.toml` was set to Monokai, at which point two
        // board tests started failing on that one machine and passing on
        // every other. The same `$HOME` trap `CLAUDE.md` records, in read form
        // rather than write form.
        let (config, config_error) =
            if cfg!(test) { (Config::default(), None) } else { Config::load() };

        let mut app = Self::with_config(config);
        app.notice = config_error;
        app.reload_theme();
        app.load_vault();
        app.rebuild_settings();
        app
    }

    /// Writes the current session list, so quitting does not lose it.
    ///
    /// Cheap and called on any change worth keeping. A failure is swallowed:
    /// losing your session list at the next launch is a nuisance, but refusing
    /// to carry on working now would be worse.
    pub fn remember_sessions(&mut self) {
        // Codex reports no id, so it is looked up from its rollout files here
        // rather than continuously.
        self.sessions.resolve_conversations();
        let _ = crate::state::State::capture(&self.sessions).save_to(&self.state_path);
    }

    /// Reopens whatever was running last time.
    ///
    /// Restored sessions come back detached and idle. Attaching to one of
    /// several on launch would be a guess about which you meant.
    pub fn restore_sessions(&mut self, size: Size) {
        let remembered = crate::state::State::load_from(&self.state_path);
        if remembered.sessions.is_empty() {
            return;
        }

        let mut lost = Vec::new();
        for session in &remembered.sessions {
            if self.sessions.restore(session, size).is_err() {
                lost.push(session.name.clone());
            }
        }

        self.sessions.detach();
        self.sessions.select(0);
        self.dirty = true;

        if !lost.is_empty() {
            self.notify(format!("could not reopen: {}", lost.join(", ")));
        }
    }

    /// Re-resolves the theme from config and disk.
    ///
    /// Called when the setting changes, so a theme file you have just edited
    /// takes effect without a restart.
    pub fn reload_theme(&mut self) {
        // `themes()` also scans `~/.houston/themes/`, so a user theme file
        // could redefine a built-in out from under the suite. See `new`.
        if cfg!(test) {
            self.theme = Theme::default();
            self.themes = Vec::new();
            return;
        }
        let (theme, available) = self.config.themes();
        self.theme = theme;
        self.themes = available;
        self.dirty = true;
    }

    /// Rebuilds the Settings form from the current config.
    ///
    /// Called after anything changes a setting, so the menu always shows what
    /// is actually in force rather than what was typed.
    pub fn rebuild_settings(&mut self) {
        let vault = self
            .config
            .vault_root()
            .map_or_else(|_| String::new(), |root| crate::paths::contract_home(&root));
        let agent = crate::paths::contract_home(&self.config.agent_root());

        self.settings = Form::new(vec![
            Field::directory(fields::VAULT, "~/.houston/vault", vault),
            Field::directory(fields::AGENT_DIRECTORY, "~/", agent),
            Field::choice(
                fields::THEME,
                "",
                self.themes.iter().map(|theme| theme.name.clone()).collect(),
                self.config.theme.as_deref().unwrap_or("Dracula"),
            ),
            // The hint carries the glyphs themselves *and* what a scan of the
            // installed fonts found. The scan can only prove absence — the
            // terminal may be pointed at some other font entirely — so the
            // glyphs are there for you to judge the rest.
            Field::toggle(
                fields::POWERLINE,
                format!("{}  {}", crate::ui::powerline::SAMPLE, self.font_hint()),
                self.config.powerline_enabled(),
            ),
            // Directly under the row it exists to fix.
            Field::action(fields::INSTALL_FONT, self.install_hint()),
            Field::toggle(
                fields::MOUSE,
                "wheel scrolls sessions; off restores text selection",
                self.config.mouse_enabled(),
            ),
        ]);
    }

    /// Opens the new-session form, seeded from config.
    pub fn open_new_session_form(&mut self) {
        let directory = crate::paths::contract_home(&self.config.agent_root());
        let mut form = Form::new(vec![
            Field::text(fields::NAME, "the agent's name", ""),
            Field::directory(fields::DIRECTORY, "~/", directory),
            Field::toggle(fields::WORKTREE, "an isolated git checkout", false),
            Field::text(fields::WORKTREE_NAME, "from the session name", ""),
            Field::action(fields::CREATE, "start the session"),
        ]);
        form.set_visible(fields::WORKTREE_NAME, false);

        self.form = Some(form);
        self.form_purpose = FormPurpose::NewSession;
        self.dirty = true;
    }

    /// Adopts whatever `config.theme` names from the already-loaded list.
    ///
    /// Distinct from `reload_theme`, which re-reads the disk. Used when the
    /// list is already in hand and only the choice has moved.
    pub fn reload_theme_from_list(&mut self) {
        self.theme = self
            .config
            .theme
            .as_deref()
            .and_then(|name| self.themes.iter().find(|theme| theme.name == name))
            .map_or_else(Theme::default, |named| named.theme);
        self.dirty = true;
    }

    /// Loads the worktree list, for the view that shows it.
    ///
    /// Reads the disk every time rather than caching. It is a handful of git
    /// calls per worktree and it happens when you open the view or ask for a
    /// refresh, never per frame — and a stale list here is worse than a slow
    /// one, because the decisions taken from it delete directories.
    pub fn load_worktrees(&mut self) {
        match crate::worktree::list() {
            Ok(list) => {
                self.worktree_selected = self.worktree_selected.min(list.len().saturating_sub(1));
                self.worktrees = Some(list);
            }
            Err(error) => {
                self.worktrees = None;
                self.notify(format!("could not read worktrees: {error}"));
            }
        }
        self.dirty = true;
    }

    /// The worktree the cursor is on.
    #[must_use]
    pub fn selected_worktree(&self) -> Option<&crate::worktree::Worktree> {
        self.worktrees.as_ref()?.get(self.worktree_selected)
    }

    /// Moves the worktree selection, stopping at both ends.
    pub fn move_worktree_selection(&mut self, forward: bool) {
        let count = self.worktrees.as_ref().map_or(0, Vec::len);
        if count == 0 {
            return;
        }

        self.worktree_selected = if forward {
            (self.worktree_selected + 1) % count
        } else {
            (self.worktree_selected + count - 1) % count
        };
        self.dirty = true;
    }

    /// Wheel scrolling on the worktrees view moves the selection.
    ///
    /// The list is short enough that there is nothing else a wheel could
    /// usefully do, and moving the selection is what you wanted anyway.
    pub fn scroll_worktrees(&mut self, scroll: i32) {
        for _ in 0..scroll.abs().min(20) {
            self.move_worktree_selection(scroll > 0);
        }
    }

    /// Counts this press as part of a multi-click, and says which it is.
    ///
    /// Two conditions, both needed. Within the double-click interval, and on
    /// the same cell — otherwise dragging out one selection and starting
    /// another somewhere else would read as a double-click and silently
    /// select a word instead.
    pub fn count_click(&mut self, row: u16, column: u16) -> u8 {
        const INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);

        let now = std::time::Instant::now();
        let clicks = match self.last_click {
            Some((at, last_row, last_column, count))
                if now.duration_since(at) < INTERVAL
                    && last_row == row
                    && last_column == column =>
            {
                // Wraps back to one after a triple, the way terminals do.
                count % 3 + 1
            }
            _ => 1,
        };

        self.last_click = Some((now, row, column, clicks));
        clicks
    }

    /// Enters copy mode on the selected session.
    pub fn start_copying(&mut self) {
        let Some(session) = self.sessions.selected() else {
            return self.notify("no session to copy from");
        };
        session.enter_copy_mode();

        let id = session.id;
        self.copying = Some(id);
        self.dirty = true;
    }

    /// Leaves copy mode, whichever session it was running in.
    pub fn stop_copying(&mut self) {
        let Some(id) = self.copying.take() else { return };
        if let Some(session) = self.sessions.by_id_mut(id.0) {
            session.exit_copy_mode();
        }
        self.dirty = true;
    }

    /// Raises a question. Nothing happens until it is answered.
    pub fn ask(&mut self, question: impl Into<String>, detail: impl Into<String>, action: Pending) {
        self.confirm = Some(Confirm { question: question.into(), detail: detail.into(), action });
        self.dirty = true;
    }

    /// Takes the pending action, and returns it only if the answer was yes.
    pub fn answer(&mut self, yes: bool) -> Option<Pending> {
        let confirm = self.confirm.take()?;
        self.dirty = true;
        yes.then_some(confirm.action)
    }

    /// What the powerline row says about your fonts.
    fn font_hint(&self) -> String {
        if self.font_install.is_some() {
            return "installing a font…".to_string();
        }

        match &self.font_detection {
            crate::fonts::Detection::Found { family } => format!("found in {family}"),
            crate::fonts::Detection::Missing => {
                "no installed font has these — expect question marks".to_string()
            }
            crate::fonts::Detection::Unknown => "arrow-shaped tabs".to_string(),
        }
    }

    /// What the install row says, which depends on whether it is worth doing.
    fn install_hint(&self) -> String {
        match &self.font_detection {
            crate::fonts::Detection::Found { .. } => {
                "you already have one; this adds another".to_string()
            }
            _ => format!("downloads {}", crate::fonts::FONT_NAME),
        }
    }

    /// Scans the installed fonts, once.
    ///
    /// Called when Settings is opened rather than at startup: it reads font
    /// files, and somebody who never opens Settings should never pay for it.
    pub fn detect_fonts(&mut self) {
        // Never in the suite. It reads every font on the machine, which makes
        // the tests slow, machine-dependent and — on a CI runner with no
        // powerline font — a full scan of `/usr/share/fonts` every time
        // anything selects Settings. The same `$HOME` rule as `App::new`.
        if cfg!(test) {
            return;
        }
        if self.font_detection == crate::fonts::Detection::Unknown {
            self.font_detection = crate::fonts::detect();
            self.rebuild_settings();
        }
    }

    /// Starts a font install on another thread.
    pub fn install_font(&mut self) {
        if self.font_install.is_some() {
            return self.notify("already installing");
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::fonts::install());
        });

        self.font_install = Some(receiver);
        self.notify(format!("downloading {}…", crate::fonts::FONT_NAME));
        self.rebuild_settings();
    }

    /// Picks up a finished install. Called from the frame tick.
    pub fn poll_font_install(&mut self) -> bool {
        let Some(receiver) = &self.font_install else { return false };
        let Ok(outcome) = receiver.try_recv() else { return false };

        self.font_install = None;
        match outcome {
            Ok(path) => {
                // Re-scan rather than assuming: the file is on disk, and the
                // scan is the thing that has been telling the truth so far.
                self.font_detection = crate::fonts::detect();
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                self.notify(format!("installed {name} — now set your terminal's font to it"));
            }
            Err(error) => self.notify(error.to_string()),
        }
        self.rebuild_settings();
        true
    }

    /// Opens the theme picker, previewing as you move through it.
    pub fn open_theme_picker(&mut self) {
        if self.themes.is_empty() {
            return self.notify("no themes are available");
        }

        let current = self.config.theme.clone();
        let selected = current
            .as_deref()
            .and_then(|name| self.themes.iter().position(|theme| theme.name == name))
            .unwrap_or(0);

        self.theme_picker = Some(ThemePicker { selected, original: current });
        self.dirty = true;
    }

    /// Moves the picker, repainting the app in whatever is now under the
    /// cursor.
    ///
    /// The preview is the feature. Cycling a hidden value and pressing Return
    /// to find out what you got is how this worked before, and it made the
    /// eighteen themes nobody had seen effectively invisible.
    pub fn move_theme_picker(&mut self, forward: bool) {
        let count = self.themes.len();
        let Some(picker) = self.theme_picker.as_mut() else { return };
        if count == 0 {
            return;
        }

        picker.selected = if forward {
            (picker.selected + 1) % count
        } else {
            (picker.selected + count - 1) % count
        };

        self.theme = self.themes[picker.selected].theme;
        self.dirty = true;
    }

    /// Keeps the previewed theme.
    pub fn accept_theme_picker(&mut self) -> Option<String> {
        let picker = self.theme_picker.take()?;
        let chosen = self.themes.get(picker.selected)?.name.clone();

        self.config.theme = Some(chosen.clone());
        self.theme = self.themes[picker.selected].theme;
        self.rebuild_settings();
        self.dirty = true;
        Some(chosen)
    }

    /// Puts back whatever was in force before the picker opened.
    pub fn cancel_theme_picker(&mut self) {
        let Some(picker) = self.theme_picker.take() else { return };

        // `config.theme` was never touched by the preview, so putting it back
        // is a matter of re-reading it.
        self.config.theme = picker.original;
        self.reload_theme_from_list();
    }

    /// Opens the form for a new note or folder.
    ///
    /// Prefilled with the folder the selection is in, so a new note lands
    /// beside what you were looking at rather than at the top of the vault.
    pub fn open_new_vault_form(&mut self) {
        let folder = self.browser.as_ref().map(Browser::target_folder).unwrap_or_default();
        let prefill = if folder.is_empty() { String::new() } else { format!("{folder}/") };

        self.form = Some(
            Form::new(vec![
                Field::text(fields::NAME, "name, or a path like projects/notes", prefill),
                Field::toggle(fields::FOLDER, "a folder rather than a note", false),
                Field::action(fields::CREATE, "make it"),
            ])
            .titled("new in the vault"),
        );
        self.form_purpose = FormPurpose::NewVaultEntry;
        self.dirty = true;
    }

    /// Opens the rename form for whatever the vault has selected.
    pub fn open_rename_vault_form(&mut self) {
        let Some(relative) = self.browser.as_ref().and_then(Browser::selected_relative) else {
            return self.notify("nothing selected");
        };

        // Prefilled with the current path, extension and all, because renaming
        // is usually editing a name rather than replacing one — and because
        // typing a folder into it is how something gets moved.
        let current = relative.strip_suffix(".md").unwrap_or(&relative).to_string();

        self.form = Some(
            Form::new(vec![
                Field::text(fields::NAME, "a new name, or a new path to move it", current),
                Field::action(fields::CREATE, "rename"),
            ])
            .titled("rename"),
        );
        self.form_purpose = FormPurpose::RenameVaultEntry;
        self.dirty = true;
    }

    /// Asks before deleting whatever the vault has selected.
    pub fn ask_remove_vault_entry(&mut self) {
        let Some(browser) = self.browser.as_ref() else { return };
        let Some(relative) = browser.selected_relative() else {
            return self.notify("nothing selected");
        };
        let Some(path) = browser.selected_path() else { return };

        // A folder takes everything under it, so the question has to say how
        // much that is. "Delete reference?" and "Delete reference? 34 notes"
        // are different questions.
        let detail = if browser.selection_is_folder() {
            match crate::vault::files::count_within(&path) {
                0 => "It is empty.".to_string(),
                1 => "The note inside it goes too.".to_string(),
                many => format!("All {many} notes inside it go too."),
            }
        } else {
            "It is not recoverable from Houston.".to_string()
        };

        self.ask(format!("Delete {relative}?"), detail, Pending::RemoveVaultEntry(path));
    }

    /// Opens the landing form for a worktree.
    ///
    /// The title names the branch and where it is going, because pushing and
    /// opening a pull request are visible to other people and a confirmation
    /// that does not say where is not a confirmation. Removal is the one
    /// toggle that starts off: it destroys a directory, and the rest of this
    /// form does not.
    pub fn open_land_form(&mut self, worktree: &crate::worktree::Worktree) {
        let branch = worktree.branch.clone().unwrap_or_else(|| "HEAD".to_string());
        let in_use = self.sessions.uses_worktree(&worktree.name);

        let mut form = Form::new(vec![
            Field::text(fields::MESSAGE, "what the agent did", ""),
            Field::toggle(fields::PUSH, "origin", true),
            Field::toggle(fields::PULL_REQUEST, "needs the gh CLI", true),
            Field::toggle(fields::REMOVE, "after it has landed", false),
            Field::action(fields::LAND, "commit, push, open a PR"),
        ])
        .titled(format!("land {branch} → origin"));

        // Offering to delete the directory a live agent is working in is not a
        // choice worth presenting.
        if in_use {
            form.set_visible(fields::REMOVE, false);
        }

        self.form = Some(form);
        self.form_purpose = FormPurpose::Land(worktree.name.clone());
        self.dirty = true;
    }

    pub fn close_form(&mut self) {
        self.form = None;
        self.form_purpose = FormPurpose::None;
        self.dirty = true;
    }

    fn with_config(config: Config) -> Self {
        Self {
            tab: Tab::Sessions,
            sessions: Sessions::new(),
            browser: None,
            config,
            config_path: if cfg!(test) {
                std::env::temp_dir().join("houston-test-config.toml")
            } else {
                crate::config::config_path()
                    .unwrap_or_else(|_| PathBuf::from("houston-config.toml"))
            },
            vault_error: None,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            should_quit: false,
            dirty: true,
            notice: None,
            quit_armed: false,
            renaming: None,
            picker: None,
            editor: None,
            form: None,
            form_purpose: FormPurpose::None,
            settings: Form::new(Vec::new()),
            last_click: None,
            dragging: false,
            copying: None,
            update_available: None,
            confirm: None,
            font_detection: crate::fonts::Detection::Unknown,
            font_install: None,
            theme_picker: None,
            diff: None,
            worktrees: None,
            worktree_selected: 0,
            input_log: VecDeque::new(),
            show_inspector: false,
            // Never the real file under test. `remember_sessions` is called
            // from half a dozen places, and a test that spawns a shell would
            // otherwise write it into the user's actual session list — the
            // same trap `Config::save_to` exists to avoid.
            state_path: if cfg!(test) {
                std::env::temp_dir().join("houston-test-state.json")
            } else {
                crate::state::path().unwrap_or_else(|_| PathBuf::from("houston-state.json"))
            },
            theme: Theme::default(),
            themes: Vec::new(),
        }
    }

    /// Opens the configured vault, creating the default one if needed.
    ///
    /// ADR-0007: only the default location is created. A configured path that
    /// is missing is reported, because it is almost certainly a typo.
    pub fn load_vault(&mut self) {
        self.vault_error = None;

        match self.config.ensure_vault().and_then(Vault::open) {
            Ok(vault) => self.browser = Some(Browser::new(vault)),
            Err(error) => {
                self.browser = None;
                self.vault_error = Some(error.to_string());
            }
        }
        self.dirty = true;
    }

    /// Where the next keystroke goes.
    ///
    /// Order matters: an overlay sits above everything, then Houston's own
    /// text fields, then an attached child, then commands.
    pub fn focus(&self) -> InputFocus {
        // Before the form check below: a confirmation can be raised from
        // Settings, and the form would otherwise eat the answer.
        if self.confirm.is_some() {
            return InputFocus::Overlay;
        }
        if self.picker.is_some() {
            return InputFocus::Overlay;
        }
        if self.form.is_some() {
            return InputFocus::Form;
        }
        // Before Settings-as-a-form, since the picker is opened *from* Settings
        // and would otherwise never see a keystroke.
        if self.theme_picker.is_some() {
            return InputFocus::Overlay;
        }
        // Settings is a form too, so editing a row there takes the keyboard
        // the same way — otherwise `q` in a path would quit the app.
        if self.tab == Tab::Settings {
            return InputFocus::Form;
        }
        if self.diff.is_some() || self.theme_picker.is_some() {
            return InputFocus::Overlay;
        }
        if self.renaming.is_some() || self.is_vault_query() {
            return InputFocus::Text;
        }
        // The editor owns both its command and its text modes, so it takes the
        // keyboard whole rather than being split across two focuses.
        if self.tab == Tab::Vault && self.editor.is_some() {
            return InputFocus::Editor;
        }
        if self.copying.is_some() {
            return InputFocus::Copy;
        }
        if self.tab == Tab::Sessions && self.sessions.focus() == Focus::Attached {
            return InputFocus::Session;
        }
        InputFocus::Commands
    }

    fn is_vault_query(&self) -> bool {
        self.tab == Tab::Vault
            && self.browser.as_ref().is_some_and(|browser| browser.mode() != VaultMode::Browsing)
    }

    /// Records an input event for the inspector.
    pub fn log_input(&mut self, description: String) {
        if self.input_log.len() == INPUT_LOG {
            self.input_log.pop_front();
        }
        self.input_log.push_back(description);
        if self.show_inspector {
            self.dirty = true;
        }
    }

    pub const fn toggle_inspector(&mut self) {
        self.show_inspector = !self.show_inspector;
        self.dirty = true;
    }

    /// Cancels any pending confirmation. Called on every key that is not the
    /// confirmation itself, so an armed quit never survives a stray keystroke.
    pub const fn disarm_quit(&mut self) {
        self.quit_armed = false;
    }

    /// First press arms, second press quits.
    ///
    /// A single keystroke should not destroy a workspace full of running
    /// agents, and `q` is far too easy to hit by accident when detaching.
    pub const fn request_quit(&mut self) {
        if self.quit_armed {
            self.should_quit = true;
        } else {
            self.quit_armed = true;
            self.dirty = true;
        }
    }

    /// Whether keystrokes belong to a child rather than to Houston.
    pub fn is_attached(&self) -> bool {
        self.focus() == InputFocus::Session
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.dirty = true;

        // Arriving is the moment the list has to be right. Loading here rather
        // than keeping it fresh in the background costs a few git calls when
        // you switch to the tab, and nothing at all while you are elsewhere.
        if tab == Tab::Worktrees {
            self.load_worktrees();
        }
        if tab == Tab::Settings {
            self.detect_fonts();
        }
    }

    pub fn cycle_tab(&mut self, forward: bool) {
        let count = Tab::ALL.len();
        let current = Tab::ALL.iter().position(|tab| *tab == self.tab).unwrap_or(0);
        let next = if forward { (current + 1) % count } else { (current + count - 1) % count };
        self.select_tab(Tab::ALL[next]);
    }

    /// Quits without confirmation. For internal use; the `q` key goes through
    /// [`Self::request_quit`].
    pub const fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.dirty = true;
    }

    pub fn clear_notice(&mut self) {
        if self.notice.take().is_some() {
            self.dirty = true;
        }
    }

    /// Keeps every child's grid matched to the area it is drawn into.
    pub fn resize_sessions(&mut self, size: Size) {
        self.sessions.resize_all(size);
    }

    /// The keybinds the footer should show, given the current view and mode.
    pub fn keybinds(&self) -> Vec<(&'static str, &'static str)> {
        if self.quit_armed {
            return vec![("q", "press again to quit"), ("esc", "stay")];
        }
        if let Some(binds) = self.focus_keybinds() {
            return binds;
        }

        let mut binds: Vec<(&'static str, &'static str)> = vec![("tab", "view"), ("1-4", "jump")];
        binds.extend(self.view_keybinds());
        binds.push(("q", "quit"));
        binds
    }

    /// Keys for whatever owns the keyboard, when that is not one of the views.
    fn focus_keybinds(&self) -> Option<Vec<(&'static str, &'static str)>> {
        if let Some(editor) = &self.editor
            && self.tab == Tab::Vault
        {
            return Some(match editor.mode {
                crate::editor::Mode::Insert => vec![("esc", "normal mode")],
                crate::editor::Mode::Jump { .. } => vec![("", "type a tag to jump")],
                crate::editor::Mode::Search { .. } => vec![("\u{21b5}", "find"), ("esc", "cancel")],
                crate::editor::Mode::Normal => vec![
                    ("hjkl", "move"),
                    ("i/a/o", "insert"),
                    ("f", "jump"),
                    ("[ ]", "heading"),
                    ("\u{21b5}", "follow link"),
                    ("t", "task"),
                    ("/", "search"),
                    ("u", "undo"),
                    ("s", "save"),
                    ("esc", "close"),
                ],
            });
        }

        match self.focus() {
            InputFocus::Session => Some(vec![
                ("ctrl+\\", "detach"),
                ("shift+pgup", "scroll back"),
                ("", "all other keys go to the session"),
            ]),
            InputFocus::Text => Some(vec![("\u{21b5}", "accept"), ("esc", "cancel")]),
            InputFocus::Overlay if self.confirm.is_some() => {
                Some(vec![("y", "yes"), ("n", "no"), ("esc", "no")])
            }
            InputFocus::Overlay if self.theme_picker.is_some() => {
                Some(vec![("j/k", "preview"), ("\u{21b5}", "keep"), ("esc", "revert")])
            }
            InputFocus::Overlay if self.diff.is_some() => Some(vec![
                ("j/k", "scroll"),
                ("u/d", "page"),
                ("g/G", "top/bottom"),
                ("esc", "close"),
            ]),
            InputFocus::Overlay => Some(vec![
                ("j/k", "choose"),
                ("1-9", "jump"),
                ("\u{21b5}", "send"),
                ("esc", "cancel"),
            ]),
            InputFocus::Copy => Some(self.copy_keybinds()),
            InputFocus::Form => Some(self.form_keybinds()),
            InputFocus::Commands | InputFocus::Editor => None,
        }
    }

    /// Keys for copy mode. What `v` does depends on whether anything is
    /// selected yet, and saying so is most of what makes the mode learnable.
    fn copy_keybinds(&self) -> Vec<(&'static str, &'static str)> {
        let selecting = self.sessions.selected().is_some_and(|session| {
            session.pty().term().lock().is_ok_and(|term| term.selection.is_some())
        });

        vec![
            ("hjkl", "move"),
            ("w/b", "word"),
            ("0/$", "line"),
            ("g/G", "top/bottom"),
            ("v", if selecting { "drop selection" } else { "start selecting" }),
            ("y", "copy"),
            ("esc", "done"),
        ]
    }

    /// Keys for a form, which behaves the same whether it is Settings or the
    /// new-session dialog.
    fn form_keybinds(&self) -> Vec<(&'static str, &'static str)> {
        let form = self.form.as_ref().unwrap_or(&self.settings);

        if form.is_editing() {
            let completes =
                form.focused().is_some_and(|f| f.kind == crate::form::FieldKind::Directory);
            let mut binds = vec![("\u{21b5}", "done"), ("esc", "cancel")];
            if completes {
                binds.insert(0, ("tab", "complete"));
            }
            return binds;
        }

        let mut binds = vec![("j/k", "select"), ("\u{21b5}", "edit")];
        if self.form.is_some() {
            binds.push(("esc", "cancel"));
        } else {
            binds.push(("tab", "view"));
            binds.push(("q", "quit"));
        }
        binds
    }

    /// Keys for the current view.
    fn view_keybinds(&self) -> Vec<(&'static str, &'static str)> {
        let mut binds: Vec<(&'static str, &'static str)> = Vec::new();

        match self.tab {
            Tab::Sessions => {
                binds.extend([("n", "agent"), ("s", "shell")]);
                if !self.sessions.is_empty() {
                    binds.extend([
                        ("j/k", "select"),
                        ("↵", "attach"),
                        ("v", "review"),
                        ("c", "copy"),
                        ("u/d", "scroll"),
                        ("r", "rename"),
                        ("x", "close"),
                    ]);
                }
            }
            Tab::Board if !self.sessions.is_empty() => {
                binds.extend([("hjkl", "move"), ("↵", "open session"), ("v", "review")]);
            }
            Tab::Worktrees => {
                if self.worktrees.as_ref().is_some_and(|list| !list.is_empty()) {
                    binds.extend([
                        ("j/k", "select"),
                        ("↵", "go to its session"),
                        ("v", "review"),
                        ("l", "land"),
                        ("d", "remove"),
                    ]);
                }
                binds.push(("r", "refresh"));
            }

            Tab::Vault if self.browser.is_some() => {
                binds.extend([
                    ("j/k", "select"),
                    ("↵", "open/close"),
                    ("n", "new"),
                    ("r", "rename"),
                    ("x", "delete"),
                    ("/", "find"),
                    ("f", "search"),
                    ("y", "yank"),
                    ("i", "to session"),
                    ("e", "edit"),
                ]);
            }
            _ => {}
        }

        binds
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycling_wraps_in_both_directions() {
        let mut app = App::new();
        assert_eq!(app.tab, Tab::Sessions);

        app.cycle_tab(false);
        assert_eq!(app.tab, Tab::Settings, "cycling back from the first tab should wrap");

        app.cycle_tab(true);
        assert_eq!(app.tab, Tab::Sessions, "cycling forward from the last tab should wrap");
    }

    #[test]
    fn selecting_the_current_tab_does_not_dirty_the_frame() {
        let mut app = App::new();
        app.dirty = false;

        app.select_tab(Tab::Sessions);
        assert!(!app.dirty, "re-selecting the active tab should not force a redraw");

        app.select_tab(Tab::Board);
        assert!(app.dirty, "changing tab should force a redraw");
    }

    fn spare_worktree(name: &str) -> crate::worktree::Worktree {
        crate::worktree::Worktree {
            name: name.to_string(),
            path: std::env::temp_dir().join(name),
            repository: std::env::temp_dir().join("repo"),
            branch: Some("agent/thing".to_string()),
            dirty: true,
            ahead: 0,
            behind: 0,
            changes: None,
        }
    }

    /// Pushing and opening a pull request are visible to other people, so the
    /// confirmation has to say where the work is going.
    #[test]
    fn the_landing_form_names_the_branch_and_the_remote() {
        let mut app = App::new();
        app.open_land_form(&spare_worktree("wt"));

        let form = app.form.as_ref().expect("the form opened");
        assert!(form.title.contains("agent/thing"), "the branch is named: {}", form.title);
        assert!(form.title.contains("origin"), "and where it is going: {}", form.title);
    }

    #[test]
    fn landing_defaults_to_publishing_but_never_to_deleting() {
        let mut app = App::new();
        app.open_land_form(&spare_worktree("wt"));

        let form = app.form.as_ref().unwrap();
        assert!(form.is_on(fields::PUSH), "landing without pushing is not landing");
        assert!(form.is_on(fields::PULL_REQUEST));
        assert!(
            !form.is_on(fields::REMOVE),
            "removing the directory is the one step here that cannot be undone"
        );
    }

    /// The whole point of the picker: you see the theme before you commit to
    /// it.
    #[test]
    fn moving_through_the_picker_repaints_the_app() {
        let mut app = App::new();
        app.themes = crate::ui::theme::built_in();
        app.open_theme_picker();

        let before = app.theme;
        app.move_theme_picker(true);

        assert_ne!(app.theme, before, "the app is drawn in whatever is under the cursor");
        assert!(app.config.theme.is_none(), "previewing must not write to config");
    }

    #[test]
    fn escaping_the_picker_puts_the_old_theme_back() {
        let mut app = App::new();
        app.themes = crate::ui::theme::built_in();
        app.config.theme = Some("Nord".to_string());
        app.reload_theme_from_list();

        let before = app.theme;
        app.open_theme_picker();
        app.move_theme_picker(true);
        app.move_theme_picker(true);
        app.cancel_theme_picker();

        assert_eq!(app.theme, before, "esc means the preview never happened");
        assert_eq!(app.config.theme.as_deref(), Some("Nord"), "and the setting is untouched");
    }

    #[test]
    fn keeping_a_theme_records_it_as_the_setting() {
        let mut app = App::new();
        app.themes = crate::ui::theme::built_in();
        app.open_theme_picker();
        app.move_theme_picker(true);

        let kept = app.accept_theme_picker().expect("something was chosen");

        assert_eq!(app.config.theme.as_deref(), Some(kept.as_str()));
        assert!(app.theme_picker.is_none(), "keeping closes the picker");
    }

    #[test]
    fn the_picker_opens_on_whatever_is_already_in_force() {
        let mut app = App::new();
        app.themes = crate::ui::theme::built_in();
        app.config.theme = Some("Gruvbox Dark".to_string());
        app.open_theme_picker();

        let picker = app.theme_picker.as_ref().unwrap();
        assert_eq!(
            app.themes[picker.selected].name, "Gruvbox Dark",
            "it starts where you are, not at the top of a list of nineteen"
        );
    }

    /// A lone capital in the footer is a bad smell, and this is the rule that
    /// came out of removing the two that were there.
    ///
    /// `D` (force-remove a worktree) and `W` (jump to worktrees) were both
    /// single capitals standing alone. The first hid a destructive action
    /// behind a key you had to already know about; the second was a second way
    /// to do what the tab strip does. Both are gone.
    ///
    /// What is still allowed is a capital shown *beside its lower-case pair* —
    /// `g/G` for top and bottom. That is vim vocabulary, it is not destructive,
    /// and showing both halves is what makes the case meaningful rather than
    /// hidden.
    #[test]
    fn no_view_advertises_a_lone_capital() {
        let mut app = App::new();
        app.sessions.spawn_shell(&std::env::temp_dir(), crate::pty::Size::new(24, 80)).unwrap();
        app.worktrees = Some(vec![spare_worktree("wt")]);

        for tab in Tab::ALL {
            app.tab = tab;
            for (key, action) in app.keybinds() {
                let lone_capital =
                    key.len() == 1 && key.chars().next().is_some_and(|c| c.is_ascii_uppercase());

                assert!(
                    !lone_capital,
                    "{tab:?} offers {key:?} for {action:?}. A capital on its own is either a \
                     destructive action hiding behind shift, or a shortcut for something the \
                     tab strip already does. Show it as `x/X` if the pair is the point."
                );
            }
        }
    }

    /// The scan can only ever prove absence, so the wording has to be honest
    /// about which way round the answer is.
    #[test]
    fn the_font_hint_distinguishes_not_looked_from_looked_and_found_nothing() {
        let mut app = App::new();

        app.font_detection = crate::fonts::Detection::Unknown;
        let unknown = app.font_hint();

        app.font_detection = crate::fonts::Detection::Missing;
        let missing = app.font_hint();
        assert!(missing.contains("question marks"), "it warns what will happen: {missing}");
        assert_ne!(unknown, missing, "'we did not look' must not read as 'you have none'");

        app.font_detection =
            crate::fonts::Detection::Found { family: "Meslo LG S for Powerline".to_string() };
        assert!(app.font_hint().contains("Meslo"), "and names what it found");
    }

    /// Installing is offered whatever the scan said, because the scan cannot
    /// see which font the terminal is actually using — but the wording changes.
    #[test]
    fn the_install_row_says_something_different_once_a_font_is_found() {
        let mut app = App::new();

        app.font_detection = crate::fonts::Detection::Missing;
        assert!(app.install_hint().contains(crate::fonts::FONT_NAME));

        app.font_detection = crate::fonts::Detection::Found { family: "Something".to_string() };
        assert!(app.install_hint().contains("already"), "it does not pretend you need it");
    }

    #[test]
    fn a_finished_install_is_picked_up_and_a_pending_one_is_not() {
        let mut app = App::new();
        assert!(!app.poll_font_install(), "nothing running, nothing to pick up");

        let (sender, receiver) = std::sync::mpsc::channel();
        app.font_install = Some(receiver);
        assert!(!app.poll_font_install(), "still running, so the loop is not blocked on it");

        sender.send(Err(anyhow::anyhow!("no network"))).unwrap();
        assert!(app.poll_font_install(), "the result arrives on a later tick");
        assert!(app.font_install.is_none(), "and the install is over");
        assert!(
            app.notice.as_deref().is_some_and(|notice| notice.contains("no network")),
            "a failure says why rather than silently doing nothing"
        );
    }

    /// Guards the trap in `App::new`: the suite must not vary with whatever
    /// is in the developer's `~/.houston/config.toml`.
    #[test]
    fn a_test_app_is_built_from_defaults_not_from_the_real_config() {
        let app = App::new();

        assert_eq!(
            app.theme,
            Theme::default(),
            "a theme set in the user's own config must not reach the suite"
        );
        assert!(app.themes.is_empty(), "nor may theme files in ~/.houston/themes");
    }

    #[test]
    fn keybinds_follow_the_view() {
        let mut app = App::new();

        let sessions: Vec<_> = app.keybinds().iter().map(|(key, _)| *key).collect();
        assert!(sessions.contains(&"n"), "the sessions view offers a new agent");
        assert!(!sessions.contains(&"x"), "with no sessions there is nothing to close");

        app.select_tab(Tab::Vault);
        let vault: Vec<_> = app.keybinds().iter().map(|(key, _)| *key).collect();
        assert!(!vault.contains(&"s"), "session keys must not leak into other views");
        assert!(vault.contains(&"q"));

        // `n` is in both, on purpose: it means "make a new one of whatever
        // this view holds". A session here, a note there.
        assert!(vault.contains(&"n"), "the vault makes new notes");
    }

    #[test]
    fn an_unattached_app_is_never_treated_as_attached() {
        let app = App::new();
        assert!(!app.is_attached(), "with no sessions there is nothing to be attached to");
    }
}
