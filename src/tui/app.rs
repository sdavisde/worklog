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
use crate::notes::{Body, Line, NoteDoc, NotesStore};
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

/// Which task population the Tasks tab shows: open work, the archive, or
/// both. Cycled with `v`; the text/category/project filters apply on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskView {
    #[default]
    Open,
    Done,
    All,
}

impl TaskView {
    fn next(self) -> Self {
        match self {
            TaskView::Open => TaskView::Done,
            TaskView::Done => TaskView::All,
            TaskView::All => TaskView::Open,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TaskView::Open => "Open",
            TaskView::Done => "Done",
            TaskView::All => "All",
        }
    }
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
    EditTask {
        id: String,
    },
    DueDate {
        id: String,
    },
    Filter,
    NewNoteTitle,
    /// `r`: retitle the note. Only the frontmatter title changes — the slug
    /// (and so the filename, `state.json` references, and note order) stays
    /// stable.
    RenameNote {
        slug: String,
    },
    AddNoteItem {
        heading: String,
    },
    EditNoteItem {
        heading: String,
        item_index: usize,
    },
    /// `o`: insert after the given item, or first in the section when `None`.
    InsertNoteItem {
        heading: String,
        after_item: Option<usize>,
    },
    /// `A`: new section after the given section index, or appended when `None`.
    NewNoteSection {
        after_section: Option<usize>,
    },
}

/// A closed-list picker for the selected task's category: `j`/`k` (or
/// arrows) move the highlight, `enter` applies it, `esc` cancels.
#[derive(Debug, Clone)]
pub struct CategoryPicker {
    pub id: String,
    pub options: Vec<String>,
    pub selected: usize,
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

    /// Accept an inline completion: replace the typed token text with the
    /// candidate's canonical spelling (case-insensitive matches must land on
    /// the exact category/project name, since `parse_task_input` compares
    /// exactly), then leave the cursor past a trailing space so the next word
    /// can start immediately.
    fn accept_suggestion(&mut self, suggestion: &TokenSuggestion) {
        let start = self.byte_at(suggestion.text_start);
        let end = self.byte_at(self.cursor);
        self.buffer.replace_range(start..end, &suggestion.candidate);
        self.cursor = suggestion.text_start + suggestion.candidate.chars().count();
        if self.cursor == self.buffer.chars().count() {
            self.buffer.push(' ');
            self.cursor += 1;
        } else if self.buffer.chars().nth(self.cursor) == Some(' ') {
            self.cursor += 1;
        }
    }
}

/// Input mode: normal navigation, an active input box, a closed-list picker
/// (task category, or the `'` note switcher), a y/n confirm, or the `?`
/// keybinds overlay.
#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Editing(Editing),
    CategoryPicker(CategoryPicker),
    /// `'`: jump the side pane to any note; the highlighted index points
    /// into `notes_list`.
    NotePicker {
        selected: usize,
    },
    ConfirmDelete,
    Help,
}

/// A selectable row in the side pane's note detail: a section heading or an
/// item resolved to the heading + per-section index the `notes` module needs.
#[derive(Debug, Clone, PartialEq)]
pub enum NoteRow {
    Heading {
        section_index: usize,
        heading: String,
    },
    Item {
        section_index: usize,
        heading: String,
        item_index: usize,
        text: String,
    },
}

impl NoteRow {
    /// Heading of the section this row belongs to (both variants carry it).
    pub fn heading(&self) -> &str {
        match self {
            NoteRow::Heading { heading, .. } | NoteRow::Item { heading, .. } => heading,
        }
    }

    /// Index into `body.sections` of the section this row belongs to.
    pub fn section_index(&self) -> usize {
        match self {
            NoteRow::Heading { section_index, .. } | NoteRow::Item { section_index, .. } => {
                *section_index
            }
        }
    }
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
    pub task_view: TaskView,

