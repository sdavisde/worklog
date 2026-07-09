//! Notes rendering: the Notes tab's document list and the per-document
//! detail (headings + items) shown in the always-on side pane.

use crate::notes::Line as NoteLine;
use crate::tui::app::App;
use crate::tui::views::{dim_style, header_style, pane_block, selection_style};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render_list(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let items: Vec<ListItem> = if app.notes_list.is_empty() {
        vec![ListItem::new(Line::from(Span::raw(
            "(no notes — press N to create one)",
        )))]
    } else {
        app.notes_list
            .iter()
            .map(|s| {
                ListItem::new(Line::from(vec![
                    Span::raw(s.title.clone()),
                    Span::styled(format!("  ({} items)", s.item_count), dim_style()),
                ]))
            })
            .collect()
    };

    let list = List::new(items)
        .block(pane_block("Notes", focused))
        .highlight_style(selection_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !app.notes_list.is_empty() {
        state.select(Some(app.notes_sel));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

pub fn render_detail(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let title = app
        .current_note
        .as_ref()
        .map(|d| d.frontmatter.title.clone())
        .unwrap_or_else(|| "Note".to_string());

    // Build a flat list of rows (headings + items) and track which rows are
    // selectable items so the selection highlight lands on the right one.
    let mut rows: Vec<ListItem> = Vec::new();
    let mut item_row_indices: Vec<usize> = Vec::new();

    if let Some(doc) = &app.current_note {
        for section in &doc.body.sections {
            if !section.heading.is_empty() {
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!("## {}", section.heading),
                    header_style(),
                ))));
            }
            for line in &section.lines {
                if let NoteLine::Item(text) = line {
                    item_row_indices.push(rows.len());
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled("  - ", Style::default().add_modifier(Modifier::DIM)),
                        Span::raw(text.clone()),
                    ])));
                }
            }
        }
    }

    if rows.is_empty() {
        let hint = if app.current_note.is_some() {
            "(empty — press a to add an item)"
        } else {
            "(no notes — press N on the Notes tab)"
        };
        rows.push(ListItem::new(Line::from(Span::raw(hint))));
    }

    let list = List::new(rows)
        .block(pane_block(title, focused))
        .highlight_style(selection_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused {
        if let Some(row) = item_row_indices.get(app.note_item_sel) {
            state.select(Some(*row));
        }
    }
    frame.render_stateful_widget(list, area, &mut state);
}
