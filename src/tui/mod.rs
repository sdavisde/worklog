//! Terminal setup/teardown and the synchronous event loop. Rendering and
//! state transitions live in [`views`] and [`app`] respectively, so the whole
//! UI is testable under `TestBackend` without a real terminal.

mod app;
mod editor;
mod textedit;
mod views;

#[cfg(test)]
mod tests;

use crate::config;
use crate::notes::NotesStore;
use crate::store::Store;
use crate::theme;
use app::App;
use color_eyre::eyre::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

/// Open the TUI: resolve storage/config, initialize the terminal (raw mode,
/// alternate screen, panic hook — all via `ratatui::init`), run the loop, and
/// always restore the terminal on the way out.
pub fn run() -> Result<()> {
    let store = Store::resolve()?;
    let config = config::load_or_create(&store.config_path())?;
    // A bad theme name is a clean startup error, never a mid-session panic.
    let theme = theme::load(&store, &config.theme)?;
    let notes = NotesStore::new(store.notes_dir());
    let mut app = App::new(store, notes, config, theme)?;

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut kitty = push_keyboard_enhancement();
    let result = run_loop_inner(terminal, app, &mut kitty);
    pop_keyboard_enhancement(kitty);
    result
}

fn run_loop_inner(terminal: &mut DefaultTerminal, app: &mut App, kitty: &mut bool) -> Result<()> {
    loop {
        terminal.draw(|frame| views::draw(app, frame))?;

        if app.should_quit {
            break;
        }

        // `App::handle_key` filters to `KeyEventKind::Press`, which also
        // covers the Release/Repeat events the kitty protocol may report.
        if let Event::Key(key) = event::read()? {
            app.handle_key(key)?;
        }

        // Editor escape hatch: suspend the TUI (dropping the keyboard
        // enhancement flags with it), run $EDITOR, always re-init both.
        if app.editor_request.is_some() {
            let editor_cmd = editor::resolve_editor(&app.config);
            pop_keyboard_enhancement(*kitty);
            ratatui::restore();
            let outcome = app.run_editor(&editor_cmd);
            *terminal = ratatui::init();
            *kitty = push_keyboard_enhancement();
            let _ = terminal.clear();
            outcome?;
        }
    }
    Ok(())
}

/// Enable the kitty keyboard protocol's escape-code disambiguation when the
/// terminal supports it, so chords like `ctrl+backspace` are distinguishable
/// from plain `backspace`. Returns whether the flags were pushed; the caller
/// must pop them again before leaving the alternate screen. Terminals
/// without the protocol degrade gracefully — `alt+backspace`/`ctrl+w` still
/// cover delete-word-back.
fn push_keyboard_enhancement() -> bool {
    matches!(terminal::supports_keyboard_enhancement(), Ok(true))
        && execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok()
}

fn pop_keyboard_enhancement(pushed: bool) {
    if pushed {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
}
