//! Pseudo-terminal sessions: spawn a child, parse its output into a terminal
//! grid, and write input back to it.
//!
//! Three deliberate differences from Chloe's equivalent
//! (`reference/chloe/src/views/instances/pty.rs`), all found while reading it:
//!
//! 1. **Blocking reads.** Chloe leaves the master fd non-blocking (as
//!    `alacritty_terminal` creates it) and sleeps 10ms on every `WouldBlock`.
//!    That costs up to 10ms of latency per read and spins a thread forever. We
//!    clear `O_NONBLOCK` and block in `read`, so the thread wakes exactly when
//!    there is output and never otherwise.
//! 2. **Terminal queries are answered.** Chloe's event listener is empty, so
//!    escape sequences that expect a *reply* — cursor position reports, device
//!    attributes, text-area size — go unanswered. Programs that query and wait
//!    stall until they time out. We write the replies back.
//! 3. **No per-read allocation.** Chloe copies each read into a fresh `Vec` and
//!    sends it down a channel purely as a "something changed" signal. We parse
//!    in place and flip an atomic flag; the frame tick picks it up. A chatty
//!    child costs no allocations and no channel traffic.

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener, OnResize, WindowSize},
    grid::Dimensions,
    term::{Config, Term},
    tty::{self, ChildEvent, EventedPty, Options, Pty, Shell},
    vte::ansi::{Processor, StdSyncHandler},
};
use anyhow::{Context, Result};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

const SCROLLBACK_LINES: usize = 10_000;
const READ_BUFFER_BYTES: usize = 65_536;

/// Grid dimensions, in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub rows: u16,
    pub columns: u16,
}

impl Size {
    pub const fn new(rows: u16, columns: u16) -> Self {
        // A zero-sized grid makes `alacritty_terminal` panic, and a terminal
        // legitimately reports 0x0 mid-resize.
        Self {
            rows: if rows == 0 { 1 } else { rows },
            columns: if columns == 0 { 1 } else { columns },
        }
    }

    const fn window(self) -> WindowSize {
        WindowSize { cell_width: 1, cell_height: 1, num_cols: self.columns, num_lines: self.rows }
    }
}

impl Dimensions for Size {
    fn columns(&self) -> usize {
        self.columns as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
}

/// What the child asks the terminal to do, other than draw.
///
/// Shared with the reader thread, so everything here is behind a lock or an
/// atomic.
#[derive(Clone)]
pub struct Notifier {
    writer: Arc<Mutex<File>>,
    title: Arc<Mutex<Option<String>>>,
    bell: Arc<AtomicBool>,
    size: Arc<Mutex<Size>>,
}

impl Notifier {
    fn reply(&self, text: &str) {
        self.reply_bytes(text.as_bytes());
    }

    fn reply_bytes(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }
}

impl EventListener for Notifier {
    fn send_event(&self, event: TermEvent) {
        match event {
            // Replies the child is waiting on. Ignoring these is Chloe's bug.
            TermEvent::PtyWrite(text) => self.reply(&text),
            TermEvent::TextAreaSizeRequest(format) => {
                let size = self.size.lock().map_or(Size::new(24, 80), |size| *size);
                self.reply(&format(size.window()));
            }
            TermEvent::ColorRequest(index, format) => {
                self.reply(&format(crate::palette::xterm_256(index)));
            }
            // Houston has no clipboard integration until Phase 5. Reply with an
            // empty paste rather than leaving the child hanging.
            TermEvent::ClipboardLoad(_, format) => self.reply(&format("")),

            TermEvent::Title(title) => {
                if let Ok(mut slot) = self.title.lock() {
                    *slot = Some(title);
                }
            }
            TermEvent::ResetTitle => {
                if let Ok(mut slot) = self.title.lock() {
                    *slot = None;
                }
            }
            TermEvent::Bell => self.bell.store(true, Ordering::Relaxed),

            // Purely advisory, or handled elsewhere (child exit comes from
            // `poll_child_event`, redraws from the `dirty` flag).
            TermEvent::MouseCursorDirty
            | TermEvent::CursorBlinkingChange
            | TermEvent::ClipboardStore(..)
            | TermEvent::Wakeup
            | TermEvent::Exit
            | TermEvent::ChildExit(_) => {}
        }
    }
}

/// How to start a child process. ADR-0006: a session is a PTY plus one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub command: Option<String>,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
}

impl LaunchSpec {
    pub fn command(command: impl Into<String>, args: Vec<String>, cwd: PathBuf) -> Self {
        Self { command: Some(command.into()), args, cwd, env: HashMap::new() }
    }
}

