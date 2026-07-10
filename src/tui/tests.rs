//! TUI tests: `TestBackend` rendering assertions per view over seeded fixture
//! data, plus state-transition tests that feed synthetic key events through
//! the same `handle_key` path the event loop uses. All I/O is confined to
//! `tempfile` temp dirs; nothing touches a real `~/.worklog`.

use super::app::{
    App, ConfirmAction, ConfirmState, EditPurpose, EditorRequest, Focus, Mode, Tab, TaskView,
};
use super::editor;
use super::textedit::VimMode;
use super::views;
use crate::config::Config;
use crate::model::{Status, Task};
use crate::notes::NotesStore;
use crate::store::Store;
use crate::theme::Theme;
use chrono::{Duration, Local, NaiveDate};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;
use tempfile::TempDir;

// ---- fixtures & helpers ---------------------------------------------------

fn app_in(dir: &Path) -> App {
    let store = Store::new(dir);
    let notes = NotesStore::new(dir.join("notes"));
    App::new(store, notes, Config::default(), Theme::default()).unwrap()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn task(text: &str, category: &str, status: Status, due: Option<NaiveDate>) -> Task {
    let mut t = Task::new(text, category, None, due);
    t.status = status;
    t
}

fn archived_on(text: &str, days_ago: i64) -> Task {
    let mut t = Task::new(text, "engineering", None, None);
    t.status = Status::Done;
    t.completed_at = Some((Local::now() - Duration::days(days_ago)).fixed_offset());
    t
}

fn render(app: &App) -> String {
    render_sized(app, 120, 40)
}

fn render_sized(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| views::draw(app, frame)).unwrap();
    terminal.backend().to_string()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(key(code)).unwrap();
}

fn press_ctrl(app: &mut App, c: char) {
    app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
        .unwrap();
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle_key(key(KeyCode::Char(c))).unwrap();
    }
}

// ---- rendering tests ------------------------------------------------------

#[test]
fn today_orders_overdue_first_and_dims_completions() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task(
                "future item",
                "engineering",
                Status::Open,
                Some(today() + Duration::days(5)),
            ),
            task(
                "overdue item",
                "engineering",
                Status::Open,
                Some(today() - Duration::days(2)),
            ),
            task("due today item", "engineering", Status::Open, Some(today())),
        ])
        .unwrap();
    store
        .append_archive(&archived_on("finished today", 0))
        .unwrap();

    let app = app_in(dir.path());
    let out = render(&app);

    let overdue = out.find("overdue item").expect("overdue rendered");
    let due_today = out.find("due today item").expect("due-today rendered");
    let future = out.find("future item").expect("future rendered");
    assert!(overdue < due_today, "overdue should sort before due-today");
    assert!(due_today < future, "due-today should sort before future");

    assert!(
        out.contains("Completed today"),
        "completions header present"
    );
    assert!(out.contains("finished today"), "completion row present");
}

#[test]
fn standup_view_shows_three_groups() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task("open work", "engineering", Status::Open, None),
            task("blocked work", "support", Status::Blocked, None),
        ])
        .unwrap();
    store
        .append_archive(&archived_on("shipped yesterday", 1))
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Standup;
    let out = render(&app);

    assert!(out.contains("Completed yesterday"));
    assert!(out.contains("shipped yesterday"));
    assert!(out.contains("Open"));
    assert!(out.contains("open work"));
    assert!(out.contains("Blocked"));
    assert!(out.contains("blocked work"));
}

#[test]
fn tasks_view_incremental_filter_narrows_list() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task("fix login bug", "engineering", Status::Open, None),
            task("write docs", "intake", Status::Open, None),
        ])
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    let before = render(&app);
    assert!(before.contains("fix login bug"));
    assert!(before.contains("write docs"));

    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "login");
    let after = render(&app);
    assert!(after.contains("fix login bug"), "match kept");
    assert!(!after.contains("write docs"), "non-match filtered out");

    // esc clears the filter
    press(&mut app, KeyCode::Esc);
    let cleared = render(&app);
    assert!(cleared.contains("write docs"), "filter cleared");
}

#[test]
fn tasks_view_footer_shows_hints() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;
    let out = render(&app);
    assert!(out.contains("filter"), "footer hint present");
    assert!(out.contains("cat["), "category filter indicator present");
    assert!(out.contains("view[Open]"), "status view indicator present");
}

#[test]
fn v_cycles_task_views_and_done_shows_archive() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("still open", "engineering", Status::Open, None)])
        .unwrap();
    store
        .append_archive(&archived_on("shipped last week", 7))
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    let open = render(&app);
    assert!(open.contains("Tasks — Open"), "open view in title");
    assert!(open.contains("still open"));
    assert!(!open.contains("shipped last week"), "archive hidden");

    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.task_view, TaskView::Done);
    let done = render(&app);
    assert!(done.contains("Tasks — Done"), "done view in title");
    assert!(done.contains("shipped last week"), "archived task shown");
    assert!(done.contains("done 20"), "completion date shown");
    assert!(!done.contains("still open"), "open task hidden");

    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.task_view, TaskView::All);
    let all = render(&app);
    assert!(all.contains("still open"));
    assert!(all.contains("shipped last week"));

    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.task_view, TaskView::Open, "cycle wraps back to open");
}

#[test]
fn done_view_sorts_recent_first_and_composes_with_text_filter() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store.append_archive(&archived_on("older fix", 9)).unwrap();
    store.append_archive(&archived_on("newer fix", 2)).unwrap();
    store.append_archive(&archived_on("other work", 1)).unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;
    press(&mut app, KeyCode::Char('v')); // Done

    let out = render(&app);
    let newer = out.find("newer fix").expect("newer rendered");
    let older = out.find("older fix").expect("older rendered");
    assert!(newer < older, "most recent completion listed first");

    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "fix");
    press(&mut app, KeyCode::Enter);
    let filtered = render(&app);
    assert!(filtered.contains("newer fix"));
    assert!(filtered.contains("older fix"));
    assert!(!filtered.contains("other work"), "text filter applies");
}

