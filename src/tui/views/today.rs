//! Today (home) view: active tasks ordered overdue → due-today → rest, with
//! today's completions dimmed below.

use crate::tui::app::App;
use crate::tui::views::{completed_line, header_style, pane_block, selection_style, task_line};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let active = app.today_active();
    let completions = app.today_completions();
    let comp_height = if completions.is_empty() {
        0
    } else {
        completions.len() as u16 + 1
    };

    let [top, bottom] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(comp_height)]).areas(area);

    let items: Vec<ListItem> = if active.is_empty() {
        vec![ListItem::new(Line::from(Span::raw("(no open tasks)")))]
    } else {
        active
            .iter()
            .map(|t| ListItem::new(task_line(t, app.today)))
            .collect()
    };

    let list = List::new(items)
        .block(pane_block("Today", focused))
        .highlight_style(selection_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !active.is_empty() {
        state.select(Some(app.today_sel));
    }
    frame.render_stateful_widget(list, top, &mut state);

    if !completions.is_empty() {
        let mut lines = vec![Line::from(Span::styled("Completed today", header_style()))];
        lines.extend(completions.iter().map(completed_line));
        frame.render_widget(Paragraph::new(lines), bottom);
    }
}