/// A running child process and the terminal grid it is drawing into.
pub struct PtySession {
    term: Arc<Mutex<Term<Notifier>>>,
    pty: Pty,
    writer: File,
    notifier: Notifier,
    dirty: Arc<AtomicBool>,
    size: Size,
}

impl PtySession {
    pub fn spawn(spec: &LaunchSpec, size: Size) -> Result<Self> {
        tty::setup_env();

        let options = Options {
            shell: spec.command.clone().map(|command| Shell::new(command, spec.args.clone())),
            working_directory: Some(spec.cwd.clone()),
            env: spec.env.clone(),
            drain_on_exit: true,
        };

        let pty = tty::new(&options, size.window(), 0).context("failed to open a pty")?;

        let writer = pty.file().try_clone().context("failed to clone the pty for writing")?;
        let notifier = Notifier {
            writer: Arc::new(Mutex::new(writer.try_clone()?)),
            title: Arc::new(Mutex::new(None)),
            bell: Arc::new(AtomicBool::new(false)),
            size: Arc::new(Mutex::new(size)),
        };

        let config = Config { scrolling_history: SCROLLBACK_LINES, ..Config::default() };
        let term = Arc::new(Mutex::new(Term::new(config, &size, notifier.clone())));
        let dirty = Arc::new(AtomicBool::new(true));

        spawn_reader(
            pty.file().try_clone().context("failed to clone the pty for reading")?,
            Arc::clone(&term),
            Arc::clone(&dirty),
        )?;

        Ok(Self { term, pty, writer, notifier, dirty, size })
    }

    /// Takes the "has drawn something since you last asked" flag.
    ///
    /// Reading clears it, so the caller must act on `true`.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Acquire)
    }

    #[expect(dead_code, reason = "Phase 6 surfaces the bell as an attention marker on the board")]
    pub fn take_bell(&self) -> bool {
        self.notifier.bell.swap(false, Ordering::Relaxed)
    }

    /// The title the child set via OSC 0/2, if any. Useful as a session label.
    pub fn title(&self) -> Option<String> {
        self.notifier.title.lock().ok().and_then(|title| title.clone())
    }

    pub const fn term(&self) -> &Arc<Mutex<Term<Notifier>>> {
        &self.term
    }

    /// Whether the child has asked for bracketed paste. ADR-0005 uses this to
    /// decide whether to wrap a paste in `ESC[200~`/`ESC[201~`.
    pub fn wants_bracketed_paste(&self) -> bool {
        self.term.lock().is_ok_and(|term| {
            term.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
        })
    }

    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.pty.on_resize(size.window());
        if let Ok(mut term) = self.term.lock() {
            term.resize(size);
        }
        if let Ok(mut shared) = self.notifier.size.lock() {
            *shared = size;
        }
        self.dirty.store(true, Ordering::Release);
    }

    /// Writes bytes to the child in a single `write_all`.
    ///
    /// Callers must batch: one call per paste, not one per character. That is
    /// the whole of ADR-0005.
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Non-blocking check for child exit. Call from the frame tick.
    pub fn poll_child_event(&mut self) -> Option<ChildEvent> {
        self.pty.next_child_event()
    }
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PtySession").field("size", &self.size).finish_non_exhaustive()
    }
}

/// Parses child output into the terminal grid until the child closes the pty.
fn spawn_reader(
    file: File,
    term: Arc<Mutex<Term<Notifier>>>,
    dirty: Arc<AtomicBool>,
) -> Result<()> {
    // `alacritty_terminal` opens the master non-blocking. We do our own I/O, so
    // clear it and let `read` park the thread instead of spinning on WouldBlock.
    let flags = fcntl_getfl(&file).context("failed to read pty flags")?;
    fcntl_setfl(&file, flags.difference(OFlags::NONBLOCK))
        .context("failed to make the pty blocking")?;

    thread::Builder::new()
        .name("houston-pty-reader".into())
        .spawn(move || {
            let mut file = file;
            let mut buffer = vec![0u8; READ_BUFFER_BYTES];
            let mut parser: Processor<StdSyncHandler> = Processor::new();

            loop {
                match file.read(&mut buffer) {
                    // EOF: the child closed the pty.
                    Ok(0) => break,
                    Ok(count) => {
                        if let Ok(mut term) = term.lock() {
                            parser.advance(&mut *term, &buffer[..count]);
                        }
                        dirty.store(true, Ordering::Release);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }

            dirty.store(true, Ordering::Release);
        })
        .context("failed to start the pty reader thread")?;

    Ok(())
}