#[test]
fn archived_tasks_are_read_only() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .append_archive(&archived_on("already done", 3))
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;
    press(&mut app, KeyCode::Char('v')); // Done view, archived task selected

    for mutation in ['b', 'x', 'e', 'd', 'C', 'D'] {
        press(&mut app, KeyCode::Char(mutation));
        assert!(
            matches!(app.mode, Mode::Normal),
            "'{mutation}' opens no prompt on an archived task"
        );
        assert!(
            app.footer_msg.is_some(),
            "'{mutation}' surfaces the read-only notice"
        );
    }

    let reader = Store::new(dir.path());
    assert_eq!(reader.load_archive().unwrap().len(), 1, "archive intact");
    assert!(
        reader.load_tasks().unwrap().is_empty(),
        "no task resurrected"
    );
}

#[test]
fn question_mark_opens_help_overlay_and_any_key_closes() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("some task", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('?'));
    assert!(matches!(app.mode, Mode::Help));
    let out = render(&app);
    assert!(out.contains("Keybinds"), "overlay title rendered");
    assert!(out.contains("Global"), "groups rendered");
    assert!(out.contains("Notes pane"), "notes group rendered");

    // any key closes without acting: 'd' must not open a delete confirm
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(app.mode, Mode::Normal), "overlay dismissed");
    let after = render(&app);
    assert!(!after.contains("Keybinds"), "overlay gone");
    assert!(!app.should_quit, "close key not re-dispatched");
}

#[test]
fn note_detail_renders_in_side_pane() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Long-term goals", None).unwrap();
    doc.body.add_item("Areas to grow into", "read DDIA ch. 8-9");
    notes.save(&mut doc).unwrap();

    // the first note is auto-loaded into the always-on side pane
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    let detail_out = render(&app);
    assert!(detail_out.contains("Long-term goals"), "title rendered");
    assert!(
        detail_out.contains("Areas to grow into"),
        "heading rendered"
    );
    assert!(detail_out.contains("read DDIA ch. 8-9"), "item rendered");
    assert!(detail_out.contains("editor"), "detail footer hint present");
}

// ---- state-transition tests ----------------------------------------------

#[test]
fn add_task_parses_category_and_project_tokens() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "fix login @engineering #auth");
    press(&mut app, KeyCode::Enter);

    let saved = Store::new(dir.path()).load_tasks().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].text, "fix login");
    assert_eq!(saved[0].category, "engineering");
    assert_eq!(saved[0].project.as_deref(), Some("auth"));
}

#[test]
fn add_task_shows_category_ghost_and_tab_completes_it() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "fix login @eng");

    // the ghost remainder renders inline after the cursor, dimmed
    let out = render(&app);
    assert!(
        out.contains("fix login @engineering"),
        "ghost remainder rendered inline: {out}"
    );
    assert!(out.contains("tab complete"), "footer advertises tab");

    press(&mut app, KeyCode::Tab);
    match &app.mode {
        Mode::TextEdit(te) => {
            assert_eq!(te.text(), "fix login @engineering ");
            assert_eq!(
                te.cursor_col(),
                te.text().chars().count(),
                "cursor after space"
            );
        }
        other => panic!("still editing after tab, got {other:?}"),
    }

    press(&mut app, KeyCode::Enter);
    let saved = Store::new(dir.path()).load_tasks().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].text, "fix login");
    assert_eq!(saved[0].category, "engineering");
}

#[test]
fn tab_completes_project_from_active_and_archived_tasks() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[Task::new(
            "active work",
            "engineering",
            Some("auth".to_string()),
            None,
        )])
        .unwrap();
    let mut done = Task::new("shipped", "engineering", Some("billing".to_string()), None);
    done.status = Status::Done;
    done.completed_at = Some(Local::now().fixed_offset());
    store.append_archive(&done).unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    // project of an archived task still completes
    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "deploy #bil");
    press(&mut app, KeyCode::Tab);
    match &app.mode {
        Mode::TextEdit(te) => assert_eq!(te.text(), "deploy #billing "),
        other => panic!("still editing after tab, got {other:?}"),
    }
    press(&mut app, KeyCode::Esc); // insert → normal
    press(&mut app, KeyCode::Esc); // normal → cancel

    // case-insensitive typing lands on the canonical project name
    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "refactor #AU");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    let saved = Store::new(dir.path()).load_tasks().unwrap();
    let added = saved.iter().find(|t| t.text == "refactor").unwrap();
    assert_eq!(added.project.as_deref(), Some("auth"));
}

#[test]
fn tab_without_suggestion_is_noop_while_editing() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "plain text");
    press(&mut app, KeyCode::Tab);
    match &app.mode {
        Mode::TextEdit(te) => assert_eq!(te.text(), "plain text", "buffer untouched"),
        other => panic!("tab must not leave editing mode, got {other:?}"),
    }
    assert_eq!(app.focus, Focus::Main, "tab does not toggle pane focus");
}

#[test]
fn complete_moves_task_from_tasks_to_archive_on_disk() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("finish it", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    // Today view, first task selected by default.
    press(&mut app, KeyCode::Char(' '));

    let reader = Store::new(dir.path());
    assert!(
        reader.load_tasks().unwrap().is_empty(),
        "tasks.jsonl emptied"
    );
    let archived = reader.load_archive().unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].status, Status::Done);
    assert_eq!(archived[0].text, "finish it");
}

#[test]
fn block_toggle_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("blockable", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].status,
        Status::Blocked
    );

    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].status,
        Status::Open
    );
}

#[test]
fn edit_task_text_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("old text", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('e'));
    // clear the prefilled buffer, then type new text
    for _ in 0.."old text".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "new text");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].text,
        "new text"
    );
}

#[test]
fn edit_task_esc_to_normal_then_esc_cancels_without_saving() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("keep me", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('e'));
    type_str(&mut app, " and more");
    press(&mut app, KeyCode::Esc);
    match &app.mode {
        Mode::TextEdit(te) => assert_eq!(te.vim, VimMode::Normal, "esc enters normal mode"),
        other => panic!("first esc must stay in the modal, got {other:?}"),
    }
    press(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Normal), "second esc cancels");
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].text,
        "keep me",
        "cancel discards the edit"
    );
}

#[test]
fn edit_task_normal_mode_ciw_then_enter_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("old text", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Esc); // insert → normal, cursor on the last char
    type_str(&mut app, "ciw"); // change the word under the cursor ("text")
    type_str(&mut app, "words");
    press(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].text,
        "old words"
    );
}

#[test]
fn edit_task_normal_mode_dw_then_enter_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("drop this", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Esc);
    type_str(&mut app, "0dw"); // to line start, delete "drop "
    press(&mut app, KeyCode::Enter);

    assert_eq!(Store::new(dir.path()).load_tasks().unwrap()[0].text, "this");
}

