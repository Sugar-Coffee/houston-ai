//! The event loop.
//!
//! Two properties matter here and both are load-bearing:
//!
//! 1. **Paste is one event, not N key events.** `Event::Paste` arrives whole
//!    and is forwarded to the child in a single write. ADR-0005.
//! 2. **Rendering is frame-budgeted, not event-driven.** Events and child
//!    output mark the app dirty; a tick draws at most once per frame. An agent
//!    emitting thousands of lines a second must not cost thousands of repaints.

use crate::{
    app::{App, FormPurpose, InputFocus, Picker, Tab, fields},
    clipboard,
    editor::Editor,
    form::Activation,
    hooks::{self, Notification},
    paths,
    pty::Size,
    terminal::Backend,
    ui,
    vault::browser::Mode as VaultMode,
};
use anyhow::Result;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use futures::StreamExt;
use ratatui::{Terminal, layout::Rect};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// ~60fps. The upper bound on how often we draw, not how often we poll.
const FRAME_BUDGET: Duration = Duration::from_millis(16);

/// How often session branches are re-read.
///
/// A branch changes when you switch it, not when you blink. Reading `.git/HEAD`
/// is cheap but not free, and doing it per frame would be sixty file reads a
/// second per session for information that changes hourly.
const BRANCH_REFRESH: Duration = Duration::from_secs(3);

pub async fn run(terminal: &mut Terminal<Backend>, mut app: App) -> Result<()> {
    let mut input = EventStream::new();
    let mut frames = tokio::time::interval(FRAME_BUDGET);
    frames.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut branches = tokio::time::interval(BRANCH_REFRESH);
    branches.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Agent lifecycle events arrive over a Unix socket. If the listener cannot
    // start, Houston still works — the board just cannot show agent state — so
    // this is reported rather than fatal.
    // The setting decides; ignoring a failure here costs the wheel, not the app.
    let _ = crate::terminal::set_mouse(app.config.mouse_enabled());

    // Reopen last time's sessions before the first frame, sized from the real
    // layout rather than a guess — a child that starts at 24x80 and is resized
    // a moment later redraws itself, which is visible.
    let restore_size = session_area(terminal)
        .map_or_else(|_| Size::new(24, 80), |area| Size::new(area.height, area.width));
    app.restore_sessions(restore_size);

    let (hook_sender, mut hook_events) = tokio::sync::mpsc::unbounded_channel();
    let _listener = match hooks::Listener::start(hook_sender) {
        Ok(listener) => Some(listener),
        Err(error) => {
            app.notify(format!("agent status unavailable: {error}"));
            None
        }
    };

    loop {
        tokio::select! {
            // Keystrokes are checked before the frame tick, so typing never
            // waits on a draw.
            biased;

            Some(event) = input.next() => {
                    let area = session_area(terminal).ok();
                    handle(&mut app, &event?, area);
                }

            Some(notification) = hook_events.recv() => on_hook(&mut app, &notification),

            _ = branches.tick() => {
                app.sessions.refresh_branches();
                app.sessions.refresh_selected_changes();
                app.dirty = true;
            }

            _ = frames.tick() => {
                // Children draw on their own schedule; ask them what changed.
                if app.sessions.poll() {
                    app.dirty = true;
                }

                if app.dirty {
                    sync_session_size(terminal, &mut app)?;
                    sync_editor_viewport(terminal, &mut app)?;
                    terminal.draw(|frame| ui::render(frame, &app))?;
                    app.dirty = false;
                }
            }
        }

        if app.should_quit {
            app.remember_sessions();
            // Signalled and detached, so restoring the terminal is never held
            // up by a child that is slow to die.
            app.sessions.shutdown();
            return Ok(());
        }
    }
}

/// The rectangle the attached session's grid is drawn into.
///
/// Needed to translate a mouse position into the child's own coordinates.
fn session_area(terminal: &Terminal<Backend>) -> Result<Rect> {
    let [_, body, _] = ui::layout(terminal.size()?.into());
    Ok(ui::sessions::terminal_area(body))
}

/// Keeps every child's grid the same size as the area it is drawn into.
///
/// Derived from the same layout functions the renderer uses, so the two cannot
/// drift. If they did, child output would wrap in the wrong column.
/// Tells the editor how many rows it has.
///
/// Only the renderer knows, and the editor needs it for scrolling and for
/// tagging exactly the lines that are on screen — a jump mode that tags
/// invisible lines is worse than no jump mode.
fn sync_editor_viewport(terminal: &Terminal<Backend>, app: &mut App) -> Result<()> {
    let Some(editor) = app.editor.as_mut() else { return Ok(()) };
    let [_, body, _] = ui::layout(terminal.size()?.into());
    let (rows, width) = ui::editor::text_shape(body);
    editor.set_viewport(rows, width);
    editor.follow_cursor();
    Ok(())
}

fn sync_session_size(terminal: &Terminal<Backend>, app: &mut App) -> Result<()> {
    let [_, body, _] = ui::layout(terminal.size()?.into());
    let pane = ui::sessions::terminal_area(body);
    app.resize_sessions(Size::new(pane.height, pane.width));
    Ok(())
}

/// Applies an agent lifecycle event reported by a hook.
fn on_hook(app: &mut App, notification: &Notification) {
    if app.sessions.apply_hook(
        notification.session,
        notification.kind,
        notification.conversation.as_deref(),
    ) {
        app.dirty = true;
        // The agent just did something, so its diff may have moved. This is
        // the cheap half of keeping the number current — see `Session::changes`.
        if let Some(session) = app.sessions.by_id_mut(notification.session) {
            session.refresh_changes();
        }
        // A newly-learned conversation id is worth persisting straight away:
        // it is the difference between resuming and starting over.
        app.remember_sessions();
    }
}

/// A one-line description of an input event, for the inspector.
///
/// Deliberately says what *arrived*, not what Houston did with it — the
/// question this answers is whether the terminal is sending anything at all.
fn describe(event: &Event) -> String {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let mods = modifier_names(key.modifiers);
            format!("key    {mods}{:?}", key.code)
        }
        Event::Key(_) => "key    (release)".to_string(),
        Event::Mouse(mouse) => {
            let mods = modifier_names(mouse.modifiers);
            format!("mouse  {mods}{:?} at {},{}", mouse.kind, mouse.column, mouse.row)
        }
        Event::Paste(text) => format!("paste  {} chars", text.chars().count()),
        Event::Resize(columns, rows) => format!("resize {columns}x{rows}"),
        Event::FocusGained => "focus  gained".to_string(),
        Event::FocusLost => "focus  lost".to_string(),
    }
}

fn modifier_names(modifiers: KeyModifiers) -> String {
    let mut names = String::new();
    for (flag, name) in [
        (KeyModifiers::CONTROL, "ctrl+"),
        (KeyModifiers::ALT, "alt+"),
        (KeyModifiers::SHIFT, "shift+"),
    ] {
        if modifiers.contains(flag) {
            names.push_str(name);
        }
    }
    names
}

