//! Tasks view: open work and/or the archive (the `v` status view), with
//! text/category/project filters on top, reordered by the `S` sort and the
//! `G` grouping (which renders a non-selectable header row per group).

use crate::model::Status;
use crate::tui::app::{App, TaskGroup, TaskSort};
use crate::tui::views::{
    archived_task_line, header_style, pane_block, selection_style, task_line, truncate_str,
    wrap_line,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let tasks = app.tasks_filtered();
    let inner = area.width.saturating_sub(2) as usize;
    let row_width = inner.saturating_sub(2).max(8); // room for the "> " shift

    // Full title with filters (plus sort/group when active) when it fits,
    // else just the status view, then ellipsis-truncated as a last resort.
    let mut full_title = format!(
        "Tasks — {} · cat:{} proj:{}",
        app.task_view.label(),
        app.category_filter_label(),
        app.project_filter_label(),
    );
    if app.task_sort != TaskSort::File {
        full_title.push_str(&format!(" · sort:{}", app.task_sort.label()));
    }
    if app.task_group != TaskGroup::Off {
        full_title.push_str(&format!(" · grp:{}", app.task_group.label()));
    }
    if !app.filter_text.is_empty() {
        full_title.push_str(&format!(" /{}", app.filter_text));
    }
    let title = if full_title.chars().count() <= inner {
        full_title
    } else {
        truncate_str(&format!("Tasks — {}", app.task_view.label()), inner)
    };

    // `tasks_sel` indexes into `tasks_filtered()`; header rows are interleaved
    // at render time only, so `selected_row` re-maps the selection to the
    // rendered index and headers stay unselectable by construction.
    let mut selected_row = None;
    let items: Vec<ListItem> = if tasks.is_empty() {
        vec![ListItem::new(Line::from(Span::raw("(no matching tasks)")))]
    } else {
        let mut items = Vec::new();
        let mut current_group: Option<String> = None;
        for (i, t) in tasks.iter().enumerate() {
            if let Some(label) = app.group_label(t)
                && current_group.as_deref() != Some(label.as_str())
            {
                items.push(ListItem::new(Line::from(Span::styled(
                    label.clone(),
                    header_style(&app.theme),
                ))));
                current_group = Some(label);
            }
            if i == app.tasks_sel {
                selected_row = Some(items.len());
            }
            let line = match t.status {
                Status::Done => archived_task_line(t, &app.theme),
                _ => task_line(t, app.today, &app.theme),
            };
            items.push(ListItem::new(Text::from(wrap_line(
                line, row_width, "    ",
            ))));
        }
        items
    };

    let list = List::new(items)
        .block(pane_block(title, focused, &app.theme))
        .highlight_style(selection_style(&app.theme))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !tasks.is_empty() {
        state.select(selected_row);
    }
    frame.render_stateful_widget(list, area, &mut state);
}
