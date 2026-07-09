//! App state machine for the TUI: the active tab, pane focus, input mode,
//! per-pane selection, and all write-through mutations against
//! [`Store`]/[`NotesStore`].
//!
//! Rendering is kept pure (see [`crate::tui::views`]): every field the views
//! read lives here, and [`App::handle_key`] is the single entry point for
//! state transitions, so `TestBackend` tests can drive the whole app by
//! feeding synthetic key events through the same path the event loop uses.

use crate::config::Config;
use crate::model::{Status, Task};
use crate::notes::{Line, NoteDoc, NotesStore};
use crate::standup::{StandupReport, build_report};
use crate::store::Store;
use crate::tui::editor;
use chrono::{Local, NaiveDate};
use color_eyre::eyre::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use std::path::PathBuf;

/// The four top-level tabs shown in the tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Today,
    Standup,
    Tasks,
    Notes,
}

/// Which pane owns keyboard input: the tab content or the notes side pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Main,
    Side,
}

/// What an in-progress single-line input buffer will do when committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditPurpose {
    AddTask,
    EditTask { id: String },
    DueDate { id: String },
    Filter,
    NewNoteTitle,
    AddNoteItem { heading: String },
    EditNoteItem { heading: String, item_index: usize },
}

/// A single-line input buffer with a char-index cursor.
#[derive(Debug, Clone)]
pub struct Editing {
    pub purpose: EditPurpose,
    pub buffer: String,
    pub cursor: usize,
}

impl Editing {
    fn new(purpose: EditPurpose, buffer: String) -> Self {
        let cursor = buffer.chars().count();
        Self {
            purpose,
            buffer,
            cursor,
        }
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.buffer.len())
    }

    fn insert(&mut self, c: char) {
        let byte = self.byte_at(self.cursor);
        self.buffer.insert(byte, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        if self.cursor < self.buffer.chars().count() {
            self.cursor += 1;
        }
    }
}

/// Input mode: normal navigation, an active input box, or a y/n confirm.
#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Editing(Editing),
    ConfirmDelete,
}

/// A note item resolved to the section heading and per-section item index the
/// `notes` module needs for edit/delete.
#[derive(Debug, Clone)]
pub struct NoteItemRef {
    pub heading: String,
    pub item_index: usize,
    pub text: String,
}

/// A note doc as shown in the Notes tab's list: title + total item count.
#[derive(Debug, Clone)]
pub struct NoteSummary {
    pub slug: String,
    pub title: String,
    pub item_count: usize,
}

pub struct App {
    pub store: Store,
    pub notes: NotesStore,
    pub config: Config,

    pub tab: Tab,
    pub focus: Focus,
    pub mode: Mode,

    pub tasks: Vec<Task>,
    pub archive: Vec<Task>,
    pub standup: StandupReport,
    pub notes_list: Vec<NoteSummary>,
    pub current_note: Option<NoteDoc>,

    pub today_sel: usize,
    pub tasks_sel: usize,
    pub notes_sel: usize,
    pub note_item_sel: usize,

    pub filter_text: String,
    pub cat_filter: Option<usize>,
    pub proj_filter: Option<usize>,

    pub footer_msg: Option<String>,
    pub editor_request: Option<PathBuf>,
    pub should_quit: bool,

    pub today: NaiveDate,
}

impl App {
    /// Build an app over already-resolved stores/config, loading initial data.
    pub fn new(store: Store, notes: NotesStore, config: Config) -> Result<Self> {
        let standup = build_report(&store)?;
        let mut app = App {
            store,
            notes,
            config,
            tab: Tab::Today,
            focus: Focus::Main,
            mode: Mode::Normal,
            tasks: Vec::new(),
            archive: Vec::new(),
            standup,
            notes_list: Vec::new(),
            current_note: None,
            today_sel: 0,
            tasks_sel: 0,
            notes_sel: 0,
            note_item_sel: 0,
            filter_text: String::new(),
            cat_filter: None,
            proj_filter: None,
            footer_msg: None,
            editor_request: None,
            should_quit: false,
            today: Local::now().date_naive(),
        };
        app.reload()?;

        // Preload the side pane: the last-opened note from `state.json` if it
        // still exists, else the first note.
        let idx = app
            .load_last_note_slug()
            .and_then(|slug| app.notes_list.iter().position(|s| s.slug == slug))
            .or(if app.notes_list.is_empty() {
                None
            } else {
                Some(0)
            });
        if let Some(idx) = idx {
            app.open_note_at(idx)?;
        }
        Ok(app)
    }