#[test]
fn filter_and_due_date_keep_the_lightweight_input() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("some task", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    press(&mut app, KeyCode::Char('/'));
    assert!(
        matches!(&app.mode, Mode::Editing(e) if e.purpose == EditPurpose::Filter),
        "filter uses the single-line input"
    );
    press(&mut app, KeyCode::Esc);
    assert!(
        matches!(app.mode, Mode::Normal),
        "one esc cancels the filter"
    );

    press(&mut app, KeyCode::Char('D'));
    assert!(
        matches!(&app.mode, Mode::Editing(e) if matches!(e.purpose, EditPurpose::DueDate { .. })),
        "due date uses the single-line input"
    );
    press(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn edit_modal_grows_with_wrapped_content() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    let row_of = |out: &str, needle: &str| {
        out.lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not rendered"))
    };
    // modal height = rows from the title border to the bottom border + 1
    let modal_height = |out: &str| row_of(out, "INSERT") - row_of(out, "Add task") + 1;

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "alpha");
    let short = render_sized(&app, 60, 40);
    assert_eq!(modal_height(&short), 3, "one text row plus borders");

    // 60 cols → 36-wide modal, 34 inner: this input wraps onto a second row
    type_str(
        &mut app,
        " bravo charlie delta echo foxtrot golf hotel india",
    );
    let tall = render_sized(&app, 60, 40);
    assert!(tall.contains("alpha"), "head of the buffer rendered");
    assert!(tall.contains("india"), "tail of the buffer rendered");
    assert!(
        row_of(&tall, "india") > row_of(&tall, "alpha bravo"),
        "wrapped content spans multiple modal rows"
    );
    assert_eq!(modal_height(&tall), 4, "modal grew by the wrapped row");
}

#[test]
fn due_date_set_and_invalid_input_shows_footer_error() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("with due", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('D'));
    type_str(&mut app, "2026-08-01");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].due,
        Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap())
    );

    // invalid date: footer error, no crash, due unchanged
    press(&mut app, KeyCode::Char('D'));
    for _ in 0.."2026-08-01".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "notadate");
    press(&mut app, KeyCode::Enter);
    assert!(app.footer_msg.is_some(), "footer error set");
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].due,
        Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()),
        "due unchanged on invalid input"
    );
}

#[test]
fn category_picker_opens_on_current_value_moves_and_persists_choice() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    // "engineering" sits at index 3 of Config::default().categories, with
    // "intake" immediately after it at index 4.
    store
        .save_tasks(&[task("recategorize me", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('C'));
    match &app.mode {
        Mode::CategoryPicker(p) => assert_eq!(p.selected, 3, "opens highlighting current category"),
        other => panic!("expected CategoryPicker mode, got {other:?}"),
    }
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].category,
        "intake"
    );
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn category_picker_esc_cancels_without_changing_category() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("leave me alone", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('C'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Esc);

    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].category,
        "engineering",
        "category unchanged on cancel"
    );
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn delete_confirm_and_cancel() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("deletable", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());

    // cancel keeps the task
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(
        app.mode,
        Mode::Confirm(ConfirmState {
            action: ConfirmAction::DeleteTask,
            ..
        })
    ));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(Store::new(dir.path()).load_tasks().unwrap().len(), 1);

    // confirm removes it
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('y'));
    assert!(Store::new(dir.path()).load_tasks().unwrap().is_empty());
}

#[test]
fn new_note_creates_doc_and_opens_detail() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());

    // N is global: works straight from the default Today tab
    press(&mut app, KeyCode::Char('N'));
    type_str(&mut app, "Scratchpad");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.focus, Focus::Side);
    assert_eq!(
        app.current_note.as_ref().unwrap().frontmatter.title,
        "Scratchpad"
    );
    let listed = NotesStore::new(dir.path().join("notes")).list().unwrap();
    assert!(listed.iter().any(|(_, title)| title == "Scratchpad"));
}

#[test]
fn new_note_key_is_global_across_tabs_and_panes() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Existing", &["item"]);
    let mut app = app_in(dir.path());

    for setup in [
        KeyCode::Char('1'), // Today
        KeyCode::Char('2'), // Standup
        KeyCode::Char('3'), // Tasks
        KeyCode::Char('4'), // Notes
        KeyCode::Tab,       // side pane
    ] {
        press(&mut app, setup);
        press(&mut app, KeyCode::Char('N'));
        assert!(
            matches!(
                &app.mode,
                Mode::TextEdit(te) if te.purpose == EditPurpose::NewNoteTitle
            ),
            "N opens the new-note input after {setup:?}"
        );
        press(&mut app, KeyCode::Esc); // insert → normal
        press(&mut app, KeyCode::Esc); // normal → cancel
    }
}

#[test]
fn new_note_from_notes_tab_selects_it_in_the_list() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["a"]);
    let mut app = app_in(dir.path());

    press(&mut app, KeyCode::Char('4'));
    press(&mut app, KeyCode::Char('N'));
    type_str(&mut app, "Brand new");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.focus, Focus::Side, "new note opens for editing");
    assert_eq!(
        app.current_note.as_ref().unwrap().frontmatter.title,
        "Brand new"
    );
    let selected = &app.notes_list[app.notes_sel];
    assert_eq!(
        selected.slug,
        app.current_note.as_ref().unwrap().slug,
        "list selection tracks the created note"
    );
}

#[test]
fn notes_list_selection_move_previews_without_focus_or_persist() {
    let dir = TempDir::new().unwrap();
    let alpha = seed_note(dir.path(), "Alpha note", &["alpha item"]);
    let beta = seed_note(dir.path(), "Beta note", &["beta item"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.current_note.as_ref().unwrap().slug, alpha);

    // j previews the next note in the side pane without stealing focus
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.focus, Focus::Main, "preview does not move focus");
    assert_eq!(
        app.current_note.as_ref().unwrap().slug,
        beta,
        "side pane mirrors the list selection"
    );
    let state = std::fs::read_to_string(dir.path().join("state.json")).unwrap();
    assert!(
        state.contains(&alpha) && !state.contains(&beta),
        "hover preview is not persisted as last-opened"
    );

    // enter is the deliberate open: focus moves in and the note is remembered
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Side);
    let state = std::fs::read_to_string(dir.path().join("state.json")).unwrap();
    assert!(state.contains(&beta), "deliberate open persisted");
}

