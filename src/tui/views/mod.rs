//! Pure rendering: `App` state → frame. No view function mutates state, so
//! the whole UI is drivable under `TestBackend`.

mod notes;
mod standup;
mod tasks;
mod today;

use crate::model::{Status, Task};
use crate::tui::app::{App, EditPurpose, Editing, Mode, View};
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Entry point: split off a one-line footer, render the active view into the
/// body, then draw any active input box / confirm prompt on top.
pub fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    match app.view {
        View::Today => today::render(app, frame, body),
        View::Standup => standup::render(app, frame, body),
        View::Tasks => tasks::render(app, frame, body),
        View::NotesList => notes::render_list(app, frame, body),
        View::NoteDetail => notes::render_detail(app, frame, body),
    }

    render_footer(app, frame, footer);

    match &app.mode {
        Mode::Editing(editing) => render_input(editing, frame, area),
        Mode::ConfirmDelete => render_confirm(frame, area),
        Mode::Normal => {}
    }
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
        Mode::Normal => match app.view {
            View::Today => {
                "a add · space done · b block · e edit · d due · D del · s standup · t tasks · n notes · q quit"
                    .to_string()
            }
            View::Tasks => format!(
                "a add · space done · b block · e edit · d due · D del · / filter · c cat[{}] · p proj[{}] · q quit",
                app.category_filter_label(),
                app.project_filter_label()
            ),
            View::Standup => "s standup · t tasks · n notes · g today · q quit".to_string(),
            View::NotesList => {
                "enter open · N new · j/k move · s/t/g switch · q quit".to_string()
            }
            View::NoteDetail => {
                "a add · e edit · D del · E editor · j/k move · esc back".to_string()
            }
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
