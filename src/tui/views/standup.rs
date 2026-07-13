//! Standup view: the shared [`crate::standup`] report rendered as grouped
//! lists — completed (yesterday, or most-recent fallback); Today, which mixes
//! items finished today (dimmed done style) with what's still open; and
//! blocked.

use crate::model::Task;
use crate::tui::app::App;
use crate::tui::views::{completed_line, dim_style, header_style, pane_block, task_line};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let report = &app.standup;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        report.completed_label.clone(),
        header_style(&app.theme),
    )));
    push_group(&mut lines, &report.completed, true, app);

    // Today: items finished today (dimmed done style) first, then still-open
    // ones, under a single heading.
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Today", header_style(&app.theme))));
    if report.completed_today.is_empty() && report.open.is_empty() {
        lines.push(Line::from(Span::styled("  (none)", dim_style(&app.theme))));
    } else {
        for task in &report.completed_today {
            push_task(&mut lines, task, true, app);
        }
        for task in &report.open {
            push_task(&mut lines, task, false, app);
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Blocked",
        header_style(&app.theme),
    )));
    push_group(&mut lines, &report.blocked, false, app);

    let para = Paragraph::new(lines)
        .block(pane_block("Standup", focused, &app.theme))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn push_group(lines: &mut Vec<Line<'static>>, tasks: &[Task], completed: bool, app: &App) {
    if tasks.is_empty() {
        lines.push(Line::from(Span::styled("  (none)", dim_style(&app.theme))));
        return;
    }
    for task in tasks {
        push_task(lines, task, completed, app);
    }
}

fn push_task(lines: &mut Vec<Line<'static>>, task: &Task, completed: bool, app: &App) {
    let mut line = if completed {
        completed_line(task, &app.theme)
    } else {
        task_line(task, app.today, &app.theme)
    };
    line.spans.insert(0, Span::raw("  "));
    lines.push(line);
}