#[test]
fn notes_tab_renders_list_with_counts_and_preview_marker() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Long-term goals", &["read DDIA ch. 8-9"]);
    seed_note(dir.path(), "Scratch", &["x", "y"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));
    let out = render(&app);
    assert!(out.contains("Long-term goals"), "note title in list");
    assert!(out.contains("(1 item)"), "singular item count");
    assert!(out.contains("(2 items)"), "plural item count");
    assert!(
        out.contains("preview (enter to edit)"),
        "side pane flags preview state"
    );
    assert!(out.contains("j/k select"), "notes footer hints present");

    press(&mut app, KeyCode::Enter);
    let focused = render(&app);
    assert!(
        !focused.contains("preview (enter to edit)"),
        "preview marker gone once the side pane is focused"
    );
}

#[test]
fn add_note_item_persists_and_delete_removes_it() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let doc = notes.create("Ideas", None).unwrap();
    let slug = doc.slug.clone();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "ship the thing");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert_eq!(reloaded.body.items("Notes"), vec!["ship the thing"]);

    // delete it via confirm
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('y'));
    let after = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert!(after.body.items("Notes").is_empty());
}

// ---- structured note editing ------------------------------------------------

/// Seed a note with two "First" items ("one", "three") and open it in the
/// side pane. Rows: heading(0), one(1), three(2).
fn open_two_item_note(dir: &Path) -> (App, String) {
    let notes = NotesStore::new(dir.join("notes"));
    let mut doc = notes.create("Multi", None).unwrap();
    doc.body.add_item("First", "one");
    doc.body.add_item("First", "three");
    notes.save(&mut doc).unwrap();

    let mut app = app_in(dir);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    (app, doc.slug)
}

#[test]
fn o_inserts_item_below_selected() {
    let dir = TempDir::new().unwrap();
    let (mut app, slug) = open_two_item_note(dir.path());

    press(&mut app, KeyCode::Char('j')); // row 1: "one"
    press(&mut app, KeyCode::Char('o'));
    type_str(&mut app, "two");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert_eq!(reloaded.body.items("First"), vec!["one", "two", "three"]);
    assert_eq!(app.note_row_sel, 2, "selection lands on the new item");
}

#[test]
fn o_on_heading_inserts_first_item() {
    let dir = TempDir::new().unwrap();
    let (mut app, slug) = open_two_item_note(dir.path());

    assert_eq!(app.note_row_sel, 0, "heading row selected on open");
    press(&mut app, KeyCode::Char('o'));
    type_str(&mut app, "zero");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert_eq!(reloaded.body.items("First"), vec!["zero", "one", "three"]);
    assert_eq!(app.note_row_sel, 1, "selection lands on the new first item");
}

#[test]
fn capital_a_creates_section_after_current() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Sections", None).unwrap();
    doc.body.add_item("First", "one");
    doc.body.add_item("Last", "two");
    notes.save(&mut doc).unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);

    // heading "First" selected (row 0)
    press(&mut app, KeyCode::Char('A'));
    type_str(&mut app, "Middle");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&doc.slug)
        .unwrap();
    let headings: Vec<&str> = reloaded
        .body
        .sections
        .iter()
        .map(|s| s.heading.as_str())
        .collect();
    assert_eq!(headings, vec!["First", "Middle", "Last"]);
    assert_eq!(
        app.note_row_sel, 2,
        "selection lands on the new heading row"
    );
}

#[test]
fn add_on_heading_row_targets_that_section() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Targeted", None).unwrap();
    doc.body.add_item("First", "one");
    doc.body.insert_section_after(None, "Second");
    notes.save(&mut doc).unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);

    // rows: heading First(0), one(1), heading Second(2)
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "added");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&doc.slug)
        .unwrap();
    assert_eq!(reloaded.body.items("Second"), vec!["added"]);
    assert_eq!(reloaded.body.items("First"), vec!["one"]);
    assert_eq!(app.note_row_sel, 3, "selection lands on the added item");
}

#[test]
fn delete_on_heading_row_is_noop_with_footer() {
    let dir = TempDir::new().unwrap();
    let (mut app, slug) = open_two_item_note(dir.path());

    assert_eq!(app.note_row_sel, 0, "heading row selected");
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(app.mode, Mode::Normal), "no confirm prompt");
    assert!(app.footer_msg.is_some(), "footer message shown for d");

    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(app.mode, Mode::Normal), "no edit input");
    assert!(app.footer_msg.is_some(), "footer message shown for e");

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert_eq!(reloaded.body.items("First"), vec!["one", "three"]);
}

#[test]
fn editor_request_carries_selected_row_file_line() {
    let dir = TempDir::new().unwrap();
    let (mut app, slug) = open_two_item_note(dir.path());

    let path = dir.path().join("notes").join(format!("{slug}.md"));
    let content = std::fs::read_to_string(&path).unwrap();
    let heading_line = content.lines().position(|l| l == "## First").unwrap() + 1;
    let item_line = content.lines().position(|l| l == "- one").unwrap() + 1;

    press(&mut app, KeyCode::Char('E'));
    assert_eq!(
        app.editor_request,
        Some(EditorRequest::NoteFile {
            path: path.clone(),
            line: Some(heading_line)
        }),
        "heading row jumps to the heading's file line"
    );

    app.editor_request = None;
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('E'));
    assert_eq!(
        app.editor_request,
        Some(EditorRequest::NoteFile {
            path,
            line: Some(item_line)
        }),
        "item row jumps to the item's file line"
    );
}

// ---- multi-pane layout & focus ---------------------------------------------

fn seed_note(dir: &Path, title: &str, items: &[&str]) -> String {
    let notes = NotesStore::new(dir.join("notes"));
    let mut doc = notes.create(title, None).unwrap();
    for item in items {
        doc.body.add_item("Notes", *item);
    }
    notes.save(&mut doc).unwrap();
    doc.slug
}

#[test]
fn tab_bar_renders_all_four_labels() {
    let dir = TempDir::new().unwrap();
    let app = app_in(dir.path());
    let out = render(&app);
    assert!(out.contains("[1] Today"));
    assert!(out.contains("[2] Standup"));
    assert!(out.contains("[3] Tasks"));
    assert!(out.contains("[4] Notes"));
}