    /// Reload all cached data from disk and re-clamp selection indices.
    pub fn reload(&mut self) -> Result<()> {
        self.tasks = self.store.load_tasks()?;
        self.archive = self.store.load_archive()?;
        self.standup = build_report(&self.store)?;
        self.notes_list = self
            .notes
            .list()?
            .into_iter()
            .map(|(slug, title)| {
                let item_count = self
                    .notes
                    .load(&slug)
                    .map(|doc| count_items(&doc))
                    .unwrap_or(0);
                NoteSummary {
                    slug,
                    title,
                    item_count,
                }
            })
            .collect();
        self.clamp_selection();
        Ok(())
    }

    fn reload_current_note(&mut self) -> Result<()> {
        if let Some(doc) = &self.current_note {
            let slug = doc.slug.clone();
            self.current_note = Some(self.notes.load(&slug)?);
        }
        self.reload()
    }

    // ---- derived, view-specific lists -------------------------------------

    /// Active (open/blocked) tasks ordered overdue → due-today → rest, then by
    /// creation time.
    pub fn today_active(&self) -> Vec<Task> {
        let today = self.today;
        let mut v: Vec<Task> = self
            .tasks
            .iter()
            .filter(|t| matches!(t.status, Status::Open | Status::Blocked))
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            due_bucket(a, today)
                .cmp(&due_bucket(b, today))
                .then(a.created_at.cmp(&b.created_at))
        });
        v
    }

    /// Tasks completed today (from the archive), for the dimmed footer of the
    /// Today view.
    pub fn today_completions(&self) -> Vec<Task> {
        self.archive
            .iter()
            .filter(|t| t.completed_at.map(|c| c.date_naive()) == Some(self.today))
            .cloned()
            .collect()
    }

    /// Distinct project names across active tasks, sorted, for project-filter
    /// cycling.
    pub fn distinct_projects(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .tasks
            .iter()
            .filter_map(|t| t.project.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Active tasks narrowed by the current text/category/project filters.
    pub fn tasks_filtered(&self) -> Vec<Task> {
        let cat = self
            .cat_filter
            .and_then(|i| self.config.categories.get(i).cloned());
        let projects = self.distinct_projects();
        let proj = self.proj_filter.and_then(|i| projects.get(i).cloned());
        let needle = self.filter_text.to_lowercase();
        self.tasks
            .iter()
            .filter(|t| {
                (needle.is_empty() || t.text.to_lowercase().contains(&needle))
                    && cat.as_ref().is_none_or(|c| &t.category == c)
                    && proj.as_ref().is_none_or(|p| t.project.as_ref() == Some(p))
            })
            .cloned()
            .collect()
    }

    pub fn category_filter_label(&self) -> String {
        match self.cat_filter.and_then(|i| self.config.categories.get(i)) {
            Some(c) => c.clone(),
            None => "all".to_string(),
        }
    }

    pub fn project_filter_label(&self) -> String {
        match self
            .proj_filter
            .and_then(|i| self.distinct_projects().get(i).cloned())
        {
            Some(p) => p,
            None => "all".to_string(),
        }
    }

    /// Flattened note items (section heading + per-section index) of the
    /// currently open doc.
    pub fn note_items(&self) -> Vec<NoteItemRef> {
        let mut out = Vec::new();
        if let Some(doc) = &self.current_note {
            for section in &doc.body.sections {
                let mut idx = 0;
                for line in &section.lines {
                    if let Line::Item(text) = line {
                        out.push(NoteItemRef {
                            heading: section.heading.clone(),
                            item_index: idx,
                            text: text.clone(),
                        });
                        idx += 1;
                    }
                }
            }
        }
        out
    }

    fn selected_task(&self) -> Option<Task> {
        match self.tab {
            Tab::Today => self.today_active().into_iter().nth(self.today_sel),
            Tab::Tasks => self.tasks_filtered().into_iter().nth(self.tasks_sel),
            _ => None,
        }
    }

    /// Heading the "add item" action targets: the selected item's section, else
    /// the first section, else a default `Notes` heading.
    fn current_heading(&self) -> String {
        if let Some(it) = self.note_items().get(self.note_item_sel) {
            return it.heading.clone();
        }
        if let Some(doc) = &self.current_note {
            if let Some(s) = doc.body.sections.iter().find(|s| !s.heading.is_empty()) {
                return s.heading.clone();
            }
        }
        "Notes".to_string()
    }

    // ---- selection helpers ------------------------------------------------

    fn current_len(&self) -> usize {
        match self.focus {
            Focus::Side => self.note_items().len(),
            Focus::Main => match self.tab {
                Tab::Today => self.today_active().len(),
                Tab::Tasks => self.tasks_filtered().len(),
                Tab::Notes => self.notes_list.len(),
                Tab::Standup => 0,
            },
        }
    }

    fn current_sel_mut(&mut self) -> &mut usize {
        match self.focus {
            Focus::Side => &mut self.note_item_sel,
            Focus::Main => match self.tab {
                Tab::Today | Tab::Standup => &mut self.today_sel,
                Tab::Tasks => &mut self.tasks_sel,
                Tab::Notes => &mut self.notes_sel,
            },
        }
    }

    fn move_sel(&mut self, delta: i32) {
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let sel = self.current_sel_mut();
        *sel = (*sel as i32 + delta).clamp(0, len as i32 - 1) as usize;
    }

    fn clamp_selection(&mut self) {
        let clamp = |sel: usize, len: usize| if len == 0 { 0 } else { sel.min(len - 1) };
        self.today_sel = clamp(self.today_sel, self.today_active().len());
        self.tasks_sel = clamp(self.tasks_sel, self.tasks_filtered().len());
        self.notes_sel = clamp(self.notes_sel, self.notes_list.len());
        self.note_item_sel = clamp(self.note_item_sel, self.note_items().len());
    }

    // ---- key dispatch -----------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        self.footer_msg = None;
        match self.mode {
            Mode::Editing(_) => self.handle_editing_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_key(key)?,
            Mode::Normal => self.handle_normal_key(key)?,
        }
        Ok(())
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if self.focus == Focus::Side {
                    self.focus = Focus::Main;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),

            // pane focus
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Main => Focus::Side,
                    Focus::Side => Focus::Main,
                };
            }
            KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Side,
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Main,

            // tab switches (global; focus returns to the main pane)
            KeyCode::Char('1') | KeyCode::Char('g') => self.switch_tab(Tab::Today),
            KeyCode::Char('2') | KeyCode::Char('s') => self.switch_tab(Tab::Standup),
            KeyCode::Char('3') | KeyCode::Char('t') => self.switch_tab(Tab::Tasks),
            KeyCode::Char('4') | KeyCode::Char('n') => self.switch_tab(Tab::Notes),

            _ => match self.focus {
                Focus::Side => self.handle_side_key(key)?,
                Focus::Main => match self.tab {
                    Tab::Today | Tab::Tasks => self.handle_task_key(key)?,
                    Tab::Notes => self.handle_notes_list_key(key)?,
                    Tab::Standup => {}
                },
            },
        }
        Ok(())
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.focus = Focus::Main;
    }

    fn handle_task_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('a') => {
                self.mode = Mode::Editing(Editing::new(EditPurpose::AddTask, String::new()));
            }
            KeyCode::Char(' ') | KeyCode::Char('x') => {
                if let Some(task) = self.selected_task() {
                    self.store.complete_task(&task.id)?;
                    self.reload()?;
                }
            }
            KeyCode::Char('b') => {
                if let Some(sel) = self.selected_task() {
                    for t in self.tasks.iter_mut() {
                        if t.id == sel.id {
                            t.status = match t.status {
                                Status::Blocked => Status::Open,
                                _ => Status::Blocked,
                            };
                        }
                    }
                    self.store.save_tasks(&self.tasks)?;
                    self.reload()?;
                }
            }
            KeyCode::Char('e') => {
                if let Some(sel) = self.selected_task() {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::EditTask { id: sel.id.clone() },
                        sel.text.clone(),
                    ));
                }
            }
            KeyCode::Char('d') => {
                if let Some(sel) = self.selected_task() {
                    let prefill = sel.due.map(|d| d.to_string()).unwrap_or_default();
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::DueDate { id: sel.id.clone() },
                        prefill,
                    ));
                }
            }
            KeyCode::Char('D') => {
                if self.selected_task().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            // Tasks-tab-only filters
            KeyCode::Char('/') if self.tab == Tab::Tasks => {
                self.mode =
                    Mode::Editing(Editing::new(EditPurpose::Filter, self.filter_text.clone()));
            }
            KeyCode::Char('c') if self.tab == Tab::Tasks => self.cycle_category(),
            KeyCode::Char('p') if self.tab == Tab::Tasks => self.cycle_project(),
            _ => {}
        }
        Ok(())
    }

    fn handle_notes_list_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => {
                if self.notes_list.get(self.notes_sel).is_some() {
                    self.open_note_at(self.notes_sel)?;
                    self.focus = Focus::Side;
                }
            }
            KeyCode::Char('N') => {
                self.mode = Mode::Editing(Editing::new(EditPurpose::NewNoteTitle, String::new()));
            }
            _ => {}
        }
        Ok(())
    }

    /// Load the note at `idx` in `notes_list` into the side pane and remember
    /// it as the last-opened note.
    fn open_note_at(&mut self, idx: usize) -> Result<()> {
        if let Some(summary) = self.notes_list.get(idx) {
            let slug = summary.slug.clone();
            self.current_note = Some(self.notes.load(&slug)?);
            self.notes_sel = idx;
            self.note_item_sel = 0;
            self.persist_last_note();
        }
        Ok(())
    }

    /// `[`/`]`: advance the side pane to the previous/next note, wrapping.
    fn cycle_note(&mut self, delta: i32) -> Result<()> {
        if self.notes_list.is_empty() {
            return Ok(());
        }
        let len = self.notes_list.len() as i32;
        let next = (self.notes_sel as i32 + delta).rem_euclid(len) as usize;
        self.open_note_at(next)
    }

    fn handle_side_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.current_note.is_none() {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('[') => self.cycle_note(-1)?,
            KeyCode::Char(']') => self.cycle_note(1)?,
            KeyCode::Char('a') => {
                let heading = self.current_heading();
                self.mode = Mode::Editing(Editing::new(
                    EditPurpose::AddNoteItem { heading },
                    String::new(),
                ));
            }
            KeyCode::Char('e') => {
                if let Some(it) = self.note_items().get(self.note_item_sel) {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::EditNoteItem {
                            heading: it.heading.clone(),
                            item_index: it.item_index,
                        },
                        it.text.clone(),
                    ));
                }
            }
            KeyCode::Char('D') => {
                if !self.note_items().is_empty() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            KeyCode::Char('E') => {
                if let Some(doc) = &self.current_note {
                    self.editor_request = Some(self.notes.path_for(&doc.slug));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.perform_delete()?;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn perform_delete(&mut self) -> Result<()> {
        match self.focus {
            Focus::Side => {
                if let Some(it) = self.note_items().get(self.note_item_sel).cloned() {
                    if let Some(doc) = &mut self.current_note {
                        doc.body.delete_item(&it.heading, it.item_index)?;
                        self.notes.save(doc)?;
                    }
                    self.reload_current_note()?;
                }
            }
            Focus::Main => match self.tab {
                Tab::Today | Tab::Tasks => {
                    if let Some(sel) = self.selected_task() {
                        self.tasks.retain(|t| t.id != sel.id);
                        self.store.save_tasks(&self.tasks)?;
                        self.reload()?;
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }

    fn handle_editing_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => self.commit_editing()?,
            KeyCode::Esc => self.cancel_editing(),
            KeyCode::Char(c) => {
                if let Mode::Editing(e) = &mut self.mode {
                    e.insert(c);
                }
                self.after_editing_change();
            }
            KeyCode::Backspace => {
                if let Mode::Editing(e) = &mut self.mode {
                    e.backspace();
                }
                self.after_editing_change();
            }
            KeyCode::Left => {
                if let Mode::Editing(e) = &mut self.mode {
                    e.left();
                }
            }
            KeyCode::Right => {
                if let Mode::Editing(e) = &mut self.mode {
                    e.right();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Live-apply incremental filter text as the user types.
    fn after_editing_change(&mut self) {
        let buf = match &self.mode {
            Mode::Editing(e) if matches!(e.purpose, EditPurpose::Filter) => Some(e.buffer.clone()),
            _ => None,
        };
        if let Some(b) = buf {
            self.filter_text = b;
            self.tasks_sel = 0;
        }
    }

    fn cancel_editing(&mut self) {
        if let Mode::Editing(e) = &self.mode {
            if matches!(e.purpose, EditPurpose::Filter) {
                self.filter_text.clear();
                self.tasks_sel = 0;
            }
        }
        self.mode = Mode::Normal;
    }

    fn commit_editing(&mut self) -> Result<()> {
        let editing = match &self.mode {
            Mode::Editing(e) => e.clone(),
            _ => return Ok(()),
        };
        let text = editing.buffer.trim().to_string();
        match editing.purpose {
            EditPurpose::AddTask => {
                if !text.is_empty() {
                    let (task_text, category, project) =
                        parse_task_input(&editing.buffer, &self.config.categories);
                    let task = Task::new(task_text, category, project, None);
                    self.store.add_task(task)?;
                    self.reload()?;
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::EditTask { id } => {
                if !text.is_empty() {
                    for t in self.tasks.iter_mut() {
                        if t.id == id {
                            t.text = text.clone();
                        }
                    }
                    self.store.save_tasks(&self.tasks)?;
                    self.reload()?;
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::DueDate { id } => {
                self.commit_due(&id, &text)?;
                self.mode = Mode::Normal;
            }
            EditPurpose::Filter => {
                self.filter_text = editing.buffer.clone();
                self.tasks_sel = 0;
                self.mode = Mode::Normal;
            }
            EditPurpose::NewNoteTitle => {
                if !text.is_empty() {
                    let doc = self.notes.create(&text, None)?;
                    self.reload()?;
                    if let Some(idx) = self.notes_list.iter().position(|s| s.slug == doc.slug) {
                        self.notes_sel = idx;
                    }
                    self.current_note = Some(doc);
                    self.note_item_sel = 0;
                    self.focus = Focus::Side;
                    self.persist_last_note();
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::AddNoteItem { heading } => {
                if !text.is_empty() {
                    if let Some(doc) = &mut self.current_note {
                        doc.body.add_item(&heading, text.clone());
                        self.notes.save(doc)?;
                    }
                    self.reload_current_note()?;
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::EditNoteItem {
                heading,
                item_index,
            } => {
                if !text.is_empty() {
                    if let Some(doc) = &mut self.current_note {
                        doc.body.edit_item(&heading, item_index, text.clone())?;
                        self.notes.save(doc)?;
                    }
                    self.reload_current_note()?;
                }
                self.mode = Mode::Normal;
            }
        }
        Ok(())
    }

    fn commit_due(&mut self, id: &str, text: &str) -> Result<()> {
        let due = if text.is_empty() {
            None
        } else {
            match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                Ok(d) => Some(d),
                Err(_) => {
                    self.footer_msg = Some(format!("invalid date {text:?} (expected YYYY-MM-DD)"));
                    return Ok(());
                }
            }
        };
        for t in self.tasks.iter_mut() {
            if t.id == id {
                t.due = due;
            }
        }
        self.store.save_tasks(&self.tasks)?;
        self.reload()
    }

    fn cycle_category(&mut self) {
        let n = self.config.categories.len();
        self.cat_filter = match self.cat_filter {
            None if n > 0 => Some(0),
            Some(i) if i + 1 < n => Some(i + 1),
            _ => None,
        };
        self.tasks_sel = 0;
    }

    fn cycle_project(&mut self) {
        let n = self.distinct_projects().len();
        self.proj_filter = match self.proj_filter {
            None if n > 0 => Some(0),
            Some(i) if i + 1 < n => Some(i + 1),
            _ => None,
        };
        self.tasks_sel = 0;
    }

    // ---- cross-session UI state (state.json) -------------------------------

    /// Slug of the last-opened note recorded in `state.json`, if readable.
    fn load_last_note_slug(&self) -> Option<String> {
        let content = std::fs::read_to_string(self.store.state_path()).ok()?;
        let v: serde_json::Value = serde_json::from_str(&content).ok()?;
        Some(v.get("last_note")?.as_str()?.to_string())
    }

    /// Best-effort save of the open note's slug: UI state is not worth
    /// failing an interaction over, so write errors are ignored.
    fn persist_last_note(&self) {
        if let Some(doc) = &self.current_note {
            let v = serde_json::json!({ "last_note": doc.slug });
            let _ = std::fs::write(self.store.state_path(), format!("{v}\n"));
        }
    }

    /// Run the pending `$EDITOR` request (if any) using `editor` as the
    /// command, then reload the doc from disk. Terminal suspend/resume is the
    /// caller's responsibility; this half is TTY-free so it is unit testable.
    pub fn run_editor(&mut self, editor: &str) -> Result<()> {
        if let Some(path) = self.editor_request.take() {
            match editor::run_editor(editor, &path) {
                Ok(status) if status.success() => {}
                Ok(_) => self.footer_msg = Some("editor exited with an error".to_string()),
                Err(e) => self.footer_msg = Some(format!("failed to launch editor: {e}")),
            }
            self.reload_current_note()?;
        }
        Ok(())
    }
}

fn count_items(doc: &NoteDoc) -> usize {
    doc.body
        .sections
        .iter()
        .flat_map(|s| s.lines.iter())
        .filter(|l| matches!(l, Line::Item(_)))
        .count()
}

fn due_bucket(t: &Task, today: NaiveDate) -> u8 {
    match t.due {
        Some(d) if d < today => 0,
        Some(d) if d == today => 1,
        _ => 2,
    }
}

/// Parse inline `@category` / `#project` tokens out of add-task input.
///
/// `@token` sets the category only if it matches a configured category
/// (otherwise the token stays in the text); the first `#token` sets the
/// project. Category defaults to `intake`.
fn parse_task_input(raw: &str, categories: &[String]) -> (String, String, Option<String>) {
    let mut text_tokens: Vec<&str> = Vec::new();
    let mut category: Option<String> = None;
    let mut project: Option<String> = None;

    for tok in raw.split_whitespace() {
        if let Some(cat) = tok.strip_prefix('@') {
            if category.is_none() && categories.iter().any(|c| c == cat) {
                category = Some(cat.to_string());
                continue;
            }
        } else if let Some(proj) = tok.strip_prefix('#') {
            if project.is_none() && !proj.is_empty() {
                project = Some(proj.to_string());
                continue;
            }
        }
        text_tokens.push(tok);
    }

    (
        text_tokens.join(" "),
        category.unwrap_or_else(|| "intake".to_string()),
        project,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_valid_category_and_project() {
        let cats = vec!["engineering".to_string(), "intake".to_string()];
        let (text, cat, proj) = parse_task_input("fix login @engineering #auth", &cats);
        assert_eq!(text, "fix login");
        assert_eq!(cat, "engineering");
        assert_eq!(proj, Some("auth".to_string()));
    }

    #[test]
    fn parse_keeps_invalid_category_token_in_text() {
        let cats = vec!["engineering".to_string()];
        let (text, cat, proj) = parse_task_input("look at @nonsense thing", &cats);
        assert_eq!(text, "look at @nonsense thing");
        assert_eq!(cat, "intake");
        assert_eq!(proj, None);
    }

    #[test]
    fn parse_defaults_category_and_no_project() {
        let cats = vec!["engineering".to_string()];
        let (text, cat, proj) = parse_task_input("plain task", &cats);
        assert_eq!(text, "plain task");
        assert_eq!(cat, "intake");
        assert_eq!(proj, None);
    }
}
