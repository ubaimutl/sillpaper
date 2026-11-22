use crate::model::{DocPos, Document};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub start: DocPos,
    pub end: DocPos,
}

pub fn find_matches(doc: &Document, query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle_len = needle.len();
    let mut matches = Vec::new();

    for (block_idx, block) in doc.blocks.iter().enumerate() {
        let buf = block.buffer_text();
        if buf.is_empty() {
            continue;
        }
        let mut lower_chars: Vec<(usize, char)> = Vec::with_capacity(buf.len());
        for (orig_idx, ch) in buf.chars().enumerate() {
            for lc in ch.to_lowercase() {
                lower_chars.push((orig_idx, lc));
            }
        }
        if lower_chars.len() < needle_len {
            continue;
        }
        let mut i = 0usize;
        while i + needle_len <= lower_chars.len() {
            let mut matched = true;
            for j in 0..needle_len {
                if lower_chars[i + j].1 != needle[j] {
                    matched = false;
                    break;
                }
            }
            if matched {
                let start_off = lower_chars[i].0;
                let end_off = lower_chars[i + needle_len - 1].0 + 1;
                matches.push(SearchMatch {
                    start: DocPos::new(block_idx, start_off),
                    end: DocPos::new(block_idx, end_off),
                });
                i += needle_len;
            } else {
                i += 1;
            }
        }
    }

    matches
}
