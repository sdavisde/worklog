//! Pure rendering: `App` state → frame. No view function mutates state, so
//! the whole UI is drivable under `TestBackend`.

mod notes;
mod standup;
mod tasks;
mod today;

use crate::model::{Status, Task};
use crate::tui::app::{App, EditPurpose, Editing, Focus, Mode, Tab};
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
        Mode::ConfirmDelete => render_confirm(frame, area),
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
    let labels = [
        (Tab::Today, "[1] Today"),
        (Tab::Standup, "[2] Standup"),
        (Tab::Tasks, "[3] Tasks"),
        (Tab::Notes, "[4] Notes"),
    ];
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

    let hints = match &app.mode {
        Mode::Editing(_) => "enter save · esc cancel".to_string(),
        Mode::ConfirmDelete => "delete? y / n".to_string(),
        Mode::Normal => match app.focus {
            Focus::Side => {
                "a add · e edit · D del · E editor · [/] note · tab main · 1-4 tabs · q quit"
                    .to_string()
            }
            Focus::Main => match app.tab {
                Tab::Today => {
                    "a add · space done · b block · e edit · d due · D del · tab notes · 1-4 tabs · q quit"
                        .to_string()
                }
                Tab::Tasks => format!(
                    "a add · space done · b block · e edit · d due · D del · / filter · c cat[{}] · p proj[{}] · tab notes · q quit",
                    app.category_filter_label(),
                    app.project_filter_label()
                ),
                Tab::Standup => "tab notes · 1-4 tabs · q quit".to_string(),
                Tab::Notes => {
                    "enter open · N new · j/k move · tab detail · 1-4 tabs · q quit".to_string()
                }
            },
        },
    };

    let para = Paragraph::new(Line::from(Span::styled(
        format!(" {hints}"),
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(para, area);
}

fn input_label(purpose: &EditPurpose) -> &'static str {
    match purpose {
        EditPurpose::AddTask => "Add task (@category #project)",
        EditPurpose::EditTask { .. } => "Edit task",
        EditPurpose::DueDate { .. } => "Due date (YYYY-MM-DD, empty clears)",
        EditPurpose::Filter => "Filter",
        EditPurpose::NewNoteTitle => "New note title",
        EditPurpose::AddNoteItem { .. } => "Add item",
        EditPurpose::EditNoteItem { .. } => "Edit item",
    }
}

fn render_input(editing: &Editing, frame: &mut Frame, area: Rect) {
    let rect = centered_rect(area, 60, 3);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(input_label(&editing.purpose))
        .border_style(header_style());
    let inner_width = rect.width.saturating_sub(2);
    let para = Paragraph::new(editing.buffer.as_str()).block(block);
    frame.render_widget(para, rect);

    let cursor_col = (editing.cursor as u16).min(inner_width.saturating_sub(1));
    frame.set_cursor_position((rect.x + 1 + cursor_col, rect.y + 1));
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
