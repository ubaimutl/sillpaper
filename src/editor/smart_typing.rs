use crate::config::Preferences;
use crate::model::{Block, BlockKind, Inline, InlineAttr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartAction {
    ReplaceText {
        start: usize,
        end: usize,
        text: String,
    },
    ConvertBlock {
        prefix_len: usize,
        kind: BlockKind,
    },
    ReplaceWithInline {
        start: usize,
        end: usize,
        inline: Inline,
        trailing_space: bool,
    },
    FormatDelimited {
        outer_start: usize,
        outer_end: usize,
        delim_len: usize,
        attrs: Vec<InlineAttr>,
    },
}

pub fn detect_smart_typing(
    block: &Block,
    cursor_after: usize,
    typed_char: char,
    prefs: &Preferences,
) -> Option<SmartAction> {
    if matches!(
        block.kind,
        BlockKind::CodeBlock { .. } | BlockKind::HorizontalRule
    ) {
        return None;
    }

    let text = block.buffer_text();
    let chars: Vec<char> = text.chars().collect();
    if cursor_after == 0 || cursor_after > chars.len() {
        return None;
    }

    let before: String = chars[..cursor_after].iter().collect();

    if block.kind == BlockKind::Paragraph {
        if typed_char != '#' && (2..=4).contains(&cursor_after) {
            let hash_count = cursor_after - 1;
            if chars[..hash_count].iter().all(|&c| c == '#') {
                let prefix_len = if typed_char == ' ' {
                    cursor_after
                } else {
                    hash_count
                };
                return Some(SmartAction::ConvertBlock {
                    prefix_len,
                    kind: BlockKind::Heading(hash_count as u8),
                });
            }
        }

        if cursor_after == 2 && chars[0] == '>' && !matches!(typed_char, '>' | '-' | '=') {
            let prefix_len = if typed_char == ' ' { 2 } else { 1 };
            return Some(SmartAction::ConvertBlock {
                prefix_len,
                kind: BlockKind::Blockquote,
            });
        }

        if typed_char == ' ' {
            if prefs.markdown_asterisk_lists && (before == "* " || before == "- ") {
                return Some(SmartAction::ConvertBlock {
                    prefix_len: 2,
                    kind: BlockKind::BulletListItem { depth: 0 },
                });
            }
            if prefs.markdown_ordered_lists && before == "1. " {
                return Some(SmartAction::ConvertBlock {
                    prefix_len: 3,
                    kind: BlockKind::OrderedListItem {
                        depth: 0,
                        number: 1,
                    },
                });
            }
        }
    }

    if typed_char == ']'
        && let Some(action) = detect_checkbox(&chars, cursor_after)
    {
        return Some(action);
    }

    if prefs.markdown_inline_asterisks && typed_char == '*' {
        if let Some(action) = detect_delimited(
            &chars,
            cursor_after,
            "***",
            vec![InlineAttr::Bold, InlineAttr::Italic],
        ) {
            return Some(action);
        }
        if let Some(action) = detect_delimited(&chars, cursor_after, "**", vec![InlineAttr::Bold]) {
            return Some(action);
        }
        if let Some(action) = detect_delimited(&chars, cursor_after, "*", vec![InlineAttr::Italic])
        {
            return Some(action);
        }
    }

    if typed_char == '~'
        && let Some(action) =
            detect_delimited(&chars, cursor_after, "~~", vec![InlineAttr::Strikethrough])
    {
        return Some(action);
    }

    if typed_char == '`'
        && let Some(action) = detect_delimited(&chars, cursor_after, "`", vec![InlineAttr::Code])
    {
        return Some(action);
    }

    if prefs.smart_quotes {
        if typed_char == '"' || typed_char == '\'' {
            let prev = if cursor_after >= 2 {
                Some(chars[cursor_after - 2])
            } else {
                None
            };
            let is_opening = prev.is_none()
                || prev
                    .is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '—' | '–'));
            let replacement = if typed_char == '"' {
                if is_opening {
                    prefs.smart_quote_pair.double_open
                } else {
                    prefs.smart_quote_pair.double_close
                }
            } else if is_opening {
                prefs.smart_quote_pair.single_open
            } else {
                prefs.smart_quote_pair.single_close
            };
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 1,
                end: cursor_after,
                text: replacement.to_string(),
            });
        }
        if typed_char == '<' && cursor_after >= 2 && chars[cursor_after - 2] == '<' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 2,
                end: cursor_after,
                text: "«".to_string(),
            });
        }
        if typed_char == '>' && cursor_after >= 2 && chars[cursor_after - 2] == '>' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 2,
                end: cursor_after,
                text: "»".to_string(),
            });
        }
    }

    if prefs.smart_arrows
        && let Some(action) = detect_arrow(&chars, cursor_after)
    {
        return Some(action);
    }

    if prefs.smart_dashes && typed_char == '-' && cursor_after >= 2 {
        let prev = chars[cursor_after - 2];
        if prev == '-' {
            let before_prev = if cursor_after >= 3 {
                Some(chars[cursor_after - 3])
            } else {
                None
            };
            if before_prev != Some('-') {
                return Some(SmartAction::ReplaceText {
                    start: cursor_after - 2,
                    end: cursor_after,
                    text: "–".to_string(),
                });
            }
        } else if prev == '–' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 2,
                end: cursor_after,
                text: "—".to_string(),
            });
        } else if prev == '—' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 2,
                end: cursor_after,
                text: "---".to_string(),
            });
        }
    }

    if prefs.smart_ellipses && typed_char == '.' {
        if cursor_after >= 3 && chars[cursor_after - 2] == '.' && chars[cursor_after - 3] == '.' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 3,
                end: cursor_after,
                text: "…".to_string(),
            });
        } else if cursor_after >= 2 && chars[cursor_after - 2] == '…' {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - 2,
                end: cursor_after,
                text: "...".to_string(),
            });
        }
    }

    None
}

