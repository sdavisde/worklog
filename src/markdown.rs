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

// ---- inline spans -----------------------------------------------------------

/// One styled run of inline text within a single line.
///
/// Deliberately conservative: no nesting, and any marker without a matching
/// closer on the same line is treated as literal text.
// Not wired into any renderer yet; kept for the upcoming styled notes view.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    /// `**bold**`
    Bold(String),
    /// `*italic*`
    Italic(String),
    /// `` `code` ``
    Code(String),
    /// `[text](url)`; the url is kept for callers that want it, but the
    /// display text is what renders.
    Link {
        text: String,
        url: String,
    },
}

/// Tokenize one line of item text into inline runs. Empty-content markers
/// (`**` alone, ` `` ` etc.) and unclosed markers fall through as literal
/// text rather than erroring.
#[allow(dead_code)]
pub fn parse_inline(input: &str) -> Vec<Inline> {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<Inline> = Vec::new();
    let mut literal = String::new();
    let mut i = 0;

    let push_literal = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    };

    while i < chars.len() {
        // Emphasis content must not be empty or start/end with a space, so
        // stray stars in prose ("2 * 3") stay literal.
        let emphasizable = |content: &[char]| {
            !content.is_empty() && content[0] != ' ' && content[content.len() - 1] != ' '
        };

        // `**bold**`
        if chars[i] == '*'
            && chars.get(i + 1) == Some(&'*')
            && let Some(close) = find_seq(&chars, i + 2, &['*', '*'])
            && emphasizable(&chars[i + 2..close])
        {
            push_literal(&mut literal, &mut out);
            out.push(Inline::Bold(chars[i + 2..close].iter().collect()));
            i = close + 2;
            continue;
        }
        // `*italic*` (single star; the double-star case is handled above)
        if chars[i] == '*'
            && let Some(close) = find_seq(&chars, i + 1, &['*'])
            && emphasizable(&chars[i + 1..close])
        {
            push_literal(&mut literal, &mut out);
            out.push(Inline::Italic(chars[i + 1..close].iter().collect()));
            i = close + 1;
            continue;
        }
        // `` `code` ``
        if chars[i] == '`'
            && let Some(close) = find_seq(&chars, i + 1, &['`'])
            && close > i + 1
        {
            push_literal(&mut literal, &mut out);
            out.push(Inline::Code(chars[i + 1..close].iter().collect()));
            i = close + 1;
            continue;
        }
        // `[text](url)`
        if chars[i] == '['
            && let Some(bracket) = find_seq(&chars, i + 1, &[']', '('])
            && let Some(paren) = find_seq(&chars, bracket + 2, &[')'])
            && bracket > i + 1
        {
            push_literal(&mut literal, &mut out);
            out.push(Inline::Link {
                text: chars[i + 1..bracket].iter().collect(),
                url: chars[bracket + 2..paren].iter().collect(),
            });
            i = paren + 1;
            continue;
        }
        literal.push(chars[i]);
        i += 1;
    }
    push_literal(&mut literal, &mut out);
    out
}

/// First index `>= from` where `seq` occurs in `chars`, if any.
#[allow(dead_code)]
fn find_seq(chars: &[char], from: usize, seq: &[char]) -> Option<usize> {
    if chars.len() < seq.len() {
        return None;
    }
    (from..=chars.len() - seq.len()).find(|&i| chars[i..i + seq.len()] == *seq)
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
    fn inline_parses_bold_italic_code_and_link() {
        let spans = parse_inline("see **bold** and *ital* plus `code` at [docs](https://x.dev)");
        assert_eq!(
            spans,
            vec![
                Inline::Text("see ".to_string()),
                Inline::Bold("bold".to_string()),
                Inline::Text(" and ".to_string()),
                Inline::Italic("ital".to_string()),
                Inline::Text(" plus ".to_string()),
                Inline::Code("code".to_string()),
                Inline::Text(" at ".to_string()),
                Inline::Link {
                    text: "docs".to_string(),
                    url: "https://x.dev".to_string(),
                },
            ]
        );
    }

    #[test]
    fn inline_unclosed_markers_render_literally() {
        assert_eq!(
            parse_inline("2 * 3 * is math"),
            vec![Inline::Text("2 * 3 * is math".to_string())],
            "space-padded stars are arithmetic, not emphasis"
        );
        assert_eq!(
            parse_inline("unclosed **bold and `code without end"),
            vec![Inline::Text(
                "unclosed **bold and `code without end".to_string()
            )]
        );
        assert_eq!(
            parse_inline("a * lone star"),
            vec![Inline::Text("a * lone star".to_string())]
        );
        assert_eq!(
            parse_inline("[text with no url]"),
            vec![Inline::Text("[text with no url]".to_string())]
        );
    }

    #[test]
    fn inline_empty_markers_are_literal() {
        assert_eq!(
            parse_inline("**** and `` stay put"),
            vec![Inline::Text("**** and `` stay put".to_string())]
        );
    }

    #[test]
    fn inline_plain_text_is_one_run() {
        assert_eq!(
            parse_inline("nothing fancy here"),
            vec![Inline::Text("nothing fancy here".to_string())]
        );
        assert_eq!(parse_inline(""), Vec::<Inline>::new());
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