/// The wheel, and clicks.
///
/// Everything except an attached session treats the wheel as "scroll what I am
/// looking at". An attached session hands it to `Session::send_mouse`, which
/// decides whether the child wants it.
fn on_mouse(app: &mut App, mouse: MouseEvent, area: Option<Rect>) {
    let scroll: i32 = match mouse.kind {
        MouseEventKind::ScrollUp => -1,
        MouseEventKind::ScrollDown => 1,
        _ => 0,
    };

    if app.is_attached() {
        let Some(area) = area else { return };
        // Translate to the child's own grid, so a forwarded click lands where
        // it was aimed rather than where it was on our screen.
        let column = mouse.column.saturating_sub(area.x);
        let line = mouse.row.saturating_sub(area.y);

        if let Some(session) = app.sessions.selected_mut()
            && let Err(error) = session.send_mouse(mouse, column, line)
        {
            app.notify(format!("mouse: {error}"));
        }
        app.dirty = true;
        return;
    }

    if scroll == 0 {
        return;
    }
    app.dirty = true;

    match app.tab {
        Tab::Sessions => {
            // Not attached, so the wheel still scrolls the visible session —
            // reading back through an agent's output without attaching first
            // is a normal thing to want.
            if let Some(session) = app.sessions.selected() {
                session.scroll(-scroll * 3);
            }
        }
        Tab::Vault => {
            if let Some(editor) = app.editor.as_mut() {
                editor.scroll_rows(isize::try_from(scroll * 3).unwrap_or(0));
            } else if let Some(browser) = app.browser.as_mut() {
                browser.scroll(scroll * 3);
            }
        }
        Tab::Worktrees => app.scroll_worktrees(scroll),
        Tab::Board | Tab::Settings => {}
    }
}

fn handle(app: &mut App, event: &Event, terminal_area: Option<Rect>) {
    app.log_input(describe(event));

    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.clear_notice();
            on_key(app, *key);
        }
        Event::Paste(text) => on_paste(app, text),
        Event::Mouse(mouse) => on_mouse(app, *mouse, terminal_area),
        Event::Resize(_, _) => app.dirty = true,
        _ => {}
    }
}

/// A pasted block goes to the child in one write, or is ignored if there is no
/// child to receive it. Never re-encoded as individual keystrokes.
fn on_paste(app: &mut App, text: &str) {
    if !app.is_attached() {
        return;
    }
    if let Some(session) = app.sessions.selected_mut()
        && let Err(error) = session.send_paste(text)
    {
        app.notify(format!("paste failed: {error}"));
    }
}

fn on_key(app: &mut App, key: KeyEvent) {
    // An armed quit is cancelled by anything that is not a second `q`. This
    // has to run before the key is dispatched, or the confirmation would
    // survive whatever the key did.
    if app.quit_armed && !matches!(key.code, KeyCode::Char('q')) {
        app.disarm_quit();
        app.dirty = true;
        // Escape is "never mind" and should do nothing else.
        if key.code == KeyCode::Esc {
            return;
        }
    }

    // Intercepted everywhere, including while attached: the inspector exists
    // for the case where something is swallowing input, so it cannot itself
    // depend on input reaching the usual place.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
        app.toggle_inspector();
        return;
    }

    match app.focus() {
        InputFocus::Form => on_key_form(app, key),
        InputFocus::Editor => on_key_editor(app, key),
        InputFocus::Session => on_key_attached(app, key),
        InputFocus::Overlay => on_key_picker(app, key),
        InputFocus::Text => on_key_typing(app, key),
        InputFocus::Commands => on_key_browsing(app, key),
    }
}

/// The editor. Dispatches on its own mode rather than Houston's focus, because
/// modality is the editor's whole idea (ADR-0003).
fn on_key_editor(app: &mut App, key: KeyEvent) {
    use crate::editor::Mode;

    let Some(editor) = app.editor.as_mut() else { return };
    app.dirty = true;

    match editor.mode.clone() {
        Mode::Insert => on_key_editor_insert(app, key),
        Mode::Jump { .. } => match key.code {
            KeyCode::Esc => editor.enter_normal(),
            KeyCode::Char(character) => {
                editor.jump_input(character);
                editor.follow_cursor();
            }
            _ => {}
        },
        Mode::Search { .. } => match key.code {
            KeyCode::Esc => editor.enter_normal(),
            KeyCode::Backspace => editor.search_backspace(),
            KeyCode::Char(character) => editor.search_input(character),
            KeyCode::Enter if !editor.search_accept() => app.notify("no match"),
            _ => {}
        },
        Mode::Normal => on_key_editor_normal(app, key),
    }
}

fn on_key_editor_insert(app: &mut App, key: KeyEvent) {
    let Some(editor) = app.editor.as_mut() else { return };

    match key.code {
        KeyCode::Esc => editor.enter_normal(),
        KeyCode::Char(character) => editor.buffer.insert(&character.to_string()),
        // List-aware: continues the list, or ends it on an empty item.
        KeyCode::Enter => editor.insert_newline(),
        // Markdown nests with spaces, and a literal tab in a note is a
        // rendering hazard nobody wants to debug.
        KeyCode::Tab => editor.buffer.insert("  "),
        KeyCode::Backspace => {
            editor.buffer.delete_backwards();
        }
        KeyCode::Delete => {
            editor.buffer.delete_forwards();
        }
        KeyCode::Left => editor.buffer.move_left(),
        KeyCode::Right => editor.buffer.move_right(),
        KeyCode::Up => editor.move_visual(false),
        KeyCode::Down => editor.move_visual(true),
        _ => {}
    }
    editor.follow_cursor();
}

fn on_key_editor_normal(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let Some(editor) = app.editor.as_mut() else { return };

    // Anything that is not another close attempt cancels a pending discard.
    if !matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
        editor.disarm_close();
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => editor.buffer.move_left(),
        KeyCode::Char('l') | KeyCode::Right => editor.buffer.move_right(),
        KeyCode::Char('j') | KeyCode::Down => editor.move_visual(true),
        KeyCode::Char('k') | KeyCode::Up => editor.move_visual(false),
        KeyCode::Char('0') | KeyCode::Home => editor.buffer.move_line_start(),
        KeyCode::Char('$') | KeyCode::End => editor.buffer.move_line_end(),
        KeyCode::Char('g') => editor.buffer.move_buffer_start(),
        KeyCode::Char('G') => editor.buffer.move_buffer_end(),
        // Paging moves by visual rows, not logical lines: one line in this
        // vault can be fifty rows tall.
        KeyCode::Char('d') if ctrl => editor.page(true),
        KeyCode::Char('u') if ctrl => editor.page(false),
        KeyCode::PageDown => editor.page(true),
        KeyCode::PageUp => editor.page(false),

        KeyCode::Char('i') => editor.enter_insert(),
        KeyCode::Char('a') => {
            editor.buffer.move_right();
            editor.enter_insert();
        }
        KeyCode::Char('A') => {
            editor.buffer.move_line_end();
            editor.enter_insert();
        }
        KeyCode::Char('o') => {
            editor.buffer.move_line_end();
            editor.buffer.insert("\n");
            editor.enter_insert();
        }

        // amp's signature move.
        KeyCode::Char('f') => editor.enter_jump(),

        // Markdown-native navigation and editing. Chosen by counting the real
        // vault: 325 of 328 notes have headings, 308 have bullet lists.
        KeyCode::Char(']') => {
            if !editor.jump_heading(true) {
                app.notify("no headings in this note");
            }
        }
        KeyCode::Char('[') => {
            if !editor.jump_heading(false) {
                app.notify("no headings in this note");
            }
        }
        KeyCode::Char('t') => {
            if !editor.toggle_task() {
                app.notify("not a task line");
            }
        }
        KeyCode::Enter => return follow_link(app),
        KeyCode::Char('/') => editor.enter_search(),
        KeyCode::Char('n') => {
            if !editor.search_next() {
                app.notify("no match");
            }
        }

        KeyCode::Char('x') => {
            editor.buffer.delete_forwards();
        }
        KeyCode::Char('d') => editor.buffer.delete_line(),
        KeyCode::Char('u') => {
            if !editor.buffer.undo() {
                app.notify("nothing to undo");
            }
        }
        KeyCode::Char('r') if ctrl => {
            if !editor.buffer.redo() {
                app.notify("nothing to redo");
            }
        }

        KeyCode::Char('s') => save_editor(app, false),
        KeyCode::Char('S') => save_editor(app, true),
        KeyCode::Esc | KeyCode::Char('q') => close_editor(app),
        _ => {}
    }

    if let Some(editor) = app.editor.as_mut() {
        editor.follow_cursor();
    }
}

