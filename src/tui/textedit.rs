//! The vim-capable soft-wrapping edit modal's buffer: a single logical line
//! held in a [`ratatui_textarea::TextArea`] with a normal/insert mode layer
//! on top.
//!
//! The buffer is always exactly one logical line; the widget wraps it
//! visually. Enter never inserts a newline (it submits), stray newline input
//! becomes a space, and vim operations that would create lines (`o`, `O`,
//! `J`) are excluded. Key handling returns an [`Outcome`] so
//! [`crate::tui::app`] owns what submit/cancel/open-editor actually do; the
//! motion/word logic is kept in pure functions so it is unit-testable
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

/// The modal's vim mode. It opens in insert; esc steps insert → normal, and
/// esc in normal cancels (surfaced as [`Outcome::Cancel`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    Insert,
    Normal,
}

/// Operator awaiting a motion (`d`/`c`/`y`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
}

/// The four pending-char find motions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindKind {
    FindForward,
    FindBack,
    TillForward,
    TillBack,
}

/// Multi-key normal-mode state: an operator waiting for its motion, a text
/// object waiting for `w`, a find/replace waiting for its char argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Op(Op),
    Object(Op),
    Find { op: Option<Op>, kind: FindKind },
    Replace,
}

/// The edit modal: purpose (what commit does), vim mode, and the textarea.
#[derive(Debug, Clone)]
pub struct TextEdit {
    pub purpose: EditPurpose,
    pub vim: VimMode,
    textarea: TextArea<'static>,
    pending: Pending,
}

impl TextEdit {
    /// Open the modal in insert mode with the cursor at the end of `initial`.
    pub fn new(purpose: EditPurpose, initial: String) -> Self {
        let mut textarea = TextArea::new(vec![single_line(&initial)]);
        textarea.set_wrap_mode(WrapMode::WordOrGlyph);
        textarea.set_cursor_line_style(Style::default());
        textarea.move_cursor(CursorMove::End);
        Self {
            purpose,
            vim: VimMode::Insert,
            textarea,
            pending: Pending::None,
        }
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
        if self.vim == VimMode::Normal {
            self.clamp_normal_col();
        }
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
        if ctrl && key.code == KeyCode::Char('o') {
            return Outcome::OpenEditor;
        }
        if key.code == KeyCode::Enter {
            return Outcome::Submit;
        }
        // Readline/word-wise chords apply in both modes: they all carry a
        // modifier, so they never shadow the plain-letter vim keys.
        if let Some(action) = chord_action(key.code, ctrl, alt, self.vim) {
            self.pending = Pending::None;
            self.apply_chord(action);
            return Outcome::Consumed;
        }
        if ctrl {
            if self.vim == VimMode::Normal && key.code == KeyCode::Char('r') {
                self.textarea.redo();
                self.clamp_normal_col();
            }
            return Outcome::Consumed;
        }
        match self.vim {
            VimMode::Insert => self.handle_insert_key(key),
            VimMode::Normal => self.handle_normal_key(key),
        }
    }

    /// Apply a readline-style chord (see [`chord_action`]). Deletions go
    /// through the textarea like the vim operators, so they fill the yank
    /// register and land in the undo history.
    fn apply_chord(&mut self, action: ChordAction) {
        let chars = self.chars();
        let len = chars.len();
        let col = self.cursor_col();
        match action {
            ChordAction::WordLeft => self.jump(word_back(&chars, col)),
            ChordAction::WordRight => {
                let target = word_forward(&chars, col);
                match self.vim {
                    VimMode::Insert => self.jump(target),
                    VimMode::Normal => self.jump(target.min(len.saturating_sub(1))),
                }
            }
            ChordAction::Head => self.jump(0),
            ChordAction::End => self.textarea.move_cursor(CursorMove::End),
            ChordAction::DeleteWordBack => self.delete_range(word_back(&chars, col), col),
            ChordAction::DeleteWordForward => self.delete_range(col, word_forward(&chars, col)),
            ChordAction::DeleteToHead => self.delete_range(0, col),
            ChordAction::DeleteToEnd => self.delete_range(col, len),
        }
    }