#[test]
fn number_and_letter_keys_switch_tabs_and_reset_focus() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Any", &["item"]);
    let mut app = app_in(dir.path());

    press(&mut app, KeyCode::Char('2'));
    assert_eq!(app.tab, Tab::Standup);
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.tab, Tab::Tasks);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.tab, Tab::Notes);
    press(&mut app, KeyCode::Char('1'));
    assert_eq!(app.tab, Tab::Today);

    // letters keep working, even from the side pane, and focus returns Main
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.tab, Tab::Standup);
    assert_eq!(app.focus, Focus::Main);
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.tab, Tab::Tasks);
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(app.tab, Tab::Notes);
    press(&mut app, KeyCode::Char('g'));
    assert_eq!(app.tab, Tab::Today);
}

#[test]
fn tab_key_toggles_focus_and_reroutes_movement() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task("task one", "engineering", Status::Open, None),
            task("task two", "engineering", Status::Open, None),
        ])
        .unwrap();
    seed_note(dir.path(), "Focus note", &["first item", "second item"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.today_sel, 1, "j moves the task selection when Main");

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.note_row_sel, 1, "j moves the note row when Side");
    assert_eq!(app.today_sel, 1, "task selection untouched");

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Main);
}

#[test]
fn bracket_keys_cycle_side_pane_note() {
    let dir = TempDir::new().unwrap();
    let alpha = seed_note(dir.path(), "Alpha note", &["alpha item"]);
    let beta = seed_note(dir.path(), "Beta note", &["beta item"]);

    let mut app = app_in(dir.path());
    assert_eq!(
        app.current_note.as_ref().unwrap().slug,
        alpha,
        "first note auto-loaded on startup"
    );

    press(&mut app, KeyCode::Char('l'));
    assert_eq!(app.focus, Focus::Side);
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(app.current_note.as_ref().unwrap().slug, beta);
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(app.current_note.as_ref().unwrap().slug, alpha, "wraps");
    press(&mut app, KeyCode::Char('['));
    assert_eq!(app.current_note.as_ref().unwrap().slug, beta);
}

#[test]
fn side_pane_shows_note_content_on_today_tab() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("some task", "engineering", Status::Open, None)])
        .unwrap();
    seed_note(dir.path(), "Side note", &["visible from today"]);

    let app = app_in(dir.path());
    assert_eq!(app.tab, Tab::Today);
    let out = render(&app);
    assert!(out.contains("some task"), "main pane content");
    assert!(out.contains("Side note"), "side pane title");
    assert!(out.contains("visible from today"), "side pane item");
}

#[test]
fn narrow_terminal_shows_only_the_focused_pane() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("wide task", "engineering", Status::Open, None)])
        .unwrap();
    seed_note(dir.path(), "Narrow note", &["hidden item"]);

    // Both narrow and short: neither a right nor a bottom split fits, so auto
    // collapses to the single focused pane.
    let mut app = app_in(dir.path());
    let main_only = render_sized(&app, 60, 15);
    assert!(main_only.contains("wide task"), "focused main pane shown");
    assert!(!main_only.contains("hidden item"), "side pane hidden");

    press(&mut app, KeyCode::Tab);
    let side_only = render_sized(&app, 60, 15);
    assert!(side_only.contains("hidden item"), "focused side pane shown");
    assert!(!side_only.contains("wide task"), "main pane hidden");
}

#[test]
fn narrow_but_tall_terminal_stacks_both_panes() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("stacked task", "engineering", Status::Open, None)])
        .unwrap();
    seed_note(dir.path(), "Stacked note", &["stacked item"]);

    // Too narrow for a sidebar but tall enough for a bottom pane: auto stacks.
    let app = app_in(dir.path());
    let out = render_sized(&app, 60, 40);
    assert!(out.contains("stacked task"), "main pane shown");
    assert!(out.contains("stacked item"), "notes pane shown below");
}

#[test]
fn esc_from_side_returns_to_main_then_quits() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Esc note", &["item"]);
    let mut app = app_in(dir.path());

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.focus, Focus::Main);
    assert!(!app.should_quit, "esc from side does not quit");
    press(&mut app, KeyCode::Esc);
    assert!(app.should_quit, "esc from main quits");
}

#[test]
fn last_opened_note_persists_across_sessions() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["a"]);
    let beta = seed_note(dir.path(), "Beta note", &["b"]);

    {
        let mut app = app_in(dir.path());
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(app.current_note.as_ref().unwrap().slug, beta);
    }
    assert!(dir.path().join("state.json").exists(), "state.json written");

    let reopened = app_in(dir.path());
    assert_eq!(
        reopened.current_note.as_ref().unwrap().slug,
        beta,
        "last-opened note restored"
    );
    assert_eq!(reopened.notes_sel, 1, "note cycling position tracks it");
}

// ---- editor escape hatch --------------------------------------------------

#[test]
fn editor_request_set_and_roundtrip_reloads_doc() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Editable", None).unwrap();
    doc.body.add_item("Notes", "original");
    notes.save(&mut doc).unwrap();
    let slug = doc.slug.clone();

    // a stub editor that appends a new list item line to the file
    let stub = dir.path().join("stub-editor.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf -- '- edited by stub\\n' >> \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();
    }

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);

    press(&mut app, KeyCode::Char('E'));
    assert!(app.editor_request.is_some(), "editor request queued");

    // Run the editor half of the escape hatch (terminal suspend/resume is the
    // event loop's job and needs no TTY here).
    let editor_cmd = editor::resolve_editor(&app.config);
    let _ = editor_cmd; // resolve chain covered by editor unit tests
    app.run_editor(stub.to_str().unwrap()).unwrap();

    assert!(app.editor_request.is_none(), "request consumed");
    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    let items = reloaded.body.items("Notes");
    assert!(
        items.contains(&"edited by stub"),
        "doc reloaded with stub edit: {items:?}"
    );
}

// ---- note switcher (`'`) ----------------------------------------------------

#[test]
fn quote_opens_note_picker_preselecting_current_note() {
    let dir = TempDir::new().unwrap();
    let alpha = seed_note(dir.path(), "Alpha note", &["a"]);
    let beta = seed_note(dir.path(), "Beta note", &["b"]);

    let mut app = app_in(dir.path());
    assert_eq!(app.current_note.as_ref().unwrap().slug, alpha);

    // ' works globally (from the Today tab), pre-highlighting the open note
    press(&mut app, KeyCode::Char('\''));
    assert!(
        matches!(app.mode, Mode::NotePicker { selected: 0 }),
        "picker opens on the current note"
    );

    // enter is a deliberate open: side pane + persistence, focus untouched
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.current_note.as_ref().unwrap().slug, beta);
    assert_eq!(app.notes_sel, 1, "note cycling position follows");
    assert_eq!(app.focus, Focus::Main, "picker does not steal focus");
    let state = std::fs::read_to_string(dir.path().join("state.json")).unwrap();
    assert!(state.contains(&beta), "picker open persisted as last note");
}