/// Opens the `[[wikilink]]` under the cursor in the editor.
///
/// Following a link should not mean closing the editor first — links are how
/// this vault is navigated, and 157 of its 328 notes use them.
fn follow_link(app: &mut App) {
    let Some(target) = app.editor.as_ref().and_then(Editor::link_under_cursor) else {
        app.notify("no link under the cursor");
        return;
    };

    let unsaved = app.editor.as_ref().is_some_and(|editor| editor.buffer.modified);
    if unsaved {
        app.notify("save first — following a link would lose unsaved changes");
        return;
    }

    let Some(resolved) = app
        .browser
        .as_ref()
        .and_then(|browser| browser.vault.resolve_link(&target))
        .and_then(|id| app.browser.as_ref().and_then(|browser| browser.vault.get(id)))
        .map(|note| note.path.clone())
    else {
        app.notify(format!("no note called {target}"));
        return;
    };

    match Editor::open(&resolved) {
        Ok(editor) => {
            app.editor = Some(editor);
            app.dirty = true;
        }
        Err(error) => app.notify(format!("could not open {target}: {error}")),
    }
}

fn save_editor(app: &mut App, force: bool) {
    let Some(editor) = app.editor.as_mut() else { return };
    match editor.buffer.save(force) {
        Ok(path) => {
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            app.notify(format!("saved {name}"));
        }
        Err(error) => app.notify(error.to_string()),
    }
}

/// Closes the editor, confirming first if there are unsaved changes.
fn close_editor(app: &mut App) {
    let confirmed = match app.editor.as_mut() {
        Some(editor) if editor.buffer.modified => editor.arm_close(),
        Some(_) => true,
        None => return,
    };

    if !confirmed {
        app.notify("unsaved changes — s to save, or esc again to discard them");
        return;
    }

    app.editor = None;
    app.dirty = true;
}

/// Forms: the new-session dialog and the Settings menu.
///
/// Two levels. Not editing, keys move between fields and Return activates one.
/// Editing, keys are text and Return commits — so `q` in a path is a `q`,
/// not a quit.
fn on_key_form(app: &mut App, key: KeyEvent) {
    let modal = app.form.is_some();
    app.dirty = true;

    let Some(form) = app.form.as_mut().or(Some(&mut app.settings)) else { return };

    if form.is_editing() {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                form.commit_field();
                apply_form_field(app, modal);
            }
            KeyCode::Tab => form.complete(),
            KeyCode::Backspace => form.pop(),
            KeyCode::Char(character) => form.push(character),
            _ => {}
        }
        return;
    }

    match key.code {
        // Tab belongs to the view switcher everywhere else, so in Settings it
        // keeps that job and the arrows walk the list. A modal form has no
        // views to switch between, so there Tab moves between fields.
        KeyCode::Tab | KeyCode::BackTab if !modal => on_key_browsing(app, key),
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => form.move_focus(true),
        KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => form.move_focus(false),
        // The Theme row opens a picker rather than cycling a hidden value.
        // Nineteen themes cycled one keypress at a time is not a choice, it is
        // an endurance test.
        KeyCode::Enter | KeyCode::Char(' ')
            if !modal && form.focused().is_some_and(|f| f.label == fields::THEME) =>
        {
            app.open_theme_picker();
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let activation = form.activate();
            sync_form_visibility(app, modal);

            if activation == Activation::Submitted {
                accept_form(app);
            } else {
                apply_form_field(app, modal);
            }
        }
        KeyCode::Esc if modal => app.close_form(),
        KeyCode::Char('q') if !modal => app.request_quit(),
        _ if !modal => on_key_browsing(app, key),
        _ => {}
    }
}

/// Shows the worktree name field only when a worktree was asked for.
fn sync_form_visibility(app: &mut App, modal: bool) {
    if !modal {
        return;
    }
    let wanted = app.form.as_ref().is_some_and(|form| form.is_on(fields::WORKTREE));
    if let Some(form) = app.form.as_mut() {
        form.set_visible(fields::WORKTREE_NAME, wanted);
    }
}

/// Settings apply as soon as a field is committed — there is no save button,
/// because a settings menu with one is a settings menu you can leave in a
/// state that does not match what the app is doing.
fn apply_form_field(app: &mut App, modal: bool) {
    if modal {
        return;
    }

    // Applied immediately: a mouse setting that needs a restart is a mouse
    // setting you cannot tell whether you like.
    let mouse = app.settings.is_on(fields::MOUSE);
    if mouse != app.config.mouse_enabled() {
        app.config.mouse = Some(mouse);
        let _ = crate::terminal::set_mouse(mouse);
    }

    // Applied immediately, so you choose a theme by looking at it rather than
    // by reading its name.
    let chosen = app.settings.value(fields::THEME);
    if Some(chosen.as_str()) != app.config.theme.as_deref() && !chosen.is_empty() {
        app.config.theme = Some(chosen);
        app.reload_theme();
    }

    let powerline = app.settings.is_on(fields::POWERLINE);
    if Some(powerline) != app.config.powerline {
        app.config.powerline = Some(powerline);
    }

    let vault = app.settings.value(fields::VAULT);
    let agent = app.settings.value(fields::AGENT_DIRECTORY);

    let previous = app.config.vault.clone();
    app.config.vault = (!vault.trim().is_empty()).then(|| PathBuf::from(vault.trim()));
    app.config.agent_directory = (!agent.trim().is_empty()).then(|| PathBuf::from(agent.trim()));

    app.load_vault();
    if let Some(error) = app.vault_error.clone() {
        app.config.vault = previous;
        app.load_vault();
        app.notify(error);
        app.rebuild_settings();
        return;
    }

    if let Err(error) = app.config.save_to(&app.config_path) {
        app.notify(format!("could not save settings: {error}"));
    }
}

/// Creates the session the new-session form describes.
fn accept_form(app: &mut App) {
    if let FormPurpose::Land(name) = app.form_purpose.clone() {
        return accept_land(app, &name);
    }

    let Some(form) = app.form.as_ref() else { return };

    let name = form.field(fields::NAME).map(|field| field.value.trim().to_string());
    let name = name.filter(|name| !name.is_empty());
    let directory = form.field(fields::DIRECTORY).map_or_else(
        || app.config.agent_root(),
        |field| paths::expand_home(Path::new(&field.value)),
    );
    let wants_worktree = form.is_on(fields::WORKTREE);
    let worktree_name = form
        .field(fields::WORKTREE_NAME)
        .map(|field| field.value.trim().to_string())
        .filter(|value| !value.is_empty());

    if !directory.is_dir() {
        app.notify(format!("{} is not a directory", directory.display()));
        return;
    }

    // A worktree replaces the working directory, so it has to succeed before
    // anything is spawned — starting an agent in the wrong place is worse than
    // not starting it.
    let (working_directory, worktree) = if wants_worktree {
        let label = worktree_name.or_else(|| name.clone()).unwrap_or_else(|| "session".to_string());

        match crate::worktree::create(&directory, &label) {
            Ok(worktree) => (worktree.path.clone(), Some(worktree)),
            Err(error) => {
                app.notify(error.to_string());
                return;
            }
        }
    } else {
        (directory, None)
    };

    app.close_form();

    let size = Size::new(24, 80);
    match app.sessions.spawn_agent(&working_directory, size) {
        Ok(_) => {
            if let Some(session) = app.sessions.selected_mut() {
                if let Some(name) = name {
                    session.rename(Some(name));
                }
                session.worktree = worktree.map(|worktree| worktree.name);
            }
            app.sessions.attach();
            app.select_tab(Tab::Sessions);
            app.remember_sessions();
            if let Some(warning) = app.sessions.take_hook_warning() {
                app.notify(warning);
            }
        }
        Err(error) => app.notify(format!("could not start a session: {error}")),
    }
}