    fn handle_insert_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => {
                self.vim = VimMode::Normal;
                self.pending = Pending::None;
                // vim steps the cursor back one when leaving insert mode
                if self.cursor_col() > 0 {
                    self.textarea.move_cursor(CursorMove::Back);
                }
            }
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

    fn handle_normal_key(&mut self, key: KeyEvent) -> Outcome {
        let pending = std::mem::replace(&mut self.pending, Pending::None);
        match pending {
            Pending::Replace => {
                if let KeyCode::Char(c) = key.code {
                    self.replace_char(c);
                }
                return Outcome::Consumed;
            }
            Pending::Find { op, kind } => {
                if let KeyCode::Char(c) = key.code {
                    self.finish_find(op, kind, c);
                }
                return Outcome::Consumed;
            }
            Pending::Object(op) => {
                if key.code == KeyCode::Char('w') {
                    let (start, end) = inner_word(&self.chars(), self.cursor_col());
                    self.apply_op(op, start, end);
                }
                return Outcome::Consumed;
            }
            Pending::Op(op) => return self.handle_operator_key(op, key),
            Pending::None => {}
        }

        let chars = self.chars();
        let len = chars.len();
        let col = self.cursor_col();
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('i') => self.vim = VimMode::Insert,
            KeyCode::Char('a') => {
                self.textarea.move_cursor(CursorMove::Forward);
                self.vim = VimMode::Insert;
            }
            KeyCode::Char('I') => {
                self.jump(first_non_blank(&chars));
                self.vim = VimMode::Insert;
            }
            KeyCode::Char('A') => {
                self.textarea.move_cursor(CursorMove::End);
                self.vim = VimMode::Insert;
            }
            KeyCode::Char('h') | KeyCode::Left => self.textarea.move_cursor(CursorMove::Back),
            KeyCode::Char('l') | KeyCode::Right => {
                self.jump((col + 1).min(len.saturating_sub(1)));
            }
            KeyCode::Char('w') => self.jump(word_forward(&chars, col).min(len.saturating_sub(1))),
            KeyCode::Char('e') => self.jump(word_end(&chars, col)),
            KeyCode::Char('b') => self.jump(word_back(&chars, col)),
            KeyCode::Char('0') | KeyCode::Home => self.jump(0),
            KeyCode::Char('^') => self.jump(first_non_blank(&chars)),
            KeyCode::Char('$') | KeyCode::End => self.jump(len.saturating_sub(1)),
            KeyCode::Char('f') => self.pend_find(None, FindKind::FindForward),
            KeyCode::Char('F') => self.pend_find(None, FindKind::FindBack),
            KeyCode::Char('t') => self.pend_find(None, FindKind::TillForward),
            KeyCode::Char('T') => self.pend_find(None, FindKind::TillBack),
            KeyCode::Char('d') => self.pending = Pending::Op(Op::Delete),
            KeyCode::Char('c') => self.pending = Pending::Op(Op::Change),
            KeyCode::Char('y') => self.pending = Pending::Op(Op::Yank),
            KeyCode::Char('x') => self.apply_op(Op::Delete, col, col + 1),
            KeyCode::Char('X') => self.apply_op(Op::Delete, col.saturating_sub(1), col),
            KeyCode::Char('D') => self.apply_op(Op::Delete, col, len),
            KeyCode::Char('C') => self.apply_op(Op::Change, col, len),
            KeyCode::Char('r') => self.pending = Pending::Replace,
            KeyCode::Char('~') => self.toggle_case(),
            KeyCode::Char('p') => self.paste_after(),
            KeyCode::Char('P') => self.paste_before(),
            KeyCode::Char('u') => {
                self.textarea.undo();
                self.clamp_normal_col();
            }
            KeyCode::Up => self.textarea.move_cursor(CursorMove::Up),
            KeyCode::Down => self.textarea.move_cursor(CursorMove::Down),
            _ => {}
        }
        Outcome::Consumed
    }

    fn handle_operator_key(&mut self, op: Op, key: KeyEvent) -> Outcome {
        let chars = self.chars();
        let len = chars.len();
        let col = self.cursor_col();
        match key.code {
            KeyCode::Char('d') if op == Op::Delete => self.apply_line_op(op),
            KeyCode::Char('c') if op == Op::Change => self.apply_line_op(op),
            KeyCode::Char('y') if op == Op::Yank => self.apply_line_op(op),
            KeyCode::Char('i') => self.pending = Pending::Object(op),
            KeyCode::Char('w') => {
                // `cw` on a word behaves like `ce`, per vim
                if op == Op::Change && col < len && char_class(chars[col]) != CharClass::Whitespace
                {
                    self.apply_op(op, col, word_end(&chars, col) + 1);
                } else {
                    self.apply_op(op, col, word_forward(&chars, col));
                }
            }
            KeyCode::Char('e') => self.apply_op(op, col, word_end(&chars, col) + 1),
            KeyCode::Char('b') => self.apply_op(op, word_back(&chars, col), col),
            KeyCode::Char('0') => self.apply_op(op, 0, col),
            KeyCode::Char('^') => {
                let head = first_non_blank(&chars);
                self.apply_op(op, head.min(col), head.max(col));
            }
            KeyCode::Char('$') => self.apply_op(op, col, len),
            KeyCode::Char('h') => self.apply_op(op, col.saturating_sub(1), col),
            KeyCode::Char('l') => self.apply_op(op, col, col + 1),
            KeyCode::Char('f') => self.pend_find(Some(op), FindKind::FindForward),
            KeyCode::Char('F') => self.pend_find(Some(op), FindKind::FindBack),
            KeyCode::Char('t') => self.pend_find(Some(op), FindKind::TillForward),
            KeyCode::Char('T') => self.pend_find(Some(op), FindKind::TillBack),
            _ => {}
        }
        Outcome::Consumed
    }

    fn pend_find(&mut self, op: Option<Op>, kind: FindKind) {
        self.pending = Pending::Find { op, kind };
    }

    fn finish_find(&mut self, op: Option<Op>, kind: FindKind, target: char) {
        let chars = self.chars();
        let col = self.cursor_col();
        let Some(found) = find_char(&chars, col, target, kind) else {
            return;
        };
        match op {
            None => self.jump(found),
            Some(op) => match kind {
                FindKind::FindForward | FindKind::TillForward => {
                    self.apply_op(op, col, found + 1);
                }
                FindKind::FindBack | FindKind::TillBack => self.apply_op(op, found, col),
            },
        }
    }

    fn replace_char(&mut self, c: char) {
        let c = if c == '\n' || c == '\r' { ' ' } else { c };
        if self.cursor_col() < self.chars().len() {
            self.textarea.delete_next_char();
            self.textarea.insert_char(c);
            self.textarea.move_cursor(CursorMove::Back);
        }
    }

    fn toggle_case(&mut self) {
        let chars = self.chars();
        if let Some(&c) = chars.get(self.cursor_col()) {
            let toggled: String = if c.is_uppercase() {
                c.to_lowercase().collect()
            } else {
                c.to_uppercase().collect()
            };
            self.textarea.delete_next_char();
            self.textarea.insert_str(toggled);
            self.clamp_normal_col();
        }
    }

    fn paste_after(&mut self) {
        if self.textarea.yank_text().is_empty() {
            return;
        }
        let len = self.chars().len();
        if len > 0 {
            self.jump((self.cursor_col() + 1).min(len));
        }
        if self.textarea.paste() {
            self.textarea.move_cursor(CursorMove::Back);
        }
    }

    fn paste_before(&mut self) {
        if self.textarea.yank_text().is_empty() {
            return;
        }
        if self.textarea.paste() {
            self.textarea.move_cursor(CursorMove::Back);
        }
    }

    /// Apply an operator to the char range `start..end` of the buffer.
    /// Deletes go through the textarea so they land in the crate's history
    /// (`u`/`ctrl+r`) and yank register (`p` pastes the last deletion).
    fn apply_op(&mut self, op: Op, start: usize, end: usize) {
        let chars = self.chars();
        let end = end.min(chars.len());
        let start = start.min(end);
        match op {
            Op::Yank => {
                if end > start {
                    let text: String = chars[start..end].iter().collect();
                    self.textarea.set_yank_text(text);
                }
                self.jump(start);
            }
            Op::Delete => self.delete_range(start, end),
            Op::Change => {
                self.jump(start);
                if end > start {
                    self.textarea.delete_str(end - start);
                }
                self.vim = VimMode::Insert;
            }
        }
    }

    /// Delete the char range `start..end` through the textarea (filling the
    /// yank register), keeping the normal-mode cursor invariant when in
    /// normal mode. In insert mode the cursor may legally sit at `len`.
    fn delete_range(&mut self, start: usize, end: usize) {
        let end = end.min(self.chars().len());
        let start = start.min(end);
        self.jump(start);
        if end > start {
            self.textarea.delete_str(end - start);
        }
        if self.vim == VimMode::Normal {
            self.clamp_normal_col();
        }
    }

    /// `dd`/`cc`/`yy`: the buffer is one logical line, so they act on all of
    /// it. `yy` keeps the cursor where it is, per vim.
    fn apply_line_op(&mut self, op: Op) {
        let len = self.chars().len();
        match op {
            Op::Yank => {
                if len > 0 {
                    let line = self.text().to_string();
                    self.textarea.set_yank_text(line);
                }
            }
            Op::Delete | Op::Change => {
                self.jump(0);
                if len > 0 {
                    self.textarea.delete_str(len);
                }
                if op == Op::Change {
                    self.vim = VimMode::Insert;
                }
            }
        }
    }

    fn chars(&self) -> Vec<char> {
        self.text().chars().collect()
    }

    fn jump(&mut self, col: usize) {
        let col = col.min(u16::MAX as usize) as u16;
        self.textarea.move_cursor(CursorMove::Jump(0, col));
    }

    /// Normal mode keeps the cursor on a char (col ≤ len-1), like vim.
    fn clamp_normal_col(&mut self) {
        let max = self.chars().len().saturating_sub(1);
        if self.cursor_col() > max {
            self.jump(max);
        }
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
/// encoding per action. The emacs-letter chords (`ctrl+a`/`e`/`u`/`k`) are
/// insert-only so normal mode stays purely vim; the arrow/backspace/delete
/// chords and `alt` letters apply in both modes since they cannot collide
/// with plain-letter vim keys.
fn chord_action(code: KeyCode, ctrl: bool, alt: bool, vim: VimMode) -> Option<ChordAction> {
    let insert = vim == VimMode::Insert;
    match code {
        KeyCode::Left if ctrl || alt => Some(ChordAction::WordLeft),
        KeyCode::Right if ctrl || alt => Some(ChordAction::WordRight),
        KeyCode::Char('b') if alt && !ctrl => Some(ChordAction::WordLeft),
        KeyCode::Char('f') if alt && !ctrl => Some(ChordAction::WordRight),
        KeyCode::Backspace if ctrl || alt => Some(ChordAction::DeleteWordBack),
        KeyCode::Char('w') if ctrl => Some(ChordAction::DeleteWordBack),
        KeyCode::Delete if ctrl || alt => Some(ChordAction::DeleteWordForward),
        KeyCode::Char('d') if alt && !ctrl => Some(ChordAction::DeleteWordForward),
        KeyCode::Char('a') if ctrl && insert => Some(ChordAction::Head),
        KeyCode::Char('e') if ctrl && insert => Some(ChordAction::End),
        KeyCode::Char('u') if ctrl && insert => Some(ChordAction::DeleteToHead),
        KeyCode::Char('k') if ctrl && insert => Some(ChordAction::DeleteToEnd),
        _ => None,
    }
}

/// Vim's three char classes: word chars, other printable chars, whitespace.
/// `w`/`b`/`e`/`iw` treat a run of the same class as one unit.
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

/// `w`: start of the next word, or `len` when there is none (so `dw` on the
/// last word deletes to the end of the line).
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

/// `b`: start of the current/previous word.
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

/// `e`: last char of the next word end after `col`; stays put when there is
/// nothing beyond.
fn word_end(chars: &[char], col: usize) -> usize {
    let len = chars.len();
    if len == 0 {
        return 0;
    }
    let mut i = col + 1;
    while i < len && char_class(chars[i]) == CharClass::Whitespace {
        i += 1;
    }
    if i >= len {
        return col.min(len - 1);
    }
    let class = char_class(chars[i]);
    while i + 1 < len && char_class(chars[i + 1]) == class {
        i += 1;
    }
    i
}

/// `^`: first non-blank char (0 for an all-blank line).
fn first_non_blank(chars: &[char]) -> usize {
    chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0)
}

