//! Pure rendering: `App` state → frame. No view function mutates state, so
//! the whole UI is drivable under `TestBackend`.

// `pub(super)` so tui-level tests can unit-test note-item wrapping.
pub(super) mod notes;
mod standup;
mod tasks;
mod today;

use crate::model::{Status, Task};
use crate::tui::app::{App, CategoryPicker, EditPurpose, Editing, Focus, Mode, Tab};
use crate::tui::textedit::{MAX_TEXT_ROWS, TextEdit, VimMode};
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Minimum body width for the two-pane layout; below this only the focused
/// pane renders, full width.
const SPLIT_MIN_WIDTH: u16 = 80;

/// Entry point: tab bar on top, footer below, and a main/side body split with
/// the notes pane always alongside the active tab. Any active input box /
/// confirm prompt draws on top.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_tab_bar(app, frame, tabs);

    if body.width >= SPLIT_MIN_WIDTH {
        let [main, side] =
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                .areas(body);
        render_main(app, frame, main, app.focus == Focus::Main);
        notes::render_detail(app, frame, side, app.focus == Focus::Side);
    } else {
        // narrow fallback: only the focused pane, full width
        match app.focus {
            Focus::Main => render_main(app, frame, body, true),
            Focus::Side => notes::render_detail(app, frame, body, true),
        }
    }

    render_footer(app, frame, footer);

    match &app.mode {
        Mode::Editing(editing) => render_input(editing, frame, area),
        Mode::TextEdit(te) => render_textedit(app, te, frame, area),
        Mode::CategoryPicker(picker) => render_category_picker(picker, frame, area),
        Mode::NotePicker { selected } => render_note_picker(app, *selected, frame, area),
        Mode::ConfirmDelete => render_confirm(frame, area),
        Mode::Help => render_help(frame, area),
        Mode::Normal => {}
    }
}

/// Dispatch the main pane to the active tab's view.
fn render_main(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    match app.tab {
        Tab::Today => today::render(app, frame, area, focused),
        Tab::Standup => standup::render(app, frame, area, focused),
        Tab::Tasks => tasks::render(app, frame, area, focused),
        Tab::Notes => notes::render_list(app, frame, area, focused),
    }
}

