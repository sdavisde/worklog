//! Terminal setup/teardown and the synchronous event loop. Rendering and
//! state transitions live in [`views`] and [`app`] respectively, so the whole
//! UI is testable under `TestBackend` without a real terminal.

mod app;
mod editor;
mod views;

#[cfg(test)]
mod tests;

use crate::config;
use crate::notes::NotesStore;
use crate::store::Store;
use app::App;
use color_eyre::eyre::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};

/// Open the TUI: resolve storage/config, initialize the terminal (raw mode,
/// alternate screen, panic hook — all via `ratatui::init`), run the loop, and
/// always restore the terminal on the way out.
pub fn run() -> Result<()> {
    let store = Store::resolve()?;
    let config = config::load_or_create(&store.config_path())?;
    let notes = NotesStore::new(store.notes_dir());
    let mut app = App::new(store, notes, config)?;

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| views::draw(app, frame))?;

        if app.should_quit {
            break;
        }

        if let Event::Key(key) = event::read()? {
            app.handle_key(key)?;
        }

        // Editor escape hatch: suspend the TUI, run $EDITOR, always re-init.
        if app.editor_request.is_some() {
            let editor_cmd = editor::resolve_editor(&app.config);
            ratatui::restore();
            let outcome = app.run_editor(&editor_cmd);
            *terminal = ratatui::init();
            let _ = terminal.clear();
            outcome?;
        }
    }
    Ok(())
}