    pub tasks: Vec<Task>,
    pub archive: Vec<Task>,
    pub standup: StandupReport,
    /// All note docs, in `NotesStore::list` order: the Notes tab's list, and
    /// what side-pane cycling (`[`/`]`) and the new-note flow index into.
    pub notes_list: Vec<NoteSummary>,
    pub current_note: Option<NoteDoc>,

    pub today_sel: usize,
    pub tasks_sel: usize,
    pub notes_sel: usize,
    pub note_row_sel: usize,

    pub filter_text: String,
    pub cat_filter: Option<usize>,
    pub proj_filter: Option<usize>,

    pub footer_msg: Option<String>,
    /// Pending `$EDITOR` escape hatch: the file to open and, when the
    /// selected row maps to a file line, the 1-based line to jump to.
    pub editor_request: Option<(PathBuf, Option<usize>)>,
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
            task_view: TaskView::default(),
            tasks: Vec::new(),
            archive: Vec::new(),
            standup,
            notes_list: Vec::new(),
            current_note: None,
            today_sel: 0,
            tasks_sel: 0,
            notes_sel: 0,
            note_row_sel: 0,
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
        // Apply the user's persisted note order (`J`/`K` on the Notes tab):
        // known slugs first in that order; notes not in it (new or created
        // externally) keep their slug-sorted order at the end; persisted
        // slugs whose files are gone simply never match, and drop out of
        // `state.json` on the next reorder.
        let order = self.load_note_order();
        self.notes_list.sort_by_key(|s| {
            order
                .iter()
                .position(|o| o == &s.slug)
                .unwrap_or(usize::MAX)
        });
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