/// `f`/`F`/`t`/`T`: the column the cursor would land on, or `None` when the
/// char is absent or the motion would not move (vim's `t` fails when the
/// target is adjacent).
fn find_char(chars: &[char], col: usize, target: char, kind: FindKind) -> Option<usize> {
    match kind {
        FindKind::FindForward => (col + 1..chars.len()).find(|&i| chars[i] == target),
        FindKind::TillForward => (col + 1..chars.len())
            .find(|&i| chars[i] == target)
            .map(|i| i - 1)
            .filter(|&t| t > col),
        FindKind::FindBack => (0..col).rev().find(|&i| chars[i] == target),
        FindKind::TillBack => (0..col)
            .rev()
            .find(|&i| chars[i] == target)
            .map(|i| i + 1)
            .filter(|&t| t < col),
    }
}

/// `iw`: the run of same-class chars under the cursor (a whitespace run when
/// the cursor sits on whitespace, per vim).
fn inner_word(chars: &[char], col: usize) -> (usize, usize) {
    let len = chars.len();
    if len == 0 {
        return (0, 0);
    }
    let col = col.min(len - 1);
    let class = char_class(chars[col]);
    let mut start = col;
    while start > 0 && char_class(chars[start - 1]) == class {
        start -= 1;
    }
    let mut end = col + 1;
    while end < len && char_class(chars[end]) == class {
        end += 1;
    }
    (start, end)
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

    /// A modal over `text` in normal mode, cursor on the last char.
    fn normal(text: &str) -> TextEdit {
        let mut te = TextEdit::new(EditPurpose::AddTask, text.to_string());
        press(&mut te, KeyCode::Esc);
        assert_eq!(te.vim, VimMode::Normal);
        te
    }

    #[test]
    fn opens_in_insert_mode_with_cursor_at_end() {
        let te = TextEdit::new(EditPurpose::AddTask, "hello".to_string());
        assert_eq!(te.vim, VimMode::Insert);
        assert_eq!(te.text(), "hello");
        assert_eq!(te.cursor_col(), 5);
    }

    #[test]
    fn esc_steps_insert_to_normal_then_cancels() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "ab".to_string());
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Consumed);
        assert_eq!(te.vim, VimMode::Normal);
        assert_eq!(te.cursor_col(), 1, "vim steps back leaving insert");
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Cancel);
    }

    #[test]
    fn esc_clears_pending_before_cancelling() {
        let mut te = normal("abc");
        press(&mut te, KeyCode::Char('f'));
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Consumed, "clears f");
        press(&mut te, KeyCode::Char('d'));
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Consumed, "clears d");
        assert_eq!(te.text(), "abc", "no operator ran");
        assert_eq!(press(&mut te, KeyCode::Esc), Outcome::Cancel);
    }

    #[test]
    fn enter_submits_from_both_modes() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "x".to_string());
        assert_eq!(press(&mut te, KeyCode::Enter), Outcome::Submit);
        press(&mut te, KeyCode::Esc);
        assert_eq!(press(&mut te, KeyCode::Enter), Outcome::Submit);
    }

    #[test]
    fn ctrl_o_opens_editor_from_both_modes() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "x".to_string());
        assert_eq!(ctrl(&mut te, 'o'), Outcome::OpenEditor);
        press(&mut te, KeyCode::Esc);
        assert_eq!(ctrl(&mut te, 'o'), Outcome::OpenEditor);
    }

    #[test]
    fn ctrl_e_is_line_end_not_editor() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "abc".to_string());
        press(&mut te, KeyCode::Home);
        assert_eq!(ctrl(&mut te, 'e'), Outcome::Consumed);
        assert_eq!(te.cursor_col(), 3, "ctrl+e jumps to line end in insert");
        press(&mut te, KeyCode::Esc);
        assert_eq!(ctrl(&mut te, 'e'), Outcome::Consumed);
        assert_eq!(te.text(), "abc", "ctrl+e is inert in normal mode");
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
        assert_eq!(te.cursor_col(), 11, "insert mode may sit at line end");
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
    fn killed_text_pastes_back_with_vim_p() {
        let mut te = TextEdit::new(EditPurpose::AddTask, "foo bar".to_string());
        ctrl(&mut te, 'w');
        press(&mut te, KeyCode::Esc);
        press(&mut te, KeyCode::Char('p'));
        assert_eq!(te.text(), "foo bar", "register round-trips through p");
    }

    #[test]
    fn normal_mode_word_chords_and_vim_keys_coexist() {
        let mut te = normal("foo bar baz");
        press(&mut te, KeyCode::Char('0'));
        chord(&mut te, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 4, "ctrl+right word motion in normal");
        chord(&mut te, KeyCode::Right, KeyModifiers::CONTROL);
        chord(&mut te, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 10, "clamped to the last char");
        chord(&mut te, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(te.cursor_col(), 8, "ctrl+left word motion in normal");
        ctrl(&mut te, 'w');
        assert_eq!(te.text(), "foo baz", "ctrl+w kills word back in normal");
        assert_eq!(te.vim, VimMode::Normal, "no mode change");
        press(&mut te, KeyCode::Char('b'));
        assert_eq!(te.cursor_col(), 0, "plain vim b still wins in normal");
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.cursor_col(), 4, "plain vim w still wins in normal");
    }

    #[test]
    fn normal_mode_chord_clears_a_pending_operator() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('d'));
        chord(&mut te, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(te.text(), "foo bar", "chord aborts the pending operator");
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.text(), "foo bar", "w moves instead of completing dw");
        assert_eq!(te.cursor_col(), 6, "w clamps to the last char");
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
    fn word_motions_move_across_classes() {
        let mut te = normal("foo bar-baz qux");
        press(&mut te, KeyCode::Char('0'));
        assert_eq!(te.cursor_col(), 0);
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.cursor_col(), 4, "w to bar");
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.cursor_col(), 7, "w stops on punctuation");
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.cursor_col(), 8, "w to baz");
        press(&mut te, KeyCode::Char('e'));
        assert_eq!(te.cursor_col(), 10, "e to end of baz");
        press(&mut te, KeyCode::Char('b'));
        assert_eq!(te.cursor_col(), 8, "b to start of baz");
        press(&mut te, KeyCode::Char('$'));
        assert_eq!(te.cursor_col(), 14, "$ to last char");
        press(&mut te, KeyCode::Char('w'));
        assert_eq!(te.cursor_col(), 14, "w at end clamps to last char");
    }

    #[test]
    fn caret_and_zero_motions() {
        let mut te = normal("  abc");
        press(&mut te, KeyCode::Char('0'));
        assert_eq!(te.cursor_col(), 0);
        press(&mut te, KeyCode::Char('^'));
        assert_eq!(te.cursor_col(), 2, "^ to first non-blank");
    }

    #[test]
    fn h_and_l_clamp_at_the_edges() {
        let mut te = normal("ab");
        press(&mut te, KeyCode::Char('l'));
        assert_eq!(te.cursor_col(), 1, "l clamps at last char");
        press(&mut te, KeyCode::Char('h'));
        press(&mut te, KeyCode::Char('h'));
        assert_eq!(te.cursor_col(), 0, "h clamps at 0");
    }

    #[test]
    fn find_and_till_motions() {
        let mut te = normal("abcabc");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('f'));
        press(&mut te, KeyCode::Char('c'));
        assert_eq!(te.cursor_col(), 2, "fc");
        press(&mut te, KeyCode::Char('t'));
        press(&mut te, KeyCode::Char('c'));
        assert_eq!(te.cursor_col(), 4, "tc lands before the next c");
        press(&mut te, KeyCode::Char('F'));
        press(&mut te, KeyCode::Char('a'));
        assert_eq!(te.cursor_col(), 3, "Fa");
        press(&mut te, KeyCode::Char('T'));
        press(&mut te, KeyCode::Char('c'));
        assert_eq!(te.cursor_col(), 3, "Tc adjacent does not move");
        press(&mut te, KeyCode::Char('f'));
        press(&mut te, KeyCode::Char('z'));
        assert_eq!(te.cursor_col(), 3, "missing char does not move");
    }

    #[test]
    fn dw_deletes_to_next_word() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "dw");
        assert_eq!(te.text(), "bar");
        assert_eq!(te.cursor_col(), 0);
        assert_eq!(te.vim, VimMode::Normal);
    }

    #[test]
    fn de_deletes_through_word_end() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "de");
        assert_eq!(te.text(), " bar");
    }

    #[test]
    fn d_dollar_deletes_to_end_and_clamps_cursor() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press_str(&mut te, "d$");
        assert_eq!(te.text(), "foo ");
        assert_eq!(te.cursor_col(), 3, "cursor clamped onto the last char");
    }

    #[test]
    fn db_deletes_back_to_word_start() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press_str(&mut te, "db");
        assert_eq!(te.text(), "bar");
    }

    #[test]
    fn dfx_deletes_through_found_char() {
        let mut te = normal("foo xbar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "dfx");
        assert_eq!(te.text(), "bar");
    }

    #[test]
    fn diw_deletes_inner_word() {
        let mut te = normal("foo bar baz");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press_str(&mut te, "diw");
        assert_eq!(te.text(), "foo  baz");
        assert_eq!(te.vim, VimMode::Normal);
    }

    #[test]
    fn diw_on_whitespace_deletes_the_run() {
        let mut te = normal("a  b");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('l'));
        press_str(&mut te, "diw");
        assert_eq!(te.text(), "ab");
    }

    #[test]
    fn ciw_changes_inner_word_and_enters_insert() {
        let mut te = normal("foo bar baz");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press_str(&mut te, "ciw");
        assert_eq!(te.vim, VimMode::Insert);
        press_str(&mut te, "qux");
        assert_eq!(te.text(), "foo qux baz");
    }

    #[test]
    fn cw_behaves_like_ce_on_a_word() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "cw");
        assert_eq!(te.text(), " bar", "trailing space kept, like ce");
        assert_eq!(te.vim, VimMode::Insert);
        press_str(&mut te, "eat");
        assert_eq!(te.text(), "eat bar");
    }

    #[test]
    fn dd_clears_the_buffer() {
        let mut te = normal("whole line");
        press_str(&mut te, "dd");
        assert_eq!(te.text(), "");
        assert_eq!(te.vim, VimMode::Normal);
    }

    #[test]
    fn cc_clears_and_enters_insert() {
        let mut te = normal("whole line");
        press_str(&mut te, "cc");
        assert_eq!(te.text(), "");
        assert_eq!(te.vim, VimMode::Insert);
        press_str(&mut te, "new");
        assert_eq!(te.text(), "new");
    }

    #[test]
    fn x_and_shift_x_delete_around_cursor() {
        let mut te = normal("abc");
        press(&mut te, KeyCode::Char('x'));
        assert_eq!(te.text(), "ab");
        assert_eq!(te.cursor_col(), 1, "cursor clamped onto last char");
        press(&mut te, KeyCode::Char('X'));
        assert_eq!(te.text(), "b");
    }

    #[test]
    fn r_replaces_char_in_place() {
        let mut te = normal("abc");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "rz");
        assert_eq!(te.text(), "zbc");
        assert_eq!(te.cursor_col(), 0);
        assert_eq!(te.vim, VimMode::Normal);
    }

    #[test]
    fn tilde_toggles_case_and_advances() {
        let mut te = normal("ab");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('~'));
        assert_eq!(te.text(), "Ab");
        assert_eq!(te.cursor_col(), 1);
        press(&mut te, KeyCode::Char('~'));
        assert_eq!(te.text(), "AB");
        assert_eq!(te.cursor_col(), 1, "clamped at last char");
    }

    #[test]
    fn shift_d_and_shift_c_cut_to_end() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press(&mut te, KeyCode::Char('D'));
        assert_eq!(te.text(), "foo ");
        assert_eq!(te.vim, VimMode::Normal);

        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('w'));
        press(&mut te, KeyCode::Char('C'));
        assert_eq!(te.text(), "foo ");
        assert_eq!(te.vim, VimMode::Insert);
    }

    #[test]
    fn yy_then_p_duplicates_the_line() {
        let mut te = normal("ab");
        press_str(&mut te, "yy");
        assert_eq!(te.text(), "ab", "yy leaves the buffer intact");
        press(&mut te, KeyCode::Char('$'));
        press(&mut te, KeyCode::Char('p'));
        assert_eq!(te.text(), "abab");
        assert_eq!(te.cursor_col(), 3, "cursor on last pasted char");
    }

    #[test]
    fn shift_p_pastes_before_cursor() {
        let mut te = normal("ab");
        press_str(&mut te, "yy");
        press(&mut te, KeyCode::Char('0'));
        press(&mut te, KeyCode::Char('P'));
        assert_eq!(te.text(), "abab");
        assert_eq!(te.cursor_col(), 1);
    }

    #[test]
    fn yw_moves_nothing_and_fills_register() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "yw");
        assert_eq!(te.text(), "foo bar");
        assert_eq!(te.cursor_col(), 0, "yank does not move a forward motion");
        press(&mut te, KeyCode::Char('$'));
        press(&mut te, KeyCode::Char('p'));
        assert_eq!(te.text(), "foo barfoo ");
    }

    #[test]
    fn deleted_text_lands_in_the_register() {
        let mut te = normal("foo bar");
        press(&mut te, KeyCode::Char('0'));
        press_str(&mut te, "dw");
        assert_eq!(te.text(), "bar");
        press(&mut te, KeyCode::Char('$'));
        press(&mut te, KeyCode::Char('p'));
        assert_eq!(te.text(), "barfoo ");
    }

    #[test]
    fn undo_and_redo_roundtrip() {
        let mut te = TextEdit::new(EditPurpose::AddTask, String::new());
        press_str(&mut te, "hello");
        press(&mut te, KeyCode::Esc);
        press_str(&mut te, "dd");
        assert_eq!(te.text(), "");
        press(&mut te, KeyCode::Char('u'));
        assert_eq!(te.text(), "hello");
        ctrl(&mut te, 'r');
        assert_eq!(te.text(), "");
    }

    #[test]
    fn line_creating_keys_are_noops() {
        let mut te = normal("ab");
        for c in ['o', 'O', 'J'] {
            press(&mut te, KeyCode::Char(c));
            assert_eq!(te.text(), "ab", "'{c}' must not change the buffer");
            assert_eq!(te.vim, VimMode::Normal, "'{c}' must not switch modes");
        }
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
