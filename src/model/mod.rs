pub mod document;
pub mod search;
pub mod stats;

pub use document::{
    Block, BlockKind, DocPos, Document, DocumentFragment, Inline, InlineAttr, InlineStyle,
    Selection, TextDirection, TextRun,
};
pub use search::{SearchMatch, find_matches};
pub use stats::{DocumentStats, TocEntry, compute_stats, extract_toc, heading_slug};
