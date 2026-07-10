//! The edit modal's buffer: a single logical line held in a
//! [`ratatui_textarea::TextArea`], always in "typing" mode — no vim layer.
//!
//! The buffer is always exactly one logical line; the widget wraps it
//! visually. Enter submits (never inserts a newline), stray newline input
//! becomes a space, and Esc always cancels. Readline/word-wise chords
//! (ctrl/alt+arrows, ctrl+w/u/k/a/e) and ctrl+z / ctrl+shift+z undo/redo are
//! layered on top. Key handling returns an [`Outcome`] so
//! [`crate::tui::app`] owns what submit/cancel/open-editor actually do; the
//! word-motion logic is kept in pure functions so it is unit-testable
//! without a widget.

use crate::tui::app::{EditPurpose, TokenSuggestion};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};

/// Cap on the modal's text rows; taller content scrolls inside the widget.
pub const MAX_TEXT_ROWS: u16 = 8;

/// What the caller should do after a key was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Consumed,
    Submit,
    Cancel,
    OpenEditor,
}

/// The edit modal: purpose (what commit does) and the textarea.
#[derive(Debug, Clone)]
pub struct TextEdit {
    pub purpose: EditPurpose,
    textarea: TextArea<'static>,
}

impl TextEdit {
    /// Open the modal with the cursor at the end of `initial`.
    pub fn new(purpose: EditPurpose, initial: String) -> Self {
        let mut textarea = TextArea::new(vec![single_line(&initial)]);
        textarea.set_wrap_mode(WrapMode::WordOrGlyph);
        textarea.set_cursor_line_style(Style::default());
        textarea.move_cursor(CursorMove::End);
        Self { purpose, textarea }
    }

    /// The buffer's single logical line.
    pub fn text(&self) -> &str {
        &self.textarea.lines()[0]
    }

    /// Cursor position as a char index into [`TextEdit::text`].
    pub fn cursor_col(&self) -> usize {
        self.textarea.cursor().1
    }