#[test]
fn note_picker_esc_cancels_without_switching() {
    let dir = TempDir::new().unwrap();
    let alpha = seed_note(dir.path(), "Alpha note", &["a"]);
    seed_note(dir.path(), "Beta note", &["b"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('\''));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Esc);

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(
        app.current_note.as_ref().unwrap().slug,
        alpha,
        "note unchanged on cancel"
    );
    let state = std::fs::read_to_string(dir.path().join("state.json")).unwrap();
    assert!(state.contains(&alpha), "persistence unchanged on cancel");
}

#[test]
fn quote_with_no_notes_shows_footer_notice() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('\''));
    assert!(matches!(app.mode, Mode::Normal), "no picker without notes");
    assert!(app.footer_msg.is_some(), "footer explains why");
}

#[test]
fn note_picker_renders_titles_and_item_counts() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["one"]);
    seed_note(dir.path(), "Beta note", &["one", "two"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('\''));
    let out = render(&app);
    assert!(out.contains("Open note"), "picker title rendered");
    assert!(out.contains("Alpha note"), "titles listed");
    assert!(out.contains("(1 item)"), "singular count");
    assert!(out.contains("(2 items)"), "plural count");
}

// ---- side-pane wrapping & inline markdown -----------------------------------

/// Flatten a wrapped line back to its display text.
fn flat(line: &ratatui::text::Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn wrap_note_item_breaks_at_spaces_and_indents_under_text() {
    let lines = views::notes::wrap_note_item("alpha beta gamma delta", 14, &Theme::default());
    let rendered: Vec<String> = lines.iter().map(flat).collect();
    // width 14 minus the 4-col "  - " prefix leaves 10 text columns
    assert_eq!(rendered, vec!["  - alpha beta", "    gamma", "    delta"]);
}

#[test]
fn wrap_note_item_hard_breaks_single_overlong_word() {
    let lines = views::notes::wrap_note_item("abcdefghijkl", 8, &Theme::default());
    let rendered: Vec<String> = lines.iter().map(flat).collect();
    assert_eq!(rendered, vec!["  - abcd", "    efgh", "    ijkl"]);
}

#[test]
fn wrap_note_item_styles_inline_markdown() {
    use ratatui::style::{Color, Modifier};
    let lines = views::notes::wrap_note_item(
        "has **bold** and `code` [docs](https://x.dev)",
        60,
        &Theme::default(),
    );
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(flat(line), "  - has bold and code docs", "markers consumed");
    assert!(
        line.spans
            .iter()
            .any(|s| s.content == "bold" && s.style.add_modifier.contains(Modifier::BOLD)),
        "bold run styled"
    );
    assert!(
        line.spans
            .iter()
            .any(|s| s.content == "code" && s.style.fg == Some(Color::Yellow)),
        "code run styled distinctly"
    );
    assert!(
        line.spans
            .iter()
            .any(|s| s.content == "docs" && s.style.add_modifier.contains(Modifier::UNDERLINED)),
        "link text styled"
    );
}

#[test]
fn wrap_note_item_unclosed_markers_render_literally() {
    let lines =
        views::notes::wrap_note_item("start **unclosed and `dangling", 60, &Theme::default());
    assert_eq!(flat(&lines[0]), "  - start **unclosed and `dangling");
}

#[test]
fn long_note_items_wrap_in_side_pane_as_one_selectable_row() {
    let dir = TempDir::new().unwrap();
    seed_note(
        dir.path(),
        "Wrappy",
        &["aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll"],
    );

    let app = app_in(dir.path());
    let out = render(&app);
    assert!(
        out.contains("│  - aaaa"),
        "item starts with the dash prefix"
    );
    assert!(
        out.contains("│    iiii") || out.contains("│    jjjj") || out.contains("│    hhhh"),
        "continuation line aligned under the text: {out}"
    );
    // wrapped or not, the item is still exactly one selectable row
    assert_eq!(app.note_rows().len(), 2, "heading + one item row");
}

// ---- narrow-width degradation ------------------------------------------------

#[test]
fn footer_hints_pick_richest_tier_that_fits() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());

    // Today's full footer is ~77 cols; Tasks' is ~90.
    let full = render_sized(&app, 120, 40);
    assert!(full.contains("b block"), "full footer when it fits");
    assert!(full.contains("' note"), "note switcher advertised");

    let medium = render_sized(&app, 70, 40);
    assert!(!medium.contains("b block"), "medium tier drops extras");
    assert!(medium.contains("a add"), "medium keeps high-value hints");
    assert!(medium.contains("? keys"), "help pointer always present");

    let minimal = render_sized(&app, 40, 40);
    assert!(
        !minimal.contains("a add"),
        "minimal footer when medium overflows"
    );
    assert!(minimal.contains("? keys"), "help pointer survives");
    assert!(minimal.contains("q quit"), "quit hint survives");

    // Fit-based, not fixed-breakpoint: at 85 cols the short Today footer
    // still fits in full while the longer Tasks footer steps down to medium.
    let today_85 = render_sized(&app, 85, 40);
    assert!(
        today_85.contains("b block"),
        "Today keeps its full footer at 85 cols"
    );
    app.tab = Tab::Tasks;
    let tasks_85 = render_sized(&app, 85, 40);
    assert!(
        !tasks_85.contains("c cat["),
        "Tasks' longer full footer does not fit at 85 cols"
    );
    assert!(
        tasks_85.contains("v view[Open]"),
        "Tasks still gets its medium footer, not the minimal one"
    );
}

#[test]
fn tasks_title_drops_filters_when_they_do_not_fit() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("t", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;

    let wide = render_sized(&app, 120, 40);
    assert!(wide.contains("cat:all"), "full title when roomy");

    let narrow = render_sized(&app, 30, 40);
    assert!(
        narrow.contains("Tasks — Open"),
        "short title keeps the view"
    );
    assert!(!narrow.contains("cat:"), "filters dropped when tight");
}

