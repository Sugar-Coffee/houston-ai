//! Houston AI — a terminal workspace where a knowledge vault and coding agents
//! share one keyboard-driven surface.
//!
//! See `docs/vision.md` for what this is, and `docs/adr/` for why it is shaped
//! the way it is.

mod app;
mod cli;
mod clipboard;
mod config;
mod editor;
mod event;
mod form;
mod hooks;
mod input;
mod palette;
mod paths;
mod provider;
mod pty;
mod session;
mod terminal;
mod ui;
mod vault;
mod worktree;

use anyhow::Result;
use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    let command = cli::parse(std::env::args().skip(1))?;
    if cli::dispatch(&command) {
        return Ok(());
    }

    let mut guard = terminal::enter()?;
    let result = event::run(&mut guard.terminal, App::new()).await;
    guard.leave()?;
    result
}