    /// The underlying widget, for rendering.
    pub fn textarea(&self) -> &TextArea<'static> {
        &self.textarea
    }

    /// Cursor position in wrapped screen coordinates (row, col). Only valid
    /// after the textarea has been rendered at the current width.
    pub fn screen_pos(&self) -> (usize, usize) {
        let sc = self.textarea.screen_cursor();
        (sc.row, sc.col)
    }

    /// Number of visual rows the buffer wraps to at `width`, capped at
    /// [`MAX_TEXT_ROWS`]. Measured with a probe render so it matches the
    /// widget's own wrapping exactly (the wrap internals are not public).
    pub fn wrapped_rows(&self, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let area = Rect::new(0, 0, width, MAX_TEXT_ROWS);
        let mut buf = Buffer::empty(area);
        Widget::render(&self.textarea, area, &mut buf);
        let untouched = ratatui::buffer::Cell::EMPTY.style();
        let mut rows = 1;
        for y in 0..area.height {
            // a row is in use if anything was drawn on it — a non-blank
            // symbol, or the styled cursor block on an otherwise empty row
            let used = (0..area.width).any(|x| {
                buf.cell((x, y))
                    .is_some_and(|cell| cell.symbol() != " " || cell.style() != untouched)
            });
            if used {
                rows = y + 1;
            }
        }
        rows
    }

    /// Replace the whole buffer (the `ctrl+o` editor round-trip): lines are
    /// joined with single spaces and the result trimmed. Preserves the yank
    /// register and keeps the change undoable.
    pub fn set_text(&mut self, text: &str) {
        let text = single_line(text.trim());
        let yank = self.textarea.yank_text();
        self.jump(0);
        let len = self.text().chars().count();
        if len > 0 {
            self.textarea.delete_str(len);
        }
        if !text.is_empty() {
            self.textarea.insert_str(text);
        }
        self.textarea.set_yank_text(yank);
    }

    /// Accept an inline `@category`/`#project` completion: replace the typed
    /// token text with the candidate's canonical spelling (case-insensitive
    /// matches must land on the exact category/project name, since
    /// `parse_task_input` compares exactly), then leave the cursor past a
    /// trailing space so the next word can start immediately.
    pub fn accept_suggestion(&mut self, suggestion: &TokenSuggestion) {
        let col = self.cursor_col();
        if suggestion.text_start > col {
            return;
        }
        let yank = self.textarea.yank_text();
        self.jump(suggestion.text_start);
        if col > suggestion.text_start {
            self.textarea.delete_str(col - suggestion.text_start);
        }
        self.textarea.insert_str(&suggestion.candidate);
        self.textarea.set_yank_text(yank);
        let chars = self.chars();
        if self.cursor_col() == chars.len() {
            self.textarea.insert_char(' ');
        } else if chars.get(self.cursor_col()) == Some(&' ') {
            self.textarea.move_cursor(CursorMove::Forward);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if ctrl && key.code == KeyCode::Char('o') {
            return Outcome::OpenEditor;
        }
        if key.code == KeyCode::Enter {
            return Outcome::Submit;
        }
        if key.code == KeyCode::Esc {
            return Outcome::Cancel;
        }
        // Readline/word-wise chords: they all carry a modifier, so they never
        // shadow plain-character typing.
        if let Some(action) = chord_action(key.code, ctrl, alt) {
            self.apply_chord(action);
            return Outcome::Consumed;
        }
        if ctrl {
            match key.code {
                KeyCode::Char('z') | KeyCode::Char('Z') => {
                    if shift || key.code == KeyCode::Char('Z') {
                        self.textarea.redo();
                    } else {
                        self.textarea.undo();
                    }
                }
                _ => {}
            }
            return Outcome::Consumed;
        }
        self.handle_edit_key(key)
    }

    /// Apply a readline-style chord (see [`chord_action`]). Deletions go
    /// through the textarea like ordinary edits, so they fill the yank
    /// register and land in the undo history.
    fn apply_chord(&mut self, action: ChordAction) {
        let chars = self.chars();
        let len = chars.len();
        let col = self.cursor_col();
        match action {
            ChordAction::WordLeft => self.jump(word_back(&chars, col)),
            ChordAction::WordRight => self.jump(word_forward(&chars, col)),
            ChordAction::Head => self.jump(0),
            ChordAction::End => self.textarea.move_cursor(CursorMove::End),
            ChordAction::DeleteWordBack => self.delete_range(word_back(&chars, col), col),
            ChordAction::DeleteWordForward => self.delete_range(col, word_forward(&chars, col)),
            ChordAction::DeleteToHead => self.delete_range(0, col),
            ChordAction::DeleteToEnd => self.delete_range(col, len),
        }
    }

    fn handle_edit_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            // A stray newline char can only arrive via paste-like input; the
            // buffer is a single logical line, so it becomes a space.
            KeyCode::Char('\n') | KeyCode::Char('\r') => self.textarea.insert_char(' '),
            KeyCode::Char(c) => self.textarea.insert_char(c),
            KeyCode::Backspace => {
                self.textarea.delete_char();
            }
            KeyCode::Delete => {
                self.textarea.delete_next_char();
            }
            KeyCode::Left => self.textarea.move_cursor(CursorMove::Back),
            KeyCode::Right => self.textarea.move_cursor(CursorMove::Forward),
            KeyCode::Home => self.textarea.move_cursor(CursorMove::Head),
            KeyCode::End => self.textarea.move_cursor(CursorMove::End),
            // Up/Down move across wrapped visual lines
            KeyCode::Up => self.textarea.move_cursor(CursorMove::Up),
            KeyCode::Down => self.textarea.move_cursor(CursorMove::Down),
            _ => {}
        }
        Outcome::Consumed
    }

    /// Delete the char range `start..end` through the textarea (filling the
    /// yank register).
    fn delete_range(&mut self, start: usize, end: usize) {
        let end = end.min(self.chars().len());
        let start = start.min(end);
        self.jump(start);
        if end > start {
            self.textarea.delete_str(end - start);
        }
    }

    fn chars(&self) -> Vec<char> {
        self.text().chars().collect()
    }

    fn jump(&mut self, col: usize) {
        let col = col.min(u16::MAX as usize) as u16;
        self.textarea.move_cursor(CursorMove::Jump(0, col));
    }
}

/// A readline-style word-wise action bound to a modifier chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChordAction {
    WordLeft,
    WordRight,
    Head,
    End,
    DeleteWordBack,
    DeleteWordForward,
    DeleteToHead,
    DeleteToEnd,
}

/// Map a modifier chord to its readline action, accepting every common
/// encoding per action.
fn chord_action(code: KeyCode, ctrl: bool, alt: bool) -> Option<ChordAction> {
    match code {
        KeyCode::Left if ctrl || alt => Some(ChordAction::WordLeft),
        KeyCode::Right if ctrl || alt => Some(ChordAction::WordRight),
        KeyCode::Char('b') if alt && !ctrl => Some(ChordAction::WordLeft),
        KeyCode::Char('f') if alt && !ctrl => Some(ChordAction::WordRight),
        KeyCode::Backspace if ctrl || alt => Some(ChordAction::DeleteWordBack),
        KeyCode::Char('w') if ctrl => Some(ChordAction::DeleteWordBack),
        KeyCode::Delete if ctrl || alt => Some(ChordAction::DeleteWordForward),
        KeyCode::Char('d') if alt && !ctrl => Some(ChordAction::DeleteWordForward),
        KeyCode::Char('a') if ctrl => Some(ChordAction::Head),
        KeyCode::Char('e') if ctrl => Some(ChordAction::End),
        KeyCode::Char('u') if ctrl => Some(ChordAction::DeleteToHead),
        KeyCode::Char('k') if ctrl => Some(ChordAction::DeleteToEnd),
        _ => None,
    }
}

