use crate::model::{BlockKind, Document, Selection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentStats {
    pub words: usize,
    pub chars_no_spaces: usize,
    pub chars_with_spaces: usize,
    pub reading_seconds: u64,
    pub is_selection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub block_idx: usize,
    pub level: u8,
    pub text: String,
    pub slug: String,
}

pub fn compute_stats(doc: &Document, selection: Option<Selection>, wpm: u32) -> DocumentStats {
    let (text, is_selection) = match selection {
        Some(sel) if !sel.is_collapsed() => {
            let (s, e) = sel.ordered();
            let frag = doc.extract_fragment(s, e);
            (frag.plain_text(false), true)
        }
        _ => (doc.plain_text(false), false),
    };

    let words = count_words(&text);
    let mut chars_with_spaces = 0usize;
    let mut chars_no_spaces = 0usize;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        chars_with_spaces += 1;
        if !ch.is_whitespace() {
            chars_no_spaces += 1;
        }
    }

    let rate = wpm.clamp(1, 25_000) as f64;
    let reading_seconds = if words == 0 {
        0
    } else {
        ((words as f64 / rate) * 60.0).round() as u64
    };

    DocumentStats {
        words,
        chars_no_spaces,
        chars_with_spaces,
        reading_seconds,
        is_selection,
    }
}

pub fn count_words(text: &str) -> usize {
    let mut words = 0usize;
    let mut in_word = false;
    for ch in text.chars() {
        if is_word_char(ch) {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else if ch == '\'' || ch == '\u{2019}' || ch == '-' {
        } else {
            in_word = false;
        }
    }
    words
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

pub fn format_reading_time(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

pub fn extract_toc(doc: &Document) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if let BlockKind::Heading(level) = block.kind
            && (1..=3).contains(&level)
        {
            let text = block.plain_text().trim().to_string();
            let slug = heading_slug(&text);
            entries.push(TocEntry {
                block_idx: idx,
                level,
                text,
                slug,
            });
        }
    }
    entries
}

pub fn heading_slug(text: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in text.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}