/// Carries out a landing, and says what happened either way.
///
/// The worktree is re-read rather than taken from the manager's list: the
/// list was loaded when the overlay opened, and an agent that has been
/// working since then will have made it dirty. Committing on a stale `dirty`
/// flag would skip the commit and push an empty branch.
fn accept_land(app: &mut App, name: &str) {
    let Some(form) = app.form.as_ref() else { return };

    let request = crate::worktree::Land {
        message: form.value(fields::MESSAGE),
        push: form.is_on(fields::PUSH),
        pull_request: form.is_on(fields::PULL_REQUEST),
        remove: form.is_on(fields::REMOVE),
    };

    let worktree = crate::worktree::list()
        .ok()
        .and_then(|list| list.into_iter().find(|item| item.name == name));
    let Some(worktree) = worktree else {
        app.close_form();
        return app.notify(format!("{name} is gone"));
    };

    match crate::worktree::plan(&worktree, &request)
        .and_then(|steps| crate::worktree::land(&worktree, &steps))
    {
        Ok(report) => {
            app.close_form();
            app.notify(report);
            app.load_worktrees();
        }
        // The form stays open on failure. A rejected push usually needs one
        // toggle changed, and reopening it from the manager to change that
        // toggle would be the app's idea of a joke.
        Err(error) => app.notify(error.to_string()),
    }
}

/// Opens the selected session's diff.
///
/// Refreshes the count first, so the header agrees with the body. The card's
/// number can be a few seconds stale by design; the view you deliberately
/// opened should not be.
fn open_diff(app: &mut App) {
    let Some(session) = app.sessions.selected_mut() else {
        return app.notify("no session to review");
    };
    session.refresh_changes();

    let (name, cwd) = (session.name.clone(), session.spec.cwd.clone());
    match crate::diff::View::open(name, &cwd) {
        Ok(view) => {
            app.diff = Some(view);
            app.dirty = true;
        }
        Err(error) => app.notify(error.to_string()),
    }
}

/// Opens the landing form for the selected worktree.
fn land_worktree(app: &mut App) {
    let Some(worktree) = app.selected_worktree().cloned() else {
        return app.notify("no worktree to land");
    };
    app.open_land_form(&worktree);
}

/// The worktrees view.
fn on_key_worktrees(app: &mut App, key: KeyEvent) {
    app.dirty = true;

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_worktree_selection(true),
        KeyCode::Char('k') | KeyCode::Up => app.move_worktree_selection(false),
        KeyCode::Char('r') => app.load_worktrees(),
        KeyCode::Char('l') => land_worktree(app),
        KeyCode::Char('v') => open_worktree_diff(app),
        KeyCode::Char(force @ ('d' | 'D')) => remove_worktree(app, force == 'D'),
        // Enter goes to whoever is working in it, which is the question you
        // ask of a worktree marked "in use".
        KeyCode::Enter => attach_to_worktree(app),
        _ => {}
    }
}

/// Jumps to the session running in the selected worktree.
fn attach_to_worktree(app: &mut App) {
    let Some(name) = app.selected_worktree().map(|worktree| worktree.name.clone()) else { return };

    if !app.sessions.select_by_worktree(&name) {
        return app.notify(format!("no session is using {name}"));
    }
    app.select_tab(Tab::Sessions);
    if !app.sessions.attach() {
        app.notify("that session has exited");
    }
}

/// Reviews a worktree's uncommitted work without needing a session in it.
fn open_worktree_diff(app: &mut App) {
    let Some(worktree) = app.selected_worktree() else { return };
    let (name, path) = (worktree.name.clone(), worktree.path.clone());

    match crate::diff::View::open(name, &path) {
        Ok(view) => {
            app.diff = Some(view);
            app.dirty = true;
        }
        Err(error) => app.notify(error.to_string()),
    }
}

/// Removes the selected worktree.
///
/// Refuses while a live session is using it, however forceful you are — the
/// agent would lose its working directory out from under it.
fn remove_worktree(app: &mut App, force: bool) {
    let Some(worktree) =
        app.worktrees.as_ref().and_then(|list| list.get(app.worktree_selected)).cloned()
    else {
        return;
    };

    if app.sessions.uses_worktree(&worktree.name) {
        app.notify(format!("{} is in use — close its session first", worktree.name));
        return;
    }

    match crate::worktree::remove(&worktree, force) {
        Ok(()) => {
            app.notify(format!("removed {}", worktree.name));
            app.load_worktrees();
        }
        Err(error) => app.notify(error.to_string()),
    }
}

/// Choosing a theme, with the app repainting as you move.
fn on_key_theme_picker(app: &mut App, key: KeyEvent) {
    app.dirty = true;

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_theme_picker(true),
        KeyCode::Char('k') | KeyCode::Up => app.move_theme_picker(false),
        KeyCode::Enter | KeyCode::Char(' ') => {
            if app.accept_theme_picker().is_some() {
                // Saved on accept, not on preview. Scrolling past a theme
                // should not survive a crash as your setting.
                if let Err(error) = app.config.save_to(&app.config_path) {
                    app.notify(format!("could not save settings: {error}"));
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.cancel_theme_picker(),
        _ => {}
    }
}

/// Reading a diff.
///
/// The page height is not known here — only the renderer knows how tall the
/// pane ended up — so paging uses the terminal height less the border and
/// title rows. Being a line or two out when you press `G` is invisible;
/// threading the real geometry through the key handler to fix it would not be.
fn on_key_diff(app: &mut App, key: KeyEvent, page: usize) {
    app.dirty = true;
    let Some(view) = app.diff.as_mut() else { return };

    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'v') => app.diff = None,
        KeyCode::Char('j') | KeyCode::Down => view.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => view.scroll_by(-1),
        KeyCode::Char('d') | KeyCode::PageDown => {
            view.scroll_by(isize::try_from(page).unwrap_or(20));
        }
        KeyCode::Char('u') | KeyCode::PageUp => {
            view.scroll_by(-isize::try_from(page).unwrap_or(20));
        }
        KeyCode::Char('g') | KeyCode::Home => view.scroll_to_top(),
        KeyCode::Char('G') | KeyCode::End => view.scroll_to_bottom(page),
        _ => {}
    }
}

/// The modal session chooser.
fn on_key_picker(app: &mut App, key: KeyEvent) {
    if app.theme_picker.is_some() {
        return on_key_theme_picker(app, key);
    }
    if app.diff.is_some() {
        return on_key_diff(app, key, 20);
    }
    if app.worktrees.is_some() {
        return on_key_worktrees(app, key);
    }

    app.dirty = true;
    let count = app.sessions.len();

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.picker = None,
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(picker) = app.picker.as_mut()
                && count > 0
            {
                picker.selected = (picker.selected + 1) % count;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(picker) = app.picker.as_mut()
                && count > 0
            {
                picker.selected = (picker.selected + count - 1) % count;
            }
        }
        // The list is ordered exactly as the Sessions sidebar, so the number
        // beside a session is the number you press.
        KeyCode::Char(digit @ '1'..='9') => {
            let index = digit as usize - '1' as usize;
            if index < count {
                if let Some(picker) = app.picker.as_mut() {
                    picker.selected = index;
                }
                deliver_picked(app);
            }
        }
        KeyCode::Enter => deliver_picked(app),
        _ => {}
    }
}