#[test]
fn overlong_task_rows_truncate_with_ellipsis() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    let long_text = "alpha ".repeat(20);
    store
        .save_tasks(&[task(long_text.trim(), "engineering", Status::Open, None)])
        .unwrap();

    let app = app_in(dir.path());
    let out = render_sized(&app, 60, 40);
    assert!(out.contains("…"), "row ellipsis-truncated");
    assert!(
        !out.contains("@engineering"),
        "clipped tail (category) is gone rather than wrapped"
    );
}

#[test]
fn custom_theme_changes_rendered_category_color() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("themed task", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    assert_eq!(app.theme, Theme::default());
    let default_fg = category_marker_fg(&app);

    let custom_fg = ratatui::style::Color::Rgb(0x12, 0x34, 0x56);
    app.theme.category = ratatui::style::Style::default().fg(custom_fg);
    let themed_fg = category_marker_fg(&app);

    assert_ne!(
        default_fg, themed_fg,
        "a custom theme's category slot must change the rendered color"
    );
    assert_eq!(themed_fg, Some(custom_fg));
}

/// Foreground color of the `@` marker in the rendered Today view — the first
/// character of the `@category` span, styled via `theme.category`.
fn category_marker_fg(app: &App) -> Option<ratatui::style::Color> {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| views::draw(app, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .find(|cell| cell.symbol() == "@")
        .map(|cell| cell.fg)
}

#[test]
fn help_overlay_shrinks_and_wraps_at_narrow_width() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('?'));

    for width in [100u16, 80, 60] {
        let out = render_sized(&app, width, 40);
        assert!(out.contains("Keybinds"), "overlay renders at {width} cols");
        assert!(out.contains("Global"), "groups render at {width} cols");
    }
}

#[test]
fn tab_bar_compacts_below_47_cols() {
    let dir = TempDir::new().unwrap();
    let app = app_in(dir.path());

    let wide = render_sized(&app, 60, 40);
    assert!(wide.contains("[1] Today"), "bracketed labels when roomy");

    let narrow = render_sized(&app, 40, 40);
    assert!(!narrow.contains("[1] Today"), "brackets dropped when tight");
    assert!(narrow.contains("1 Today"), "compact labels still labeled");
}

// ---- note rename (`r`) & reorder (`J`/`K`) -----------------------------------

#[test]
fn r_renames_note_from_list_keeping_slug() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["item"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));
    press(&mut app, KeyCode::Char('r'));
    match &app.mode {
        Mode::Editing(e) => assert_eq!(e.buffer, "Alpha note", "prefilled with current title"),
        other => panic!("expected rename input, got {other:?}"),
    }
    for _ in 0.."Alpha note".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "Alpha renamed");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        app.notes_list[0].title, "Alpha renamed",
        "list shows new title"
    );
    assert_eq!(app.notes_list[0].slug, "alpha-note", "slug unchanged");
    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load("alpha-note")
        .unwrap();
    assert_eq!(reloaded.frontmatter.title, "Alpha renamed", "title on disk");

    // the stable slug means last-note restoration still resolves
    let reopened = app_in(dir.path());
    let current = reopened.current_note.as_ref().unwrap();
    assert_eq!(current.slug, "alpha-note");
    assert_eq!(current.frontmatter.title, "Alpha renamed");
}

#[test]
fn r_renames_current_note_from_side_pane() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Side title", &["item"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Side);
    press(&mut app, KeyCode::Char('r'));
    for _ in 0.."Side title".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "Better title");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        app.current_note.as_ref().unwrap().frontmatter.title,
        "Better title",
        "open note reflects the rename"
    );
    assert_eq!(app.current_note.as_ref().unwrap().slug, "side-title");
}

#[test]
fn reorder_persists_and_drives_picker_and_cycling() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["a"]);
    seed_note(dir.path(), "Beta note", &["b"]);
    seed_note(dir.path(), "Gamma note", &["g"]);
    let slugs =
        |app: &App| -> Vec<String> { app.notes_list.iter().map(|s| s.slug.clone()).collect() };

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(slugs(&app), ["alpha-note", "beta-note", "gamma-note"]);

    // K at the top is a no-op; J swaps down and the selection follows
    press(&mut app, KeyCode::Char('K'));
    assert_eq!(slugs(&app), ["alpha-note", "beta-note", "gamma-note"]);
    press(&mut app, KeyCode::Char('J'));
    assert_eq!(slugs(&app), ["beta-note", "alpha-note", "gamma-note"]);
    assert_eq!(app.notes_sel, 1, "selection follows the moved note");
    assert_eq!(
        app.current_note.as_ref().unwrap().slug,
        "alpha-note",
        "preview still shows the moved note"
    );

    // the custom order survives a restart
    let mut reopened = app_in(dir.path());
    assert_eq!(slugs(&reopened), ["beta-note", "alpha-note", "gamma-note"]);

    // [/] cycling walks the custom order, not slug order
    reopened.tab = Tab::Today;
    press(&mut reopened, KeyCode::Tab); // side pane; alpha (last note) is open
    assert_eq!(reopened.current_note.as_ref().unwrap().slug, "alpha-note");
    press(&mut reopened, KeyCode::Char(']'));
    assert_eq!(
        reopened.current_note.as_ref().unwrap().slug,
        "gamma-note",
        "next in custom order"
    );
    press(&mut reopened, KeyCode::Char('['));
    press(&mut reopened, KeyCode::Char('['));
    assert_eq!(
        reopened.current_note.as_ref().unwrap().slug,
        "beta-note",
        "previous twice wraps the custom order"
    );

    // the ' picker walks the same list: top entry is now Beta
    press(&mut reopened, KeyCode::Esc); // side -> main
    press(&mut reopened, KeyCode::Char('\''));
    press(&mut reopened, KeyCode::Char('k'));
    press(&mut reopened, KeyCode::Char('k')); // to the top
    press(&mut reopened, KeyCode::Enter);
    assert_eq!(reopened.current_note.as_ref().unwrap().slug, "beta-note");
}