    /// Project-name candidates for inline `#project` completion: every
    /// project seen across active tasks and the archive (a project whose
    /// tasks are all done is still likely to gain new ones), alphabetical.
    fn project_candidates(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .tasks
            .iter()
            .chain(self.archive.iter())
            .filter_map(|t| t.project.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Inline ghost-text completion for the `@category`/`#project` token at
    /// the add-task input's cursor, if one applies. `None` in every other
    /// mode/purpose: editing an existing task's text does not re-parse
    /// tokens (see [`EditPurpose::EditTask`] in `commit_editing`), so
    /// offering completions there would suggest behavior that doesn't exist.
    pub fn editing_suggestion(&self) -> Option<TokenSuggestion> {
        match &self.mode {
            Mode::Editing(e) if e.purpose == EditPurpose::AddTask => suggest_token_completion(
                &e.buffer,
                e.cursor,
                &self.config.categories,
                &self.project_candidates(),
            ),
            _ => None,
        }
    }

    /// Tasks for the Tasks tab: the `v` status view picks the population
    /// (open work, the archive, or both — archived most-recent-first),
    /// narrowed by the current text/category/project filters.
    pub fn tasks_filtered(&self) -> Vec<Task> {
        let cat = self
            .cat_filter
            .and_then(|i| self.config.categories.get(i).cloned());
        let projects = self.distinct_projects();
        let proj = self.proj_filter.and_then(|i| projects.get(i).cloned());
        let needle = self.filter_text.to_lowercase();

        let matches = |t: &Task| {
            (needle.is_empty() || t.text.to_lowercase().contains(&needle))
                && cat.as_ref().is_none_or(|c| &t.category == c)
                && proj.as_ref().is_none_or(|p| t.project.as_ref() == Some(p))
        };
        let done = || {
            let mut v: Vec<Task> = self
                .archive
                .iter()
                .filter(|t| matches(t))
                .cloned()
                .collect();
            v.sort_by_key(|b| std::cmp::Reverse(b.completed_at));
            v
        };

        let mut out: Vec<Task> = match self.task_view {
            TaskView::Done => return done(),
            TaskView::Open | TaskView::All => {
                self.tasks.iter().filter(|t| matches(t)).cloned().collect()
            }
        };
        if self.task_view == TaskView::All {
            out.extend(done());
        }
        out
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

    /// Flattened selectable rows of the currently open doc: each non-empty
    /// heading, then that section's items. The walk order matches
    /// `render_detail`'s exactly, so selection indices and rendered rows stay
    /// in lockstep by construction. Free-form `Text` lines stay invisible.
    pub fn note_rows(&self) -> Vec<NoteRow> {
        let mut out = Vec::new();
        if let Some(doc) = &self.current_note {
            for (section_index, section) in doc.body.sections.iter().enumerate() {
                if !section.heading.is_empty() {
                    out.push(NoteRow::Heading {
                        section_index,
                        heading: section.heading.clone(),
                    });
                }
                let mut idx = 0;
                for line in &section.lines {
                    if let Line::Item(text) = line {
                        out.push(NoteRow::Item {
                            section_index,
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

    /// The currently selected side-pane row, if any.
    fn selected_note_row(&self) -> Option<NoteRow> {
        self.note_rows().into_iter().nth(self.note_row_sel)
    }

    /// Position the side-pane selection on the given item, e.g. right after
    /// an insert so the new item is selected.
    fn select_item(&mut self, heading: &str, item_index: usize) {
        if let Some(row) = self.note_rows().iter().position(|r| {
            matches!(r, NoteRow::Item { heading: h, item_index: i, .. }
                if h == heading && *i == item_index)
        }) {
            self.note_row_sel = row;
        }
    }

    /// 1-based file line of the selected side-pane row for the `$EDITOR`
    /// jump. The saved doc is `---`, the frontmatter YAML, `---`, a blank
    /// line, then the body, so the body's line 0 sits at file line
    /// `n_yaml + 4`.
    fn selected_row_editor_line(&self) -> Option<usize> {
        let doc = self.current_note.as_ref()?;
        let row = self.selected_note_row()?;
        let body_line = body_line_of_row(&doc.body, &row)?;
        let n_yaml = serde_norway::to_string(&doc.frontmatter)
            .ok()?
            .lines()
            .count();
        Some(n_yaml + body_line + 4)
    }

    fn selected_task(&self) -> Option<Task> {
        match self.tab {
            Tab::Today => self.today_active().into_iter().nth(self.today_sel),
            Tab::Tasks => self.tasks_filtered().into_iter().nth(self.tasks_sel),
            _ => None,
        }
    }

    /// Selected task, but only if it is still mutable: archived (done) tasks
    /// are the permanent record, so mutation keys get a footer notice instead.
    fn selected_active_task(&mut self) -> Option<Task> {
        let sel = self.selected_task()?;
        if sel.status == Status::Done {
            self.footer_msg = Some("archived tasks are read-only".to_string());
            None
        } else {
            Some(sel)
        }
    }

    /// Heading the "add item" action targets: the selected row's section
    /// (heading or item), else the first section, else a default `Notes`
    /// heading.
    fn current_heading(&self) -> String {
        if let Some(row) = self.selected_note_row() {
            return row.heading().to_string();
        }
        if let Some(doc) = &self.current_note
            && let Some(s) = doc.body.sections.iter().find(|s| !s.heading.is_empty())
        {
            return s.heading.clone();
        }
        "Notes".to_string()
    }

    // ---- selection helpers ------------------------------------------------

    fn current_len(&self) -> usize {
        match self.focus {
            Focus::Side => self.note_rows().len(),
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
            Focus::Side => &mut self.note_row_sel,
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
        self.note_row_sel = clamp(self.note_row_sel, self.note_rows().len());
    }

    // ---- key dispatch -----------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        self.footer_msg = None;
        match self.mode {
            Mode::Editing(_) => self.handle_editing_key(key)?,
            Mode::CategoryPicker(_) => self.handle_category_picker_key(key)?,
            Mode::NotePicker { .. } => self.handle_note_picker_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_key(key)?,
            // any key dismisses the keybinds overlay
            Mode::Help => self.mode = Mode::Normal,
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
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_sel(1);
                self.preview_selected_note()?;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_sel(-1);
                self.preview_selected_note()?;
            }

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
            KeyCode::Char('4') | KeyCode::Char('n') => {
                self.switch_tab(Tab::Notes);
                self.preview_selected_note()?;
            }

            // global: new note (opens in the side pane), note switcher
            // (`'` — jump to a note, vim-mark style; `f` stays free for a
            // future find/filter), and keybinds overlay
            KeyCode::Char('N') => {
                self.mode = Mode::Editing(Editing::new(EditPurpose::NewNoteTitle, String::new()));
            }
            KeyCode::Char('\'') => {
                if self.notes_list.is_empty() {
                    self.footer_msg = Some("no notes yet — press N to create one".to_string());
                } else {
                    // pre-highlight the note currently in the side pane
                    self.mode = Mode::NotePicker {
                        selected: self.notes_sel.min(self.notes_list.len() - 1),
                    };
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,

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
                if let Some(task) = self.selected_active_task() {
                    self.store.complete_task(&task.id)?;
                    self.reload()?;
                }
            }
            KeyCode::Char('b') => {
                if let Some(sel) = self.selected_active_task() {
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
                if let Some(sel) = self.selected_active_task() {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::EditTask { id: sel.id.clone() },
                        sel.text.clone(),
                    ));
                }
            }
            KeyCode::Char('C') => {
                if let Some(sel) = self.selected_active_task() {
                    let options = self.config.categories.clone();
                    let selected = options.iter().position(|c| c == &sel.category).unwrap_or(0);
                    self.mode = Mode::CategoryPicker(CategoryPicker {
                        id: sel.id.clone(),
                        options,
                        selected,
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(sel) = self.selected_active_task() {
                    let prefill = sel.due.map(|d| d.to_string()).unwrap_or_default();
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::DueDate { id: sel.id.clone() },
                        prefill,
                    ));
                }
            }
            KeyCode::Char('D') => {
                if self.selected_active_task().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            // Tasks-tab-only status view + filters
            KeyCode::Char('v') if self.tab == Tab::Tasks => {
                self.task_view = self.task_view.next();
                self.tasks_sel = 0;
            }
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
                    // deliberate open: focus the side pane and remember the note
                    self.open_note_at(self.notes_sel)?;
                    self.focus = Focus::Side;
                }
            }
            KeyCode::Char('r') => {
                if let Some(summary) = self.notes_list.get(self.notes_sel) {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::RenameNote {
                            slug: summary.slug.clone(),
                        },
                        summary.title.clone(),
                    ));
                }
            }
            // reorder: the selection follows the moved note
            KeyCode::Char('J') => self.move_note(1),
            KeyCode::Char('K') => self.move_note(-1),
            _ => {}
        }
        Ok(())
    }

    /// `J`/`K`: swap the selected note with its neighbor and persist the new
    /// order. `notes_list` is the single source of truth for note order, so
    /// the `'` picker and `[`/`]` cycling follow automatically.
    fn move_note(&mut self, delta: i32) {
        let len = self.notes_list.len();
        if len < 2 {
            return;
        }
        let from = self.notes_sel;
        let to = (from as i32 + delta).clamp(0, len as i32 - 1) as usize;
        if from == to {
            return;
        }
        self.notes_list.swap(from, to);
        self.notes_sel = to;
        self.persist_note_order();
    }

    /// Live preview for the Notes tab: mirror the list selection into the
    /// side pane as it moves, without persisting it as the last-opened note
    /// (that happens on a deliberate `enter` open). No-op elsewhere.
    fn preview_selected_note(&mut self) -> Result<()> {
        if self.focus != Focus::Main || self.tab != Tab::Notes {
            return Ok(());
        }
        if let Some(summary) = self.notes_list.get(self.notes_sel)
            && self.current_note.as_ref().map(|d| d.slug.as_str()) != Some(summary.slug.as_str())
        {
            let slug = summary.slug.clone();
            self.current_note = Some(self.notes.load(&slug)?);
            self.note_row_sel = 0;
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
            self.note_row_sel = 0;
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
            KeyCode::Char('o') => {
                if let Some(row) = self.selected_note_row() {
                    let (heading, after_item) = match row {
                        NoteRow::Item {
                            heading,
                            item_index,
                            ..
                        } => (heading, Some(item_index)),
                        NoteRow::Heading { heading, .. } => (heading, None),
                    };
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::InsertNoteItem {
                            heading,
                            after_item,
                        },
                        String::new(),
                    ));
                }
            }
            KeyCode::Char('A') => {
                let after_section = self.selected_note_row().map(|r| r.section_index());
                self.mode = Mode::Editing(Editing::new(
                    EditPurpose::NewNoteSection { after_section },
                    String::new(),
                ));
            }
            KeyCode::Char('r') => {
                if let Some(doc) = &self.current_note {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::RenameNote {
                            slug: doc.slug.clone(),
                        },
                        doc.frontmatter.title.clone(),
                    ));
                }
            }
            KeyCode::Char('e') => match self.selected_note_row() {
                Some(NoteRow::Item {
                    heading,
                    item_index,
                    text,
                    ..
                }) => {
                    self.mode = Mode::Editing(Editing::new(
                        EditPurpose::EditNoteItem {
                            heading,
                            item_index,
                        },
                        text,
                    ));
                }
                Some(NoteRow::Heading { .. }) => {
                    self.footer_msg = Some("select an item to edit".to_string());
                }
                None => {}
            },
            KeyCode::Char('D') => match self.selected_note_row() {
                Some(NoteRow::Item { .. }) => self.mode = Mode::ConfirmDelete,
                Some(NoteRow::Heading { .. }) => {
                    self.footer_msg = Some("select an item to delete".to_string());
                }
                None => {}
            },
            KeyCode::Char('E') => {
                if let Some(doc) = &self.current_note {
                    let line = self.selected_row_editor_line();
                    self.editor_request = Some((self.notes.path_for(&doc.slug), line));
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
                if let Some(NoteRow::Item {
                    heading,
                    item_index,
                    ..
                }) = self.selected_note_row()
                {
                    if let Some(doc) = &mut self.current_note {
                        doc.body.delete_item(&heading, item_index)?;
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
            // Accept the inline @category/#project completion. Tab is
            // otherwise unbound while editing (the Normal-mode pane toggle
            // does not apply here), so no-suggestion Tab stays a no-op.
            KeyCode::Tab => {
                if let Some(suggestion) = self.editing_suggestion()
                    && let Mode::Editing(e) = &mut self.mode
                {
                    e.accept_suggestion(&suggestion);
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
        if let Mode::Editing(e) = &self.mode
            && matches!(e.purpose, EditPurpose::Filter)
        {
            self.filter_text.clear();
            self.tasks_sel = 0;
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
                    self.note_row_sel = 0;
                    self.focus = Focus::Side;
                    self.persist_last_note();
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::RenameNote { slug } => {
                if !text.is_empty() {
                    let mut doc = self.notes.load(&slug)?;
                    doc.frontmatter.title = text.clone();
                    self.notes.save(&mut doc)?;
                    if self.current_note.as_ref().map(|d| d.slug.as_str()) == Some(slug.as_str()) {
                        self.current_note = Some(doc);
                    }
                    self.reload()?;
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
                    let appended = self
                        .current_note
                        .as_ref()
                        .map(|doc| doc.body.items(&heading).len().saturating_sub(1))
                        .unwrap_or(0);
                    self.select_item(&heading, appended);
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
            EditPurpose::InsertNoteItem {
                heading,
                after_item,
            } => {
                if !text.is_empty() {
                    if let Some(doc) = &mut self.current_note {
                        match after_item {
                            Some(i) => doc.body.insert_item_after(&heading, i, text.clone())?,
                            None => doc.body.insert_item_first(&heading, text.clone())?,
                        }
                        self.notes.save(doc)?;
                    }
                    self.reload_current_note()?;
                    self.select_item(&heading, after_item.map(|i| i + 1).unwrap_or(0));
                }
                self.mode = Mode::Normal;
            }
            EditPurpose::NewNoteSection { after_section } => {
                if !text.is_empty() {
                    if let Some(doc) = &mut self.current_note {
                        doc.body.insert_section_after(after_section, text.clone());
                        self.notes.save(doc)?;
                    }
                    self.reload_current_note()?;
                    let new_section_index = after_section.map(|i| i + 1).unwrap_or_else(|| {
                        self.current_note
                            .as_ref()
                            .map(|d| d.body.sections.len().saturating_sub(1))
                            .unwrap_or(0)
                    });
                    if let Some(row) = self.note_rows().iter().position(|r| {
                        matches!(r, NoteRow::Heading { section_index, .. }
                            if *section_index == new_section_index)
                    }) {
                        self.note_row_sel = row;
                    }
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

    fn handle_category_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        let Mode::CategoryPicker(picker) = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if picker.selected + 1 < picker.options.len() {
                    picker.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                picker.selected = picker.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let id = picker.id.clone();
                let category = picker.options.get(picker.selected).cloned();
                self.mode = Mode::Normal;
                if let Some(category) = category {
                    for t in self.tasks.iter_mut() {
                        if t.id == id {
                            t.category = category.clone();
                        }
                    }
                    self.store.save_tasks(&self.tasks)?;
                    self.reload()?;
                }
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
        Ok(())
    }

    /// The `'` note switcher: same interaction as the category picker, but
    /// enter is a deliberate open (side pane + last-note persistence). Focus
    /// is left where it was, matching the category picker's behavior.
    fn handle_note_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        let Mode::NotePicker { selected } = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if *selected + 1 < self.notes_list.len() {
                    *selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                *selected = selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let idx = *selected;
                self.mode = Mode::Normal;
                self.open_note_at(idx)?;
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
        Ok(())
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
    /// `state.json` as a JSON object; empty when missing or unreadable.
    fn read_state(&self) -> serde_json::Map<String, serde_json::Value> {
        std::fs::read_to_string(self.store.state_path())
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    /// Best-effort merge of one key into `state.json`: UI state is not worth
    /// failing an interaction over, so write errors are ignored.
    fn write_state_key(&self, key: &str, value: serde_json::Value) {
        let mut state = self.read_state();
        state.insert(key.to_string(), value);
        let v = serde_json::Value::Object(state);
        let _ = std::fs::write(self.store.state_path(), format!("{v}\n"));
    }

    fn load_last_note_slug(&self) -> Option<String> {
        Some(self.read_state().get("last_note")?.as_str()?.to_string())
    }

    fn persist_last_note(&self) {
        if let Some(doc) = &self.current_note {
            self.write_state_key("last_note", serde_json::json!(doc.slug));
        }
    }

    /// Slugs in the user's chosen Notes order, as persisted in `state.json`.
    fn load_note_order(&self) -> Vec<String> {
        self.read_state()
            .get("note_order")
            .and_then(|v| v.as_array().cloned())
            .map(|a| {
                a.into_iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn persist_note_order(&self) {
        let slugs: Vec<&str> = self.notes_list.iter().map(|s| s.slug.as_str()).collect();
        self.write_state_key("note_order", serde_json::json!(slugs));
    }

    /// Run the pending `$EDITOR` request (if any) using `editor` as the
    /// command, then reload the doc from disk. Terminal suspend/resume is the
    /// caller's responsibility; this half is TTY-free so it is unit testable.
    pub fn run_editor(&mut self, editor: &str) -> Result<()> {
        if let Some((path, line)) = self.editor_request.take() {
            match editor::run_editor(editor, &path, line) {
                Ok(status) if status.success() => {}
                Ok(_) => self.footer_msg = Some("editor exited with an error".to_string()),
                Err(e) => self.footer_msg = Some(format!("failed to launch editor: {e}")),
            }
            self.reload_current_note()?;
        }
        Ok(())
    }
}

/// 0-based line index of `target` within `serialize_body`'s output, replaying
/// its exact layout: a blank separator before every section after the first,
/// one line per non-empty heading, one line per `Line` (items and free-form
/// text alike). Valid because `parse_body` folds the separator back out, so
/// the in-memory body maps 1:1 onto the file.
fn body_line_of_row(body: &Body, target: &NoteRow) -> Option<usize> {
    let mut line = 0;
    for (section_index, section) in body.sections.iter().enumerate() {
        if section_index > 0 {
            line += 1;
        }
        if !section.heading.is_empty() {
            if matches!(target, NoteRow::Heading { section_index: s, .. } if *s == section_index) {
                return Some(line);
            }
            line += 1;
        }
        let mut item_idx = 0;
        for l in &section.lines {
            if matches!(l, Line::Item(_)) {
                if matches!(target, NoteRow::Item { section_index: s, item_index: i, .. }
                    if *s == section_index && *i == item_idx)
                {
                    return Some(line);
                }
                item_idx += 1;
            }
            line += 1;
        }
    }
    None
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
        } else if let Some(proj) = tok.strip_prefix('#')
            && project.is_none()
            && !proj.is_empty()
        {
            project = Some(proj.to_string());
            continue;
        }
        text_tokens.push(tok);
    }

    (
        text_tokens.join(" "),
        category.unwrap_or_else(|| "intake".to_string()),
        project,
    )
}

/// A pending inline completion for the `@category` / `#project` token being
/// typed: the ghost `remainder` is rendered dimmed after the cursor, and
/// `<tab>` replaces the typed token text with `candidate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSuggestion {
    /// Char index just after the `@`/`#` sigil, where the typed text starts.
    pub text_start: usize,
    /// The full name the token completes to (canonical casing).
    pub candidate: String,
    /// The chars of `candidate` beyond what is already typed (never empty).
    pub remainder: String,
}

/// Compute the inline completion for the token ending at `cursor`, if any.
///
/// A suggestion applies only when the cursor sits at the end of a
/// whitespace-delimited token that *starts* with `@` (matched against
/// `categories`, in their configured order) or `#` (matched against
/// `projects`, as given). Matching is case-insensitive prefix matching; the
/// first candidate wins. A bare sigil suggests the first candidate, and a
/// fully typed name yields no suggestion (nothing left to complete).
pub fn suggest_token_completion(
    buffer: &str,
    cursor: usize,
    categories: &[String],
    projects: &[String],
) -> Option<TokenSuggestion> {
    let chars: Vec<char> = buffer.chars().collect();
    if cursor > chars.len() {
        return None;
    }
    // Only complete at the end of the token being typed, not mid-token.
    if chars.get(cursor).is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let mut start = cursor;
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    if start == cursor {
        return None; // cursor is not at the end of any token
    }
    let candidates = match chars[start] {
        '@' => categories,
        '#' => projects,
        _ => return None, // sigil must open the token (emails etc. don't count)
    };
    let typed: String = chars[start + 1..cursor].iter().collect();
    let typed_lower = typed.to_lowercase();
    let typed_len = typed.chars().count();
    let candidate = candidates
        .iter()
        .find(|c| c.to_lowercase().starts_with(&typed_lower) && c.chars().count() > typed_len)?;
    Some(TokenSuggestion {
        text_start: start + 1,
        candidate: candidate.clone(),
        remainder: candidate.chars().skip(typed_len).collect(),
    })
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

    // ---- inline @category/#project completion -----------------------------

    fn cats() -> Vec<String> {
        vec![
            "priority".to_string(),
            "engineering".to_string(),
            "intake".to_string(),
        ]
    }

    fn projs() -> Vec<String> {
        vec!["auth".to_string(), "billing".to_string()]
    }

    #[test]
    fn suggest_completes_category_prefix_at_end_of_buffer() {
        let s = suggest_token_completion("fix login @eng", 14, &cats(), &projs()).unwrap();
        assert_eq!(s.text_start, 11);
        assert_eq!(s.candidate, "engineering");
        assert_eq!(s.remainder, "ineering");
    }

    #[test]
    fn suggest_completes_project_token_mid_buffer() {
        // cursor right after "#au", with more text following
        let s = suggest_token_completion("fix #au now", 7, &cats(), &projs()).unwrap();
        assert_eq!(s.text_start, 5);
        assert_eq!(s.candidate, "auth");
        assert_eq!(s.remainder, "th");
    }

    #[test]
    fn suggest_is_case_insensitive_and_returns_canonical_candidate() {
        let s = suggest_token_completion("@ENG", 4, &cats(), &projs()).unwrap();
        assert_eq!(s.candidate, "engineering");
        assert_eq!(s.remainder, "ineering");
    }

    #[test]
    fn suggest_first_matching_candidate_wins_in_given_order() {
        let s = suggest_token_completion("@p", 2, &cats(), &projs()).unwrap();
        assert_eq!(s.candidate, "priority", "config order decides ties");
    }

    #[test]
    fn bare_sigil_suggests_first_candidate() {
        let s = suggest_token_completion("do it @", 7, &cats(), &projs()).unwrap();
        assert_eq!(s.candidate, "priority");
        assert_eq!(s.remainder, "priority");
    }

    #[test]
    fn no_suggestion_when_cursor_inside_token() {
        // cursor between "@e" and "ng"
        assert_eq!(
            suggest_token_completion("fix @eng", 6, &cats(), &projs()),
            None
        );
    }

    #[test]
    fn no_suggestion_when_cursor_not_at_a_token() {
        assert_eq!(suggest_token_completion("", 0, &cats(), &projs()), None);
        // cursor on the whitespace after a completed word + space
        assert_eq!(
            suggest_token_completion("fix @eng ", 9, &cats(), &projs()),
            None
        );
    }

    #[test]
    fn no_suggestion_for_mid_word_sigil_or_no_match() {
        // token starts with 'f', not a sigil: emails don't trigger completion
        assert_eq!(
            suggest_token_completion("mail foo@bar", 12, &cats(), &projs()),
            None
        );
        assert_eq!(suggest_token_completion("@zzz", 4, &cats(), &projs()), None);
        assert_eq!(suggest_token_completion("#x", 2, &cats(), &[]), None);
    }

    #[test]
    fn no_suggestion_when_token_already_complete() {
        assert_eq!(
            suggest_token_completion("@engineering", 12, &cats(), &projs()),
            None
        );
    }

    #[test]
    fn body_line_of_row_matches_serialized_layout() {
        let mut body = Body::default();
        body.add_item("First", "one");
        body.add_item("First", "two");
        body.add_item("Second", "three");

        let heading = |section_index: usize, heading: &str| NoteRow::Heading {
            section_index,
            heading: heading.to_string(),
        };
        let item = |section_index: usize, heading: &str, item_index: usize| NoteRow::Item {
            section_index,
            heading: heading.to_string(),
            item_index,
            text: String::new(),
        };

        // 0: "## First", 1: "- one", 2: "- two", 3: "" (separator),
        // 4: "## Second", 5: "- three"
        assert_eq!(body_line_of_row(&body, &heading(0, "First")), Some(0));
        assert_eq!(body_line_of_row(&body, &item(0, "First", 0)), Some(1));
        assert_eq!(body_line_of_row(&body, &item(0, "First", 1)), Some(2));
        assert_eq!(body_line_of_row(&body, &heading(1, "Second")), Some(4));
        assert_eq!(body_line_of_row(&body, &item(1, "Second", 0)), Some(5));
        assert_eq!(body_line_of_row(&body, &item(1, "Second", 9)), None);

        // Cross-check the layout replay against the real serializer.
        let lines: Vec<String> = crate::notes::serialize_body(&body)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(lines[4], "## Second");
        assert_eq!(lines[5], "- three");
    }
}
