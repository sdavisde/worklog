//! Tasks view: the full active list with text/category/project filters.

use crate::tui::app::App;
use crate::tui::views::{pane_block, selection_style, task_line};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let tasks = app.tasks_filtered();

    let title = format!(
        "Tasks — cat:{} proj:{}{}",
        app.category_filter_label(),
        app.project_filter_label(),
        if app.filter_text.is_empty() {
            String::new()
        } else {
            format!(" /{}", app.filter_text)
        }
    );

    let items: Vec<ListItem> = if tasks.is_empty() {
        vec![ListItem::new(Line::from(Span::raw("(no matching tasks)")))]
    } else {
        tasks
            .iter()
            .map(|t| ListItem::new(task_line(t, app.today)))
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
