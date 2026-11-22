use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_block_id() -> u64 {
    NEXT_BLOCK_ID.fetch_add(1, AtomicOrdering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineAttr {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Link(Option<String>),
}

impl InlineStyle {
    pub fn is_plain(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && !self.strikethrough
            && !self.code
            && self.link.is_none()
    }

    pub fn has_attr(&self, attr: &InlineAttr) -> bool {
        match attr {
            InlineAttr::Bold => self.bold,
            InlineAttr::Italic => self.italic,
            InlineAttr::Underline => self.underline,
            InlineAttr::Strikethrough => self.strikethrough,
            InlineAttr::Code => self.code,
            InlineAttr::Link(_) => self.link.is_some(),
        }
    }

    pub fn set_attr(&mut self, attr: &InlineAttr, value: bool) {
        match attr {
            InlineAttr::Bold => self.bold = value,
            InlineAttr::Italic => self.italic = value,
            InlineAttr::Underline => self.underline = value,
            InlineAttr::Strikethrough => self.strikethrough = value,
            InlineAttr::Code => self.code = value,
            InlineAttr::Link(url) => {
                if value {
                    self.link = url.clone();
                } else {
                    self.link = None;
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub style: InlineStyle,
}

impl TextRun {
    pub fn new(text: impl Into<String>, style: InlineStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::default(),
        }
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Inline {
    Text(TextRun),
    LineBreak,
    Checkbox {
        checked: bool,
    },
    Comment {
        text: String,
    },
    Image {
        src: String,
        alt: String,
        original_src: Option<String>,
    },
}

impl Inline {
    pub fn char_len(&self) -> usize {
        match self {
            Inline::Text(run) => run.char_len(),
            Inline::LineBreak
            | Inline::Checkbox { .. }
            | Inline::Comment { .. }
            | Inline::Image { .. } => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlockKind {
    #[default]
    Paragraph,
    Heading(u8),
    Blockquote,
    BulletListItem {
        depth: u8,
    },
    OrderedListItem {
        depth: u8,
        number: u32,
    },
    CodeBlock {
        language: Option<String>,
    },
    HorizontalRule,
}

impl BlockKind {
    pub fn is_heading(&self) -> bool {
        matches!(self, BlockKind::Heading(_))
    }

    pub fn is_list(&self) -> bool {
        matches!(
            self,
            BlockKind::BulletListItem { .. } | BlockKind::OrderedListItem { .. }
        )
    }

    pub fn list_depth(&self) -> Option<u8> {
        match self {
            BlockKind::BulletListItem { depth } | BlockKind::OrderedListItem { depth, .. } => {
                Some(*depth)
            }
            _ => None,
        }
    }

    pub fn same_family(&self, other: &BlockKind) -> bool {
        match (self, other) {
            (BlockKind::Paragraph, BlockKind::Paragraph) => true,
            (BlockKind::Heading(a), BlockKind::Heading(b)) => a == b,
            (BlockKind::Blockquote, BlockKind::Blockquote) => true,
            (BlockKind::BulletListItem { .. }, BlockKind::BulletListItem { .. }) => true,
            (BlockKind::OrderedListItem { .. }, BlockKind::OrderedListItem { .. }) => true,
            (BlockKind::CodeBlock { .. }, BlockKind::CodeBlock { .. }) => true,
            (BlockKind::HorizontalRule, BlockKind::HorizontalRule) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    #[serde(default = "next_block_id")]
    pub id: u64,
    pub kind: BlockKind,
    pub direction: TextDirection,
    pub inlines: Vec<Inline>,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            id: next_block_id(),
            kind,
            direction: TextDirection::Auto,
            inlines: Vec::new(),
        }
    }

    pub fn paragraph_with_text(text: impl Into<String>) -> Self {
        let mut block = Self::new(BlockKind::Paragraph);
        let s = text.into();
        if !s.is_empty() {
            block.inlines.push(Inline::Text(TextRun::plain(s)));
        }
        block
    }

    pub fn with_direction(mut self, direction: TextDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn with_inlines(mut self, inlines: Vec<Inline>) -> Self {
        self.inlines = inlines;
        self.normalize();
        self
    }

    pub fn char_len(&self) -> usize {
        if self.kind == BlockKind::HorizontalRule {
            return 0;
        }
        self.inlines.iter().map(Inline::char_len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.char_len() == 0
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for inline in &self.inlines {
            match inline {
                Inline::Text(run) => out.push_str(&run.text),
                Inline::LineBreak => out.push('\n'),
                Inline::Checkbox { checked } => {
                    out.push_str(if *checked { "[X] " } else { "[ ] " });
                }
                Inline::Comment { .. } | Inline::Image { .. } => {}
            }
        }
        out
    }

    pub fn buffer_text(&self) -> String {
        if self.kind == BlockKind::HorizontalRule {
            return "\u{FFFC}".to_string();
        }
        let mut out = String::new();
        for inline in &self.inlines {
            match inline {
                Inline::Text(run) => out.push_str(&run.text),
                Inline::LineBreak => out.push('\u{2028}'),
                Inline::Checkbox { .. } | Inline::Comment { .. } | Inline::Image { .. } => {
                    out.push('\u{FFFC}');
                }
            }
        }
        out
    }

    pub fn normalize(&mut self) {
        if self.kind == BlockKind::HorizontalRule {
            self.inlines.clear();
            return;
        }
        let mut merged: Vec<Inline> = Vec::with_capacity(self.inlines.len());
        for inline in self.inlines.drain(..) {
            match inline {
                Inline::Text(run) => {
                    if run.text.is_empty() {
                        continue;
                    }
                    if let Some(Inline::Text(prev)) = merged.last_mut()
                        && prev.style == run.style
                    {
                        prev.text.push_str(&run.text);
                        continue;
                    }
                    merged.push(Inline::Text(run));
                }
                other => merged.push(other),
            }
        }
        self.inlines = merged;
    }

    pub fn style_at(&self, offset: usize) -> InlineStyle {
        if self.inlines.is_empty() {
            return InlineStyle::default();
        }
        let probe = offset.saturating_sub(1);
        let mut cursor = 0usize;
        for inline in &self.inlines {
            let len = inline.char_len();
            if probe < cursor + len {
                if let Inline::Text(run) = inline {
                    let mut style = run.style.clone();
                    if offset == cursor + len && style.link.is_some() {
                        style.link = None;
                    }
                    return style;
                }
                return InlineStyle::default();
            }
            cursor += len;
        }
        InlineStyle::default()
    }

    pub fn inline_at(&self, offset: usize) -> Option<(usize, &Inline)> {
        let mut cursor = 0usize;
        for (idx, inline) in self.inlines.iter().enumerate() {
            let len = inline.char_len();
            if offset >= cursor && offset < cursor + len {
                return Some((idx, inline));
            }
            cursor += len;
        }
        None
    }

    pub fn insert_text(&mut self, offset: usize, text: &str, style: InlineStyle) {
        if text.is_empty() || self.kind == BlockKind::HorizontalRule {
            return;
        }
        let target = offset.min(self.char_len());
        let (left, right) = split_inlines(&self.inlines, target);
        let mut next = left;
        next.push(Inline::Text(TextRun::new(text, style)));
        next.extend(right);
        self.inlines = next;
        self.normalize();
    }

    pub fn insert_inline(&mut self, offset: usize, item: Inline) {
        if self.kind == BlockKind::HorizontalRule {
            return;
        }
        let target = offset.min(self.char_len());
        let (left, right) = split_inlines(&self.inlines, target);
        let mut next = left;
        next.push(item);
        next.extend(right);
        self.inlines = next;
        self.normalize();
    }

    pub fn delete_range(&mut self, start: usize, end: usize) -> Vec<Inline> {
        let len = self.char_len();
        let s = start.min(len);
        let e = end.min(len);
        if s >= e {
            return Vec::new();
        }
        let (left, rest) = split_inlines(&self.inlines, s);
        let (removed, right) = split_inlines(&rest, e - s);
        let mut next = left;
        next.extend(right);
        self.inlines = next;
        self.normalize();
        removed
    }

    pub fn slice_inlines(&self, start: usize, end: usize) -> Vec<Inline> {
        let len = self.char_len();
        let s = start.min(len);
        let e = end.min(len);
        if s >= e {
            return Vec::new();
        }
        let (_, rest) = split_inlines(&self.inlines, s);
        let (mid, _) = split_inlines(&rest, e - s);
        mid
    }

    pub fn apply_style_range(&mut self, start: usize, end: usize, attr: &InlineAttr, value: bool) {
        let len = self.char_len();
        let s = start.min(len);
        let e = end.min(len);
        if s >= e {
            return;
        }
        let (left, rest) = split_inlines(&self.inlines, s);
        let (mut mid, right) = split_inlines(&rest, e - s);
        for inline in &mut mid {
            if let Inline::Text(run) = inline {
                run.style.set_attr(attr, value);
            }
        }
        let mut next = left;
        next.extend(mid);
        next.extend(right);
        self.inlines = next;
        self.normalize();
    }

    pub fn replace_range(
        &mut self,
        start: usize,
        end: usize,
        replacement: Vec<Inline>,
    ) -> Vec<Inline> {
        let len = self.char_len();
        let s = start.min(len);
        let e = end.min(len);
        let (left, rest) = split_inlines(&self.inlines, s);
        let (removed, right) = split_inlines(&rest, e.saturating_sub(s));
        let mut next = left;
        next.extend(replacement);
        next.extend(right);
        self.inlines = next;
        self.normalize();
        removed
    }

    pub fn split_at(&mut self, offset: usize) -> Vec<Inline> {
        let target = offset.min(self.char_len());
        let (left, right) = split_inlines(&self.inlines, target);
        self.inlines = left;
        self.normalize();
        right
    }
}

pub fn split_inlines(inlines: &[Inline], at: usize) -> (Vec<Inline>, Vec<Inline>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut cursor = 0usize;

    for inline in inlines {
        let len = inline.char_len();
        if cursor + len <= at {
            left.push(inline.clone());
        } else if cursor >= at {
            right.push(inline.clone());
        } else if let Inline::Text(run) = inline {
            let split_chars = at - cursor;
            let byte_idx = char_to_byte_idx(&run.text, split_chars);
            let l_str = &run.text[..byte_idx];
            let r_str = &run.text[byte_idx..];
            if !l_str.is_empty() {
                left.push(Inline::Text(TextRun::new(l_str, run.style.clone())));
            }
            if !r_str.is_empty() {
                right.push(Inline::Text(TextRun::new(r_str, run.style.clone())));
            }
        }
        cursor += len;
    }

    (left, right)
}

pub fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocPos {
    pub block: usize,
    pub offset: usize,
}

impl DocPos {
    pub const fn new(block: usize, offset: usize) -> Self {
        Self { block, offset }
    }
}

impl Ord for DocPos {
    fn cmp(&self, other: &Self) -> Ordering {
        self.block
            .cmp(&other.block)
            .then_with(|| self.offset.cmp(&other.offset))
    }
}

impl PartialOrd for DocPos {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: DocPos,
    pub focus: DocPos,
}

impl Selection {
    pub const fn collapsed(pos: DocPos) -> Self {
        Self {
            anchor: pos,
            focus: pos,
        }
    }

    pub const fn range(anchor: DocPos, focus: DocPos) -> Self {
        Self { anchor, focus }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    pub fn ordered(&self) -> (DocPos, DocPos) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    pub fn start(&self) -> DocPos {
        self.ordered().0
    }

    pub fn end(&self) -> DocPos {
        self.ordered().1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentFragment {
    pub blocks: Vec<Block>,
}

impl DocumentFragment {
    pub fn from_plain_text(text: &str) -> Self {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let blocks = normalized
            .split('\n')
            .map(Block::paragraph_with_text)
            .collect();
        Self { blocks }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() || (self.blocks.len() == 1 && self.blocks[0].is_empty())
    }

    pub fn plain_text(&self, double_newlines: bool) -> String {
        let sep = if double_newlines { "\n\n" } else { "\n" };
        self.blocks
            .iter()
            .map(Block::plain_text)
            .collect::<Vec<_>>()
            .join(sep)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new_empty(TextDirection::Auto)
    }
}

impl Document {
    pub fn new_empty(default_dir: TextDirection) -> Self {
        Self {
            blocks: vec![Block::new(BlockKind::Paragraph).with_direction(default_dir)],
        }
    }

    pub fn from_blocks(mut blocks: Vec<Block>) -> Self {
        if blocks.is_empty() {
            blocks.push(Block::new(BlockKind::Paragraph));
        }
        for b in &mut blocks {
            b.normalize();
        }
        let mut doc = Self { blocks };
        doc.renumber_ordered_lists();
        doc
    }

    pub fn clamp_pos(&self, pos: DocPos) -> DocPos {
        if self.blocks.is_empty() {
            return DocPos::new(0, 0);
        }
        let block = pos.block.min(self.blocks.len() - 1);
        let offset = pos.offset.min(self.blocks[block].char_len());
        DocPos::new(block, offset)
    }

    pub fn clamp_selection(&self, sel: Selection) -> Selection {
        Selection {
            anchor: self.clamp_pos(sel.anchor),
            focus: self.clamp_pos(sel.focus),
        }
    }

    pub fn flat_offset_to_pos(&self, mut flat: usize) -> DocPos {
        for (i, b) in self.blocks.iter().enumerate() {
            let len = b.char_len();
            if flat <= len {
                return DocPos::new(i, flat);
            }
            flat = flat.saturating_sub(len + 1);
        }
        let last = self.blocks.len().saturating_sub(1);
        DocPos::new(last, self.blocks.get(last).map_or(0, Block::char_len))
    }

    pub fn pos_to_flat_offset(&self, pos: DocPos) -> usize {
        let clamped = self.clamp_pos(pos);
        let mut total = 0usize;
        for block in self.blocks.iter().take(clamped.block) {
            total += block.char_len() + 1;
        }
        total + clamped.offset
    }

    pub fn is_trivially_empty(&self) -> bool {
        self.blocks.len() == 1
            && self.blocks[0].kind == BlockKind::Paragraph
            && self.blocks[0].is_empty()
    }

    pub fn derived_title(&self) -> Option<String> {
        for block in &self.blocks {
            let text = block.plain_text();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
                let truncated: String = first_line.chars().take(60).collect();
                if !truncated.is_empty() {
                    return Some(truncated);
                }
            }
        }
        None
    }

    pub fn plain_text(&self, double_newlines: bool) -> String {
        let sep = if double_newlines { "\n\n" } else { "\n" };
        self.blocks
            .iter()
            .map(Block::plain_text)
            .collect::<Vec<_>>()
            .join(sep)
    }

    pub fn renumber_ordered_lists(&mut self) {
        let mut counters: Vec<u32> = Vec::new();
        for block in &mut self.blocks {
            match &mut block.kind {
                BlockKind::OrderedListItem { depth, number } => {
                    let d = *depth as usize;
                    if counters.len() <= d {
                        counters.resize(d + 1, 0);
                    }
                    counters.truncate(d + 1);
                    counters[d] += 1;
                    *number = counters[d];
                }
                BlockKind::BulletListItem { depth } => {
                    let d = *depth as usize;
                    if counters.len() > d {
                        counters.truncate(d);
                    }
                }
                _ => {
                    counters.clear();
                }
            }
        }
    }

    pub fn extract_fragment(&self, start: DocPos, end: DocPos) -> DocumentFragment {
        let s = self.clamp_pos(start);
        let e = self.clamp_pos(end);
        if s >= e {
            return DocumentFragment { blocks: Vec::new() };
        }
        if s.block == e.block {
            let orig = &self.blocks[s.block];
            let mut b = Block::new(orig.kind.clone()).with_direction(orig.direction);
            b.inlines = orig.slice_inlines(s.offset, e.offset);
            b.normalize();
            return DocumentFragment { blocks: vec![b] };
        }

        let mut blocks = Vec::with_capacity(e.block - s.block + 1);
        let first = &self.blocks[s.block];
        blocks.push(
            Block::new(first.kind.clone())
                .with_direction(first.direction)
                .with_inlines(first.slice_inlines(s.offset, first.char_len())),
        );
        for block in self.blocks.iter().take(e.block).skip(s.block + 1) {
            blocks.push(block.clone());
        }
        let last = &self.blocks[e.block];
        blocks.push(
            Block::new(last.kind.clone())
                .with_direction(last.direction)
                .with_inlines(last.slice_inlines(0, e.offset)),
        );
        DocumentFragment { blocks }
    }

    pub fn selection_has_attr(&self, sel: Selection, attr: &InlineAttr) -> bool {
        let (s, e) = sel.ordered();
        if s == e {
            return self
                .blocks
                .get(s.block)
                .map(|b| b.style_at(s.offset).has_attr(attr))
                .unwrap_or(false);
        }
        let frag = self.extract_fragment(s, e);
        let mut saw_text = false;
        for block in &frag.blocks {
            for inline in &block.inlines {
                if let Inline::Text(run) = inline
                    && !run.text.is_empty()
                {
                    saw_text = true;
                    if !run.style.has_attr(attr) {
                        return false;
                    }
                }
            }
        }
        saw_text
    }

    pub fn link_in_selection(&self, sel: Selection) -> Option<String> {
        let (s, e) = sel.ordered();
        if s == e {
            return self
                .blocks
                .get(s.block)
                .and_then(|b| b.style_at(s.offset).link);
        }
        let frag = self.extract_fragment(s, e);
        for block in &frag.blocks {
            for inline in &block.inlines {
                if let Inline::Text(run) = inline
                    && let Some(url) = &run.style.link
                {
                    return Some(url.clone());
                }
            }
        }
        None
    }

    pub fn has_underline(&self) -> bool {
        self.blocks.iter().any(|b| {
            b.inlines
                .iter()
                .any(|i| matches!(i, Inline::Text(r) if r.style.underline))
        })
    }

    pub fn has_rich_formatting(&self) -> bool {
        self.blocks.iter().any(|b| {
            b.kind != BlockKind::Paragraph
                || b.direction != TextDirection::Auto
                || b.inlines.iter().any(|i| match i {
                    Inline::Text(r) => !r.style.is_plain(),
                    Inline::LineBreak => false,
                    Inline::Checkbox { .. } | Inline::Comment { .. } | Inline::Image { .. } => true,
                })
        })
    }
}
