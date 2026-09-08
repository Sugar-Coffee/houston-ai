//! Houston AI — a terminal workspace where a knowledge vault and coding agents
//! share one keyboard-driven surface.
//!
//! See `docs/vision.md` for what this is, and `docs/adr/` for why it is shaped
//! the way it is.

mod app;
mod event;
mod terminal;
mod ui;

use anyhow::Result;
use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    let mut guard = terminal::enter()?;
    let result = event::run(&mut guard.terminal, App::new()).await;
    guard.leave()?;
    result
}