#[test]
fn notes_missing_from_persisted_order_append_and_stale_entries_drop() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Alpha note", &["a"]);
    seed_note(dir.path(), "Beta note", &["b"]);

    // persist a custom order: [beta, alpha]
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));
    press(&mut app, KeyCode::Char('J'));
    drop(app);

    // an externally created note (slug-sorts first!) appends at the end
    seed_note(dir.path(), "Aaa note", &["new"]);
    let app = app_in(dir.path());
    let slugs: Vec<&str> = app.notes_list.iter().map(|s| s.slug.as_str()).collect();
    assert_eq!(slugs, ["beta-note", "alpha-note", "aaa-note"]);
    drop(app);

    // a deleted note silently drops out; the rest keep their order
    std::fs::remove_file(dir.path().join("notes").join("beta-note.md")).unwrap();
    let app = app_in(dir.path());
    let slugs: Vec<&str> = app.notes_list.iter().map(|s| s.slug.as_str()).collect();
    assert_eq!(slugs, ["alpha-note", "aaa-note"]);
}

#[test]
fn lowercase_d_deletes_note_via_bare_enter() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Keeper note", &["a"]);
    seed_note(dir.path(), "Doomed note", &["b"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4')); // Notes tab
    // slug-sorted: "doomed-note" is first, so it's selected on entry.
    assert_eq!(app.notes_list[app.notes_sel].slug, "doomed-note");

    // d opens the shared confirm carrying the target slug
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(
        &app.mode,
        Mode::Confirm(ConfirmState { action: ConfirmAction::DeleteNote { slug }, .. })
            if slug == "doomed-note"
    ));

    // bare Enter (no y needed) confirms
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Normal));

    let slugs: Vec<&str> = app.notes_list.iter().map(|s| s.slug.as_str()).collect();
    assert_eq!(slugs, ["keeper-note"], "deleted note gone from the list");
    assert!(
        !dir.path().join("notes").join("doomed-note.md").exists(),
        "note file removed from disk"
    );
    // the side pane no longer references the deleted note
    assert_ne!(
        app.current_note.as_ref().map(|d| d.slug.as_str()),
        Some("doomed-note")
    );
}

#[test]
fn note_delete_cancel_keeps_the_note() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Survivor", &["a"]);

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));

    // n cancels
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('n'));
    assert!(matches!(app.mode, Mode::Normal));
    assert!(dir.path().join("notes").join("survivor.md").exists());

    // Esc cancels too
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Normal));
    assert!(
        dir.path().join("notes").join("survivor.md").exists(),
        "note survives cancel"
    );
    assert_eq!(app.notes_list.len(), 1);
}

#[test]
fn notes_footer_and_help_advertise_rename_and_move() {
    let dir = TempDir::new().unwrap();
    seed_note(dir.path(), "Any", &["item"]);
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('4'));

    let out = render(&app);
    assert!(out.contains("r rename"), "footer advertises rename");
    assert!(out.contains("J/K move"), "footer advertises reorder");

    press(&mut app, KeyCode::Char('?'));
    let overlay = render(&app);
    assert!(
        overlay.contains("J/K move note up/down"),
        "overlay notes group"
    );
}

#[test]
fn ctrl_o_roundtrips_modal_buffer_through_editor() {
    let dir = TempDir::new().unwrap();

    // a stub editor that replaces the temp file with multi-line content
    let stub = dir.path().join("stub-editor.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf 'line one\\nline two\\n' > \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();
    }

    let mut app = app_in(dir.path());
    app.tab = Tab::Tasks;
    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "first draft");

    press_ctrl(&mut app, 'o');
    let Some(EditorRequest::ModalBuffer { path }) = app.editor_request.clone() else {
        panic!("ctrl+o must queue a modal-buffer editor request");
    };
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "first draft",
        "temp file seeded with the buffer"
    );

    app.run_editor(stub.to_str().unwrap()).unwrap();
    assert!(app.editor_request.is_none(), "request consumed");
    assert!(!path.exists(), "temp file deleted");
    match &app.mode {
        Mode::TextEdit(te) => {
            assert_eq!(te.text(), "line one line two", "lines joined with spaces");
            assert_eq!(te.vim, VimMode::Insert, "still in the modal, no submit");
        }
        other => panic!("modal must stay open after the editor, got {other:?}"),
    }

    press(&mut app, KeyCode::Enter);
    let saved = Store::new(dir.path()).load_tasks().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].text, "line one line two");
}

// ---- theme picker ---------------------------------------------------------

#[test]
fn ctrl_t_opens_theme_picker() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    press_ctrl(&mut app, 't');
    match &app.mode {
        Mode::ThemePicker(picker) => {
            // "default" is always first and pre-selected.
            assert_eq!(picker.options.first().map(String::as_str), Some("default"));
            assert_eq!(picker.original, "default");
            assert_eq!(picker.selected, 0);
        }
        other => panic!("ctrl+t must open the theme picker, got {other:?}"),
    }
}

#[test]
fn theme_picker_enter_applies_and_persists() {
    let dir = TempDir::new().unwrap();
    // Materialize config.yaml so persistence has a file to rewrite.
    let cfg_path = Store::new(dir.path()).config_path();
    crate::config::load_or_create(&cfg_path).unwrap();

    let mut app = app_in(dir.path());
    let default_theme = app.theme.clone();

    press_ctrl(&mut app, 't');
    // Move off "default" onto the first built-in preset, then apply.
    press(&mut app, KeyCode::Char('j'));
    let chosen = match &app.mode {
        Mode::ThemePicker(p) => p.options[p.selected].clone(),
        other => panic!("expected theme picker, got {other:?}"),
    };
    assert_ne!(chosen, "default");
    press(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.config.theme, chosen);
    assert_ne!(app.theme, default_theme, "live theme changed");

    // The config file on disk now reads back the new theme.
    let reloaded = crate::config::load_or_create(&cfg_path).unwrap();
    assert_eq!(reloaded.theme, chosen);
}

#[test]
fn theme_picker_esc_reverts_and_leaves_config_unchanged() {
    let dir = TempDir::new().unwrap();
    let cfg_path = Store::new(dir.path()).config_path();
    crate::config::load_or_create(&cfg_path).unwrap();
    let before = std::fs::read_to_string(&cfg_path).unwrap();

    let mut app = app_in(dir.path());
    let default_theme = app.theme.clone();

    press_ctrl(&mut app, 't');
    press(&mut app, KeyCode::Char('j')); // preview a non-default theme
    assert_ne!(app.theme, default_theme, "preview changed the live theme");
    press(&mut app, KeyCode::Esc);

    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(
        app.theme, default_theme,
        "esc reverts to the original theme"
    );
    assert_eq!(app.config.theme, "default", "config value untouched");
    assert_eq!(
        std::fs::read_to_string(&cfg_path).unwrap(),
        before,
        "esc must not write config.yaml"
    );
}