pub fn is_horizontal_rule_trigger(block: &Block) -> bool {
    if block.kind != BlockKind::Paragraph {
        return false;
    }
    let text = block.plain_text();
    let trimmed = text.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-')
}

pub fn code_fence_trigger(block: &Block) -> Option<Option<String>> {
    if block.kind != BlockKind::Paragraph {
        return None;
    }
    let text = block.plain_text();
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix("```")?;
    let lang = rest.trim_start_matches('`');
    if lang.len() > 24 {
        return None;
    }
    if lang.is_empty() {
        return Some(None);
    }
    if lang
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '#' | '.'))
    {
        return Some(Some(lang.to_string()));
    }
    None
}

pub fn wrap_pair_for_char(ch: char, prefs: &Preferences) -> Option<(String, String)> {
    match ch {
        '"' => {
            if prefs.smart_quotes {
                Some((
                    prefs.smart_quote_pair.double_open.to_string(),
                    prefs.smart_quote_pair.double_close.to_string(),
                ))
            } else {
                Some(("\"".into(), "\"".into()))
            }
        }
        '\'' => {
            if prefs.smart_quotes {
                Some((
                    prefs.smart_quote_pair.single_open.to_string(),
                    prefs.smart_quote_pair.single_close.to_string(),
                ))
            } else {
                Some(("'".into(), "'".into()))
            }
        }
        '(' | ')' => Some(("(".into(), ")".into())),
        '[' | ']' => Some(("[".into(), "]".into())),
        _ => None,
    }
}

fn detect_checkbox(chars: &[char], cursor_after: usize) -> Option<SmartAction> {
    let patterns: &[(&[char], bool)] = &[
        (&['[', ']'], false),
        (&['[', ' ', ']'], false),
        (&['[', 'x', ']'], true),
        (&['[', 'X', ']'], true),
    ];
    for (pat, checked) in patterns {
        let len = pat.len();
        if cursor_after >= len && &chars[cursor_after - len..cursor_after] == *pat {
            let start = cursor_after - len;
            if start == 0 || chars[start - 1].is_whitespace() {
                return Some(SmartAction::ReplaceWithInline {
                    start,
                    end: cursor_after,
                    inline: Inline::Checkbox { checked: *checked },
                    trailing_space: true,
                });
            }
        }
    }
    None
}

fn detect_delimited(
    chars: &[char],
    cursor_after: usize,
    delim: &str,
    attrs: Vec<InlineAttr>,
) -> Option<SmartAction> {
    let dchars: Vec<char> = delim.chars().collect();
    let dlen = dchars.len();
    if cursor_after < dlen * 2 + 1 {
        return None;
    }
    if chars[cursor_after - dlen..cursor_after] != dchars[..] {
        return None;
    }
    if dlen == 1 && dchars[0] == '*' && cursor_after >= 2 && chars[cursor_after - 2] == '*' {
        return None;
    }
    let inner_end = cursor_after - dlen;
    if chars[inner_end - 1].is_whitespace() || chars[inner_end - 1] == dchars[0] {
        return None;
    }

    let mut pos = inner_end - 1;
    loop {
        if pos >= dlen {
            let candidate_start = pos - dlen;
            if chars[candidate_start..pos] == dchars[..] {
                let preceded_by_same =
                    candidate_start > 0 && chars[candidate_start - 1] == dchars[0];
                let followed_by_same = chars[pos] == dchars[0];
                let followed_by_space = chars[pos].is_whitespace();
                if !preceded_by_same && !followed_by_same && !followed_by_space && pos < inner_end {
                    return Some(SmartAction::FormatDelimited {
                        outer_start: candidate_start,
                        outer_end: cursor_after,
                        delim_len: dlen,
                        attrs,
                    });
                }
            }
        }
        if pos == 0 {
            break;
        }
        pos -= 1;
    }
    None
}

fn detect_arrow(chars: &[char], cursor_after: usize) -> Option<SmartAction> {
    const RULES: &[(&str, &str)] = &[
        ("<->", "↔"),
        ("<=>", "⇔"),
        ("<-<", "↢"),
        (">->", "↣"),
        ("<-|", "↤"),
        ("|->", "↦"),
        ("|<-", "⇤"),
        ("->|", "⇥"),
        ("←>", "↔"),
        ("⇐>", "⇔"),
        ("←<", "↢"),
        ("←|", "↤"),
        ("→|", "⇥"),
        ("–>", "→"),
        ("<-", "←"),
        ("->", "→"),
        ("<=", "⇐"),
        ("=>", "⇒"),
    ];
    for (pat, rep) in RULES {
        let pchars: Vec<char> = pat.chars().collect();
        let len = pchars.len();
        if cursor_after >= len && chars[cursor_after - len..cursor_after] == pchars[..] {
            return Some(SmartAction::ReplaceText {
                start: cursor_after - len,
                end: cursor_after,
                text: (*rep).to_string(),
            });
        }
    }
    None
}
