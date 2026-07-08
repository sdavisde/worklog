//! Standup view: the shared [`crate::standup`] report — completed (yesterday,
//! or most-recent fallback), open, and blocked — rendered as grouped lists.

use crate::model::Task;
use crate::tui::app::App;
use crate::tui::views::{completed_line, dim_style, header_style, task_line};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let report = &app.standup;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        report.completed_label.clone(),
        header_style(),
    )));
    push_group(&mut lines, &report.completed, true, app);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Open", header_style())));
    push_group(&mut lines, &report.open, false, app);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Blocked", header_style())));
    push_group(&mut lines, &report.blocked, false, app);

    let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Standup"));
    frame.render_widget(para, area);
}

fn push_group(lines: &mut Vec<Line<'static>>, tasks: &[Task], completed: bool, app: &App) {
    if tasks.is_empty() {
        lines.push(Line::from(Span::styled("  (none)", dim_style())));
        return;
    }
    for task in tasks {
        let mut line = if completed {
            completed_line(task)
        } else {
            task_line(task, app.today)
        };
        line.spans.insert(0, Span::raw("  "));
        lines.push(line);
    }
}
