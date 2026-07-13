//! Today (home) view: active tasks ordered overdue → due-today → rest, with
//! today's completions dimmed below.

use crate::tui::app::App;
use crate::tui::views::{
    completed_line, header_style, pane_block, selection_style, task_line, wrap_line,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let active = app.today_active();
    let completions = app.today_completions();
    let inner = area.width.saturating_sub(2) as usize;
    let row_width = inner.saturating_sub(2).max(8); // room for the "> " shift

    // Completions are pre-wrapped so the footer height is exact; it is then
    // capped at half the pane so the active list above keeps its space.
    let comp_lines: Vec<Line> = if completions.is_empty() {
        Vec::new()
    } else {
        let mut lines = vec![Line::from(Span::styled(
            "Completed today",
            header_style(&app.theme),
        ))];
        for t in &completions {
            lines.extend(wrap_line(
                completed_line(t, &app.theme),
                (area.width as usize).max(1),
                "    ",
            ));
        }
        lines
    };
    let comp_height = (comp_lines.len() as u16).min(area.height / 2);

    let [top, bottom] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(comp_height)]).areas(area);

    let items: Vec<ListItem> = if active.is_empty() {
        vec![ListItem::new(Line::from(Span::raw("(no open tasks)")))]
    } else {
        active
            .iter()
            .map(|t| {
                ListItem::new(Text::from(wrap_line(
                    task_line(t, app.today, &app.theme),
                    row_width,
                    "    ",
                )))
            })
            .collect()
    };

    let list = List::new(items)
        .block(pane_block("Today", focused, &app.theme))
        .highlight_style(selection_style(&app.theme))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !active.is_empty() {
        state.select(Some(app.today_sel));
    }
    frame.render_stateful_widget(list, top, &mut state);

    if !comp_lines.is_empty() {
        frame.render_widget(Paragraph::new(comp_lines), bottom);
    }
}
