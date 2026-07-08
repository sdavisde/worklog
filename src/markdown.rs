//! Minimal line-based markdown block parser.
//!
//! Daily notes and note-doc bodies are simple, single-level documents (a
//! heading, some checklist/bullet lines, maybe a stray paragraph) — a full
//! AST is unnecessary. This module classifies each non-blank line into one
//! of a handful of block kinds; callers (the legacy importer, note-doc
//! parsing) walk the flat sequence themselves to track "current section".

/// One classified, non-blank line of a markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `#` .. `######` heading; `level` is the number of leading `#`
    /// characters (1-6).
    Heading { level: usize, text: String },
    /// `- [ ]` / `- [x]` (or `- [X]`) checklist item. `text` is everything
    /// after the `] ` marker, preserved verbatim (internal spacing intact).
    Checklist { checked: bool, text: String },
    /// A plain `- ` bullet that is not a checklist item.
    Bullet { text: String },
    /// Any other non-blank line.
    Paragraph { text: String },
}

/// Parse `input` into a flat sequence of [`Block`]s, one per non-blank line.
/// Blank lines carry no structural meaning for the documents this parser
/// targets and are dropped.
pub fn parse_blocks(input: &str) -> Vec<Block> {
    input.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Block> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(block) = parse_heading(trimmed) {
        return Some(block);
    }
    if let Some(block) = parse_checklist(trimmed) {
        return Some(block);
    }
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return Some(Block::Bullet {
            text: rest.to_string(),
        });
    }

    Some(Block::Paragraph {
        text: trimmed.to_string(),
    })
}

fn parse_heading(trimmed: &str) -> Option<Block> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    let text = rest.strip_prefix(' ')?;
    Some(Block::Heading {
        level: hashes,
        text: text.trim().to_string(),
    })
}

/// Matches `- [ ] text`, `- [x] text`, `- [X] text`. The text after the `] `
/// marker is kept exactly as written (no further trimming beyond the
/// trailing newline already stripped by `str::lines`).
fn parse_checklist(trimmed: &str) -> Option<Block> {
    let rest = trimmed.strip_prefix("- [")?;
    let mut chars = rest.chars();
    let marker = chars.next()?;
    if !matches!(marker, ' ' | 'x' | 'X') {
        return None;
    }
    let rest = chars.as_str().strip_prefix("] ")?;
    Some(Block::Checklist {
        checked: matches!(marker, 'x' | 'X'),
        text: rest.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heading_levels() {
        let blocks = parse_blocks("# Title\n## Section\n### Subsection\n");
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    text: "Title".to_string()
                },
                Block::Heading {
                    level: 2,
                    text: "Section".to_string()
                },
                Block::Heading {
                    level: 3,
                    text: "Subsection".to_string()
                },
            ]
        );
    }

    #[test]
    fn parses_checklist_items_checked_and_unchecked() {
        let blocks = parse_blocks("- [ ] open item\n- [x] done item\n- [X] also done\n");
        assert_eq!(
            blocks,
            vec![
                Block::Checklist {
                    checked: false,
                    text: "open item".to_string()
                },
                Block::Checklist {
                    checked: true,
                    text: "done item".to_string()
                },
                Block::Checklist {
                    checked: true,
                    text: "also done".to_string()
                },
            ]
        );
    }

    #[test]
    fn preserves_timer_suffix_verbatim() {
        let line = "- [x] Finish timer plugin  [08.16.2025@17:39 - 08.17.2025@09:19]";
        let blocks = parse_blocks(line);
        assert_eq!(
            blocks,
            vec![Block::Checklist {
                checked: true,
                text: "Finish timer plugin  [08.16.2025@17:39 - 08.17.2025@09:19]".to_string(),
            }]
        );
    }

    #[test]
    fn parses_plain_bullets() {
        let blocks = parse_blocks("- just a bullet\n- another one\n");
        assert_eq!(
            blocks,
            vec![
                Block::Bullet {
                    text: "just a bullet".to_string()
                },
                Block::Bullet {
                    text: "another one".to_string()
                },
            ]
        );
    }

    #[test]
    fn parses_paragraphs() {
        let blocks = parse_blocks("New task\nAnother loose line\n");
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph {
                    text: "New task".to_string()
                },
                Block::Paragraph {
                    text: "Another loose line".to_string()
                },
            ]
        );
    }

    #[test]
    fn blank_lines_are_dropped() {
        let blocks = parse_blocks("# Title\n\n\n## Section\n\n- item\n\n");
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 1,
                    text: "Title".to_string()
                },
                Block::Heading {
                    level: 2,
                    text: "Section".to_string()
                },
                Block::Bullet {
                    text: "item".to_string()
                },
            ]
        );
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert_eq!(parse_blocks(""), Vec::new());
        assert_eq!(parse_blocks("\n\n   \n"), Vec::new());
    }

    #[test]
    fn checklist_marker_must_be_space_x_or_capital_x() {
        // "- [y] not a checklist" should fall through to a bullet-ish line;
        // since it starts with "- " but not "- [ ]"/"- [x]", it's a bullet
        // whose text happens to start with "[y] ...".
        let blocks = parse_blocks("- [y] not a checklist\n");
        assert_eq!(
            blocks,
            vec![Block::Bullet {
                text: "[y] not a checklist".to_string()
            }]
        );
    }
}