/// Sends the picker's payload to the chosen session and jumps to it.
fn deliver_picked(app: &mut App) {
    let Some(picker) = app.picker.take() else { return };

    app.sessions.select(picker.selected);
    let Some(session) = app.sessions.selected_mut() else { return };
    let name = session.display_name();

    match session.send_paste(&picker.payload) {
        Ok(()) => {
            app.select_tab(Tab::Sessions);
            app.sessions.attach();
            app.notify(format!("sent to {name}"));
        }
        Err(error) => app.notify(format!("could not send: {error}")),
    }
}

/// While text is being typed — a vault query, or the Settings path field —
/// ordinary letters are content. `q` must not quit and `1` must not switch
/// view.
fn on_key_typing(app: &mut App, key: KeyEvent) {
    if app.renaming.is_some() {
        on_key_renaming(app, key);
        return;
    }

    let Some(browser) = app.browser.as_mut() else { return };
    app.dirty = true;

    match key.code {
        KeyCode::Esc => {
            browser.end_query();
            browser.show_all();
        }
        KeyCode::Enter => {
            if browser.mode() == VaultMode::Searching {
                browser.run_search();
            } else {
                browser.end_query();
            }
        }
        KeyCode::Backspace => browser.pop_query(),
        KeyCode::Char(character) => browser.push_query(character),
        KeyCode::Down => browser.select_next(),
        KeyCode::Up => browser.select_previous(),
        _ => {}
    }
}

/// While attached, only the detach key is ours. Everything else is the child's,
/// including Ctrl-C, `q` and the digits — intercepting those would make the
/// agent unusable.
fn on_key_attached(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl-\ is the detach key precisely because almost nothing else uses it:
    // Ctrl-C, Ctrl-D and Escape all belong to the agent.
    //
    // It has to be matched on both spellings. On Unix, crossterm decodes the
    // byte 0x1C as `Char('4')` with CONTROL, not `Char('\\')`
    // (`parse.rs`: `c @ 0x1C..=0x1F => Char(c - 0x1C + b'4')`), which is the
    // historical Ctrl-4 == Ctrl-\ equivalence. Matching only the backslash
    // spelling compiles, passes a hand-written unit test, and never fires on a
    // real keyboard.
    if ctrl && matches!(key.code, KeyCode::Char('\\' | '4')) {
        app.sessions.detach();
        app.dirty = true;
        return;
    }

    // Scrollback without detaching. Shift+Page is the long-standing terminal
    // binding for exactly this, and no TUI uses it, so it is safe to take.
    // Whether it survives depends on the outer terminal — the keys on the
    // Sessions view work regardless, which is why both exist.
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if shift && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
        if let Some(session) = app.sessions.selected() {
            session.page(key.code == KeyCode::PageUp);
        }
        app.dirty = true;
        return;
    }

    if let Some(session) = app.sessions.selected_mut()
        && let Err(error) = session.send_key(key)
    {
        app.notify(format!("session write failed: {error}"));
    }
}

fn on_key_browsing(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        // Two presses to quit: one keystroke should not take down a
        // workspace full of running agents.
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Char('c') if ctrl => app.quit(),
        KeyCode::Tab => app.cycle_tab(true),
        KeyCode::BackTab => app.cycle_tab(false),
        KeyCode::Char(digit @ '1'..='5') => {
            let index = digit as usize - '1' as usize;
            app.select_tab(Tab::ALL[index]);
        }
        _ if app.tab == Tab::Sessions => on_key_sessions(app, key),
        _ if app.tab == Tab::Vault => on_key_vault(app, key),
        _ if app.tab == Tab::Board => on_key_board(app, key),
        _ if app.tab == Tab::Worktrees => on_key_worktrees(app, key),
        _ => {}
    }
}

/// Moving around the board.
///
/// Left and right change column; up and down move within one. The selection is
/// the *session* selection, shared with the sidebar, so opening a card and
/// opening its sidebar row are the same act.
fn on_key_board(app: &mut App, key: KeyEvent) {
    if app.sessions.is_empty() {
        return;
    }
    app.dirty = true;

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => move_column(app, false),
        KeyCode::Char('l') | KeyCode::Right => move_column(app, true),
        KeyCode::Char('j') | KeyCode::Down => move_within_column(app, true),
        KeyCode::Char('k') | KeyCode::Up => move_within_column(app, false),
        KeyCode::Enter => {
            // Jump to the selected session and attach, which is what "open" is
            // going to mean to anyone pressing Enter on a card.
            app.select_tab(Tab::Sessions);
            if !app.sessions.attach() {
                app.notify("that session has exited — press x on the Sessions view to close it");
            }
        }
        // The board is where you notice a card has gone quiet, so it is where
        // you want to ask what it did. Same key, same selected session.
        KeyCode::Char('v') => open_diff(app),
        _ => {}
    }
}

/// Which board column the selected session is in, and where in it.
fn board_position(app: &App) -> Option<(usize, usize)> {
    use crate::ui::board::{COLUMNS, Column, members};

    let selected = app.sessions.selected_index();
    let session = app.sessions.iter().nth(selected)?;
    let column = Column::of(&session.kind, session.state);

    let column_index = COLUMNS.iter().position(|(_, candidate)| *candidate == column)?;
    let row = members(&app.sessions, column).iter().position(|index| *index == selected)?;
    Some((column_index, row))
}

/// Moves to the next column that has anything in it.
///
/// Empty columns are skipped rather than swallowing the keypress — landing on
/// nothing and having to press again would be worse than jumping over it.
fn move_column(app: &mut App, forward: bool) {
    use crate::ui::board::{COLUMNS, members};

    let Some((column_index, row)) = board_position(app) else { return };
    let count = COLUMNS.len();

    for offset in 1..=count {
        let next = if forward {
            (column_index + offset) % count
        } else {
            (column_index + count - offset % count) % count
        };

        let candidate = members(&app.sessions, COLUMNS[next].1);
        if candidate.is_empty() {
            continue;
        }
        // Keep the same depth where possible, so crossing a board of equal
        // columns does not reset you to the top every time.
        let target = candidate[row.min(candidate.len() - 1)];
        app.sessions.select(target);
        return;
    }
}

fn move_within_column(app: &mut App, forward: bool) {
    use crate::ui::board::{COLUMNS, members};

    let Some((column_index, row)) = board_position(app) else { return };
    let candidate = members(&app.sessions, COLUMNS[column_index].1);
    if candidate.is_empty() {
        return;
    }

    let length = candidate.len();
    let next = if forward { (row + 1) % length } else { (row + length - 1) % length };
    app.sessions.select(candidate[next]);
}

/// Renaming the selected session.
fn on_key_renaming(app: &mut App, key: KeyEvent) {
    app.dirty = true;

    match key.code {
        KeyCode::Esc => app.renaming = None,
        KeyCode::Backspace => {
            if let Some(draft) = app.renaming.as_mut() {
                draft.pop();
            }
        }
        KeyCode::Char(character) => {
            if let Some(draft) = app.renaming.as_mut() {
                draft.push(character);
            }
        }
        KeyCode::Enter => {
            let Some(draft) = app.renaming.take() else { return };
            let name = draft.trim().to_string();
            if let Some(session) = app.sessions.selected_mut() {
                // An empty name would leave an unlabelled row, so it means
                // "put it back to the default" rather than "clear it".
                session.rename(if name.is_empty() { None } else { Some(name) });
            }
        }
        _ => {}
    }
}

