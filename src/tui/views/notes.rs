//! Notes rendering: the Notes tab's document list (which live-previews into
//! the side pane) and the per-document detail (headings + items) shown in
//! the always-on side pane. Item text gets minimal inline markdown styling
//! and is pre-wrapped to the pane width, since ratatui's `List` does neither.

use crate::markdown::{Inline, parse_inline};
use crate::tui::app::{App, Focus, NoteRow, Tab};
use crate::tui::views::{
    dim_style, header_style, pane_block, selection_style, truncate_line, truncate_str,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState};

pub fn render_list(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let inner = area.width.saturating_sub(2) as usize;
    let row_width = inner.saturating_sub(2).max(8); // room for the "> " shift
    let items: Vec<ListItem> = if app.notes_list.is_empty() {
        vec![ListItem::new(Line::from(Span::raw(truncate_str(
            "(no notes yet — press N to create one; it opens in the side pane)",
            row_width,
        ))))]
    } else {
        app.notes_list
            .iter()
            .map(|s| {
                let count = match s.item_count {
                    1 => "1 item".to_string(),
                    n => format!("{n} items"),
                };
                let line = Line::from(vec![
                    Span::raw(s.title.clone()),
                    Span::styled(format!("  ({count})"), dim_style()),
                ]);
                ListItem::new(truncate_line(line, row_width))
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
    let inner = area.width.saturating_sub(2) as usize;
    let wrap_width = inner.saturating_sub(2).max(8); // room for the "> " shift

    let mut title = app
        .current_note
        .as_ref()
        .map(|d| d.frontmatter.title.clone())
        .unwrap_or_else(|| "Note".to_string());
    // On the Notes tab the side pane mirrors the list selection live; make
    // the state legible and point at the key that moves focus in.
    if app.tab == Tab::Notes && app.focus == Focus::Main && app.current_note.is_some() {
        title.push_str(if inner >= 40 {
            " — preview (enter to edit)"
        } else {
            " — preview"
        });
    }
    let title = truncate_str(&title, inner);

    // Rows come straight from `note_rows()`, so the rendered list and the
    // selection index share one source of truth. A wrapped item is one
    // multi-line ListItem, so it stays a single selectable row.
    let mut rows: Vec<ListItem> = app
        .note_rows()
        .into_iter()
        .map(|row| match row {
            NoteRow::Heading { heading, .. } => ListItem::new(truncate_line(
                Line::from(Span::styled(format!("## {heading}"), header_style())),
                wrap_width,
            )),
            NoteRow::Item { text, .. } => {
                ListItem::new(Text::from(wrap_note_item(&text, wrap_width)))
            }
        })
        .collect();

    if rows.is_empty() {
        let hint = if app.current_note.is_some() {
            "(empty — press a to add an item)"
        } else {
            "(no notes — press N to create one)"
        };
        rows.push(ListItem::new(Line::from(Span::raw(truncate_str(
            hint, wrap_width,
        )))));
    }

    let list = List::new(rows)
        .block(pane_block(title, focused))
        .highlight_style(selection_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if focused && !app.note_rows().is_empty() {
        state.select(Some(app.note_row_sel));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

/// Style for one inline markdown run.
fn inline_run(inline: Inline) -> (String, Style) {
    match inline {
        Inline::Text(s) => (s, Style::default()),
        Inline::Bold(s) => (s, Style::default().add_modifier(Modifier::BOLD)),
        Inline::Italic(s) => (s, Style::default().add_modifier(Modifier::ITALIC)),
        Inline::Code(s) => (s, Style::default().fg(Color::Yellow)),
        Inline::Link { text, .. } => (
            text,
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
        ),
    }
}

/// Pre-wrap one item's text (with inline markdown styling) to `width` total
/// columns: a dimmed `  - ` prefix on the first line, and continuations
/// indented four spaces so they align under the item text, not the dash.
/// Breaks at the last space that fits, or mid-word when a single word
/// overflows the line.
pub(in crate::tui) fn wrap_note_item(text: &str, width: usize) -> Vec<Line<'static>> {
    const PREFIX: &str = "  - ";
    const INDENT: &str = "    ";
    let text_width = width.saturating_sub(PREFIX.len()).max(1);

    // Flatten the styled runs into one char stream tagged with its run index,
    // so wrapping can cut anywhere without losing style boundaries.
    let runs: Vec<(String, Style)> = parse_inline(text).into_iter().map(inline_run).collect();
    let chars: Vec<(char, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(ri, (s, _))| s.chars().map(move |c| (c, ri)))
        .collect();

    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    loop {
        if chars.len() - start <= text_width {
            line_ranges.push((start, chars.len()));
            break;
        }
        let window_end = start + text_width; // exclusive hard-break point
        let break_at = (start + 1..=window_end)
            .rev()
            .find(|&i| chars[i - 1].0 != ' ' && chars.get(i).map(|c| c.0) == Some(' '));
        match break_at {
            Some(i) => {
                line_ranges.push((start, i));
                // skip the run of spaces the break landed on
                let mut next = i;
                while chars.get(next).map(|c| c.0) == Some(' ') {
                    next += 1;
                }
                start = next;
            }
            None => {
                line_ranges.push((start, window_end));
                start = window_end;
            }
        }
    }

    line_ranges
        .into_iter()
        .enumerate()
        .map(|(li, (s, e))| {
            let lead = if li == 0 { PREFIX } else { INDENT };
            let mut spans = vec![Span::styled(lead, dim_style())];
            let mut i = s;
            while i < e {
                let run = chars[i].1;
                let mut j = i;
                while j < e && chars[j].1 == run {
                    j += 1;
                }
                let chunk: String = chars[i..j].iter().map(|c| c.0).collect();
                spans.push(Span::styled(chunk, runs[run].1));
                i = j;
            }
            Line::from(spans)
        })
        .collect()
}
