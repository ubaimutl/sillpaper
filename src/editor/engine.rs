use crate::config::Preferences;
use crate::editor::smart_typing::{
    SmartAction, code_fence_trigger, detect_smart_typing, is_horizontal_rule_trigger,
    wrap_pair_for_char,
};
use crate::editor::undo::{EditOp, UndoEntry, UndoStack, is_word_boundary_text};
use crate::model::{
    Block, BlockKind, DocPos, Document, DocumentFragment, Inline, InlineAttr, InlineStyle,
    Selection, TextDirection, TextRun,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferPatch {
    None,
    InsertInBlock {
        block: usize,
        offset: usize,
        text: String,
    },
    SpliceBlocks {
        start_block: usize,
        old_count: usize,
        new_count: usize,
    },
    RestyleBlocks {
        start_block: usize,
        count: usize,
    },
    FullReload,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolbarState {
    pub heading: Option<u8>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: Option<String>,
    pub blockquote: bool,
    pub bullet_list: bool,
    pub ordered_list: bool,
    pub can_heading: bool,
    pub can_inline: bool,
    pub can_blockquote: bool,
    pub can_list: bool,
}

#[derive(Debug, Clone)]
pub struct EditorState {
    pub doc: Document,
    pub selection: Selection,
    pub pending_style: Option<InlineStyle>,
    pub undo: UndoStack,
    pub default_direction: TextDirection,
}

impl EditorState {
    pub fn new(default_direction: TextDirection) -> Self {
        Self {
            doc: Document::new_empty(default_direction),
            selection: Selection::collapsed(DocPos::new(0, 0)),
            pending_style: None,
            undo: UndoStack::new(),
            default_direction,
        }
    }

    pub fn from_document(doc: Document, default_direction: TextDirection) -> Self {
        Self {
            doc,
            selection: Selection::collapsed(DocPos::new(0, 0)),
            pending_style: None,
            undo: UndoStack::new(),
            default_direction,
        }
    }

    pub fn set_selection(&mut self, sel: Selection) {
        let clamped = self.doc.clamp_selection(sel);
        if clamped != self.selection {
            self.selection = clamped;
            self.pending_style = None;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.undo.is_dirty()
    }

    pub fn mark_saved(&mut self) {
        self.undo.mark_saved();
    }

    pub fn active_style(&self) -> InlineStyle {
        if let Some(style) = &self.pending_style {
            return style.clone();
        }
        let pos = self.selection.start();
        self.doc
            .blocks
            .get(pos.block)
            .map(|b| b.style_at(pos.offset))
            .unwrap_or_default()
    }

    pub fn toolbar_state(&self) -> ToolbarState {
        let (s, e) = self.selection.ordered();
        let blocks = &self.doc.blocks[s.block..=e.block.min(self.doc.blocks.len() - 1)];

        let mut intersects_heading = false;
        let mut intersects_code_or_hr = false;
        let mut all_p_or_heading = true;
        let mut all_p_or_quote = true;
        let mut common_heading: Option<Option<u8>> = None;
        let mut all_quote = true;
        let mut all_ul = true;
        let mut all_ol = true;

        for b in blocks {
            match b.kind {
                BlockKind::Paragraph => {
                    common_heading = Some(None);
                    all_quote = false;
                    all_ul = false;
                    all_ol = false;
                }
                BlockKind::Heading(lvl) => {
                    intersects_heading = true;
                    all_p_or_quote = false;
                    all_quote = false;
                    all_ul = false;
                    all_ol = false;
                    common_heading = match common_heading {
                        None => Some(Some(lvl)),
                        Some(Some(prev)) if prev == lvl => Some(Some(lvl)),
                        _ => Some(None),
                    };
                }
                BlockKind::Blockquote => {
                    all_p_or_heading = false;
                    common_heading = Some(None);
                    all_ul = false;
                    all_ol = false;
                }
                BlockKind::BulletListItem { .. } => {
                    all_p_or_heading = false;
                    all_p_or_quote = false;
                    common_heading = Some(None);
                    all_quote = false;
                    all_ol = false;
                }
                BlockKind::OrderedListItem { .. } => {
                    all_p_or_heading = false;
                    all_p_or_quote = false;
                    common_heading = Some(None);
                    all_quote = false;
                    all_ul = false;
                }
                BlockKind::CodeBlock { .. } | BlockKind::HorizontalRule => {
                    intersects_code_or_hr = true;
                    all_p_or_heading = false;
                    all_p_or_quote = false;
                    common_heading = Some(None);
                    all_quote = false;
                    all_ul = false;
                    all_ol = false;
                }
            }
        }

        let bold = if self.selection.is_collapsed() {
            self.active_style().bold
        } else {
            self.doc
                .selection_has_attr(self.selection, &InlineAttr::Bold)
        };
        let italic = if self.selection.is_collapsed() {
            self.active_style().italic
        } else {
            self.doc
                .selection_has_attr(self.selection, &InlineAttr::Italic)
        };
        let underline = if self.selection.is_collapsed() {
            self.active_style().underline
        } else {
            self.doc
                .selection_has_attr(self.selection, &InlineAttr::Underline)
        };
        let strikethrough = if self.selection.is_collapsed() {
            self.active_style().strikethrough
        } else {
            self.doc
                .selection_has_attr(self.selection, &InlineAttr::Strikethrough)
        };
        let code = if self.selection.is_collapsed() {
            self.active_style().code
        } else {
            self.doc
                .selection_has_attr(self.selection, &InlineAttr::Code)
        };

        ToolbarState {
            heading: common_heading.flatten(),
            bold,
            italic,
            underline,
            strikethrough,
            code,
            link: self.doc.link_in_selection(self.selection),
            blockquote: all_quote && !blocks.is_empty(),
            bullet_list: all_ul && !blocks.is_empty(),
            ordered_list: all_ol && !blocks.is_empty(),
            can_heading: all_p_or_heading && !intersects_code_or_hr,
            can_inline: !intersects_code_or_hr,
            can_blockquote: all_p_or_quote && !intersects_heading && !intersects_code_or_hr,
            can_list: !intersects_heading && !intersects_code_or_hr,
        }
    }

    pub fn insert_text(&mut self, text: &str, prefs: Option<&Preferences>) -> BufferPatch {
        if text.is_empty() {
            return BufferPatch::None;
        }

        if let Some(p) = prefs
            && !self.selection.is_collapsed()
            && text.chars().count() == 1
            && let Some(ch) = text.chars().next()
        {
            let in_code = matches!(
                self.doc.blocks[self.selection.start().block].kind,
                BlockKind::CodeBlock { .. }
            );
            let pair = if in_code {
                match ch {
                    '"' => Some(("\"".into(), "\"".into())),
                    '\'' => Some(("'".into(), "'".into())),
                    '(' | ')' => Some(("(".into(), ")".into())),
                    '[' | ']' => Some(("[".into(), "]".into())),
                    _ => None,
                }
            } else {
                wrap_pair_for_char(ch, p)
            };
            if let Some((open, close)) = pair {
                return self.wrap_selection(&open, &close);
            }
        }

        if text.contains('\n') || text.contains('\r') {
            return self.paste_plain_text(text);
        }

        if !self.selection.is_collapsed() {
            let style = self.active_style();
            let mut frag = DocumentFragment {
                blocks: vec![
                    Block::new(BlockKind::Paragraph)
                        .with_inlines(vec![Inline::Text(TextRun::new(text, style))]),
                ],
            };
            if let Some(first) = frag.blocks.first_mut() {
                first.kind = self.doc.blocks[self.selection.start().block].kind.clone();
            }
            return self.replace_selection_with_fragment(frag);
        }

        let pos = self.selection.start();
        if self.doc.blocks[pos.block].kind == BlockKind::HorizontalRule {
            self.insert_paragraph_after_hr(pos.block);
        }

        let pos = self.selection.start();
        let style = self.active_style();
        self.pending_style = None;

        let before_sel = self.selection;
        let inserted_chars = text.chars().count();
        self.doc.blocks[pos.block].insert_text(pos.offset, text, style.clone());
        let after_pos = DocPos::new(pos.block, pos.offset + inserted_chars);
        self.selection = Selection::collapsed(after_pos);

        self.undo.push(UndoEntry {
            op: EditOp::InsertText {
                block: pos.block,
                offset: pos.offset,
                text: text.to_string(),
                style,
            },
            before_sel,
            after_sel: self.selection,
            coalesceable: !is_word_boundary_text(text),
        });

        if let Some(p) = prefs
            && inserted_chars == 1
            && let Some(ch) = text.chars().next()
            && let Some(action) =
                detect_smart_typing(&self.doc.blocks[pos.block], after_pos.offset, ch, p)
        {
            return self.apply_smart_action(pos.block, action);
        }

        BufferPatch::InsertInBlock {
            block: pos.block,
            offset: pos.offset,
            text: text.to_string(),
        }
    }

    fn apply_smart_action(&mut self, block_idx: usize, action: SmartAction) -> BufferPatch {
        self.undo.break_coalescing();
        let before_sel = self.selection;
        let old_block = self.doc.blocks[block_idx].clone();
        let mut new_block = old_block.clone();
        let new_cursor;

        match action {
            SmartAction::ReplaceText { start, end, text } => {
                let style = new_block.style_at(start);
                let rep_len = text.chars().count();
                new_block.replace_range(start, end, vec![Inline::Text(TextRun::new(text, style))]);
                new_cursor = DocPos::new(block_idx, start + rep_len);
            }
            SmartAction::ConvertBlock { prefix_len, kind } => {
                new_block.delete_range(0, prefix_len);
                new_block.kind = kind;
                new_cursor = DocPos::new(
                    block_idx,
                    before_sel.focus.offset.saturating_sub(prefix_len),
                );
            }
            SmartAction::ReplaceWithInline {
                start,
                end,
                inline,
                trailing_space,
            } => {
                let mut replacement = vec![inline];
                let mut added = 1;
                if trailing_space {
                    let style = new_block.style_at(start);
                    replacement.push(Inline::Text(TextRun::new(" ", style)));
                    added += 1;
                }
                new_block.replace_range(start, end, replacement);
                new_cursor = DocPos::new(block_idx, start + added);
            }
            SmartAction::FormatDelimited {
                outer_start,
                outer_end,
                delim_len,
                attrs,
            } => {
                let inner_start = outer_start + delim_len;
                let inner_end = outer_end - delim_len;
                let mut inner = new_block.slice_inlines(inner_start, inner_end);
                for inline in &mut inner {
                    if let Inline::Text(run) = inline {
                        for attr in &attrs {
                            run.style.set_attr(attr, true);
                        }
                    }
                }
                let inner_len: usize = inner.iter().map(Inline::char_len).sum();
                new_block.replace_range(outer_start, outer_end, inner);
                new_cursor = DocPos::new(block_idx, outer_start + inner_len);
                let mut next_style = new_block.style_at(outer_start);
                for attr in &attrs {
                    next_style.set_attr(attr, false);
                }
                self.pending_style = Some(next_style);
            }
        }

        self.doc.blocks[block_idx] = new_block.clone();
        self.doc.renumber_ordered_lists();
        self.selection = Selection::collapsed(new_cursor);

        self.undo.push(UndoEntry {
            op: EditOp::ReplaceBlocks {
                start_block: block_idx,
                old_blocks: vec![old_block],
                new_blocks: vec![self.doc.blocks[block_idx].clone()],
            },
            before_sel,
            after_sel: self.selection,
            coalesceable: false,
        });

        BufferPatch::SpliceBlocks {
            start_block: block_idx,
            old_count: 1,
            new_count: 1,
        }
    }

    fn wrap_selection(&mut self, open: &str, close: &str) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();
        let mut new_blocks = old_blocks.clone();

        let open_len = open.chars().count();
        let style_start = new_blocks[0].style_at(s.offset);
        let end_idx = new_blocks.len() - 1;
        let style_end = new_blocks[end_idx].style_at(e.offset);

        if s.block == e.block {
            new_blocks[0].insert_text(e.offset, close, style_end);
            new_blocks[0].insert_text(s.offset, open, style_start);
        } else {
            new_blocks[end_idx].insert_text(e.offset, close, style_end);
            new_blocks[0].insert_text(s.offset, open, style_start);
        }

        let new_anchor = DocPos::new(s.block, s.offset + open_len);
        let new_focus = if s.block == e.block {
            DocPos::new(e.block, e.offset + open_len)
        } else {
            e
        };
        let new_sel = Selection::range(new_anchor, new_focus);
        self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, new_sel)
    }

    pub fn insert_paragraph(&mut self) -> BufferPatch {
        self.pending_style = None;
        if !self.selection.is_collapsed() {
            self.delete_selection();
        }
        let pos = self.selection.start();
        let before_sel = self.selection;
        let old_block = self.doc.blocks[pos.block].clone();

        if is_horizontal_rule_trigger(&old_block) {
            let hr = Block::new(BlockKind::HorizontalRule);
            let next_p = Block::new(BlockKind::Paragraph).with_direction(old_block.direction);
            let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
            return self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![hr, next_p],
                before_sel,
                new_sel,
            );
        }

        if let Some(language) = code_fence_trigger(&old_block) {
            let code_block =
                Block::new(BlockKind::CodeBlock { language }).with_direction(TextDirection::Ltr);
            let new_sel = Selection::collapsed(DocPos::new(pos.block, 0));
            return self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![code_block],
                before_sel,
                new_sel,
            );
        }

        if matches!(old_block.kind, BlockKind::CodeBlock { .. }) {
            if old_block.is_empty() {
                let mut replacement = old_block.clone();
                replacement.kind = BlockKind::Paragraph;
                let new_sel = Selection::collapsed(DocPos::new(pos.block, 0));
                return self.apply_block_splice(
                    pos.block,
                    vec![old_block],
                    vec![replacement],
                    before_sel,
                    new_sel,
                );
            }

            let buf_text = old_block.buffer_text();
            let chars: Vec<char> = buf_text.chars().collect();
            let line_start = chars[..pos.offset]
                .iter()
                .rposition(|&c| c == '\u{2028}')
                .map(|idx| idx + 1)
                .unwrap_or(0);
            let cur_line_is_blank = chars[line_start..pos.offset]
                .iter()
                .all(|&c| c == ' ' || c == '\t');

            if pos.offset == chars.len() && line_start > 0 && cur_line_is_blank {
                let mut trimmed = old_block.clone();
                trimmed.delete_range(line_start - 1, chars.len());
                let next_p =
                    Block::new(BlockKind::Paragraph).with_direction(self.default_direction);
                let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
                return self.apply_block_splice(
                    pos.block,
                    vec![old_block],
                    vec![trimmed, next_p],
                    before_sel,
                    new_sel,
                );
            }

            let indent: String = if cur_line_is_blank {
                String::new()
            } else {
                chars[line_start..pos.offset]
                    .iter()
                    .take_while(|&&c| c == ' ' || c == '\t')
                    .collect()
            };
            let indent_len = indent.chars().count();
            let mut replacement = vec![Inline::LineBreak];
            if !indent.is_empty() {
                replacement.push(Inline::Text(TextRun::plain(indent)));
            }
            let mut updated = old_block.clone();
            updated.replace_range(pos.offset, pos.offset, replacement);
            let new_sel = Selection::collapsed(DocPos::new(pos.block, pos.offset + 1 + indent_len));
            return self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![updated],
                before_sel,
                new_sel,
            );
        }

        if old_block.is_empty()
            && matches!(
                old_block.kind,
                BlockKind::Blockquote
                    | BlockKind::BulletListItem { .. }
                    | BlockKind::OrderedListItem { .. }
            )
        {
            let mut replacement = old_block.clone();
            replacement.kind = BlockKind::Paragraph;
            let new_sel = Selection::collapsed(DocPos::new(pos.block, 0));
            return self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![replacement],
                before_sel,
                new_sel,
            );
        }

        if old_block.kind == BlockKind::HorizontalRule {
            let next_p = Block::new(BlockKind::Paragraph).with_direction(self.default_direction);
            let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
            return self.apply_block_splice(
                pos.block,
                vec![old_block.clone()],
                vec![old_block, next_p],
                before_sel,
                new_sel,
            );
        }

        let mut left = old_block.clone();
        let right_inlines = left.split_at(pos.offset);

        let next_kind = match &old_block.kind {
            BlockKind::Heading(_) | BlockKind::Blockquote => BlockKind::Paragraph,
            BlockKind::BulletListItem { depth } => BlockKind::BulletListItem { depth: *depth },
            BlockKind::OrderedListItem { depth, number } => BlockKind::OrderedListItem {
                depth: *depth,
                number: number + 1,
            },
            BlockKind::CodeBlock { language } => BlockKind::CodeBlock {
                language: language.clone(),
            },
            BlockKind::Paragraph | BlockKind::HorizontalRule => BlockKind::Paragraph,
        };

        let right = Block::new(next_kind)
            .with_direction(old_block.direction)
            .with_inlines(right_inlines);

        let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
        self.apply_block_splice(
            pos.block,
            vec![old_block],
            vec![left, right],
            before_sel,
            new_sel,
        )
    }

    fn insert_paragraph_after_hr(&mut self, hr_block: usize) {
        let before_sel = self.selection;
        let hr = self.doc.blocks[hr_block].clone();
        let p = Block::new(BlockKind::Paragraph).with_direction(self.default_direction);
        let new_sel = Selection::collapsed(DocPos::new(hr_block + 1, 0));
        self.apply_block_splice(hr_block, vec![hr.clone()], vec![hr, p], before_sel, new_sel);
    }

    pub fn insert_soft_break(&mut self) -> BufferPatch {
        self.insert_inline_object(Inline::LineBreak)
    }

    pub fn exit_code_block_below(&mut self) -> Option<BufferPatch> {
        let pos = self.selection.end();
        let old_block = self.doc.blocks.get(pos.block)?.clone();
        if !matches!(old_block.kind, BlockKind::CodeBlock { .. }) {
            return None;
        }
        let before_sel = self.selection;
        let next_p = Block::new(BlockKind::Paragraph).with_direction(self.default_direction);
        let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
        Some(self.apply_block_splice(
            pos.block,
            vec![old_block.clone()],
            vec![old_block, next_p],
            before_sel,
            new_sel,
        ))
    }

    pub fn indent_code_block(&mut self, increase: bool) -> Option<BufferPatch> {
        let (s, e) = self.selection.ordered();
        if s.block != e.block {
            return None;
        }
        let old_block = self.doc.blocks.get(s.block)?.clone();
        if !matches!(old_block.kind, BlockKind::CodeBlock { .. }) {
            return None;
        }
        let before_sel = self.selection;
        if increase {
            let mut updated = old_block.clone();
            updated.replace_range(
                s.offset,
                e.offset,
                vec![Inline::Text(TextRun::plain("    "))],
            );
            let new_sel = Selection::collapsed(DocPos::new(s.block, s.offset + 4));
            return Some(self.apply_block_splice(
                s.block,
                vec![old_block],
                vec![updated],
                before_sel,
                new_sel,
            ));
        }

        let buf_text = old_block.buffer_text();
        let chars: Vec<char> = buf_text.chars().collect();
        let line_start = chars[..s.offset]
            .iter()
            .rposition(|&c| c == '\u{2028}')
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let remove_len = if chars.get(line_start) == Some(&'\t') {
            1
        } else {
            chars[line_start..]
                .iter()
                .take(4)
                .take_while(|&&c| c == ' ')
                .count()
        };
        if remove_len == 0 {
            return Some(BufferPatch::None);
        }
        let mut updated = old_block.clone();
        updated.delete_range(line_start, line_start + remove_len);
        let new_s = if s.offset >= line_start + remove_len {
            s.offset - remove_len
        } else {
            line_start
        };
        let new_e = if e.offset >= line_start + remove_len {
            e.offset - remove_len
        } else {
            line_start
        };
        let new_sel = Selection::range(DocPos::new(s.block, new_s), DocPos::new(e.block, new_e));
        Some(self.apply_block_splice(s.block, vec![old_block], vec![updated], before_sel, new_sel))
    }

    pub fn insert_inline_object(&mut self, item: Inline) -> BufferPatch {
        if !self.selection.is_collapsed() {
            self.delete_selection();
        }
        let pos = self.selection.start();
        if self.doc.blocks[pos.block].kind == BlockKind::HorizontalRule {
            self.insert_paragraph_after_hr(pos.block);
        }
        let pos = self.selection.start();
        let before_sel = self.selection;
        let old_block = self.doc.blocks[pos.block].clone();
        let mut new_block = old_block.clone();
        new_block.insert_inline(pos.offset, item);
        let new_sel = Selection::collapsed(DocPos::new(pos.block, pos.offset + 1));
        self.apply_block_splice(
            pos.block,
            vec![old_block],
            vec![new_block],
            before_sel,
            new_sel,
        )
    }

    pub fn insert_horizontal_rule(&mut self) -> BufferPatch {
        if !self.selection.is_collapsed() {
            self.delete_selection();
        }
        let pos = self.selection.start();
        let before_sel = self.selection;
        let old_block = self.doc.blocks[pos.block].clone();
        let hr = Block::new(BlockKind::HorizontalRule);
        let after_p = Block::new(BlockKind::Paragraph).with_direction(old_block.direction);

        if old_block.is_empty() && old_block.kind == BlockKind::Paragraph {
            let new_sel = Selection::collapsed(DocPos::new(pos.block + 1, 0));
            self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![hr, after_p],
                before_sel,
                new_sel,
            )
        } else {
            let new_sel = Selection::collapsed(DocPos::new(pos.block + 2, 0));
            self.apply_block_splice(
                pos.block,
                vec![old_block.clone()],
                vec![old_block, hr, after_p],
                before_sel,
                new_sel,
            )
        }
    }

    pub fn backspace(&mut self) -> BufferPatch {
        self.pending_style = None;
        if !self.selection.is_collapsed() {
            return self.delete_selection();
        }
        let pos = self.selection.start();
        let before_sel = self.selection;

        if pos.offset > 0 {
            let old_block = self.doc.blocks[pos.block].clone();
            let mut new_block = old_block.clone();
            new_block.delete_range(pos.offset - 1, pos.offset);
            let new_sel = Selection::collapsed(DocPos::new(pos.block, pos.offset - 1));
            return self.apply_block_splice(
                pos.block,
                vec![old_block],
                vec![new_block],
                before_sel,
                new_sel,
            );
        }

        let curr = self.doc.blocks[pos.block].clone();
        if !matches!(curr.kind, BlockKind::Paragraph | BlockKind::HorizontalRule) {
            let mut unwrapped = curr.clone();
            unwrapped.kind = match curr.kind {
                BlockKind::BulletListItem { depth } if depth > 0 => {
                    BlockKind::BulletListItem { depth: depth - 1 }
                }
                BlockKind::OrderedListItem { depth, number } if depth > 0 => {
                    BlockKind::OrderedListItem {
                        depth: depth - 1,
                        number,
                    }
                }
                _ => BlockKind::Paragraph,
            };
            let new_sel = Selection::collapsed(DocPos::new(pos.block, 0));
            return self.apply_block_splice(
                pos.block,
                vec![curr],
                vec![unwrapped],
                before_sel,
                new_sel,
            );
        }

        if pos.block == 0 {
            if curr.kind == BlockKind::HorizontalRule {
                let p = Block::new(BlockKind::Paragraph).with_direction(self.default_direction);
                return self.apply_block_splice(
                    0,
                    vec![curr],
                    vec![p],
                    before_sel,
                    Selection::collapsed(DocPos::new(0, 0)),
                );
            }
            return BufferPatch::None;
        }

        let prev = self.doc.blocks[pos.block - 1].clone();
        if prev.kind == BlockKind::HorizontalRule {
            let new_sel = Selection::collapsed(DocPos::new(pos.block - 1, 0));
            return self.apply_block_splice(
                pos.block - 1,
                vec![prev, curr.clone()],
                vec![curr],
                before_sel,
                new_sel,
            );
        }
        if curr.kind == BlockKind::HorizontalRule {
            let new_sel = Selection::collapsed(DocPos::new(pos.block - 1, prev.char_len()));
            return self.apply_block_splice(
                pos.block - 1,
                vec![prev.clone(), curr],
                vec![prev],
                before_sel,
                new_sel,
            );
        }

        let join_offset = prev.char_len();
        let mut merged = prev.clone();
        merged.inlines.extend(curr.inlines.clone());
        merged.normalize();
        let new_sel = Selection::collapsed(DocPos::new(pos.block - 1, join_offset));
        self.apply_block_splice(
            pos.block - 1,
            vec![prev, curr],
            vec![merged],
            before_sel,
            new_sel,
        )
    }

    pub fn delete_forward(&mut self) -> BufferPatch {
        self.pending_style = None;
        if !self.selection.is_collapsed() {
            return self.delete_selection();
        }
        let pos = self.selection.start();
        let before_sel = self.selection;
        let curr = self.doc.blocks[pos.block].clone();

        if pos.offset < curr.char_len() {
            let mut new_block = curr.clone();
            new_block.delete_range(pos.offset, pos.offset + 1);
            return self.apply_block_splice(
                pos.block,
                vec![curr],
                vec![new_block],
                before_sel,
                before_sel,
            );
        }

        if pos.block + 1 >= self.doc.blocks.len() {
            return BufferPatch::None;
        }

        let next = self.doc.blocks[pos.block + 1].clone();
        if next.kind == BlockKind::HorizontalRule {
            return self.apply_block_splice(
                pos.block,
                vec![curr.clone(), next],
                vec![curr],
                before_sel,
                before_sel,
            );
        }
        if curr.kind == BlockKind::HorizontalRule {
            return self.apply_block_splice(
                pos.block,
                vec![curr, next.clone()],
                vec![next],
                before_sel,
                Selection::collapsed(DocPos::new(pos.block, 0)),
            );
        }

        let mut merged = curr.clone();
        merged.inlines.extend(next.inlines.clone());
        merged.normalize();
        self.apply_block_splice(
            pos.block,
            vec![curr, next],
            vec![merged],
            before_sel,
            before_sel,
        )
    }

    pub fn delete_selection(&mut self) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        if s == e {
            return BufferPatch::None;
        }
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();

        if s.block == e.block {
            let mut nb = old_blocks[0].clone();
            nb.delete_range(s.offset, e.offset);
            let new_sel = Selection::collapsed(s);
            return self.apply_block_splice(s.block, old_blocks, vec![nb], before_sel, new_sel);
        }

        let first = &old_blocks[0];
        let last = old_blocks.last().unwrap();
        let mut merged = Block::new(if first.kind == BlockKind::HorizontalRule {
            BlockKind::Paragraph
        } else {
            first.kind.clone()
        })
        .with_direction(first.direction);
        let mut inlines = first.slice_inlines(0, s.offset);
        inlines.extend(last.slice_inlines(e.offset, last.char_len()));
        merged.inlines = inlines;
        merged.normalize();

        let new_sel = Selection::collapsed(s);
        self.apply_block_splice(s.block, old_blocks, vec![merged], before_sel, new_sel)
    }

    pub fn delete_range(&mut self, start: DocPos, end: DocPos) -> BufferPatch {
        self.selection = Selection::range(start, end);
        self.delete_selection()
    }

    pub fn toggle_inline_attr(&mut self, attr: InlineAttr) -> BufferPatch {
        if self.selection.is_collapsed() {
            let mut style = self.active_style();
            let current = style.has_attr(&attr);
            style.set_attr(&attr, !current);
            self.pending_style = Some(style);
            return BufferPatch::None;
        }

        let enable = !self.doc.selection_has_attr(self.selection, &attr);
        self.set_inline_attr(attr, enable)
    }

    pub fn set_inline_attr(&mut self, attr: InlineAttr, value: bool) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        if s == e {
            let mut style = self.active_style();
            style.set_attr(&attr, value);
            self.pending_style = Some(style);
            return BufferPatch::None;
        }

        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();
        let mut new_blocks = old_blocks.clone();

        for (rel_idx, block) in new_blocks.iter_mut().enumerate() {
            let start = if rel_idx == 0 { s.offset } else { 0 };
            let end = if rel_idx + 1 == (e.block - s.block + 1) {
                e.offset
            } else {
                block.char_len()
            };
            block.apply_style_range(start, end, &attr, value);
        }

        self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, before_sel)
    }

    pub fn set_link(&mut self, url: Option<String>) -> BufferPatch {
        match url.map(|u| normalize_url(&u)).filter(|u| !u.is_empty()) {
            Some(clean) => self.set_inline_attr(InlineAttr::Link(Some(clean)), true),
            None => self.set_inline_attr(InlineAttr::Link(None), false),
        }
    }

    pub fn toggle_block_kind(&mut self, target: BlockKind) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();

        let all_already_target = old_blocks.iter().all(|b| b.kind.same_family(&target));
        let mut new_blocks = old_blocks.clone();

        for block in &mut new_blocks {
            if block.kind == BlockKind::HorizontalRule {
                continue;
            }
            if all_already_target {
                block.kind = BlockKind::Paragraph;
            } else {
                block.kind = target.clone();
            }
        }

        let patch =
            self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, before_sel);
        if target == BlockKind::HorizontalRule {
            patch
        } else {
            BufferPatch::RestyleBlocks {
                start_block: s.block,
                count: e.block - s.block + 1,
            }
        }
    }

    pub fn adjust_list_indent(&mut self, increase: bool) -> Option<BufferPatch> {
        let (s, e) = self.selection.ordered();
        let in_list = self.doc.blocks[s.block..=e.block]
            .iter()
            .all(|b| b.kind.is_list());
        if !in_list {
            return None;
        }
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();
        let mut new_blocks = old_blocks.clone();
        let mut changed = false;

        for block in &mut new_blocks {
            match &mut block.kind {
                BlockKind::BulletListItem { depth } => {
                    let next = if increase {
                        (*depth + 1).min(4)
                    } else {
                        depth.saturating_sub(1)
                    };
                    if next != *depth {
                        *depth = next;
                        changed = true;
                    }
                }
                BlockKind::OrderedListItem { depth, .. } => {
                    let next = if increase {
                        (*depth + 1).min(4)
                    } else {
                        depth.saturating_sub(1)
                    };
                    if next != *depth {
                        *depth = next;
                        changed = true;
                    }
                }
                _ => {}
            }
        }

        if !changed {
            return Some(BufferPatch::None);
        }
        Some(self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, before_sel))
    }

    pub fn set_direction(&mut self, dir: TextDirection) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();
        let mut new_blocks = old_blocks.clone();
        for b in &mut new_blocks {
            b.direction = dir;
        }
        self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, before_sel)
    }

    pub fn toggle_checkbox_at(&mut self, block_idx: usize, char_offset: usize) -> BufferPatch {
        let Some(old_block) = self.doc.blocks.get(block_idx).cloned() else {
            return BufferPatch::None;
        };
        let mut new_block = old_block.clone();
        let mut cursor = 0usize;
        let mut toggled = false;
        for inline in &mut new_block.inlines {
            let len = inline.char_len();
            if cursor == char_offset
                && let Inline::Checkbox { checked } = inline
            {
                *checked = !*checked;
                toggled = true;
                break;
            }
            cursor += len;
        }
        if !toggled {
            return BufferPatch::None;
        }
        let sel = self.selection;
        self.apply_block_splice(block_idx, vec![old_block], vec![new_block], sel, sel)
    }

    pub fn update_comment_at(
        &mut self,
        block_idx: usize,
        char_offset: usize,
        new_text: Option<String>,
    ) -> BufferPatch {
        let Some(old_block) = self.doc.blocks.get(block_idx).cloned() else {
            return BufferPatch::None;
        };
        let mut new_block = old_block.clone();
        let mut cursor = 0usize;
        let mut target_idx = None;
        for (i, inline) in new_block.inlines.iter().enumerate() {
            if cursor == char_offset && matches!(inline, Inline::Comment { .. }) {
                target_idx = Some(i);
                break;
            }
            cursor += inline.char_len();
        }
        let Some(idx) = target_idx else {
            return BufferPatch::None;
        };

        match new_text {
            Some(txt) if !txt.trim().is_empty() => {
                new_block.inlines[idx] = Inline::Comment { text: txt };
            }
            _ => {
                new_block.inlines.remove(idx);
            }
        }
        new_block.normalize();
        let sel = self.doc.clamp_selection(self.selection);
        self.apply_block_splice(
            block_idx,
            vec![old_block],
            vec![new_block],
            self.selection,
            sel,
        )
    }

    pub fn copy_fragment(&self, entire_if_collapsed: bool) -> Option<DocumentFragment> {
        if self.selection.is_collapsed() {
            if entire_if_collapsed {
                return Some(DocumentFragment {
                    blocks: self.doc.blocks.clone(),
                });
            }
            return None;
        }
        let (s, e) = self.selection.ordered();
        Some(self.doc.extract_fragment(s, e))
    }

    pub fn paste_plain_text(&mut self, text: &str) -> BufferPatch {
        let (s, e) = self.selection.ordered();
        let in_code_block = s.block == e.block
            && matches!(self.doc.blocks[s.block].kind, BlockKind::CodeBlock { .. });
        if in_code_block {
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            let mut inlines = Vec::new();
            let lines: Vec<&str> = normalized.split('\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if !line.is_empty() {
                    inlines.push(Inline::Text(TextRun::plain(*line)));
                }
                if idx + 1 < lines.len() {
                    inlines.push(Inline::LineBreak);
                }
            }
            let frag = DocumentFragment {
                blocks: vec![
                    Block::new(self.doc.blocks[s.block].kind.clone()).with_inlines(inlines),
                ],
            };
            return self.replace_selection_with_fragment(frag);
        }

        if !self.selection.is_collapsed() && is_probable_url(text.trim()) {
            return self.set_link(Some(text.trim().to_string()));
        }
        let frag = DocumentFragment::from_plain_text(text);
        self.replace_selection_with_fragment(frag)
    }

    pub fn replace_selection_with_fragment(&mut self, frag: DocumentFragment) -> BufferPatch {
        if frag.blocks.is_empty() {
            return BufferPatch::None;
        }
        let (s, e) = self.selection.ordered();
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks[s.block..=e.block].to_vec();

        let first_orig = &old_blocks[0];
        let last_orig = old_blocks.last().unwrap();
        let prefix = first_orig.slice_inlines(0, s.offset);
        let suffix = last_orig.slice_inlines(e.offset, last_orig.char_len());

        let mut new_blocks: Vec<Block> = Vec::with_capacity(frag.blocks.len());

        if frag.blocks.len() == 1 {
            let pasted = &frag.blocks[0];
            let kind = if first_orig.is_empty()
                && first_orig.kind == BlockKind::Paragraph
                && pasted.kind != BlockKind::Paragraph
            {
                pasted.kind.clone()
            } else if first_orig.kind == BlockKind::HorizontalRule {
                BlockKind::Paragraph
            } else {
                first_orig.kind.clone()
            };
            let mut merged = Block::new(kind).with_direction(first_orig.direction);
            let pasted_len = pasted.char_len();
            let mut inlines = prefix;
            inlines.extend(pasted.inlines.clone());
            inlines.extend(suffix);
            merged.inlines = inlines;
            merged.normalize();
            new_blocks.push(merged);
            let new_sel = Selection::collapsed(DocPos::new(s.block, s.offset + pasted_len));
            return self.apply_block_splice(s.block, old_blocks, new_blocks, before_sel, new_sel);
        }

        for (i, pasted) in frag.blocks.iter().enumerate() {
            if i == 0 {
                let kind = if prefix.is_empty() && first_orig.kind == BlockKind::Paragraph {
                    pasted.kind.clone()
                } else {
                    first_orig.kind.clone()
                };
                let mut b = Block::new(kind).with_direction(first_orig.direction);
                let mut inlines = prefix.clone();
                inlines.extend(pasted.inlines.clone());
                b.inlines = inlines;
                b.normalize();
                new_blocks.push(b);
            } else if i + 1 == frag.blocks.len() {
                let mut b = pasted.clone();
                let end_offset = b.char_len();
                b.inlines.extend(suffix.clone());
                b.normalize();
                new_blocks.push(b);
                let new_sel = Selection::collapsed(DocPos::new(s.block + i, end_offset));
                return self
                    .apply_block_splice(s.block, old_blocks, new_blocks, before_sel, new_sel);
            } else {
                let mut b = pasted.clone();
                b.normalize();
                new_blocks.push(b);
            }
        }

        BufferPatch::None
    }

    pub fn replace_document_undoable(&mut self, new_doc: Document) -> BufferPatch {
        let before_sel = self.selection;
        let old_blocks = self.doc.blocks.clone();
        let new_blocks = new_doc.blocks;
        let new_sel = Selection::collapsed(
            Document::from_blocks(new_blocks.clone()).clamp_pos(before_sel.start()),
        );
        self.apply_block_splice(0, old_blocks, new_blocks, before_sel, new_sel);
        BufferPatch::FullReload
    }

    pub fn undo(&mut self) -> BufferPatch {
        let Some(entry) = self.undo.pop_undo() else {
            return BufferPatch::None;
        };
        self.pending_style = None;
        let patch = match entry.op {
            EditOp::InsertText {
                block,
                offset,
                text,
                ..
            } => {
                let len = text.chars().count();
                if let Some(b) = self.doc.blocks.get_mut(block) {
                    b.delete_range(offset, offset + len);
                }
                BufferPatch::SpliceBlocks {
                    start_block: block,
                    old_count: 1,
                    new_count: 1,
                }
            }
            EditOp::ReplaceBlocks {
                start_block,
                old_blocks,
                new_blocks,
            } => {
                let new_len = new_blocks.len();
                let old_len = old_blocks.len();
                self.doc
                    .blocks
                    .splice(start_block..start_block + new_len, old_blocks);
                self.doc.renumber_ordered_lists();
                BufferPatch::SpliceBlocks {
                    start_block,
                    old_count: new_len,
                    new_count: old_len,
                }
            }
        };
        self.selection = self.doc.clamp_selection(entry.before_sel);
        patch
    }

    pub fn redo(&mut self) -> BufferPatch {
        let Some(entry) = self.undo.pop_redo() else {
            return BufferPatch::None;
        };
        self.pending_style = None;
        let patch = match entry.op {
            EditOp::InsertText {
                block,
                offset,
                text,
                style,
            } => {
                if let Some(b) = self.doc.blocks.get_mut(block) {
                    b.insert_text(offset, &text, style);
                }
                BufferPatch::SpliceBlocks {
                    start_block: block,
                    old_count: 1,
                    new_count: 1,
                }
            }
            EditOp::ReplaceBlocks {
                start_block,
                old_blocks,
                new_blocks,
            } => {
                let old_len = old_blocks.len();
                let new_len = new_blocks.len();
                self.doc
                    .blocks
                    .splice(start_block..start_block + old_len, new_blocks);
                self.doc.renumber_ordered_lists();
                BufferPatch::SpliceBlocks {
                    start_block,
                    old_count: old_len,
                    new_count: new_len,
                }
            }
        };
        self.selection = self.doc.clamp_selection(entry.after_sel);
        patch
    }

    fn apply_block_splice(
        &mut self,
        start_block: usize,
        old_blocks: Vec<Block>,
        new_blocks: Vec<Block>,
        before_sel: Selection,
        after_sel: Selection,
    ) -> BufferPatch {
        let old_count = old_blocks.len();
        let new_count = new_blocks.len();
        self.doc
            .blocks
            .splice(start_block..start_block + old_count, new_blocks);
        if self.doc.blocks.is_empty() {
            self.doc
                .blocks
                .push(Block::new(BlockKind::Paragraph).with_direction(self.default_direction));
        }
        self.doc.renumber_ordered_lists();
        let stored_new = self.doc.blocks[start_block..start_block + new_count].to_vec();
        self.selection = self.doc.clamp_selection(after_sel);

        self.undo.push(UndoEntry {
            op: EditOp::ReplaceBlocks {
                start_block,
                old_blocks,
                new_blocks: stored_new,
            },
            before_sel,
            after_sel: self.selection,
            coalesceable: false,
        });

        BufferPatch::SpliceBlocks {
            start_block,
            old_count,
            new_count,
        }
    }
}

pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with('#')
    {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

pub fn is_probable_url(text: &str) -> bool {
    (text.starts_with("http://") || text.starts_with("https://") || text.starts_with("mailto:"))
        && !text.contains(char::is_whitespace)
        && text.len() > 8
}