fn on_key_vault(app: &mut App, key: KeyEvent) {
    if app.browser.is_none() {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let theme = app.theme;
    app.dirty = true;

    // Split so the borrow of `app.browser` ends before the arms that need
    // `app` as a whole (yank, and sending into a session).
    match key.code {
        KeyCode::Char('y') => yank_selected_path(app),
        KeyCode::Char('i') => send_selected_to_session(app),
        _ => {
            let Some(browser) = app.browser.as_mut() else { return };
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => browser.select_next(),
                KeyCode::Char('k') | KeyCode::Up => browser.select_previous(),
                KeyCode::Enter => {
                    if let Err(error) = browser.open_selected(theme) {
                        app.notify(format!("could not open note: {error}"));
                    }
                }
                KeyCode::Char('/') => browser.begin_find(),
                KeyCode::Char('f') => browser.begin_search(),
                KeyCode::Char('a') => browser.show_all(),
                KeyCode::Char('l') => {
                    if !browser.show_links() {
                        app.notify("this note has no outgoing links");
                    }
                }
                KeyCode::Char('b') => {
                    if !browser.show_backlinks() {
                        app.notify("no backlinks yet — they build up as you visit notes");
                    }
                }
                KeyCode::Backspace => {
                    if !browser.go_back(theme) {
                        app.notify("nowhere to go back to");
                    }
                }
                KeyCode::Char('d') if ctrl => browser.scroll(15),
                KeyCode::Char('u') if ctrl => browser.scroll(-15),
                KeyCode::PageDown => browser.scroll(15),
                KeyCode::PageUp => browser.scroll(-15),
                KeyCode::Char('w') => browser.toggle_wrap(),
                KeyCode::Char('e') => open_editor(app),
                KeyCode::Char('g') => browser.scroll_to_top(),
                KeyCode::Char('G') => browser.scroll_to_bottom(),
                _ => {}
            }
        }
    }
}

/// Opens the selected note in the editor.
fn open_editor(app: &mut App) {
    let Some(path) = selected_path(app) else {
        app.notify("no note selected");
        return;
    };

    match crate::editor::Editor::open(std::path::Path::new(&path)) {
        Ok(editor) => {
            app.editor = Some(editor);
            app.dirty = true;
        }
        Err(error) => app.notify(format!("could not open for editing: {error}")),
    }
}

/// Copies the selected note's absolute path. The minimum bar the brief set.
fn yank_selected_path(app: &mut App) {
    let Some(path) = selected_path(app) else {
        app.notify("no note selected");
        return;
    };

    match clipboard::copy(&path) {
        Ok(_) => app.notify(format!("copied {path}")),
        Err(error) => app.notify(format!("could not copy: {error}")),
    }
}

/// Pushes `@<path>` into a session's prompt and jumps to it.
///
/// This is ADR-0004's north star: getting a note into an agent's context
/// without a clipboard round-trip or leaving the app. Deliberately does *not*
/// press Return — you almost always want to type a question after the path.
///
/// With one session there is nothing to decide, so it goes straight there.
/// With several, a chooser opens rather than guessing at the selected one.
fn send_selected_to_session(app: &mut App) {
    let Some(path) = selected_path(app) else {
        app.notify("no note selected");
        return;
    };
    if app.sessions.is_empty() {
        app.notify("no session to send to — start one with n on the Sessions view");
        return;
    }

    let payload = format!("@{path} ");

    if app.sessions.len() == 1 {
        app.picker = Some(Picker { prompt: String::new(), payload, selected: 0 });
        deliver_picked(app);
        return;
    }

    let name = app
        .browser
        .as_ref()
        .and_then(|browser| browser.selected_note())
        .map_or_else(String::new, |note| note.stem.clone());

    app.picker = Some(Picker {
        prompt: format!("Send {name} to which session?"),
        payload,
        selected: app.sessions.selected_index(),
    });
    app.dirty = true;
}

fn selected_path(app: &App) -> Option<String> {
    app.browser.as_ref()?.selected_note().map(|note| note.path.to_string_lossy().into_owned())
}

fn on_key_sessions(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.sessions.select_next();
            app.dirty = true;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.sessions.select_previous();
            app.dirty = true;
        }
        // `n` asks where and how; `s` is the quick shell you want immediately.
        KeyCode::Char('n') => app.open_new_session_form(),
        KeyCode::Char('s') => spawn(app, true),
        KeyCode::Char('W') => app.select_tab(Tab::Worktrees),
        // `v` for review. `d` would be the better mnemonic and is long since
        // spoken for by half-page scrolling.
        KeyCode::Char('v') => open_diff(app),
        // Reading back through an agent's output is a normal thing to want,
        // and it must not depend on the terminal reporting the mouse.
        KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::Char('u' | 'd' | 'g' | 'G')
            if !app.sessions.is_empty() =>
        {
            scroll_selected_session(app, key.code);
        }
        KeyCode::Char('r') => {
            if let Some(session) = app.sessions.selected() {
                app.renaming = Some(session.name.clone());
                app.dirty = true;
            } else {
                app.notify("no session to rename");
            }
        }
        KeyCode::Char('x') => {
            app.sessions.close_selected();
            app.remember_sessions();
            app.dirty = true;
        }
        KeyCode::Enter => {
            if app.sessions.is_empty() {
                app.notify("no session to attach to — press n or s to start one");
            } else if app.sessions.attach() {
                app.dirty = true;
            } else {
                app.notify("that session has exited — press x to close it");
            }
        }
        _ => {}
    }
}

/// Scrollback keys on the Sessions view.
///
/// Deliberately plain keys rather than a modifier chord: the mouse path can be
/// silently disabled by the outer terminal, so this one has to be reachable
/// with no configuration at all.
fn scroll_selected_session(app: &App, code: KeyCode) {
    let Some(session) = app.sessions.selected() else { return };

    match code {
        KeyCode::PageUp | KeyCode::Char('u') => session.page(true),
        KeyCode::PageDown | KeyCode::Char('d') => session.page(false),
        KeyCode::Home | KeyCode::Char('g') => session.scroll_to_edge(true),
        KeyCode::End | KeyCode::Char('G') => session.scroll_to_edge(false),
        _ => {}
    }
}