/// Word/other/whitespace char classes. `w`-style motions treat a run of the
/// same class as one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Word,
    Punct,
}

fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Collapse any multi-line text to the single logical line the modal edits:
/// lines trimmed and joined with single spaces. Text that is already a
/// single line is preserved exactly.
fn single_line(text: &str) -> String {
    if !text.contains(['\n', '\r']) {
        return text.to_string();
    }
    text.split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Start of the next word, or `len` when there is none (so a delete-word
/// motion at the last word deletes to the end of the line).
fn word_forward(chars: &[char], col: usize) -> usize {
    let len = chars.len();
    if col >= len {
        return len;
    }
    let mut i = col;
    let class = char_class(chars[i]);
    if class != CharClass::Whitespace {
        while i < len && char_class(chars[i]) == class {
            i += 1;
        }
    }
    while i < len && char_class(chars[i]) == CharClass::Whitespace {
        i += 1;
    }
    i
}

/// Start of the current/previous word.
fn word_back(chars: &[char], col: usize) -> usize {
    let mut i = col.min(chars.len());
    while i > 0 && char_class(chars[i - 1]) == CharClass::Whitespace {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let class = char_class(chars[i - 1]);
    while i > 0 && char_class(chars[i - 1]) == class {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn press(te: &mut TextEdit, code: KeyCode) -> Outcome {
        te.handle_key(key(code))
    }

    fn press_str(te: &mut TextEdit, s: &str) {
        for c in s.chars() {
            press(te, KeyCode::Char(c));
        }
    }

    fn ctrl(te: &mut TextEdit, c: char) -> Outcome {
        te.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    fn chord(te: &mut TextEdit, code: KeyCode, mods: KeyModifiers) -> Outcome {
        te.handle_key(KeyEvent::new(code, mods))
    }

    #[test]
    fn opens_with_cursor_at_end() {
        let te = TextEdit::new(EditPurpose::AddTask, "hello".to_string());
        assert_eq!(te.text(), "hello");
        assert_eq!(te.cursor_col(), 5);
    }

    #[test]
    fn esc_cancels_immediately() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "abc".to_string());
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Cancel);
    }

    #[test]
    fn enter_submits() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "x".to_string());
        assert_eq!(press(&mut te, KeyCode::Enter), Outcome::Submit);
    }

    #[test]
    fn ctrl_o_opens_editor() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "x".to_string());
        assert_eq!(ctrl(&mut te, 'o'), Outcome::OpenEditor);
    }

    #[test]
    fn ctrl_e_is_line_end() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "abc".to_string());
        press(&mut te, KeyCode::Home);
        assert_eq!(ctrl(&mut te, 'e'), Outcome::Consumed);
        assert_eq!(te.cursor_col(), 3, "ctrl+e jumps to line end");
    }

    #[test]
    fn u_types_a_literal_u() {
        let mut te = TextEdit::new(EditPurpose::AddTask, String::new());
        press_str(&mut te, "undo");
        assert_eq!(te.text(), "undo");
    }

    #[test]
    fn undo_redo_with_ctrl_z() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        ctrl(&mut te, 'w'); // one undo group: delete word back
        assert_eq!(te.text(), "foo ");
        ctrl(&mut te, 'z');
        assert_eq!(te.text(), "foo bar", "ctrl+z undoes the deletion");
        te.handle_key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(te.text(), "foo ", "ctrl+shift+z redoes it");
    }

    #[test]
    fn insert_word_chords_move_word_wise() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar baz".to_string());
        assert_eq!(te.cursor_col(), 11);
        chord(&mut te, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 8, "ctrl+left to word start");
        chord(&mut te, KeyCode::Char('b'), KeyModifiers::ALT);
        assert_eq!(te.cursor_col(), 4, "alt+b to previous word");
        chord(&mut te, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(te.cursor_col(), 0, "alt+left to line start word");
        chord(&mut te, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 4, "ctrl+right to next word");
        chord(&mut te, KeyCode::Char('f'), KeyModifiers::ALT);
        assert_eq!(te.cursor_col(), 8, "alt+f to next word");
        chord(&mut te, KeyCode::Right, KeyModifiers::ALT);
        assert_eq!(te.cursor_col(), 11, "cursor may sit at line end");
    }

    #[test]
    fn insert_ctrl_a_jumps_to_line_start() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        assert_eq!(ctrl(&mut te, 'a'), Outcome::Consumed);
        assert_eq!(te.cursor_col(), 0);
    }

    #[test]
    fn insert_delete_word_back_variants_fill_register() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        ctrl(&mut te, 'w');
        assert_eq!(te.text(), "foo ");
        assert_eq!(te.cursor_col(), 4);
        assert_eq!(te.textarea().yank_text(), "bar", "kill fills the register");

        press_str(&mut te, "qux");
        chord(&mut te, KeyCode::Backspace, KeyModifiers::ALT);
        assert_eq!(te.text(), "foo ", "alt+backspace variant");
        assert_eq!(te.textarea().yank_text(), "qux");

        press_str(&mut te, "zed");
        chord(&mut te, KeyCode::Backspace, KeyModifiers::CONTROL);
        assert_eq!(te.text(), "foo ", "ctrl+backspace variant");
        assert_eq!(te.textarea().yank_text(), "zed");
    }

    #[test]
    fn insert_delete_word_forward_variants_fill_register() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar baz".to_string());
        ctrl(&mut te, 'a');
        chord(&mut te, KeyCode::Delete, KeyModifiers::CONTROL);
        assert_eq!(te.text(), "bar baz", "ctrl+delete kills to next word");
        assert_eq!(te.textarea().yank_text(), "foo ");
        chord(&mut te, KeyCode::Char('d'), KeyModifiers::ALT);
        assert_eq!(te.text(), "baz", "alt+d variant");
        assert_eq!(te.textarea().yank_text(), "bar ");
    }

    #[test]
    fn insert_ctrl_u_and_ctrl_k_kill_to_line_ends() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        chord(&mut te, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 4);
        ctrl(&mut te, 'u');
        assert_eq!(te.text(), "bar", "ctrl+u kills to line start");
        assert_eq!(te.textarea().yank_text(), "foo ");

        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        chord(&mut te, KeyCode::Left, KeyModifiers::CONTROL);
        ctrl(&mut te, 'k');
        assert_eq!(te.text(), "foo ", "ctrl+k kills to line end");
        assert_eq!(te.cursor_col(), 4, "insert cursor may sit at line end");
        assert_eq!(te.textarea().yank_text(), "bar");
    }

    #[test]
    fn insert_newline_char_becomes_space() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "a".to_string());
        press(&mut te, KeyCode::Char('\n'));
        press(&mut te, KeyCode::Char('b'));
        assert_eq!(te.text(), "a b");
    }

    #[test]
    fn insert_backspace_and_arrows_edit_buffer() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "abc".to_string());
        press(&mut te, KeyCode::Backspace);
        assert_eq!(te.text(), "ab");
        press(&mut te, KeyCode::Left);
        press_str(&mut te, "x");
        assert_eq!(te.text(), "axb");
        press(&mut te, KeyCode::Right);
        press_str(&mut te, "y");
        assert_eq!(te.text(), "axby");
    }

    #[test]
    fn set_text_joins_lines_with_single_spaces() {
        let mut te = TextEdit::new(EditPurpose::AddTask, String::new());
        te.set_text("  one \n\n two\r\nthree ");
        assert_eq!(te.text(), "one two three");
    }

    #[test]
    fn new_sanitizes_multiline_initial_text() {
        let te = TextEdit::new(EditPurpose::AddTask, "a\nb".to_string());
        assert_eq!(te.text(), "a b");
    }

    #[test]
    fn wrapped_rows_grows_with_content_and_caps() {
        let te = TextEdit::new(EditPurpose::AddTask, String::new());
        assert_eq!(te.wrapped_rows(10), 1, "empty buffer is one row");

        let te = TextEdit::new(EditPurpose::AddTask, "alpha bravo charlie".to_string());
        assert_eq!(te.wrapped_rows(80), 1);
        assert!(te.wrapped_rows(8) > 1, "narrow width wraps");

        let long = "word ".repeat(100);
        let te = TextEdit::new(EditPurpose::AddTask, long);
        assert_eq!(te.wrapped_rows(10), MAX_TEXT_ROWS, "capped at the max");
    }

    #[test]
    fn accept_suggestion_replaces_token_and_adds_space() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "fix @eng".to_string());
        te.accept_suggestion(&TokenSuggestion {
            text_start: 5,
            candidate: "engineering".to_string(),
            remainder: "ineering".to_string(),
        });
        assert_eq!(te.text(), "fix @engineering ");
        assert_eq!(te.cursor_col(), 17);
    }
}