fn render_tab_bar(app: &App, frame: &mut Frame, area: Rect) {
    // The full bar needs ~47 cols; drop the [n] brackets when tighter.
    let labels: [(Tab, &str); 4] = if area.width >= 47 {
        [
            (Tab::Today, "[1] Today"),
            (Tab::Standup, "[2] Standup"),
            (Tab::Tasks, "[3] Tasks"),
            (Tab::Notes, "[4] Notes"),
        ]
    } else {
        [
            (Tab::Today, "1 Today"),
            (Tab::Standup, "2 Standup"),
            (Tab::Tasks, "3 Tasks"),
            (Tab::Notes, "4 Notes"),
        ]
    };
    let mut spans = vec![Span::raw(" ")];
    for (tab, label) in labels {
        let style = if app.tab == tab {
            header_style()
        } else {
            dim_style()
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw("  "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---- shared styling helpers -----------------------------------------------

pub(super) fn selection_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

pub(super) fn dim_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub(super) fn header_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

/// Ellipsis-truncate a styled line to `width` columns (this app's content is
/// ASCII-leaning, so chars ≈ columns). ratatui's `List` clips hard otherwise.
pub(super) fn truncate_line(line: Line<'static>, width: usize) -> Line<'static> {
    let total: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if total <= width || width == 0 {
        return line;
    }
    let keep = width - 1;
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for span in line.spans {
        let len = span.content.chars().count();
        if used + len <= keep {
            used += len;
            out.push(span);
            continue;
        }
        let take = keep - used;
        if take > 0 {
            let cut: String = span.content.chars().take(take).collect();
            out.push(Span::styled(cut, span.style));
        }
        break;
    }
    out.push(Span::styled("…", dim_style()));
    Line::from(out)
}

/// Ellipsis-truncate a plain string to `width` chars (for pane titles).
pub(super) fn truncate_str(s: &str, width: usize) -> String {
    if s.chars().count() <= width || width == 0 {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push('…');
    out
}

/// Bordered pane block: the focused pane gets the highlighted border.
pub(super) fn pane_block(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    let block = Block::default().borders(Borders::ALL).title(title);
    if focused {
        block.border_style(header_style())
    } else {
        block
    }
}

fn status_marker(status: Status) -> &'static str {
    match status {
        Status::Open => "[ ] ",
        Status::Blocked => "[~] ",
        Status::Done => "[x] ",
    }
}

/// Row for an active task: marker, text, `@category`, `#project`, due date
/// (overdue in red).
pub(super) fn task_line(task: &Task, today: NaiveDate) -> Line<'static> {
    let mut spans = vec![
        Span::raw(status_marker(task.status)),
        Span::raw(task.text.clone()),
        Span::styled(
            format!("  @{}", task.category),
            Style::default().fg(Color::Green),
        ),
    ];
    if let Some(project) = &task.project {
        spans.push(Span::styled(
            format!(" #{project}"),
            Style::default().fg(Color::Magenta),
        ));
    }
    if let Some(due) = task.due {
        let overdue = due < today;
        let style = if overdue {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        spans.push(Span::styled(format!("  due {due}"), style));
    }
    Line::from(spans)
}

/// Dimmed row for a completed task (Today view footer / Standup completions).
pub(super) fn completed_line(task: &Task) -> Line<'static> {
    Line::from(Span::styled(
        format!("[x] {}  @{}", task.text, task.category),
        dim_style(),
    ))
}

/// Dimmed row for an archived task in the Tasks tab's Done/All views:
/// completed marker, text, category/project, and the completion date.
pub(super) fn archived_task_line(task: &Task) -> Line<'static> {
    let mut text = format!("[x] {}  @{}", task.text, task.category);
    if let Some(project) = &task.project {
        text.push_str(&format!(" #{project}"));
    }
    if let Some(completed) = task.completed_at {
        text.push_str(&format!("  done {}", completed.date_naive()));
    }
    Line::from(Span::styled(text, dim_style()))
}

// ---- footer, input box, confirm prompt ------------------------------------

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    if let Some(msg) = &app.footer_msg {
        let para = Paragraph::new(Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(para, area);
        return;
    }

    let para = Paragraph::new(Line::from(Span::styled(
        format!(" {}", footer_hints(app, area.width)),
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(para, area);
}

/// Hint line for the current mode/context. In Normal mode the tiers for the
/// current context are measured against the actual width and the richest one
/// that fits wins — full → medium → a bare help pointer. Fit-based (not a
/// fixed breakpoint) so a short tab's full hints survive at widths where
/// only the longest tab's wouldn't; the `?` overlay carries the complete
/// list, so a cramped footer only has to advertise it.
fn footer_hints(app: &App, width: u16) -> String {
    match &app.mode {
        Mode::Editing(_) => "enter save · esc cancel".to_string(),
        Mode::TextEdit(te) => match te.vim {
            VimMode::Insert if app.editing_suggestion().is_some() => {
                "enter save · esc normal-mode · tab complete · ctrl+e editor".to_string()
            }
            VimMode::Insert => "enter save · esc normal-mode · ctrl+e editor".to_string(),
            VimMode::Normal => "enter save · esc cancel · i insert · ctrl+e editor".to_string(),
        },
        Mode::CategoryPicker(_) => "j/k move · enter select · esc cancel".to_string(),
        Mode::NotePicker { .. } => "j/k move · enter open · esc cancel".to_string(),
        Mode::ConfirmDelete => "delete? y / n".to_string(),
        Mode::Help => "any key to close".to_string(),
        Mode::Normal => {
            let [full, medium] = match app.focus {
                Focus::Side => [
                    "a add · o insert · e edit · D del · E editor · [/] note · ' note · tab main · ? keys · q quit"
                        .to_string(),
                    "a add · e edit · [/] note · ? keys · q quit".to_string(),
                ],
                Focus::Main => match app.tab {
                    Tab::Today => [
                        "a add · space done · b block · e edit · ' note · tab notes · ? keys · q quit"
                            .to_string(),
                        "a add · space done · e edit · ? keys · q quit".to_string(),
                    ],
                    Tab::Tasks => [
                        format!(
                            "a add · space done · v view[{}] · / filter · c cat[{}] · p proj[{}] · ? keys · q quit",
                            app.task_view.label(),
                            app.category_filter_label(),
                            app.project_filter_label()
                        ),
                        format!(
                            "a add · v view[{}] · / filter · ? keys · q quit",
                            app.task_view.label()
                        ),
                    ],
                    Tab::Standup => [
                        "1-4 tabs · ' note · tab notes · ? keys · q quit".to_string(),
                        "1-4 tabs · ? keys · q quit".to_string(),
                    ],
                    Tab::Notes => [
                        "j/k select · enter open · r rename · J/K move · N new · tab side pane · ? keys · q quit"
                            .to_string(),
                        "enter open · r rename · J/K move · N new · ? keys · q quit".to_string(),
                    ],
                },
            };
            // the renderer prepends one space of padding
            let avail = width.saturating_sub(1) as usize;
            [full, medium]
                .into_iter()
                .find(|t| t.chars().count() <= avail)
                .unwrap_or_else(|| "? keys · q quit".to_string())
        }
    }
}

fn input_label(purpose: &EditPurpose) -> &'static str {
    match purpose {
        EditPurpose::AddTask => "Add task (@category #project)",
        EditPurpose::EditTask { .. } => "Edit task",
        EditPurpose::DueDate { .. } => "Due date (YYYY-MM-DD, empty clears)",
        EditPurpose::Filter => "Filter",
        EditPurpose::NewNoteTitle => "New note title",
        EditPurpose::RenameNote { .. } => "Rename note",
        EditPurpose::AddNoteItem { .. } => "Add item",
        EditPurpose::EditNoteItem { .. } => "Edit item",
        EditPurpose::InsertNoteItem { .. } => "Insert item",
        EditPurpose::NewNoteSection { .. } => "New section heading",
    }
}

/// The lightweight single-line input (filter and due date): a 1-row clipping
/// box with a terminal cursor. Content edits use [`render_textedit`] instead.
fn render_input(editing: &Editing, frame: &mut Frame, area: Rect) {
    let rect = centered_rect(area, 60, 3);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(input_label(&editing.purpose))
        .border_style(header_style());
    let inner_width = rect.width.saturating_sub(2);

    let para = Paragraph::new(Line::from(Span::raw(editing.buffer.clone()))).block(block);
    frame.render_widget(para, rect);

    let cursor_col = (editing.cursor as u16).min(inner_width.saturating_sub(1));
    frame.set_cursor_position((rect.x + 1 + cursor_col, rect.y + 1));
}

/// The vim edit modal: a soft-wrapping textarea whose height tracks the
/// wrapped content (1..=[`MAX_TEXT_ROWS`] text rows, scrolling inside past
/// the cap), with the vim mode shown in the bottom border. The ghost
/// `@category`/`#project` completion remainder is overlaid dimmed at the
/// cursor; its first char keeps the reversed cursor block visible.
fn render_textedit(app: &App, te: &TextEdit, frame: &mut Frame, area: Rect) {
    let [column] = Layout::horizontal([Constraint::Percentage(60)])
        .flex(Flex::Center)
        .areas(area);
    let inner_width = column.width.saturating_sub(2);
    let rows = te.wrapped_rows(inner_width);
    let rect = centered_rect(area, 60, (rows + 2).min(area.height));
    frame.render_widget(Clear, rect);

    let (mode_label, mode_style) = match te.vim {
        VimMode::Insert => (
            " INSERT ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        VimMode::Normal => (
            " NORMAL ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(input_label(&te.purpose))
        .title_bottom(Line::from(Span::styled(mode_label, mode_style)))
        .border_style(header_style());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(te.textarea(), inner);

    // `rows < MAX_TEXT_ROWS` guarantees the widget is not scrolled, so the
    // cursor's wrapped screen position maps 1:1 onto the inner rect.
    if let Some(suggestion) = app.editing_suggestion()
        && rows < MAX_TEXT_ROWS
    {
        let (row, col) = te.screen_pos();
        let x = inner.x.saturating_add(col as u16);
        let y = inner.y.saturating_add(row as u16);
        if x < inner.right() && y < inner.bottom() {
            let mut chars = suggestion.remainder.chars();
            let first: String = chars.by_ref().take(1).collect();
            let rest: String = chars.collect();
            let width = (suggestion.remainder.chars().count() as u16).min(inner.right() - x);
            let ghost = Line::from(vec![
                Span::styled(first, dim_style().add_modifier(Modifier::REVERSED)),
                Span::styled(rest, dim_style()),
            ]);
            frame.render_widget(Paragraph::new(ghost), Rect::new(x, y, width, 1));
        }
    }
}

fn render_category_picker(picker: &CategoryPicker, frame: &mut Frame, area: Rect) {
    let height = picker.options.len() as u16 + 2;
    let rect = centered_rect(area, 40, height);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Category (j/k move · enter select · esc cancel)")
        .border_style(header_style());
    let lines: Vec<Line> = picker
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            if i == picker.selected {
                Line::from(Span::styled(
                    format!("> {opt}"),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::raw(format!("  {opt}")))
            }
        })
        .collect();
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, rect);
}

/// The `'` note switcher: the notes list (title + item count) as a
/// closed-list overlay, same interaction shape as the category picker.
fn render_note_picker(app: &App, selected: usize, frame: &mut Frame, area: Rect) {
    let height = (app.notes_list.len() as u16 + 2).min(area.height);
    let percent_x = if area.width < 90 { 80 } else { 50 };
    let rect = centered_rect(area, percent_x, height);
    frame.render_widget(Clear, rect);
    let title = if rect.width >= 50 {
        "Open note (j/k move · enter open · esc cancel)"
    } else {
        "Open note"
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(header_style());
    let inner_width = rect.width.saturating_sub(2) as usize;
    let lines: Vec<Line> = app
        .notes_list
        .iter()
        .enumerate()
        .map(|(i, note)| {
            let count = match note.item_count {
                1 => "1 item".to_string(),
                n => format!("{n} items"),
            };
            let line = if i == selected {
                Line::from(Span::styled(
                    format!("> {}  ({count})", note.title),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::raw(format!("  {}", note.title)),
                    Span::styled(format!("  ({count})"), dim_style()),
                ])
            };
            truncate_line(line, inner_width)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// The `?` overlay: every keybind, grouped by the context it applies in.
fn render_help(frame: &mut Frame, area: Rect) {
    let groups: [(&str, &[&str]); 6] = [
        (
            "Global",
            &[
                "1/g today · 2/s standup · 3/t tasks · 4/n notes",
                "N new note · ' switch note · tab/h/l pane focus · j/k move",
                "? this help · q/esc quit",
            ],
        ),
        (
            "Today & Tasks",
            &[
                "a add · space/x done · b block · e edit",
                "d due date · C category · D delete",
            ],
        ),
        (
            "Tasks only",
            &["v view (open/done/all) · / filter · c category · p project"],
        ),
        (
            "Notes tab",
            &[
                "j/k preview in side pane · enter open · N new note",
                "r rename · J/K move note up/down (order persists)",
            ],
        ),
        (
            "Notes pane",
            &[
                "a add item · o insert below · A new section · r rename note",
                "e edit · D delete · E open in $EDITOR · [/] switch note",
            ],
        ),
        (
            "Editing",
            &[
                "insert mode: enter save · esc to normal mode · ctrl+e $EDITOR",
                "normal mode: h l w b e 0 ^ $ f t · d c y x p · u undo · esc cancel",
                "tab accept @category/#project completion (add task)",
            ],
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (i, (title, rows)) in groups.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(*title, header_style())));
        for row in *rows {
            lines.push(Line::from(Span::raw(format!("  {row}"))));
        }
    }

    // Narrow terminals get a wider overlay and wrapped rows; the height
    // estimate replays the wrap so the box fits its content.
    let percent_x = if area.width < 90 { 94 } else { 70 };
    let inner_width = (area.width as usize * percent_x / 100)
        .saturating_sub(2)
        .max(10);
    let wrapped_rows: usize = lines
        .iter()
        .map(|l| {
            let len: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            len.div_ceil(inner_width).max(1)
        })
        .sum();
    let height = (wrapped_rows as u16 + 2).min(area.height);
    let rect = centered_rect(area, percent_x as u16, height);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Keybinds (any key to close)")
        .border_style(header_style());
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(block),
        rect,
    );
}

fn render_confirm(frame: &mut Frame, area: Rect) {
    let rect = centered_rect(area, 40, 3);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm")
        .border_style(Style::default().fg(Color::Red));
    let para = Paragraph::new("Delete? (y/n)").block(block);
    frame.render_widget(para, rect);
}

fn centered_rect(area: Rect, percent_x: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [col] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(row);
    col
}