fn spawn(app: &mut App, shell: bool) {
    // A real size arrives on the next frame; this only has to be non-degenerate
    // so the child's first output does not wrap against a 1x1 grid.
    let size = Size::new(24, 80);
    let cwd = app.cwd.clone();

    let result = if shell {
        app.sessions.spawn_shell(&cwd, size)
    } else {
        app.sessions.spawn_agent(&cwd, size)
    };

    match result {
        Ok(_) => {
            app.sessions.attach();
            app.remember_sessions();
            app.dirty = true;
            // Hook installation is best-effort, but a silent failure would
            // leave the board quietly wrong for the rest of the session.
            if let Some(warning) = app.sessions.take_hook_warning() {
                app.notify(warning);
            }
        }
        Err(error) => app.notify(format!("could not start a session: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Focus;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_a_query_does_not_trigger_commands() {
        let mut app = App::new();
        // Only meaningful when a vault exists on this machine.
        if app.browser.is_none() {
            return;
        }

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('/')));
        assert_eq!(app.focus(), InputFocus::Text);

        // Every one of these is a command while browsing.
        for character in "q1f".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }

        assert!(!app.should_quit, "q inside a query is text, not a command");
        assert_eq!(app.tab, Tab::Vault, "1 inside a query must not switch view");
        assert_eq!(app.browser.as_ref().unwrap().query(), "q1f");
    }

    #[test]
    fn escape_leaves_a_query_without_quitting() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('/')));
        on_key(&mut app, press(KeyCode::Esc));

        assert_ne!(app.focus(), InputFocus::Text);
        assert!(!app.should_quit);
    }

    #[test]
    fn sending_a_note_with_no_session_says_so() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.notice.as_deref().is_some_and(|notice| notice.contains("no session")));
        assert_eq!(app.tab, Tab::Vault, "a failed send must not switch view");
    }

    /// Tab is the view switcher everywhere else, so Settings must not steal
    /// it — the arrows are what you reach for in a list.
    #[test]
    fn tab_still_switches_view_from_settings() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);

        on_key(&mut app, press(KeyCode::Tab));
        assert_ne!(app.tab, Tab::Settings, "tab should leave Settings, not walk its rows");
    }

    #[test]
    fn arrows_walk_the_settings_list() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);
        let first = app.settings.focused().unwrap().label;

        on_key(&mut app, press(KeyCode::Down));
        assert_ne!(app.settings.focused().unwrap().label, first);
        assert_eq!(app.tab, Tab::Settings, "and stay in Settings");
    }

    /// A modal form has no views to switch between, so Tab keeps moving
    /// between its fields.
    #[test]
    fn tab_still_moves_between_fields_in_a_modal_form() {
        let mut app = App::new();
        app.open_new_session_form();
        let first = app.form.as_ref().unwrap().focused().unwrap().label;

        on_key(&mut app, press(KeyCode::Tab));
        assert_ne!(app.form.as_ref().unwrap().focused().unwrap().label, first);
        assert!(app.form.is_some(), "and does not switch view out from under you");
    }

    #[test]
    fn settings_is_a_menu_you_move_through() {
        let mut app = App::new();
        app.select_tab(Tab::Settings);
        assert_eq!(app.focus(), InputFocus::Form);
        assert!(!app.settings.is_editing(), "it opens as a menu, not a text box");

        let first = app.settings.focused().unwrap().label;
        on_key(&mut app, press(KeyCode::Char('j')));
        assert_ne!(app.settings.focused().unwrap().label, first, "j moves the selection");

        on_key(&mut app, press(KeyCode::Enter));
        assert!(app.settings.is_editing(), "Return edits the selected row");
    }

    #[test]
    fn editing_a_settings_row_captures_every_key() {
        let mut app = App::new();
        app.config_path = std::env::temp_dir().join("houston-settings-menu-test.toml");
        app.select_tab(Tab::Settings);
        on_key(&mut app, press(KeyCode::Enter));

        for character in "q1".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }
        assert!(!app.should_quit, "q inside a path is a q");
        assert_eq!(app.tab, Tab::Settings, "1 must not switch view");

        std::fs::remove_file(&app.config_path).ok();
    }

    #[test]
    fn a_bad_vault_path_is_rejected_and_the_old_one_kept() {
        let mut app = App::new();
        app.config_path = std::env::temp_dir().join("houston-bad-path-config.toml");
        let indexed_before = app.browser.as_ref().map(|browser| browser.vault.len());

        app.select_tab(Tab::Settings);
        on_key(&mut app, press(KeyCode::Enter));
        for _ in 0..80 {
            on_key(&mut app, press(KeyCode::Backspace));
        }
        for character in "/tmp/houston-nope-not-here".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }
        on_key(&mut app, press(KeyCode::Enter));

        assert!(app.notice.is_some(), "the failure is reported");
        assert_eq!(
            app.browser.as_ref().map(|browser| browser.vault.len()),
            indexed_before,
            "a typo must not leave you with no vault"
        );

        std::fs::remove_file(&app.config_path).ok();
    }

    #[test]
    fn the_new_session_form_opens_instead_of_spawning_immediately() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('n')));

        assert!(app.form.is_some(), "n asks where and how");
        assert_eq!(app.focus(), InputFocus::Form);
        assert_eq!(app.sessions.len(), 0, "nothing has been started yet");

        on_key(&mut app, press(KeyCode::Esc));
        assert!(app.form.is_none());
        assert_eq!(app.sessions.len(), 0, "cancelling starts nothing");
    }

    #[test]
    fn the_worktree_name_field_appears_only_when_asked_for() {
        let mut app = App::new();
        app.open_new_session_form();

        let hidden = app.form.as_ref().unwrap().field(fields::WORKTREE_NAME).unwrap();
        assert!(!hidden.visible, "pointless until a worktree is wanted");

        // Move to the toggle and turn it on.
        for _ in 0..2 {
            on_key(&mut app, press(KeyCode::Down));
        }
        on_key(&mut app, press(KeyCode::Char(' ')));

        assert!(app.form.as_ref().unwrap().is_on(fields::WORKTREE));
        assert!(app.form.as_ref().unwrap().field(fields::WORKTREE_NAME).unwrap().visible);
    }

    #[test]
    fn a_shell_still_starts_immediately_without_a_form() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));

        assert!(app.form.is_none(), "the quick shell asks nothing");
        assert_eq!(app.sessions.len(), 1);
    }

    /// `W` used to open a popup. It now goes to the view, because the list
    /// grew five facts per row and a modal box is the wrong size for that.
    #[test]
    fn w_goes_to_the_worktrees_view_and_tab_leaves_it() {
        let mut app = App::new();
        on_key(&mut app, KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT));

        assert_eq!(app.tab, Tab::Worktrees, "W selects the view");
        assert_eq!(app.focus(), InputFocus::Commands, "a view is not a modal");

        on_key(&mut app, press(KeyCode::Tab));
        assert_ne!(app.tab, Tab::Worktrees, "and Tab leaves it like any other view");
    }

    #[test]
    fn a_worktree_in_use_is_not_removed() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();
        app.sessions.selected_mut().unwrap().worktree = Some("busy".to_string());
        app.tab = Tab::Worktrees;

        app.worktrees = Some(vec![crate::worktree::Worktree {
            name: "busy".to_string(),
            path: std::env::temp_dir().join("houston-never-removed"),
            repository: std::path::PathBuf::new(),
            branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            changes: None,
        }]);
        app.worktree_selected = 0;

        on_key(&mut app, press(KeyCode::Char('d')));
        assert!(app.notice.as_deref().is_some_and(|n| n.contains("in use")));
        assert_eq!(app.worktrees.as_ref().unwrap().len(), 1, "nothing was removed");
    }

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE }
    }

    /// The keyboard path must work whatever the terminal decides to do with
    /// the wheel — that is the whole reason it exists.
    #[test]
    fn the_inspector_records_what_arrived_not_what_was_done_with_it() {
        let mut app = App::new();

        handle(&mut app, &Event::Key(press(KeyCode::Char('2'))), None);
        handle(&mut app, &Event::Mouse(wheel(MouseEventKind::ScrollUp)), None);

        let log: Vec<&String> = app.input_log.iter().collect();
        assert!(log.iter().any(|entry| entry.starts_with("key")));
        assert!(
            log.iter().any(|entry| entry.starts_with("mouse")),
            "the whole point is answering whether mouse events arrive"
        );
    }

    #[test]
    fn the_inspector_toggles_even_while_attached() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        assert!(app.is_attached());

        on_key(&mut app, KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        assert!(app.show_inspector, "it must work when something is swallowing input");

        on_key(&mut app, KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        assert!(!app.show_inspector);
    }

    #[test]
    fn the_input_log_does_not_grow_without_limit() {
        let mut app = App::new();
        for _ in 0..200 {
            handle(&mut app, &Event::Key(press(KeyCode::Char('x'))), None);
        }
        assert!(app.input_log.len() <= 14, "got {}", app.input_log.len());
    }

    #[test]
    fn the_sessions_view_scrolls_a_session_without_any_mouse() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        // Nothing has been printed, so there is no history to move through;
        // what matters is that the keys reach the session rather than being
        // swallowed or treated as commands.
        for code in [KeyCode::PageUp, KeyCode::Char('u'), KeyCode::Home] {
            on_key(&mut app, press(code));
            assert!(!app.should_quit, "{code:?} must not be a command here");
        }

        on_key(&mut app, press(KeyCode::End));
        assert_eq!(app.sessions.selected().unwrap().scrollback_offset(), 0);
    }

    #[test]
    fn shift_page_scrolls_without_leaving_the_child() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        assert!(app.is_attached());

        on_key(&mut app, KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT));
        assert!(app.is_attached(), "scrolling must not detach you");

        // An unshifted page key still belongs to the child.
        on_key(&mut app, press(KeyCode::PageUp));
        assert!(app.is_attached());
    }

    #[test]
    fn the_wheel_scrolls_a_sessions_own_scrollback() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));

        // A shell prints nothing yet, so there is no history to move through —
        // what matters is that the wheel reaches the session at all rather
        // than being handled as an arrow key.
        on_mouse(&mut app, wheel(MouseEventKind::ScrollUp), Some(Rect::new(0, 0, 80, 24)));
        assert!(app.dirty, "the wheel is acted on, not ignored");
        assert!(!app.should_quit);
    }

    #[test]
    fn typing_returns_a_scrolled_session_to_the_live_output() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));

        app.sessions.selected().unwrap().scroll(5);
        on_key(&mut app, press(KeyCode::Char('x')));

        assert_eq!(
            app.sessions.selected().unwrap().scrollback_offset(),
            0,
            "writing into a scrolled-back view and not seeing it is disorienting"
        );
    }

    #[test]
    fn the_wheel_scrolls_the_vault_reader_when_not_attached() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        app.select_tab(Tab::Vault);

        on_mouse(&mut app, wheel(MouseEventKind::ScrollDown), None);
        assert!(app.dirty);
        assert!(!app.should_quit, "the wheel is never a command");
    }

    #[test]
    fn quitting_takes_two_presses() {
        let mut app = App::new();

        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(!app.should_quit, "one press only arms it");
        assert!(app.quit_armed);

        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn any_other_key_disarms_a_pending_quit() {
        for interrupting in [KeyCode::Esc, KeyCode::Char('2'), KeyCode::Char('j')] {
            let mut app = App::new();
            on_key(&mut app, press(KeyCode::Char('q')));
            assert!(app.quit_armed);

            on_key(&mut app, press(interrupting));
            assert!(!app.quit_armed, "{interrupting:?} should cancel the pending quit");

            // And a later `q` starts over rather than completing the old one.
            on_key(&mut app, press(KeyCode::Char('q')));
            assert!(!app.should_quit, "the confirmation must not survive an interruption");
        }
    }

    #[test]
    fn escape_only_cancels_the_quit_and_does_nothing_else() {
        let mut app = App::new();
        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('q')));

        on_key(&mut app, press(KeyCode::Esc));
        assert!(!app.quit_armed);
        assert_eq!(app.tab, Tab::Vault, "escape should not have side effects");
    }

    #[test]
    fn renaming_a_session_sticks_and_beats_the_terminal_title() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        on_key(&mut app, press(KeyCode::Char('r')));
        assert_eq!(app.focus(), InputFocus::Text, "renaming captures keys");

        // Clear the seeded name, then type a new one including a `q`.
        for _ in 0..40 {
            on_key(&mut app, press(KeyCode::Backspace));
        }
        for character in "queue runner".chars() {
            on_key(&mut app, press(KeyCode::Char(character)));
        }
        assert!(!app.should_quit, "q inside a name is text");

        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().display_name(), "queue runner");
    }

    #[test]
    fn an_empty_rename_restores_the_default_name() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();
        let original = app.sessions.selected().unwrap().name.clone();

        app.renaming = Some("temporary".to_string());
        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().display_name(), "temporary");

        app.renaming = Some("   ".to_string());
        on_key(&mut app, press(KeyCode::Enter));
        assert_eq!(app.sessions.selected().unwrap().name, original);
    }

    #[test]
    fn sending_a_note_with_several_sessions_opens_a_chooser() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        for _ in 0..2 {
            on_key(&mut app, press(KeyCode::Char('s')));
            app.sessions.detach();
        }
        assert_eq!(app.sessions.len(), 2);

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.picker.is_some(), "with a choice to make, ask");
        assert_eq!(app.focus(), InputFocus::Overlay);
        assert!(app.picker.as_ref().unwrap().payload.starts_with('@'));

        on_key(&mut app, press(KeyCode::Esc));
        assert!(app.picker.is_none(), "escape closes the chooser");
        assert_eq!(app.tab, Tab::Vault, "cancelling does not move you");
    }

    #[test]
    fn sending_with_one_session_skips_the_chooser() {
        let mut app = App::new();
        if app.browser.is_none() {
            return;
        }
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        app.select_tab(Tab::Vault);
        on_key(&mut app, press(KeyCode::Char('i')));

        assert!(app.picker.is_none(), "no choice to make, so no question asked");
        assert_eq!(app.tab, Tab::Sessions, "it jumps straight to the session");
    }

    #[test]
    fn digits_jump_between_views() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('3')));
        assert_eq!(app.tab, Tab::Board);
    }

    #[test]
    fn attaching_with_no_sessions_explains_itself() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Enter));
        assert!(app.notice.is_some(), "the user should be told why nothing happened");
        assert!(!app.is_attached());
    }

    #[test]
    fn a_pressed_key_clears_a_stale_notice() {
        let mut app = App::new();
        app.notify("something went wrong");
        handle(&mut app, &Event::Key(press(KeyCode::Char('2'))), None);
        assert!(app.notice.is_none());
    }

    #[test]
    fn paste_while_browsing_is_dropped_not_typed() {
        let mut app = App::new();
        // Nothing to receive it, and it must never be re-encoded as keystrokes.
        handle(&mut app, &Event::Paste("rm -rf /".to_string()), None);
        assert!(!app.should_quit);
        assert!(app.notice.is_none());
    }

    #[test]
    fn spawning_a_shell_attaches_to_it() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));

        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions.focus(), Focus::Attached);
        assert!(app.is_attached());
    }

    #[test]
    fn while_attached_houston_keys_belong_to_the_child() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        assert!(app.is_attached());

        // `q` would quit while browsing. Attached, it is just a character.
        on_key(&mut app, press(KeyCode::Char('q')));
        assert!(!app.should_quit, "q must reach the child, not quit Houston");

        // Same for the view-switching digits.
        on_key(&mut app, press(KeyCode::Char('2')));
        assert_eq!(app.tab, Tab::Sessions, "digits must reach the child too");
    }

    /// Both spellings must work. The `'4'` case is the one that actually
    /// reaches us from a real terminal — see the comment in `on_key_attached`.
    #[test]
    fn ctrl_backslash_detaches_in_both_of_crossterms_spellings() {
        for code in [KeyCode::Char('\\'), KeyCode::Char('4')] {
            let mut app = App::new();
            on_key(&mut app, press(KeyCode::Char('s')));
            assert!(app.is_attached());

            on_key(&mut app, KeyEvent::new(code, KeyModifiers::CONTROL));
            assert!(!app.is_attached(), "ctrl+{code:?} should detach");
            assert_eq!(app.sessions.focus(), Focus::Browsing);
        }
    }

    /// A plain `4` while attached is a character the child should receive, not
    /// a detach. Only the control-modified form is ours.
    #[test]
    fn an_unmodified_four_does_not_detach() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        on_key(&mut app, press(KeyCode::Char('4')));
        assert!(app.is_attached(), "plain 4 belongs to the child");
    }

    #[test]
    fn closing_the_only_session_returns_to_browsing() {
        let mut app = App::new();
        on_key(&mut app, press(KeyCode::Char('s')));
        app.sessions.detach();

        on_key(&mut app, press(KeyCode::Char('x')));
        assert!(app.sessions.is_empty());
        assert!(!app.is_attached());
    }
}
