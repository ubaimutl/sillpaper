use crate::model::{Block, InlineStyle, Selection};

const MAX_UNDO_HISTORY: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    InsertText {
        block: usize,
        offset: usize,
        text: String,
        style: InlineStyle,
    },
    ReplaceBlocks {
        start_block: usize,
        old_blocks: Vec<Block>,
        new_blocks: Vec<Block>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoEntry {
    pub op: EditOp,
    pub before_sel: Selection,
    pub after_sel: Selection,
    pub coalesceable: bool,
}

#[derive(Debug, Default, Clone)]
pub struct UndoStack {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    saved_revision: usize,
    current_revision: usize,
    next_revision: usize,
    revision_stack: Vec<usize>,
    redo_revision_stack: Vec<usize>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            saved_revision: 0,
            current_revision: 0,
            next_revision: 1,
            revision_stack: Vec::new(),
            redo_revision_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: UndoEntry) {
        self.redo.clear();
        self.redo_revision_stack.clear();

        if entry.coalesceable
            && let EditOp::InsertText {
                block,
                offset,
                ref text,
                ref style,
            } = entry.op
            && let Some(last) = self.undo.last_mut()
            && last.coalesceable
            && let EditOp::InsertText {
                block: last_block,
                offset: last_offset,
                text: ref mut last_text,
                style: ref last_style,
            } = last.op
            && last_block == block
            && last_offset + last_text.chars().count() == offset
            && last_style == style
        {
            last_text.push_str(text);
            last.after_sel = entry.after_sel;
            let rev = self.next_revision;
            self.next_revision += 1;
            self.current_revision = rev;
            return;
        }

        let prev_rev = self.current_revision;
        let rev = self.next_revision;
        self.next_revision += 1;
        self.revision_stack.push(prev_rev);
        self.current_revision = rev;

        self.undo.push(entry);
        if self.undo.len() > MAX_UNDO_HISTORY {
            self.undo.remove(0);
            if !self.revision_stack.is_empty() {
                self.revision_stack.remove(0);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn pop_undo(&mut self) -> Option<UndoEntry> {
        let entry = self.undo.pop()?;
        self.redo_revision_stack.push(self.current_revision);
        self.current_revision = self.revision_stack.pop().unwrap_or(0);
        self.redo.push(entry.clone());
        Some(entry)
    }

    pub fn pop_redo(&mut self) -> Option<UndoEntry> {
        let entry = self.redo.pop()?;
        self.revision_stack.push(self.current_revision);
        self.current_revision = self.redo_revision_stack.pop().unwrap_or(self.next_revision);
        self.undo.push(entry.clone());
        Some(entry)
    }

    pub fn break_coalescing(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.coalesceable = false;
        }
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.current_revision;
        self.break_coalescing();
    }

    pub fn is_dirty(&self) -> bool {
        self.current_revision != self.saved_revision
    }

    pub fn force_dirty(&mut self) {
        self.saved_revision = usize::MAX;
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.revision_stack.clear();
        self.redo_revision_stack.clear();
        self.saved_revision = 0;
        self.current_revision = 0;
        self.next_revision = 1;
    }
}

pub fn is_word_boundary_text(text: &str) -> bool {
    if text.chars().count() != 1 {
        return true;
    }
    matches!(
        text.chars().next(),
        Some(' ' | '\t' | '\n' | '.' | ',' | ';' | ':' | '!' | '?')
    )
}
