//! Tasks view: open work and/or the archive (the `v` status view), with
//! text/category/project filters on top.

use crate::model::Status;
use crate::tui::app::App;
use crate::tui::views::{
    archived_task_line, pane_block, selection_style, task_line, truncate_line, truncate_str,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let tasks = app.tasks_filtered();
    let inner = area.width.saturating_sub(2) as usize;
    let row_width = inner.saturating_sub(2).max(8); // room for the "> " shift

    // Full title with filters when it fits, else just the status view, then
    // ellipsis-truncated as a last resort.
    let full_title = format!(
        "Tasks — {} · cat:{} proj:{}{}",
        app.task_view.label(),
        app.category_filter_label(),
        app.project_filter_label(),
        if app.filter_text.is_empty() {
            String::new()
        } else {
            format!(" /{}", app.filter_text)
        }
    );
    let title = if full_title.chars().count() <= inner {
        full_title
    } else {
        truncate_str(&format!("Tasks — {}", app.task_view.label()), inner)
    };

    let items: Vec<ListItem> = if tasks.is_empty() {
        vec![ListItem::new(Line::from(Span::raw("(no matching tasks)")))]
    } else {
        tasks
            .iter()
            .map(|t| {
                let line = match t.status {
                    Status::Done => archived_task_line(t),
                    _ => task_line(t, app.today),
                };
                ListItem::new(truncate_line(line, row_width))
            })
            .collect()
    };

    let list = List::new(items)
        .block(pane_block(title, focused))
        .highlight_style(selection_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !tasks.is_empty() {
        state.select(Some(app.tasks_sel));
    }
    frame.render_stateful_widget(list, area, &mut state);
}
