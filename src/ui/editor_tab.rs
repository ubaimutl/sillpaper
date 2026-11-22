use crate::config::{Preferences, resolve_palette};
use crate::editor::{BufferPatch, EditorState, ToolbarState};
use crate::extensions::ExtensionDiagnostic;
use crate::io::backup::{StorageManager, file_mtime_ms};
use crate::io::html::{document_to_html, fragment_to_html, html_to_fragment};
use crate::io::markdown::{document_to_markdown, fragment_to_markdown, markdown_to_document};
use crate::model::{
    BlockKind, DocPos, Document, DocumentFragment, Inline, InlineAttr, SearchMatch, Selection,
    TextDirection, find_matches, heading_slug,
};
use crate::ui::dialogs::{show_comment_dialog, show_image_url_dialog};
use crate::ui::rich_text_view::{BlockDecoration, DecorationColors, RichTextView, rgba_from_css};
use crate::ui::word_counter::WritingGoal;
use adw::prelude::AnimationExt;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::glib::translate::IntoGlib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub enum ExtensionTrigger {
    RequestCompletion {
        manual: bool,
    },
    RequestDiagnostics {
        block: usize,
        manual: bool,
    },
    ShowQuickFix {
        block: usize,
        diagnostic: ExtensionDiagnostic,
    },
    CancelSuggestions,
}

static NEXT_TAB_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn generate_tab_id() -> String {
    let n = NEXT_TAB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = crate::io::backup::now_epoch_ms();
    format!("tab_{ts}_{n}")
}

#[derive(Clone)]
pub struct EditorTabCallbacks {
    pub on_state_changed: Rc<dyn Fn()>,
    pub on_selection_changed: Rc<dyn Fn(bool)>,
    pub on_hide_toolbar: Rc<dyn Fn()>,
    pub on_typing_activity: Rc<dyn Fn()>,
    pub on_external_file_changed: Rc<dyn Fn(String)>,
    pub on_open_link_input: Rc<dyn Fn()>,
    pub on_extension_trigger: Rc<dyn Fn(ExtensionTrigger)>,
    pub on_toast: Rc<dyn Fn(&str)>,
    pub get_window: Rc<dyn Fn() -> Option<gtk4::Window>>,
}

#[derive(Clone)]
struct InlineImageEntry {
    anchor: gtk4::TextChildAnchor,
    picture: gtk4::Picture,
    natural_w: f64,
    natural_h: f64,
    standalone: bool,
}

#[derive(Clone)]
pub struct EditorTab {
    pub id: String,
    pub root: gtk4::Overlay,
    pub scrolled: gtk4::ScrolledWindow,
    pub view: RichTextView,
    pub state: Rc<RefCell<EditorState>>,
    pub file_path: Rc<RefCell<Option<PathBuf>>>,
    pub last_file_mtime_ms: Rc<RefCell<Option<u64>>>,
    pub goal: Rc<RefCell<Option<WritingGoal>>>,
    pub goal_announced: Rc<Cell<bool>>,
    prefs: Rc<RefCell<Preferences>>,
    storage: Rc<StorageManager>,
    callbacks: EditorTabCallbacks,
    is_syncing: Rc<Cell<bool>>,
    pending_smart_sync: Rc<Cell<u32>>,
    is_mouse_selecting: Rc<Cell<bool>>,
    last_focus_block: Rc<Cell<Option<usize>>>,
    last_typewriter_mode: Rc<Cell<bool>>,
    scroll_source: Rc<RefCell<Option<glib::SourceId>>>,
    margin_sync_source: Rc<RefCell<Option<glib::SourceId>>>,
    scroll_anim: Rc<RefCell<Option<adw::TimedAnimation>>>,
    scroll_anim_target: Rc<Cell<Option<f64>>>,
    is_animating_scroll: Rc<Cell<bool>>,
    autosave_source: Rc<RefCell<Option<glib::SourceId>>>,
    backup_write_count: Rc<Cell<u32>>,
    file_monitor: Rc<RefCell<Option<gio::FileMonitor>>>,
    search_matches: Rc<RefCell<Vec<SearchMatch>>>,
    current_match: Rc<Cell<usize>>,
    link_pill: gtk4::Button,
    hovered_link: Rc<RefCell<Option<String>>>,
    pending_link_selection: Rc<RefCell<Option<Selection>>>,
    ghost_suggestion: Rc<RefCell<Option<(usize, usize, String)>>>,
    diagnostics: Rc<RefCell<HashMap<usize, Vec<ExtensionDiagnostic>>>>,
    child_anchors: Rc<RefCell<Vec<gtk4::TextChildAnchor>>>,
    inline_images: Rc<RefCell<Vec<InlineImageEntry>>>,
}

impl EditorTab {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        doc: Document,
        file_path: Option<PathBuf>,
        is_dirty: bool,
        last_mtime: Option<u64>,
        cursor_flat: usize,
        prefs: Rc<RefCell<Preferences>>,
        storage: Rc<StorageManager>,
        callbacks: EditorTabCallbacks,
    ) -> Self {
        Self::new_with_tag_table(
            id,
            doc,
            file_path,
            is_dirty,
            last_mtime,
            cursor_flat,
            prefs,
            storage,
            callbacks,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_tag_table(
        id: String,
        doc: Document,
        file_path: Option<PathBuf>,
        is_dirty: bool,
        last_mtime: Option<u64>,
        cursor_flat: usize,
        prefs: Rc<RefCell<Preferences>>,
        storage: Rc<StorageManager>,
        callbacks: EditorTabCallbacks,
        shared_tag_table: Option<&gtk4::TextTagTable>,
    ) -> Self {
        let default_dir = prefs.borrow().default_direction;
        let mut ed_state = EditorState::from_document(doc, default_dir);
        let initial_pos = ed_state.doc.flat_offset_to_pos(cursor_flat);
        ed_state.selection = Selection::collapsed(initial_pos);
        if is_dirty {
            ed_state.undo.force_dirty();
        }

        let view = RichTextView::new();
        view.set_left_margin(BASE_SIDE_MARGIN);
        view.set_right_margin(BASE_SIDE_MARGIN);
        view.set_top_margin(52);
        let buffer = if let Some(table) = shared_tag_table {
            let b = gtk4::TextBuffer::new(Some(table));
            view.set_buffer(Some(&b));
            b
        } else {
            let b = view.buffer();
            setup_tags(&b, &prefs.borrow());
            b
        };
        buffer.set_enable_undo(false);

        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.add_css_class("sillpaper-editor-scroll");
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        scrolled.set_propagate_natural_width(false);
        scrolled.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Automatic);
        scrolled.set_child(Some(&view));

        let root = gtk4::Overlay::new();
        root.set_child(Some(&scrolled));

        let link_pill = gtk4::Button::with_label("Open link ↗");
        link_pill.add_css_class("sillpaper-link-pill");
        link_pill.set_halign(gtk4::Align::Start);
        link_pill.set_valign(gtk4::Align::End);
        link_pill.set_margin_start(16);
        link_pill.set_margin_bottom(14);
        link_pill.set_visible(false);
        link_pill.set_focus_on_click(false);
        root.add_overlay(&link_pill);

        let tab = Self {
            id,
            root,
            scrolled,
            view,
            state: Rc::new(RefCell::new(ed_state)),
            file_path: Rc::new(RefCell::new(file_path.clone())),
            last_file_mtime_ms: Rc::new(RefCell::new(last_mtime)),
            goal: Rc::new(RefCell::new(None)),
            goal_announced: Rc::new(Cell::new(false)),
            prefs,
            storage,
            callbacks,
            is_syncing: Rc::new(Cell::new(false)),
            pending_smart_sync: Rc::new(Cell::new(0)),
            is_mouse_selecting: Rc::new(Cell::new(false)),
            last_focus_block: Rc::new(Cell::new(None)),
            last_typewriter_mode: Rc::new(Cell::new(false)),
            scroll_source: Rc::new(RefCell::new(None)),
            margin_sync_source: Rc::new(RefCell::new(None)),
            scroll_anim: Rc::new(RefCell::new(None)),
            scroll_anim_target: Rc::new(Cell::new(None)),
            is_animating_scroll: Rc::new(Cell::new(false)),
            autosave_source: Rc::new(RefCell::new(None)),
            backup_write_count: Rc::new(Cell::new(0)),
            file_monitor: Rc::new(RefCell::new(None)),
            search_matches: Rc::new(RefCell::new(Vec::new())),
            current_match: Rc::new(Cell::new(0)),
            link_pill,
            hovered_link: Rc::new(RefCell::new(None)),
            pending_link_selection: Rc::new(RefCell::new(None)),
            ghost_suggestion: Rc::new(RefCell::new(None)),
            diagnostics: Rc::new(RefCell::new(HashMap::new())),
            child_anchors: Rc::new(RefCell::new(Vec::new())),
            inline_images: Rc::new(RefCell::new(Vec::new())),
        };

        tab.full_render_buffer();
        tab.apply_preferences_with_tags(shared_tag_table.is_none());
        tab.wire_events();

        if let Some(path) = file_path {
            tab.watch_file(path);
        }

        tab
    }

    pub fn display_title(&self) -> String {
        if let Some(path) = self.file_path.borrow().as_ref()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            return name.to_string();
        }
        self.state
            .borrow()
            .doc
            .derived_title()
            .unwrap_or_else(|| "Untitled".to_string())
    }

    pub fn is_dirty(&self) -> bool {
        self.state.borrow().is_dirty()
    }

    pub fn is_pristine_empty(&self) -> bool {
        self.file_path.borrow().is_none()
            && !self.is_dirty()
            && self.state.borrow().doc.is_trivially_empty()
    }

    pub fn focus_editor(&self) {
        self.view.grab_focus();
    }

    pub fn apply_preferences(&self) {
        self.apply_preferences_with_tags(true);
    }

    pub fn apply_preferences_with_tags(&self, update_tags: bool) {
        let prefs = self.prefs.borrow().clone();
        if update_tags {
            let buffer = self.view.buffer();
            update_tag_styles(&buffer, &prefs);
        }

        let pal = resolve_palette(&prefs);
        self.view.set_colors(DecorationColors {
            text: rgba_from_css(&pal.text, gdk::RGBA::new(0.24, 0.24, 0.24, 1.0)),
            muted: rgba_from_css(&pal.muted_text, gdk::RGBA::new(0.48, 0.48, 0.48, 1.0)),
            quote_bar: rgba_from_css(&pal.quote_bar, gdk::RGBA::new(0.24, 0.24, 0.24, 0.28)),
            code_bg: rgba_from_css(&pal.code_bg, gdk::RGBA::new(0.0, 0.0, 0.0, 0.05)),
            border: rgba_from_css(&pal.border, gdk::RGBA::new(0.5, 0.5, 0.5, 0.12)),
            selection_bg: rgba_from_css(&pal.selection_bg, gdk::RGBA::new(0.16, 0.43, 0.66, 0.55)),
        });

        match prefs.default_direction {
            TextDirection::Rtl => self.view.set_direction(gtk4::TextDirection::Rtl),
            TextDirection::Ltr => self.view.set_direction(gtk4::TextDirection::Ltr),
            TextDirection::Auto => self.view.set_direction(gtk4::TextDirection::None),
        }

        self.state.borrow_mut().default_direction = prefs.default_direction;
        let was_typewriter = self.last_typewriter_mode.replace(prefs.typewriter_mode);
        self.recompute_canvas_margins();
        self.refresh_focus_mode(true);
        if prefs.typewriter_mode && !was_typewriter {
            self.schedule_scroll_caret();
        } else if !prefs.typewriter_mode && was_typewriter {
            self.cancel_scroll_animation();
        }
    }

    fn current_column_width_px(&self, raw_vw: i32) -> i32 {
        let prefs = self.prefs.borrow();
        let vw = raw_vw.max(480);
        match prefs.text_width {
            crate::config::TextWidth::Full => (vw - BASE_SIDE_MARGIN * 2).max(200),
            crate::config::TextWidth::Percent75 => (((vw as f64) * 0.75) as i32).max(320),
            fixed => fixed.resolve_px(vw),
        }
    }

    fn resize_inline_images(&self, raw_vw: i32, raw_vh: i32, col_px: i32) {
        let mut imgs = self.inline_images.borrow_mut();
        imgs.retain(|e| !e.anchor.is_deleted() && e.picture.parent().is_some());
        for entry in imgs.iter() {
            let (w, h) = compute_image_display_size(
                entry.natural_w,
                entry.natural_h,
                raw_vw,
                raw_vh,
                col_px,
                entry.standalone,
            );
            if entry.picture.size_request() != (w, h) {
                entry.picture.set_size_request(w, h);
            }
        }
    }

    pub fn schedule_recompute_canvas_margins(&self) {
        if self.margin_sync_source.borrow().is_some() {
            return;
        }
        let tab_c = self.clone();
        let src = glib::idle_add_local_once(move || {
            *tab_c.margin_sync_source.borrow_mut() = None;
            tab_c.recompute_canvas_margins();
        });
        *self.margin_sync_source.borrow_mut() = Some(src);
    }

    pub fn recompute_canvas_margins(&self) {
        let raw_vw = self.scrolled.width();
        let raw_vh = self.scrolled.height();
        let vh = raw_vh.max(320);
        let col_px = self.current_column_width_px(raw_vw);
        let side_inset = if raw_vw > 1 {
            ((raw_vw - col_px) / 2).max(BASE_SIDE_MARGIN)
        } else {
            BASE_SIDE_MARGIN
        };

        self.view.set_column_inset(side_inset);
        if raw_vw > 1 || raw_vh > 1 {
            self.resize_inline_images(raw_vw, raw_vh, col_px);
        }

        let prefs = self.prefs.borrow();
        let top = if prefs.typewriter_mode {
            ((vh as f64) * 0.44) as i32
        } else {
            52
        }
        .max(52);

        let bottom = if prefs.typewriter_mode {
            ((vh as f64) * 0.56) as i32
        } else {
            ((vh as f64) * 0.42) as i32
        }
        .max(140);

        let margins_changed = self.view.top_margin() != top || self.view.bottom_margin() != bottom;
        if self.view.top_margin() != top {
            self.view.set_top_margin(top);
        }
        if self.view.bottom_margin() != bottom {
            self.view.set_bottom_margin(bottom);
        }
        let typewriter = prefs.typewriter_mode;
        drop(prefs);
        if margins_changed && typewriter {
            self.schedule_scroll_caret();
        }
    }

    fn detach_child_widgets_in_range(
        &self,
        buffer: &gtk4::TextBuffer,
        start: &gtk4::TextIter,
        end: &gtk4::TextIter,
    ) {
        let (lo, hi) = if start <= end {
            (*start, *end)
        } else {
            (*end, *start)
        };
        let mut anchors = self.child_anchors.borrow_mut();
        if anchors.is_empty() {
            return;
        }
        let mut widgets_to_remove = Vec::new();
        anchors.retain(|anchor| {
            if anchor.is_deleted() {
                return false;
            }
            let it = buffer.iter_at_child_anchor(anchor);
            if it >= lo && it < hi {
                widgets_to_remove.extend(anchor.widgets());
                return false;
            }
            true
        });
        drop(anchors);
        for w in widgets_to_remove {
            self.view.remove(&w);
        }
        self.inline_images
            .borrow_mut()
            .retain(|e| !e.anchor.is_deleted() && e.picture.parent().is_some());
    }

    fn detach_all_child_widgets(&self) {
        self.inline_images.borrow_mut().clear();
        let anchors: Vec<_> = self.child_anchors.borrow_mut().drain(..).collect();
        for anchor in anchors {
            if !anchor.is_deleted() {
                for w in anchor.widgets() {
                    self.view.remove(&w);
                }
            }
        }
    }

    pub fn full_render_buffer(&self) {
        self.is_syncing.set(true);
        let buffer = self.view.buffer();
        self.detach_all_child_widgets();
        buffer.place_cursor(&buffer.start_iter());
        buffer.set_text("");

        let blocks_snapshot = self.state.borrow().doc.blocks.clone();
        let mut iter = buffer.start_iter();
        for (idx, block) in blocks_snapshot.iter().enumerate() {
            if idx > 0 {
                buffer.insert(&mut iter, "\n");
            }
            self.insert_block_content(&buffer, &mut iter, idx, block);
        }

        for (idx, block) in blocks_snapshot.iter().enumerate() {
            self.apply_block_tags(&buffer, idx, block);
        }

        self.sync_decorations(&blocks_snapshot);
        self.sync_selection_to_buffer();
        self.is_syncing.set(false);
        self.refresh_focus_mode(true);
    }

    pub fn apply_patch(&self, patch: BufferPatch) {
        match patch {
            BufferPatch::None => {
                self.sync_selection_to_buffer();
                (self.callbacks.on_selection_changed)(false);
                return;
            }
            BufferPatch::FullReload => {
                self.full_render_buffer();
            }
            BufferPatch::InsertInBlock {
                block,
                offset,
                text,
            } => {
                self.is_syncing.set(true);
                let buffer = self.view.buffer();
                if let Some(mut it) = iter_at_doc_pos(&buffer, DocPos::new(block, offset)) {
                    buffer.insert(&mut it, &text);
                }
                if let Some(b) = self.state.borrow().doc.blocks.get(block).cloned() {
                    self.sync_trailing_sentinel(&buffer, block, &b);
                    self.apply_block_tags(&buffer, block, &b);
                }
                self.sync_selection_to_buffer();
                self.is_syncing.set(false);
            }
            BufferPatch::SpliceBlocks {
                start_block,
                old_count,
                new_count,
            } => {
                self.is_syncing.set(true);
                let buffer = self.view.buffer();
                let doc_blocks = self.state.borrow().doc.blocks.clone();
                let total_lines_before = buffer.line_count().max(1) as usize;
                let end_old_exclusive = (start_block + old_count).min(total_lines_before);

                if let Some(mut del_start) = buffer.iter_at_line(start_block as i32) {
                    let mut del_end = if end_old_exclusive < total_lines_before {
                        buffer
                            .iter_at_line(end_old_exclusive as i32)
                            .unwrap_or_else(|| buffer.end_iter())
                    } else {
                        if start_block > 0 && new_count == 0 {
                            del_start.backward_char();
                        }
                        buffer.end_iter()
                    };

                    let had_following = end_old_exclusive < total_lines_before;
                    if had_following && new_count > 0 {
                        del_end.backward_char();
                    }

                    let start_off = del_start.offset();
                    let end_off = del_end.offset();
                    let (lo_off, hi_off) = if start_off <= end_off {
                        (start_off, end_off)
                    } else {
                        (end_off, start_off)
                    };
                    if lo_off < hi_off {
                        self.detach_child_widgets_in_range(&buffer, &del_start, &del_end);
                        buffer.place_cursor(&buffer.iter_at_offset(lo_off));
                        let mut del_start = buffer.iter_at_offset(lo_off);
                        let mut del_end = buffer.iter_at_offset(hi_off);
                        buffer.delete(&mut del_start, &mut del_end);
                    }

                    let mut ins_iter = buffer
                        .iter_at_line(start_block as i32)
                        .unwrap_or_else(|| buffer.end_iter());
                    if new_count > 0 {
                        for rel in 0..new_count {
                            if rel > 0 {
                                buffer.insert(&mut ins_iter, "\n");
                            }
                            let b_idx = start_block + rel;
                            if let Some(b) = doc_blocks.get(b_idx) {
                                self.insert_block_content(&buffer, &mut ins_iter, b_idx, b);
                            }
                        }
                    }

                    for rel in 0..new_count {
                        let b_idx = start_block + rel;
                        if let Some(b) = doc_blocks.get(b_idx) {
                            self.apply_block_tags(&buffer, b_idx, b);
                        }
                    }
                }

                self.sync_decorations(&doc_blocks);
                self.sync_selection_to_buffer();
                self.is_syncing.set(false);
            }
        }

        self.refresh_focus_mode(false);
        self.schedule_scroll_caret();
        self.schedule_autosave(false);
        (self.callbacks.on_state_changed)();
        (self.callbacks.on_selection_changed)(false);
    }

    fn insert_block_content(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        block_idx: usize,
        block: &crate::model::Block,
    ) {
        if block.kind == BlockKind::HorizontalRule {
            let anchor = buffer.create_child_anchor(iter);
            self.child_anchors.borrow_mut().push(anchor.clone());
            let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            sep.add_css_class("sillpaper-hr-line");
            sep.set_size_request(360, 2);
            self.view.add_child_at_anchor(&sep, &anchor);
            return;
        }

        let mut char_offset = 0usize;
        for inline in &block.inlines {
            match inline {
                Inline::Text(run) => {
                    buffer.insert(iter, &run.text);
                    char_offset += run.char_len();
                }
                Inline::LineBreak => {
                    buffer.insert(iter, "\u{2028}");
                    char_offset += 1;
                }
                Inline::Checkbox { checked } => {
                    let anchor = buffer.create_child_anchor(iter);
                    self.child_anchors.borrow_mut().push(anchor.clone());
                    let cb = gtk4::CheckButton::new();
                    cb.set_active(*checked);
                    cb.set_focus_on_click(false);
                    cb.set_valign(gtk4::Align::Center);
                    let tab_c = self.clone();
                    let anchor_c = anchor.clone();
                    cb.connect_toggled(move |_| {
                        if tab_c.is_syncing.get() || anchor_c.is_deleted() {
                            return;
                        }
                        let it = tab_c.view.buffer().iter_at_child_anchor(&anchor_c);
                        let b_i = it.line().max(0) as usize;
                        let off = it.line_offset().max(0) as usize;
                        let patch = tab_c.state.borrow_mut().toggle_checkbox_at(b_i, off);
                        let _ = patch;
                        tab_c.schedule_autosave(true);
                        (tab_c.callbacks.on_state_changed)();
                    });
                    self.view.add_child_at_anchor(&cb, &anchor);
                    char_offset += 1;
                }
                Inline::Comment { text } => {
                    let anchor = buffer.create_child_anchor(iter);
                    self.child_anchors.borrow_mut().push(anchor.clone());
                    let btn = gtk4::Button::with_label("✎");
                    btn.add_css_class("sillpaper-comment-badge");
                    btn.set_tooltip_text(Some(text));
                    btn.set_focus_on_click(false);
                    let tab_c = self.clone();
                    let anchor_c = anchor.clone();
                    btn.connect_clicked(move |_| {
                        if anchor_c.is_deleted() {
                            return;
                        }
                        let it = tab_c.view.buffer().iter_at_child_anchor(&anchor_c);
                        let b_i = it.line().max(0) as usize;
                        let off = it.line_offset().max(0) as usize;
                        tab_c.open_edit_comment_dialog(b_i, off);
                    });
                    self.view.add_child_at_anchor(&btn, &anchor);
                    char_offset += 1;
                }
                Inline::Image { src, alt, .. } => {
                    let anchor = buffer.create_child_anchor(iter);
                    self.child_anchors.borrow_mut().push(anchor.clone());
                    let standalone = is_standalone_image_block(block);
                    let raw_vw = self.scrolled.width();
                    let raw_vh = self.scrolled.height();
                    let col_px = self.current_column_width_px(raw_vw);
                    let (widget, maybe_pic) = build_inline_image_widget(
                        src,
                        alt,
                        self.file_path.borrow().as_deref(),
                        raw_vw,
                        raw_vh,
                        col_px,
                        standalone,
                    );
                    self.view.add_child_at_anchor(&widget, &anchor);
                    if let Some((picture, natural_w, natural_h)) = maybe_pic {
                        self.inline_images.borrow_mut().push(InlineImageEntry {
                            anchor,
                            picture,
                            natural_w,
                            natural_h,
                            standalone,
                        });
                    }
                    char_offset += 1;
                }
            }
        }
        if block_needs_trailing_sentinel(block) {
            buffer.insert(iter, "\u{200B}");
        }
        let _ = (block_idx, char_offset);
    }

    fn sync_trailing_sentinel(
        &self,
        buffer: &gtk4::TextBuffer,
        line_idx: usize,
        block: &crate::model::Block,
    ) {
        let Some((line_start, mut line_end)) = line_iter_bounds(buffer, line_idx) else {
            return;
        };
        let buf_len = line_end.line_offset().max(0) as usize;
        let model_len = block.char_len();
        let needs = block_needs_trailing_sentinel(block);

        if needs && buf_len == model_len {
            let was_syncing = self.is_syncing.replace(true);
            buffer.insert(&mut line_end, "\u{200B}");
            self.is_syncing.set(was_syncing);
        } else if !needs && buf_len == model_len + 1 {
            let mut prev = line_end;
            if prev.backward_char() && prev >= line_start && prev.char() == '\u{200B}' {
                let was_syncing = self.is_syncing.replace(true);
                buffer.delete(&mut prev, &mut line_end);
                self.is_syncing.set(was_syncing);
            }
        }
    }

    fn apply_block_tags(
        &self,
        buffer: &gtk4::TextBuffer,
        block_idx: usize,
        block: &crate::model::Block,
    ) {
        let Some(line_start) = buffer.iter_at_line(block_idx as i32) else {
            return;
        };
        let mut line_end = line_start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let mut block_end = line_end;
        if !block_end.is_end() {
            block_end.forward_char();
        }

        for tag_name in ALL_FORMAT_TAGS {
            buffer.remove_tag_by_name(tag_name, &line_start, &block_end);
        }

        let standalone_image = is_standalone_image_block(block);
        let block_tag = if standalone_image {
            "image_block"
        } else {
            match &block.kind {
                BlockKind::Paragraph => "paragraph",
                BlockKind::Heading(1) => "h1",
                BlockKind::Heading(2) => "h2",
                BlockKind::Heading(3) => "h3",
                BlockKind::Heading(_) => "h4",
                BlockKind::Blockquote => "blockquote",
                BlockKind::BulletListItem { depth } => list_tag_name(false, *depth),
                BlockKind::OrderedListItem { depth, .. } => list_tag_name(true, *depth),
                BlockKind::CodeBlock { .. } => "codeblock",
                BlockKind::HorizontalRule => "hr_block",
            }
        };
        buffer.apply_tag_by_name(block_tag, &line_start, &block_end);

        if !standalone_image && block.kind != BlockKind::HorizontalRule {
            match block.direction {
                TextDirection::Rtl => buffer.apply_tag_by_name("dir_rtl", &line_start, &block_end),
                TextDirection::Ltr => buffer.apply_tag_by_name("dir_ltr", &line_start, &block_end),
                TextDirection::Auto => {}
            }
        }

        if block
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::Image { .. }))
        {
            let mut changed = false;
            {
                let mut imgs = self.inline_images.borrow_mut();
                for entry in imgs.iter_mut() {
                    if !entry.anchor.is_deleted()
                        && buffer.iter_at_child_anchor(&entry.anchor).line().max(0) as usize
                            == block_idx
                        && entry.standalone != standalone_image
                    {
                        entry.standalone = standalone_image;
                        changed = true;
                    }
                }
            }
            if changed {
                let raw_vw = self.scrolled.width();
                let raw_vh = self.scrolled.height();
                let col_px = self.current_column_width_px(raw_vw);
                self.resize_inline_images(raw_vw, raw_vh, col_px);
            }
        }

        if !matches!(block.kind, BlockKind::CodeBlock { .. }) {
            let mut offset = 0usize;
            for inline in &block.inlines {
                let len = inline.char_len();
                if let Inline::Text(run) = inline
                    && !run.style.is_plain()
                {
                    let mut s_it = line_start;
                    s_it.set_line_offset(offset as i32);
                    let mut e_it = line_start;
                    e_it.set_line_offset((offset + len) as i32);

                    if run.style.bold {
                        buffer.apply_tag_by_name("bold", &s_it, &e_it);
                    }
                    if run.style.italic {
                        buffer.apply_tag_by_name("italic", &s_it, &e_it);
                    }
                    if run.style.underline {
                        buffer.apply_tag_by_name("underline", &s_it, &e_it);
                    }
                    if run.style.strikethrough {
                        buffer.apply_tag_by_name("strikethrough", &s_it, &e_it);
                    }
                    if run.style.code {
                        buffer.apply_tag_by_name("code", &s_it, &e_it);
                    }
                    if run.style.link.is_some() {
                        buffer.apply_tag_by_name("link", &s_it, &e_it);
                    }
                }
                offset += len;
            }
        }
    }

    fn sync_decorations(&self, blocks: &[crate::model::Block]) {
        let decos: Vec<BlockDecoration> = blocks
            .iter()
            .map(|b| match &b.kind {
                BlockKind::Blockquote => BlockDecoration::Blockquote,
                BlockKind::BulletListItem { depth } => BlockDecoration::Bullet { depth: *depth },
                BlockKind::OrderedListItem { depth, number } => BlockDecoration::Ordered {
                    depth: *depth,
                    number: *number,
                },
                BlockKind::CodeBlock { language } => BlockDecoration::CodeBlock {
                    language: language.clone(),
                },
                _ => BlockDecoration::None,
            })
            .collect();
        self.view.set_decorations(decos);
    }

    fn sync_selection_to_buffer(&self) {
        let sel = self.state.borrow().selection;
        let buffer = self.view.buffer();
        if let (Some(ins), Some(bound)) = (
            iter_at_doc_pos(&buffer, sel.focus),
            iter_at_doc_pos(&buffer, sel.anchor),
        ) {
            buffer.select_range(&ins, &bound);
            self.view.queue_draw();
        }
    }

    fn read_selection_from_buffer(&self) -> Selection {
        let buffer = self.view.buffer();
        let ins = buffer.iter_at_mark(&buffer.get_insert());
        let bound = buffer.iter_at_mark(&buffer.selection_bound());
        let focus = DocPos::new(
            ins.line().max(0) as usize,
            ins.line_offset().max(0) as usize,
        );
        let anchor = DocPos::new(
            bound.line().max(0) as usize,
            bound.line_offset().max(0) as usize,
        );
        self.state
            .borrow()
            .doc
            .clamp_selection(Selection::range(anchor, focus))
    }

    pub fn toolbar_state(&self) -> ToolbarState {
        self.state.borrow().toolbar_state()
    }

    pub fn selection_bounds_in(
        &self,
        target: &impl IsA<gtk4::Widget>,
    ) -> Option<(f32, f32, f32, f32)> {
        let buffer = self.view.buffer();
        let (start, end) = buffer.selection_bounds()?;
        if start == end {
            return None;
        }
        let loc_s = self.view.iter_location(&start);
        let loc_e = self.view.iter_location(&end);
        let (sx, sy) =
            self.view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, loc_s.x(), loc_s.y());
        let (ex, ey) =
            self.view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, loc_e.x(), loc_e.y());

        let (left, right, top, bottom) = if start.line() == end.line() {
            (
                sx.min(ex) as f32,
                sx.max(ex) as f32,
                sy.min(ey) as f32,
                (sy + loc_s.height()).max(ey + loc_e.height()) as f32,
            )
        } else {
            let inset = self.view.column_inset();
            let col_l = inset as f32;
            let col_r = (self.view.width() - inset).max(inset + 40) as f32;
            let vis_top = (sy as f32).max(16.0);
            let vis_bot = ((ey + loc_e.height()) as f32)
                .min(self.view.height() as f32 - 16.0)
                .max(vis_top + 24.0);
            (col_l, col_r, vis_top, vis_bot)
        };

        let p1 = self
            .view
            .compute_point(target, &gtk4::graphene::Point::new(left, top))?;
        let p2 = self
            .view
            .compute_point(target, &gtk4::graphene::Point::new(right, bottom))?;
        Some((
            p1.x().min(p2.x()),
            p1.x().max(p2.x()),
            p1.y().min(p2.y()),
            p1.y().max(p2.y()),
        ))
    }

    fn sync_image_focus_opacity(
        &self,
        buffer: &gtk4::TextBuffer,
        active_block: Option<usize>,
        dim_opacity: f64,
    ) {
        let mut imgs = self.inline_images.borrow_mut();
        imgs.retain(|e| !e.anchor.is_deleted() && e.picture.parent().is_some());
        for entry in imgs.iter() {
            let target = match active_block {
                None => 1.0,
                Some(active) => {
                    let line = buffer.iter_at_child_anchor(&entry.anchor).line().max(0) as usize;
                    if line == active { 1.0 } else { dim_opacity }
                }
            };
            if (entry.picture.opacity() - target).abs() > 0.005 {
                entry.picture.set_opacity(target);
            }
        }
    }

    pub fn refresh_focus_mode(&self, force_all: bool) {
        let (enabled, alpha) = {
            let prefs = self.prefs.borrow();
            (
                prefs.focus_mode,
                prefs.focus_mode_opacity.clamp(0.05, 1.0) as f32,
            )
        };
        let buffer = self.view.buffer();
        if !enabled {
            self.view.set_focus_state(None, 1.0);
            self.sync_image_focus_opacity(&buffer, None, 1.0);
            if self.last_focus_block.get().is_some() || force_all {
                buffer.remove_tag_by_name("focus_dim", &buffer.start_iter(), &buffer.end_iter());
                self.last_focus_block.set(None);
            }
            return;
        }

        let active_block = self.state.borrow().selection.focus.block;
        self.view.set_focus_state(Some(active_block), alpha);
        self.sync_image_focus_opacity(&buffer, Some(active_block), alpha as f64);
        let prev = self.last_focus_block.get();
        if !force_all && prev == Some(active_block) {
            return;
        }

        if force_all || prev.is_none() {
            buffer.apply_tag_by_name("focus_dim", &buffer.start_iter(), &buffer.end_iter());
            if let Some((s, e)) = line_iter_bounds(&buffer, active_block) {
                buffer.remove_tag_by_name("focus_dim", &s, &e);
            }
        } else if let Some(prev_b) = prev {
            if let Some((ps, pe)) = line_iter_bounds(&buffer, prev_b) {
                buffer.apply_tag_by_name("focus_dim", &ps, &pe);
            }
            if let Some((s, e)) = line_iter_bounds(&buffer, active_block) {
                buffer.remove_tag_by_name("focus_dim", &s, &e);
            }
        }
        self.last_focus_block.set(Some(active_block));
    }

    pub fn schedule_scroll_caret(&self) {
        if let Some(src) = self.scroll_source.borrow_mut().take() {
            src.remove();
        }
        let tab_c = self.clone();
        let src = glib::timeout_add_local_once(std::time::Duration::from_millis(16), move || {
            *tab_c.scroll_source.borrow_mut() = None;
            tab_c.scroll_caret_comfortably();
        });
        *self.scroll_source.borrow_mut() = Some(src);
    }

    pub fn cancel_scroll_animation(&self) {
        if let Some(anim) = self.scroll_anim.borrow_mut().take() {
            anim.pause();
        }
        self.scroll_anim_target.set(None);
    }

    fn animate_scroll_to(&self, target: f64) {
        let vadj = self.scrolled.vadjustment();
        let max_scroll = (vadj.upper() - vadj.page_size()).max(vadj.lower());
        let target = target.clamp(vadj.lower(), max_scroll);
        let start = vadj.value();
        let distance = (target - start).abs();
        if distance <= 1.5 {
            return;
        }

        if let Some(prev_target) = self.scroll_anim_target.get()
            && (prev_target - target).abs() <= 1.5
            && self
                .scroll_anim
                .borrow()
                .as_ref()
                .is_some_and(|a| a.state() == adw::AnimationState::Playing)
        {
            return;
        }

        self.cancel_scroll_animation();

        let duration_ms = (140.0 + distance * 0.45).clamp(140.0, 360.0).round() as u32;

        let tab_step = self.clone();
        let cb_target = adw::CallbackAnimationTarget::new(move |val| {
            let vadj = tab_step.scrolled.vadjustment();
            let max_s = (vadj.upper() - vadj.page_size()).max(vadj.lower());
            tab_step.is_animating_scroll.set(true);
            vadj.set_value(val.clamp(vadj.lower(), max_s));
            tab_step.is_animating_scroll.set(false);
        });

        let anim = adw::TimedAnimation::builder()
            .widget(&self.scrolled)
            .value_from(start)
            .value_to(target)
            .duration(duration_ms)
            .easing(adw::Easing::EaseOutCubic)
            .target(&cb_target)
            .build();

        let tab_done = self.clone();
        anim.connect_done(move |finished| {
            let is_current = tab_done
                .scroll_anim
                .borrow()
                .as_ref()
                .is_some_and(|cur| cur == finished);
            if is_current {
                *tab_done.scroll_anim.borrow_mut() = None;
                tab_done.scroll_anim_target.set(None);
            }
        });

        self.scroll_anim_target.set(Some(target));
        *self.scroll_anim.borrow_mut() = Some(anim.clone());
        anim.play();
    }

    pub fn scroll_caret_comfortably(&self) {
        if let Some(src) = self.scroll_source.borrow_mut().take() {
            src.remove();
        }
        let buffer = self.view.buffer();
        let insert_iter = buffer.iter_at_mark(&buffer.get_insert());
        let loc = self.view.iter_location(&insert_iter);
        let vadj = self.scrolled.vadjustment();
        let page = vadj.page_size();
        if page <= 1.0 {
            return;
        }

        let (_, widget_y) =
            self.view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, loc.x(), loc.y());
        let caret_mid_widget_y = widget_y as f64 + (loc.height().max(16) as f64) * 0.5;
        let cur_scroll = vadj.value();
        let max_scroll = (vadj.upper() - page).max(vadj.lower());
        let typewriter = self.prefs.borrow().typewriter_mode;

        if typewriter {
            let desired_widget_y = page * 0.45;
            let delta = caret_mid_widget_y - desired_widget_y;
            let target = (cur_scroll + delta).clamp(vadj.lower(), max_scroll);
            if (target - cur_scroll).abs() > 1.5 {
                self.animate_scroll_to(target);
            }
        } else {
            let caret_top_widget = widget_y as f64;
            let caret_bot_widget = widget_y as f64 + loc.height().max(16) as f64;
            let comfort_top = 76.0;
            let comfort_bottom = page * 0.68;
            if caret_bot_widget > comfort_bottom {
                let target =
                    (cur_scroll + (caret_bot_widget - page * 0.62)).clamp(vadj.lower(), max_scroll);
                vadj.set_value(target);
            } else if caret_top_widget < comfort_top && cur_scroll > vadj.lower() {
                let target =
                    (cur_scroll - (comfort_top - caret_top_widget)).clamp(vadj.lower(), max_scroll);
                vadj.set_value(target);
            }
        }
    }

    pub fn scroll_to_block(&self, block_idx: usize) {
        let buffer = self.view.buffer();
        if let Some(mut it) = buffer.iter_at_line(block_idx as i32) {
            buffer.place_cursor(&it);
            if self.prefs.borrow().typewriter_mode && self.scrolled.vadjustment().page_size() > 1.0
            {
                self.scroll_caret_comfortably();
            } else {
                self.view.scroll_to_iter(&mut it, 0.12, true, 0.0, 0.18);
            }
        }
    }

    pub fn toggle_inline(&self, attr: InlineAttr) {
        let patch = self.state.borrow_mut().toggle_inline_attr(attr);
        self.apply_patch(patch);
    }

    pub fn toggle_block(&self, kind: BlockKind) {
        let patch = self.state.borrow_mut().toggle_block_kind(kind);
        self.apply_patch(patch);
    }

    pub fn set_link(&self, url: Option<String>) {
        let patch = self.state.borrow_mut().set_link(url);
        self.apply_patch(patch);
    }

    pub fn begin_pending_link(&self) -> Option<Selection> {
        let sel = self.state.borrow().selection;
        if sel.is_collapsed() {
            return None;
        }
        *self.pending_link_selection.borrow_mut() = Some(sel);
        let buffer = self.view.buffer();
        let (s, e) = sel.ordered();
        if let (Some(si), Some(ei)) = (iter_at_doc_pos(&buffer, s), iter_at_doc_pos(&buffer, e)) {
            self.view
                .set_override_selection(Some((si.offset(), ei.offset())));
        }
        Some(sel)
    }

    pub fn has_pending_link(&self) -> bool {
        self.pending_link_selection.borrow().is_some()
    }

    pub fn commit_pending_link(&self, url: Option<String>) {
        self.view.set_override_selection(None);
        if let Some(sel) = self.pending_link_selection.borrow_mut().take() {
            self.state.borrow_mut().set_selection(sel);
        }
        self.set_link(url);
    }

    pub fn cancel_pending_link(&self, restore_selection: bool) {
        self.view.set_override_selection(None);
        if let Some(sel) = self.pending_link_selection.borrow_mut().take()
            && restore_selection
        {
            self.state.borrow_mut().set_selection(sel);
            self.is_syncing.set(true);
            self.sync_selection_to_buffer();
            self.is_syncing.set(false);
        }
    }

    pub fn has_ghost_suggestion(&self) -> bool {
        self.ghost_suggestion.borrow().is_some()
    }

    pub fn clear_ghost_suggestion(&self) {
        let _ = self.ghost_suggestion.borrow_mut().take();
        self.view.set_ghost_text(None);
    }

    pub fn set_ghost_loading(&self, block: usize, offset: usize) {
        let sel = self.state.borrow().selection;
        if !sel.is_collapsed() || sel.focus.block != block || sel.focus.offset != offset {
            return;
        }
        let buffer = self.view.buffer();
        if let Some(it) = iter_at_doc_pos(&buffer, DocPos::new(block, offset)) {
            *self.ghost_suggestion.borrow_mut() = None;
            self.view
                .set_ghost_text(Some((it.offset(), "…".to_string())));
        }
    }

    pub fn set_ghost_suggestion(&self, block: usize, offset: usize, text: String) {
        if text.is_empty() {
            self.clear_ghost_suggestion();
            return;
        }
        let sel = self.state.borrow().selection;
        if !sel.is_collapsed() || sel.focus.block != block || sel.focus.offset != offset {
            return;
        }
        let buffer = self.view.buffer();
        let Some(it) = iter_at_doc_pos(&buffer, DocPos::new(block, offset)) else {
            return;
        };
        let char_off = it.offset();
        *self.ghost_suggestion.borrow_mut() = Some((block, offset, text.clone()));
        self.view.set_ghost_text(Some((char_off, text)));
    }

    pub fn accept_ghost_suggestion(&self, word_only: bool) -> bool {
        let Some((block, offset, text)) = self.ghost_suggestion.borrow_mut().take() else {
            return false;
        };
        self.view.set_ghost_text(None);

        let sel = self.state.borrow().selection;
        if !sel.is_collapsed() || sel.focus.block != block || sel.focus.offset != offset {
            return false;
        }

        let (commit, remainder) = if word_only {
            let (w, rest) = take_next_word(&text);
            (w.to_string(), rest.to_string())
        } else {
            (text, String::new())
        };
        if commit.is_empty() {
            return false;
        }
        let patch = self.state.borrow_mut().insert_text(&commit, None);
        self.apply_patch(patch);

        if !remainder.is_empty() {
            let new_pos = self.state.borrow().selection.focus;
            self.set_ghost_suggestion(new_pos.block, new_pos.offset, remainder);
        }
        true
    }

    pub fn completion_context(&self) -> Option<(usize, usize, String, String, String)> {
        let st = self.state.borrow();
        if !st.selection.is_collapsed() {
            return None;
        }
        let pos = st.selection.focus;
        let block = st.doc.blocks.get(pos.block)?;
        let para = block.plain_text();
        let chars: Vec<char> = para.chars().collect();
        let off = pos.offset.min(chars.len());
        let cur_prefix: String = chars[..off].iter().collect();
        let cur_suffix: String = chars[off..].iter().collect();

        let mut prefix = String::new();
        let start_b = pos.block.saturating_sub(3);
        for b in start_b..pos.block {
            if let Some(prev) = st.doc.blocks.get(b) {
                let t = prev.plain_text();
                if !t.is_empty() {
                    prefix.push_str(&t);
                    prefix.push('\n');
                }
            }
        }
        prefix.push_str(&cur_prefix);
        if prefix.chars().count() > 1200 {
            let skip = prefix.chars().count() - 1200;
            prefix = prefix.chars().skip(skip).collect();
        }
        let suffix: String = cur_suffix.chars().take(400).collect();
        Some((pos.block, off, prefix, suffix, para))
    }

    pub fn active_block_text(&self) -> Option<(usize, String)> {
        let st = self.state.borrow();
        let b = st.selection.focus.block;
        let text = st.doc.blocks.get(b)?.plain_text();
        Some((b, text))
    }

    pub fn set_block_diagnostics(&self, block: usize, items: Vec<ExtensionDiagnostic>) {
        let block_len = self
            .state
            .borrow()
            .doc
            .blocks
            .get(block)
            .map(|b| b.char_len())
            .unwrap_or(0);
        let valid: Vec<ExtensionDiagnostic> = items
            .into_iter()
            .filter(|d| d.start < d.end && d.end <= block_len)
            .collect();

        let buffer = self.view.buffer();
        if let Some(ls) = buffer.iter_at_line(block as i32) {
            let mut le = ls;
            if !le.ends_line() {
                le.forward_to_line_end();
            }
            buffer.remove_tag_by_name("diagnostic", &ls, &le);
            for d in &valid {
                if let (Some(si), Some(ei)) = (
                    buffer.iter_at_line_offset(block as i32, d.start as i32),
                    buffer.iter_at_line_offset(block as i32, d.end as i32),
                ) {
                    buffer.apply_tag_by_name("diagnostic", &si, &ei);
                }
            }
        }
        if valid.is_empty() {
            self.diagnostics.borrow_mut().remove(&block);
        } else {
            self.diagnostics.borrow_mut().insert(block, valid);
        }
    }

    pub fn clear_block_diagnostics(&self, block: usize) {
        if self.diagnostics.borrow_mut().remove(&block).is_some() {
            let buffer = self.view.buffer();
            if let Some(ls) = buffer.iter_at_line(block as i32) {
                let mut le = ls;
                if !le.ends_line() {
                    le.forward_to_line_end();
                }
                buffer.remove_tag_by_name("diagnostic", &ls, &le);
            }
        }
    }

    pub fn clear_all_diagnostics(&self) {
        self.diagnostics.borrow_mut().clear();
        let buffer = self.view.buffer();
        buffer.remove_tag_by_name("diagnostic", &buffer.start_iter(), &buffer.end_iter());
    }

    pub fn diagnostic_at(&self, block: usize, offset: usize) -> Option<ExtensionDiagnostic> {
        let map = self.diagnostics.borrow();
        let list = map.get(&block)?;
        list.iter()
            .find(|d| offset >= d.start && offset <= d.end)
            .cloned()
    }

    pub fn apply_diagnostic_fix(&self, block: usize, diag: &ExtensionDiagnostic) {
        let Some(ref rep) = diag.replacement else {
            return;
        };
        self.clear_ghost_suggestion();
        self.clear_block_diagnostics(block);
        let patch = {
            let mut st = self.state.borrow_mut();
            st.set_selection(Selection::range(
                DocPos::new(block, diag.start),
                DocPos::new(block, diag.end),
            ));
            st.insert_text(rep, None)
        };
        self.apply_patch(patch);
    }

    pub fn dismiss_diagnostic(&self, block: usize, start: usize, end: usize) {
        let updated: Vec<ExtensionDiagnostic> = {
            let map = self.diagnostics.borrow();
            map.get(&block)
                .map(|list| {
                    list.iter()
                        .filter(|d| !(d.start == start && d.end == end))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        self.set_block_diagnostics(block, updated);
    }

    pub fn selected_markdown(&self) -> Option<(Selection, String)> {
        let st = self.state.borrow();
        if st.selection.is_collapsed() {
            return None;
        }
        let sel = st.selection;
        let frag = st.copy_fragment(false)?;
        let md = fragment_to_markdown(&frag).markdown;
        if md.trim().is_empty() {
            None
        } else {
            Some((sel, md))
        }
    }

    pub fn replace_range_with_markdown(&self, sel: Selection, markdown: &str, insert_below: bool) {
        self.clear_ghost_suggestion();
        self.clear_all_diagnostics();
        let parsed = markdown_to_document(markdown);
        let frag = DocumentFragment {
            blocks: parsed.blocks,
        };
        let patch = {
            let mut st = self.state.borrow_mut();
            if insert_below {
                let (_, end) = sel.ordered();
                st.set_selection(Selection::collapsed(end));
                let _ = st.insert_paragraph();
                st.replace_selection_with_fragment(frag)
            } else {
                st.set_selection(sel);
                st.replace_selection_with_fragment(frag)
            }
        };
        self.apply_patch(patch);
    }

    pub fn char_range_bounds_in(
        &self,
        block: usize,
        start: usize,
        end: usize,
        target: &impl IsA<gtk4::Widget>,
    ) -> Option<(f32, f32, f32, f32)> {
        let buffer = self.view.buffer();
        let si = buffer.iter_at_line_offset(block as i32, start as i32)?;
        let ei = buffer.iter_at_line_offset(block as i32, end as i32)?;
        let ls = self.view.iter_location(&si);
        let le = self.view.iter_location(&ei);
        let (sx, sy) =
            self.view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, ls.x(), ls.y());
        let (ex, ey) =
            self.view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, le.x(), le.y());
        let p1 = self.view.compute_point(
            target,
            &gtk4::graphene::Point::new(sx.min(ex) as f32, sy.min(ey) as f32),
        )?;
        let p2 = self.view.compute_point(
            target,
            &gtk4::graphene::Point::new(
                sx.max(ex) as f32,
                (sy + ls.height()).max(ey + le.height()) as f32,
            ),
        )?;
        Some((
            p1.x().min(p2.x()),
            p1.x().max(p2.x()),
            p1.y().min(p2.y()),
            p1.y().max(p2.y()),
        ))
    }

    pub fn set_direction(&self, dir: TextDirection) {
        let patch = self.state.borrow_mut().set_direction(dir);
        self.apply_patch(patch);
    }

    pub fn undo(&self) {
        let patch = self.state.borrow_mut().undo();
        self.apply_patch(patch);
    }

    pub fn redo(&self) {
        let patch = self.state.borrow_mut().redo();
        self.apply_patch(patch);
    }

    pub fn open_insert_comment_dialog(&self) {
        let Some(win) = (self.callbacks.get_window)() else {
            return;
        };
        let tab_c = self.clone();
        show_comment_dialog(&win, None, false, move |res| {
            if let Some(text) = res
                && !text.trim().is_empty()
            {
                let patch = tab_c
                    .state
                    .borrow_mut()
                    .insert_inline_object(Inline::Comment { text });
                tab_c.apply_patch(patch);
                tab_c.flush_backup_now();
            }
            tab_c.focus_editor();
        });
    }

    pub fn open_edit_comment_dialog(&self, block_idx: usize, char_offset: usize) {
        let Some(win) = (self.callbacks.get_window)() else {
            return;
        };
        let initial = self
            .state
            .borrow()
            .doc
            .blocks
            .get(block_idx)
            .and_then(|b| b.inline_at(char_offset))
            .and_then(|(_, inline)| match inline {
                Inline::Comment { text } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();

        let tab_c = self.clone();
        show_comment_dialog(&win, Some(&initial), true, move |res| {
            let patch = tab_c
                .state
                .borrow_mut()
                .update_comment_at(block_idx, char_offset, res);
            tab_c.apply_patch(patch);
            tab_c.flush_backup_now();
            tab_c.focus_editor();
        });
    }

    pub fn open_insert_image_url_dialog(&self) {
        let Some(win) = (self.callbacks.get_window)() else {
            return;
        };
        let tab_c = self.clone();
        show_image_url_dialog(&win, move |url| {
            let patch = tab_c
                .state
                .borrow_mut()
                .insert_inline_object(Inline::Image {
                    src: url,
                    alt: String::new(),
                    original_src: None,
                });
            tab_c.apply_patch(patch);
            tab_c.focus_editor();
        });
    }

    pub fn open_insert_image_file_dialog(&self) {
        let Some(win) = (self.callbacks.get_window)() else {
            return;
        };
        let dialog = gtk4::FileDialog::builder()
            .title("Insert Picture")
            .modal(true)
            .build();
        let filter = gtk4::FileFilter::new();
        filter.set_name(Some("Images"));
        filter.add_mime_type("image/*");
        let list = gio::ListStore::new::<gtk4::FileFilter>();
        list.append(&filter);
        dialog.set_filters(Some(&list));

        let tab_c = self.clone();
        dialog.open(Some(&win), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res
                && let Some(path) = file.path()
            {
                tab_c.insert_image_from_path(&path);
                tab_c.focus_editor();
            }
        });
    }

    pub fn insert_image_from_path(&self, path: &std::path::Path) {
        let src = if let Some(doc_dir) = self.file_path.borrow().as_ref().and_then(|p| p.parent())
            && let Ok(rel) = path.strip_prefix(doc_dir)
        {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            format!("file://{}", path.to_string_lossy())
        };
        let alt = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        self.clear_ghost_suggestion();
        self.clear_all_diagnostics();
        let patch = self.state.borrow_mut().insert_inline_object(Inline::Image {
            src,
            alt,
            original_src: None,
        });
        self.apply_patch(patch);
    }

    pub fn insert_image_bytes(&self, png_bytes: &[u8], alt: &str) {
        let maybe_md_asset = {
            let fp = self.file_path.borrow();
            fp.as_ref().and_then(|doc_path| {
                let is_md = doc_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"));
                if !is_md {
                    return None;
                }
                let doc_dir = doc_path.parent()?;
                let assets_dir = doc_dir.join("assets");
                std::fs::create_dir_all(&assets_dir).ok()?;
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let filename = format!("image-{ts}.png");
                let target = assets_dir.join(&filename);
                std::fs::write(&target, png_bytes).ok()?;
                Some(format!("assets/{filename}"))
            })
        };

        let src = maybe_md_asset.unwrap_or_else(|| {
            format!(
                "data:image/png;base64,{}",
                crate::io::docx::base64_encode(png_bytes)
            )
        });
        self.clear_ghost_suggestion();
        self.clear_all_diagnostics();
        let patch = self.state.borrow_mut().insert_inline_object(Inline::Image {
            src,
            alt: alt.to_string(),
            original_src: None,
        });
        self.apply_patch(patch);
    }

    fn paste_image_from_clipboard(&self) {
        let clipboard = self.view.clipboard();
        let tab_c = self.clone();
        clipboard.read_texture_async(gio::Cancellable::NONE, move |res| {
            if let Ok(Some(texture)) = res {
                let bytes = texture.save_to_png_bytes();
                tab_c.insert_image_bytes(bytes.as_ref(), "");
                (tab_c.callbacks.on_toast)("Pasted image");
            }
        });
    }

    pub fn copy_to_clipboard(&self) {
        let double_nl = self.prefs.borrow().paragraph_margins;
        let Some(frag) = self.state.borrow().copy_fragment(false) else {
            return;
        };
        let html = fragment_to_html(&frag);
        let plain = frag.plain_text(double_nl);
        let html_provider = gdk::ContentProvider::for_bytes(
            "text/html",
            &glib::Bytes::from_owned(html.into_bytes()),
        );
        let plain_bytes = glib::Bytes::from_owned(plain.as_bytes().to_vec());
        let plain_utf8_provider =
            gdk::ContentProvider::for_bytes("text/plain;charset=utf-8", &plain_bytes);
        let plain_bare_provider = gdk::ContentProvider::for_bytes("text/plain", &plain_bytes);
        let plain_value_provider = gdk::ContentProvider::for_value(&plain.to_value());
        let union = gdk::ContentProvider::new_union(&[
            html_provider,
            plain_utf8_provider,
            plain_bare_provider,
            plain_value_provider,
        ]);
        let _ = self.view.clipboard().set_content(Some(&union));
    }

    pub fn cut_to_clipboard(&self) {
        if self.state.borrow().selection.is_collapsed() {
            return;
        }
        self.copy_to_clipboard();
        let patch = self.state.borrow_mut().delete_selection();
        self.apply_patch(patch);
    }

    pub fn copy_html_to_clipboard(&self) {
        let collapsed = self.state.borrow().selection.is_collapsed();
        let html = if collapsed {
            document_to_html(&self.state.borrow().doc)
        } else if let Some(frag) = self.state.borrow().copy_fragment(false) {
            fragment_to_html(&frag)
        } else {
            return;
        };
        self.view.clipboard().set_text(&html);
        (self.callbacks.on_toast)("HTML copied to clipboard");
    }

    pub fn copy_markdown_to_clipboard(&self) {
        let collapsed = self.state.borrow().selection.is_collapsed();
        let md = if collapsed {
            document_to_markdown(&self.state.borrow().doc).markdown
        } else if let Some(frag) = self.state.borrow().copy_fragment(false) {
            fragment_to_markdown(&frag).markdown
        } else {
            return;
        };
        self.view.clipboard().set_text(&md);
        (self.callbacks.on_toast)("Markdown copied to clipboard");
    }

    pub fn paste_from_clipboard(&self, plain_only: bool) {
        let clipboard = self.view.clipboard();
        let tab_c = self.clone();

        let in_code_block = {
            let st = self.state.borrow();
            let (s, e) = st.selection.ordered();
            s.block == e.block && matches!(st.doc.blocks[s.block].kind, BlockKind::CodeBlock { .. })
        };
        let plain_only = plain_only || in_code_block;

        if plain_only {
            clipboard.read_text_async(gio::Cancellable::NONE, move |res| {
                if let Ok(Some(text)) = res {
                    tab_c.clear_ghost_suggestion();
                    tab_c.clear_all_diagnostics();
                    let patch = tab_c.state.borrow_mut().paste_plain_text(&text);
                    tab_c.apply_patch(patch);
                }
            });
            return;
        }

        let formats = clipboard.formats();
        let has_texture = formats.contains_type(gdk::Texture::static_type())
            || formats.contain_mime_type("image/png")
            || formats.contain_mime_type("image/jpeg")
            || formats.contain_mime_type("image/webp")
            || formats.contain_mime_type("image/bmp");
        let has_plain = formats.contains_type(String::static_type())
            || formats.contain_mime_type("text/plain")
            || formats.contain_mime_type("text/plain;charset=utf-8");

        if has_texture && !has_plain && !formats.contain_mime_type("text/html") {
            self.paste_image_from_clipboard();
            return;
        }

        if formats.contain_mime_type("text/html") {
            clipboard.read_async(
                &["text/html"],
                glib::Priority::DEFAULT,
                gio::Cancellable::NONE,
                move |res| {
                    let Ok((stream, _mime)) = res else {
                        if has_texture && !has_plain {
                            tab_c.paste_image_from_clipboard();
                        } else {
                            tab_c.paste_from_clipboard(true);
                        }
                        return;
                    };
                    read_stream_to_string_async(stream, move |maybe_html| {
                        if let Some(html) = maybe_html {
                            let frag = html_to_fragment(&html);
                            let is_only_remote_img = frag.blocks.len() == 1
                                && matches!(
                                    frag.blocks[0].inlines.as_slice(),
                                    [Inline::Image { src, .. }]
                                        if src.starts_with("http://") || src.starts_with("https://")
                                );
                            if has_texture && is_only_remote_img {
                                tab_c.paste_image_from_clipboard();
                                return;
                            }
                            if !frag.blocks.is_empty() {
                                tab_c.clear_ghost_suggestion();
                                tab_c.clear_all_diagnostics();
                                let patch = tab_c
                                    .state
                                    .borrow_mut()
                                    .replace_selection_with_fragment(frag);
                                tab_c.apply_patch(patch);
                                return;
                            }
                        }
                        if has_texture && !has_plain {
                            tab_c.paste_image_from_clipboard();
                        } else {
                            tab_c.paste_from_clipboard(true);
                        }
                    });
                },
            );
        } else if has_texture && !has_plain {
            self.paste_image_from_clipboard();
        } else {
            self.paste_from_clipboard(true);
        }
    }

    pub fn run_search(&self, query: &str) -> (usize, usize) {
        self.clear_search_tags();
        let matches = if query.chars().count() >= 1 {
            find_matches(&self.state.borrow().doc, query)
        } else {
            Vec::new()
        };
        let total = matches.len();
        *self.search_matches.borrow_mut() = matches;
        self.current_match.set(0);

        if total > 0 {
            self.paint_search_tags();
            self.focus_current_match();
        }
        (0, total)
    }

    pub fn step_search(&self, forward: bool) -> (usize, usize) {
        let total = self.search_matches.borrow().len();
        if total == 0 {
            return (0, 0);
        }
        let cur = self.current_match.get();
        let next = if forward {
            (cur + 1) % total
        } else if cur == 0 {
            total - 1
        } else {
            cur - 1
        };
        self.current_match.set(next);
        self.paint_search_tags();
        self.focus_current_match();
        (next, total)
    }

    pub fn clear_search(&self) {
        self.search_matches.borrow_mut().clear();
        self.current_match.set(0);
        self.clear_search_tags();
    }

    fn clear_search_tags(&self) {
        let buffer = self.view.buffer();
        buffer.remove_tag_by_name("search_match", &buffer.start_iter(), &buffer.end_iter());
        buffer.remove_tag_by_name("search_current", &buffer.start_iter(), &buffer.end_iter());
    }

    fn paint_search_tags(&self) {
        self.clear_search_tags();
        let buffer = self.view.buffer();
        let matches = self.search_matches.borrow();
        let cur = self.current_match.get();
        for (i, m) in matches.iter().enumerate() {
            if let (Some(s), Some(e)) = (
                iter_at_doc_pos(&buffer, m.start),
                iter_at_doc_pos(&buffer, m.end),
            ) {
                let tag = if i == cur {
                    "search_current"
                } else {
                    "search_match"
                };
                buffer.apply_tag_by_name(tag, &s, &e);
            }
        }
    }

    fn focus_current_match(&self) {
        let matches = self.search_matches.borrow();
        let cur = self.current_match.get();
        if let Some(m) = matches.get(cur)
            && let Some(mut s) = iter_at_doc_pos(&self.view.buffer(), m.start)
        {
            self.view.scroll_to_iter(&mut s, 0.15, true, 0.0, 0.4);
        }
    }

    pub fn watch_file(&self, path: PathBuf) {
        *self.file_monitor.borrow_mut() = None;
        let gfile = gio::File::for_path(&path);
        if let Ok(monitor) = gfile.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        {
            let tab_c = self.clone();
            let path_c = path;
            monitor.connect_changed(move |_, _, _, event| {
                if !matches!(
                    event,
                    gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Changed
                ) {
                    return;
                }
                let Some(disk_mtime) = file_mtime_ms(&path_c) else {
                    return;
                };
                let known = *tab_c.last_file_mtime_ms.borrow();
                if known.is_some_and(|k| disk_mtime <= k) {
                    return;
                }
                (tab_c.callbacks.on_external_file_changed)(tab_c.id.clone());
            });
            *self.file_monitor.borrow_mut() = Some(monitor);
        }
    }

    pub fn reload_from_disk(&self) -> Result<(), String> {
        let Some(path) = self.file_path.borrow().clone() else {
            return Ok(());
        };
        let (new_doc, _fmt) = crate::io::load_document(&path)?;
        *self.last_file_mtime_ms.borrow_mut() = file_mtime_ms(&path);
        let patch = self.state.borrow_mut().replace_document_undoable(new_doc);
        self.state.borrow_mut().mark_saved();
        self.apply_patch(patch);
        self.flush_backup_now();
        Ok(())
    }

    pub fn schedule_autosave(&self, immediate: bool) {
        let had_pending = self.autosave_source.borrow_mut().take().map(|src| {
            src.remove();
            true
        }) == Some(true);

        if immediate {
            self.flush_backup_now();
            return;
        }

        let delay_ms = if had_pending { 500 } else { 2000 };
        let tab_c = self.clone();
        let src_id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(delay_ms), move || {
                *tab_c.autosave_source.borrow_mut() = None;
                tab_c.flush_backup_now();
            });
        *self.autosave_source.borrow_mut() = Some(src_id);
    }

    pub fn flush_backup_now(&self) {
        if let Some(src) = self.autosave_source.borrow_mut().take() {
            src.remove();
        }
        let st = self.state.borrow();
        let cursor_flat = st.doc.pos_to_flat_offset(st.selection.focus);
        let backup = StorageManager::build_backup(
            &self.id,
            &st.doc,
            self.file_path.borrow().clone(),
            !st.is_dirty(),
            *self.last_file_mtime_ms.borrow(),
            cursor_flat,
        );
        let count = self.backup_write_count.get().wrapping_add(1);
        self.backup_write_count.set(count);
        let _ = self.storage.save_tab_backup(&backup, count);
    }

    pub fn remove_backup(&self) {
        if let Some(src) = self.autosave_source.borrow_mut().take() {
            src.remove();
        }
        self.storage.remove_tab_backup(&self.id);
    }

    fn activate_link(&self, url: &str) {
        if let Some(slug) = url.strip_prefix('#') {
            let target_slug = slug.to_lowercase();
            let blocks = &self.state.borrow().doc.blocks;
            for (i, b) in blocks.iter().enumerate() {
                if b.kind.is_heading() && heading_slug(&b.plain_text()) == target_slug {
                    self.scroll_to_block(i);
                    return;
                }
            }
            return;
        }
        if let Some(win) = (self.callbacks.get_window)() {
            let launcher = gtk4::UriLauncher::new(url);
            launcher.launch(Some(&win), gio::Cancellable::NONE, |_| {});
        } else {
            let _ = gio::AppInfo::launch_default_for_uri(url, gio::AppLaunchContext::NONE);
        }
    }

    fn wire_events(&self) {
        let buffer = self.view.buffer();

        {
            let tab_c = self.clone();
            self.scrolled
                .hadjustment()
                .connect_page_size_notify(move |_| {
                    tab_c.schedule_recompute_canvas_margins();
                });
        }
        {
            self.scrolled
                .hadjustment()
                .connect_value_changed(move |adj| {
                    if adj.value() != 0.0 {
                        let adj_c = adj.clone();
                        glib::idle_add_local_once(move || {
                            if adj_c.value() != 0.0 {
                                adj_c.set_value(0.0);
                            }
                        });
                    }
                });
        }
        {
            let tab_c = self.clone();
            self.scrolled
                .vadjustment()
                .connect_page_size_notify(move |_| {
                    tab_c.schedule_recompute_canvas_margins();
                });
        }
        {
            let tab_c = self.clone();
            self.scrolled.connect_map(move |_| {
                tab_c.schedule_recompute_canvas_margins();
            });
        }

        {
            let margin_click = gtk4::GestureClick::builder()
                .button(1)
                .propagation_phase(gtk4::PropagationPhase::Capture)
                .build();
            let tab_c = self.clone();
            margin_click.connect_pressed(move |_, _, x, y| {
                let Some(bounds) = tab_c.view.compute_bounds(&tab_c.scrolled) else {
                    return;
                };
                let fx = x as f32;
                let fy = y as f32;
                let outside_x = fx < bounds.x() || fx > bounds.x() + bounds.width();
                let outside_y = fy < bounds.y() || fy > bounds.y() + bounds.height();
                if !outside_x && !outside_y {
                    return;
                }

                (tab_c.callbacks.on_hide_toolbar)();

                let rel_y = (fy - bounds.y()).round() as i32;
                let (_, buf_y) =
                    tab_c
                        .view
                        .window_to_buffer_coords(gtk4::TextWindowType::Widget, 0, rel_y);
                let (mut iter, _) = tab_c.view.line_at_y(buf_y.max(0));
                if fx > bounds.x() + bounds.width() || fy > bounds.y() + bounds.height() {
                    if !iter.ends_line() {
                        iter.forward_to_line_end();
                    }
                } else {
                    iter.set_line_offset(0);
                }

                let pos = DocPos::new(
                    iter.line().max(0) as usize,
                    iter.line_offset().max(0) as usize,
                );
                tab_c.is_syncing.set(true);
                tab_c.view.buffer().place_cursor(&iter);
                tab_c.is_syncing.set(false);
                tab_c
                    .state
                    .borrow_mut()
                    .set_selection(Selection::collapsed(pos));
                tab_c.refresh_focus_mode(false);
                if tab_c.prefs.borrow().typewriter_mode {
                    tab_c.schedule_scroll_caret();
                }
                (tab_c.callbacks.on_selection_changed)(false);
                if !tab_c.view.has_focus() {
                    tab_c.view.grab_focus();
                }
            });
            self.scrolled.add_controller(margin_click);
        }

        {
            let scroll_ctrl =
                gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
            scroll_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let tab_c = self.clone();
            scroll_ctrl.connect_scroll(move |_, _, _| {
                if let Some(src) = tab_c.scroll_source.borrow_mut().take() {
                    src.remove();
                }
                tab_c.cancel_scroll_animation();
                glib::Propagation::Proceed
            });
            let tab_b = self.clone();
            scroll_ctrl.connect_scroll_begin(move |_| {
                if let Some(src) = tab_b.scroll_source.borrow_mut().take() {
                    src.remove();
                }
                tab_b.cancel_scroll_animation();
            });
            self.scrolled.add_controller(scroll_ctrl);
        }

        {
            let tab_c = self.clone();
            self.scrolled.connect_unmap(move |_| {
                if let Some(src) = tab_c.margin_sync_source.borrow_mut().take() {
                    src.remove();
                }
                tab_c.cancel_scroll_animation();
            });
        }

        {
            let tab_c = self.clone();
            self.scrolled.vadjustment().connect_value_changed(move |_| {
                if !tab_c.is_animating_scroll.get() {
                    tab_c.cancel_scroll_animation();
                }
                (tab_c.callbacks.on_hide_toolbar)();
            });
        }

        {
            let tab_c = self.clone();
            self.link_pill.connect_clicked(move |_| {
                if let Some(url) = tab_c.hovered_link.borrow().clone() {
                    tab_c.activate_link(&url);
                }
            });
        }

        {
            let tab_c = self.clone();
            buffer.connect_mark_set(move |buf, _iter, mark| {
                if tab_c.is_syncing.get()
                    || tab_c.pending_smart_sync.get() > 0
                    || tab_c.has_pending_link()
                {
                    return;
                }
                let name = mark.name();
                if matches!(name.as_deref(), Some("insert") | Some("selection_bound")) {
                    tab_c.view.queue_draw();
                    let raw_ins = buf.iter_at_mark(&buf.get_insert());
                    let raw_bound = buf.iter_at_mark(&buf.selection_bound());
                    let raw_collapsed = raw_ins == raw_bound;
                    let raw_line = raw_ins.line().max(0) as usize;
                    let raw_off = raw_ins.line_offset().max(0) as usize;

                    let mut sel = tab_c.read_selection_from_buffer();

                    if raw_collapsed {
                        let sentinel_fix = {
                            let st = tab_c.state.borrow();
                            st.doc.blocks.get(raw_line).and_then(|blk| {
                                let model_len = blk.char_len();
                                if block_needs_trailing_sentinel(blk) && raw_off > model_len {
                                    let prev_at_end = st.selection.is_collapsed()
                                        && st.selection.focus == DocPos::new(raw_line, model_len);
                                    if prev_at_end
                                        && !tab_c.is_mouse_selecting.get()
                                        && raw_line + 1 < st.doc.blocks.len()
                                    {
                                        Some(DocPos::new(raw_line + 1, 0))
                                    } else {
                                        Some(DocPos::new(raw_line, model_len))
                                    }
                                } else {
                                    None
                                }
                            })
                        };
                        if let Some(target_pos) = sentinel_fix
                            && let Some(target_it) = iter_at_doc_pos(buf, target_pos)
                        {
                            tab_c.is_syncing.set(true);
                            buf.place_cursor(&target_it);
                            tab_c.is_syncing.set(false);
                            sel = Selection::collapsed(target_pos);
                        }
                    }

                    let keep_ghost = {
                        let g = tab_c.ghost_suggestion.borrow();
                        matches!(
                            *g,
                            Some((gb, goff, _))
                                if sel.is_collapsed()
                                    && sel.focus.block == gb
                                    && sel.focus.offset == goff
                        )
                    };
                    if !keep_ghost {
                        tab_c.clear_ghost_suggestion();
                    }
                    tab_c.state.borrow_mut().set_selection(sel);
                    tab_c.refresh_focus_mode(false);
                    if sel.is_collapsed() {
                        (tab_c.callbacks.on_hide_toolbar)();
                        if !tab_c.is_mouse_selecting.get() && tab_c.prefs.borrow().typewriter_mode {
                            tab_c.schedule_scroll_caret();
                        }
                    } else if !tab_c.is_mouse_selecting.get() {
                        (tab_c.callbacks.on_selection_changed)(false);
                    }
                }
            });
        }

        {
            let tab_c = self.clone();
            buffer.connect_delete_range(move |buf, start, end| {
                if tab_c.is_syncing.get() {
                    return;
                }
                tab_c.detach_child_widgets_in_range(buf, start, end);
                let s = DocPos::new(
                    start.line().max(0) as usize,
                    start.line_offset().max(0) as usize,
                );
                let e = DocPos::new(
                    end.line().max(0) as usize,
                    end.line_offset().max(0) as usize,
                );
                tab_c.clear_ghost_suggestion();
                tab_c.clear_block_diagnostics(s.block);
                tab_c.state.borrow_mut().delete_range(s, e);
                let tab_idle = tab_c.clone();
                let target_block = s.block;
                glib::idle_add_local_once(move || {
                    let buf = tab_idle.view.buffer();
                    let doc_blocks = tab_idle.state.borrow().doc.blocks.clone();
                    if let Some(b) = doc_blocks.get(target_block) {
                        tab_idle.sync_trailing_sentinel(&buf, target_block, b);
                    }
                    tab_idle.sync_decorations(&doc_blocks);
                    tab_idle.schedule_scroll_caret();
                    tab_idle.schedule_autosave(false);
                    (tab_idle.callbacks.on_state_changed)();
                });
            });
        }

        {
            let tab_c = self.clone();
            buffer.connect_insert_text(move |_buf, location, text| {
                if tab_c.is_syncing.get() {
                    return;
                }
                let pos = if tab_c.pending_smart_sync.get() > 0 {
                    tab_c.state.borrow().selection.focus
                } else {
                    let raw_pos = DocPos::new(
                        location.line().max(0) as usize,
                        location.line_offset().max(0) as usize,
                    );
                    tab_c.state.borrow().doc.clamp_pos(raw_pos)
                };
                let trimmed_ghost = {
                    let cur = tab_c.ghost_suggestion.borrow().clone();
                    if let Some((gb, goff, gtxt)) = cur
                        && gb == pos.block
                        && goff == pos.offset
                        && let Some(rem) = gtxt.strip_prefix(text)
                        && !rem.is_empty()
                    {
                        Some((gb, goff + text.chars().count(), rem.to_string()))
                    } else {
                        None
                    }
                };
                tab_c.clear_ghost_suggestion();
                tab_c.clear_block_diagnostics(pos.block);

                let prefs = tab_c.prefs.borrow().clone();
                let patch = {
                    let mut st = tab_c.state.borrow_mut();
                    st.selection = Selection::collapsed(pos);
                    st.insert_text(text, Some(&prefs))
                };
                let is_smart_splice = !matches!(patch, BufferPatch::InsertInBlock { .. });
                if is_smart_splice {
                    tab_c
                        .pending_smart_sync
                        .set(tab_c.pending_smart_sync.get() + 1);
                }

                if prefs.hide_caret_while_typing && text != " " {
                    tab_c.view.add_css_class("hide-caret-active");
                }
                (tab_c.callbacks.on_typing_activity)();

                let tab_idle = tab_c.clone();
                glib::idle_add_local_once(move || match patch {
                    BufferPatch::InsertInBlock { block, .. } => {
                        let buf = tab_idle.view.buffer();
                        if let Some(b) = tab_idle.state.borrow().doc.blocks.get(block).cloned() {
                            tab_idle.sync_trailing_sentinel(&buf, block, &b);
                            tab_idle.apply_block_tags(&buf, block, &b);
                        }
                        if let Some((gb, goff, rem)) = trimmed_ghost {
                            tab_idle.set_ghost_suggestion(gb, goff, rem);
                        }
                        tab_idle.schedule_scroll_caret();
                        tab_idle.schedule_autosave(false);
                        (tab_idle.callbacks.on_state_changed)();
                        (tab_idle.callbacks.on_selection_changed)(false);
                        (tab_idle.callbacks.on_extension_trigger)(
                            ExtensionTrigger::RequestCompletion { manual: false },
                        );
                        (tab_idle.callbacks.on_extension_trigger)(
                            ExtensionTrigger::RequestDiagnostics {
                                block,
                                manual: false,
                            },
                        );
                    }
                    other => {
                        tab_idle
                            .pending_smart_sync
                            .set(tab_idle.pending_smart_sync.get().saturating_sub(1));
                        tab_idle.apply_patch(other);
                    }
                });
            });
        }

        {
            let key = gtk4::EventControllerKey::new();
            key.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let tab_c = self.clone();
            key.connect_key_pressed(move |_, keyval, _, mods| {
                let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
                let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
                let alt = mods.contains(gdk::ModifierType::ALT_MASK);

                if ctrl
                    && !alt
                    && !shift
                    && keyval == gdk::Key::Right
                    && tab_c.has_ghost_suggestion()
                {
                    tab_c.accept_ghost_suggestion(true);
                    return glib::Propagation::Stop;
                }
                if !ctrl
                    && !alt
                    && !shift
                    && keyval == gdk::Key::Escape
                    && (tab_c.has_ghost_suggestion() || tab_c.view.ghost_text().is_some())
                {
                    tab_c.clear_ghost_suggestion();
                    (tab_c.callbacks.on_extension_trigger)(ExtensionTrigger::CancelSuggestions);
                    return glib::Propagation::Stop;
                }

                if matches!(
                    keyval,
                    gdk::Key::Left
                        | gdk::Key::Right
                        | gdk::Key::Up
                        | gdk::Key::Down
                        | gdk::Key::Home
                        | gdk::Key::End
                        | gdk::Key::Page_Up
                        | gdk::Key::Page_Down
                ) {
                    tab_c.view.remove_css_class("hide-caret-active");
                    return glib::Propagation::Proceed;
                }

                if ctrl && !alt {
                    match keyval {
                        gdk::Key::_1 | gdk::Key::KP_1 => {
                            tab_c.toggle_block(BlockKind::Heading(1));
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::_2 | gdk::Key::KP_2 => {
                            tab_c.toggle_block(BlockKind::Heading(2));
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::_3 | gdk::Key::KP_3 => {
                            tab_c.toggle_block(BlockKind::Heading(3));
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::b | gdk::Key::B => {
                            if shift {
                                tab_c.toggle_block(BlockKind::Blockquote);
                            } else {
                                tab_c.toggle_inline(InlineAttr::Bold);
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::i | gdk::Key::I if !shift => {
                            tab_c.toggle_inline(InlineAttr::Italic);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::u | gdk::Key::U if !shift => {
                            tab_c.toggle_inline(InlineAttr::Underline);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::d | gdk::Key::D if !shift => {
                            tab_c.toggle_inline(InlineAttr::Strikethrough);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::e | gdk::Key::E if !shift => {
                            tab_c.toggle_inline(InlineAttr::Code);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::k | gdk::Key::K if !shift => {
                            if !tab_c.state.borrow().selection.is_collapsed() {
                                (tab_c.callbacks.on_open_link_input)();
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::z | gdk::Key::Z => {
                            if shift {
                                tab_c.redo();
                            } else {
                                tab_c.undo();
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::y | gdk::Key::Y if !shift => {
                            tab_c.redo();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::c | gdk::Key::C if !shift => {
                            tab_c.copy_to_clipboard();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::x | gdk::Key::X if !shift => {
                            tab_c.cut_to_clipboard();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::v | gdk::Key::V => {
                            tab_c.paste_from_clipboard(shift);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Return | gdk::Key::KP_Enter if !shift => {
                            if let Some(patch) = tab_c.state.borrow_mut().exit_code_block_below() {
                                tab_c.apply_patch(patch);
                                (tab_c.callbacks.on_typing_activity)();
                                return glib::Propagation::Stop;
                            }
                        }
                        gdk::Key::space | gdk::Key::KP_Space | gdk::Key::nobreakspace if !shift => {
                            (tab_c.callbacks.on_extension_trigger)(
                                ExtensionTrigger::RequestCompletion { manual: true },
                            );
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::period if !shift => {
                            let focus = tab_c.state.borrow().selection.focus;
                            if let Some(diag) = tab_c.diagnostic_at(focus.block, focus.offset) {
                                (tab_c.callbacks.on_extension_trigger)(
                                    ExtensionTrigger::ShowQuickFix {
                                        block: focus.block,
                                        diagnostic: diag,
                                    },
                                );
                            } else {
                                (tab_c.callbacks.on_extension_trigger)(
                                    ExtensionTrigger::RequestDiagnostics {
                                        block: focus.block,
                                        manual: true,
                                    },
                                );
                            }
                            return glib::Propagation::Stop;
                        }
                        _ => {}
                    }
                }

                if ctrl && alt && matches!(keyval, gdk::Key::c | gdk::Key::C) {
                    tab_c.open_insert_comment_dialog();
                    return glib::Propagation::Stop;
                }

                if !ctrl && !alt {
                    match keyval {
                        gdk::Key::Return | gdk::Key::KP_Enter => {
                            let patch = if shift {
                                tab_c.state.borrow_mut().insert_soft_break()
                            } else {
                                tab_c.state.borrow_mut().insert_paragraph()
                            };
                            tab_c.apply_patch(patch);
                            (tab_c.callbacks.on_typing_activity)();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::BackSpace => {
                            let patch = tab_c.state.borrow_mut().backspace();
                            tab_c.apply_patch(patch);
                            (tab_c.callbacks.on_typing_activity)();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Delete | gdk::Key::KP_Delete => {
                            let patch = tab_c.state.borrow_mut().delete_forward();
                            tab_c.apply_patch(patch);
                            (tab_c.callbacks.on_typing_activity)();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                            if !shift
                                && keyval == gdk::Key::Tab
                                && tab_c.accept_ghost_suggestion(false)
                            {
                                return glib::Propagation::Stop;
                            }
                            let increase = !shift && keyval == gdk::Key::Tab;
                            let code_res = tab_c.state.borrow_mut().indent_code_block(increase);
                            if let Some(patch) = code_res {
                                tab_c.apply_patch(patch);
                            } else {
                                let list_res =
                                    tab_c.state.borrow_mut().adjust_list_indent(increase);
                                if let Some(patch) = list_res {
                                    tab_c.apply_patch(patch);
                                } else if !shift {
                                    let patch =
                                        tab_c.state.borrow_mut().insert_text("\u{2003}", None);
                                    tab_c.apply_patch(patch);
                                }
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::space if shift && tab_c.prefs.borrow().zwnj_on_shift_space => {
                            let patch = tab_c.state.borrow_mut().insert_text("\u{200C}", None);
                            tab_c.apply_patch(patch);
                            return glib::Propagation::Stop;
                        }
                        _ => {
                            if !tab_c.state.borrow().selection.is_collapsed()
                                && let Some(ch) = keyval.to_unicode()
                                && matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']')
                            {
                                let prefs = tab_c.prefs.borrow().clone();
                                let s = ch.to_string();
                                let patch = tab_c.state.borrow_mut().insert_text(&s, Some(&prefs));
                                tab_c.apply_patch(patch);
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                }

                glib::Propagation::Proceed
            });
            self.view.add_controller(key);
        }

        {
            let motion = gtk4::EventControllerMotion::new();
            let tab_c = self.clone();
            motion.connect_motion(move |_, x, y| {
                let (bx, by) = tab_c.view.window_to_buffer_coords(
                    gtk4::TextWindowType::Widget,
                    x as i32,
                    y as i32,
                );
                let found_url = tab_c.view.iter_at_location(bx, by).and_then(|iter| {
                    let b_idx = iter.line().max(0) as usize;
                    let off = iter.line_offset().max(0) as usize;
                    let st = tab_c.state.borrow();
                    let block = st.doc.blocks.get(b_idx)?;
                    if off < block.char_len() {
                        block.style_at(off + 1).link
                    } else {
                        None
                    }
                });

                if let Some(url) = found_url {
                    let display = if url.len() > 42 {
                        format!("Open {}… ↗", &url[..40])
                    } else {
                        format!("Open {url} ↗")
                    };
                    tab_c.link_pill.set_label(&display);
                    tab_c.link_pill.set_visible(true);
                    *tab_c.hovered_link.borrow_mut() = Some(url);
                } else if tab_c.hovered_link.borrow().is_some() {
                    tab_c.link_pill.set_visible(false);
                    *tab_c.hovered_link.borrow_mut() = None;
                }
            });
            self.view.add_controller(motion);
        }

        {
            let legacy = gtk4::EventControllerLegacy::new();
            legacy.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let tab_ev = self.clone();
            legacy.connect_event(move |_, ev| {
                match ev.event_type() {
                    gdk::EventType::ButtonPress => {
                        if let Some(btn_ev) = ev.downcast_ref::<gdk::ButtonEvent>()
                            && btn_ev.button() == 1
                        {
                            tab_ev.is_mouse_selecting.set(true);
                            (tab_ev.callbacks.on_hide_toolbar)();
                            tab_ev.view.remove_css_class("hide-caret-active");

                            if ev
                                .modifier_state()
                                .contains(gdk::ModifierType::CONTROL_MASK)
                                && let Some((x, y)) = ev.position()
                            {
                                let (bx, by) = tab_ev.view.window_to_buffer_coords(
                                    gtk4::TextWindowType::Widget,
                                    x as i32,
                                    y as i32,
                                );
                                if let Some(iter) = tab_ev.view.iter_at_location(bx, by) {
                                    let b_idx = iter.line().max(0) as usize;
                                    let off = iter.line_offset().max(0) as usize;
                                    let url =
                                        tab_ev.state.borrow().doc.blocks.get(b_idx).and_then(|b| {
                                            if off < b.char_len() {
                                                b.style_at(off + 1).link
                                            } else {
                                                None
                                            }
                                        });
                                    if let Some(u) = url {
                                        tab_ev.activate_link(&u);
                                    }
                                }
                            }
                        }
                    }
                    gdk::EventType::ButtonRelease => {
                        if let Some(btn_ev) = ev.downcast_ref::<gdk::ButtonEvent>()
                            && btn_ev.button() == 1
                        {
                            tab_ev.is_mouse_selecting.set(false);
                            let tab_after = tab_ev.clone();
                            glib::idle_add_local_once(move || {
                                let sel = tab_after.read_selection_from_buffer();
                                tab_after.state.borrow_mut().set_selection(sel);
                                if !sel.is_collapsed() {
                                    (tab_after.callbacks.on_selection_changed)(true);
                                } else {
                                    (tab_after.callbacks.on_hide_toolbar)();
                                    if let Some(diag) =
                                        tab_after.diagnostic_at(sel.focus.block, sel.focus.offset)
                                    {
                                        (tab_after.callbacks.on_extension_trigger)(
                                            ExtensionTrigger::ShowQuickFix {
                                                block: sel.focus.block,
                                                diagnostic: diag,
                                            },
                                        );
                                    }
                                    if tab_after.prefs.borrow().typewriter_mode {
                                        tab_after.schedule_scroll_caret();
                                    }
                                }
                            });
                        }
                    }
                    _ => {}
                }
                glib::Propagation::Proceed
            });
            self.view.add_controller(legacy);
        }

        {
            let rclick = gtk4::GestureClick::builder().button(3).build();
            rclick.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let tab_c = self.clone();
            rclick.connect_pressed(move |gesture, _, x, y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                (tab_c.callbacks.on_hide_toolbar)();
                tab_c.show_context_menu(x as i32, y as i32);
            });
            self.view.add_controller(rclick);
        }
    }

    fn show_context_menu(&self, x: i32, y: i32) {
        let menu = gio::Menu::new();
        let has_sel = !self.state.borrow().selection.is_collapsed();

        let sec_undo = gio::Menu::new();
        sec_undo.append(Some("Undo"), Some("ctx.undo"));
        sec_undo.append(Some("Redo"), Some("ctx.redo"));
        menu.append_section(None, &sec_undo);

        let sec_clip = gio::Menu::new();
        sec_clip.append(Some("Cut"), Some("ctx.cut"));
        sec_clip.append(Some("Copy"), Some("ctx.copy"));
        sec_clip.append(Some("Paste"), Some("ctx.paste"));
        sec_clip.append(Some("Paste Without Formatting"), Some("ctx.paste-plain"));
        menu.append_section(None, &sec_clip);

        let sec_export_clip = gio::Menu::new();
        sec_export_clip.append(
            Some(if has_sel {
                "Copy HTML"
            } else {
                "Copy Document as HTML"
            }),
            Some("ctx.copy-html"),
        );
        sec_export_clip.append(
            Some(if has_sel {
                "Copy Markdown"
            } else {
                "Copy Document as Markdown"
            }),
            Some("ctx.copy-md"),
        );
        menu.append_section(None, &sec_export_clip);

        let sec_insert = gio::Menu::new();
        sec_insert.append(Some("Insert Comment"), Some("ctx.comment"));
        sec_insert.append(Some("Insert Picture from File…"), Some("ctx.img-file"));
        sec_insert.append(Some("Link Picture from URL…"), Some("ctx.img-url"));
        sec_insert.append(Some("Insert Horizontal Rule"), Some("ctx.hr"));
        menu.append_section(None, &sec_insert);

        let sec_dir = gio::Menu::new();
        sec_dir.append(Some("Left-to-Right Writing"), Some("ctx.dir-ltr"));
        sec_dir.append(Some("Right-to-Left Writing"), Some("ctx.dir-rtl"));
        menu.append_section(None, &sec_dir);

        let group = gio::SimpleActionGroup::new();
        let add_act = |name: &str, enabled: bool, f: Box<dyn Fn()>| {
            let act = gio::SimpleAction::new(name, None);
            act.set_enabled(enabled);
            act.connect_activate(move |_, _| f());
            group.add_action(&act);
        };

        let st = self.state.borrow();
        let can_undo = st.undo.can_undo();
        let can_redo = st.undo.can_redo();
        drop(st);

        let t = self.clone();
        add_act("undo", can_undo, Box::new(move || t.undo()));
        let t = self.clone();
        add_act("redo", can_redo, Box::new(move || t.redo()));
        let t = self.clone();
        add_act("cut", has_sel, Box::new(move || t.cut_to_clipboard()));
        let t = self.clone();
        add_act("copy", has_sel, Box::new(move || t.copy_to_clipboard()));
        let t = self.clone();
        add_act(
            "paste",
            true,
            Box::new(move || t.paste_from_clipboard(false)),
        );
        let t = self.clone();
        add_act(
            "paste-plain",
            true,
            Box::new(move || t.paste_from_clipboard(true)),
        );
        let t = self.clone();
        add_act(
            "copy-html",
            true,
            Box::new(move || t.copy_html_to_clipboard()),
        );
        let t = self.clone();
        add_act(
            "copy-md",
            true,
            Box::new(move || t.copy_markdown_to_clipboard()),
        );
        let t = self.clone();
        add_act(
            "comment",
            true,
            Box::new(move || t.open_insert_comment_dialog()),
        );
        let t = self.clone();
        add_act(
            "img-file",
            true,
            Box::new(move || t.open_insert_image_file_dialog()),
        );
        let t = self.clone();
        add_act(
            "img-url",
            true,
            Box::new(move || t.open_insert_image_url_dialog()),
        );
        let t = self.clone();
        add_act(
            "hr",
            true,
            Box::new(move || {
                let patch = t.state.borrow_mut().insert_horizontal_rule();
                t.apply_patch(patch);
            }),
        );
        let t = self.clone();
        add_act(
            "dir-ltr",
            true,
            Box::new(move || t.set_direction(TextDirection::Ltr)),
        );
        let t = self.clone();
        add_act(
            "dir-rtl",
            true,
            Box::new(move || t.set_direction(TextDirection::Rtl)),
        );

        let pop = gtk4::PopoverMenu::from_model(Some(&menu));
        pop.insert_action_group("ctx", Some(&group));
        pop.set_parent(&self.view);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 1, 1)));
        let pop_c = pop.clone();
        pop.connect_closed(move |_| {
            pop_c.unparent();
        });
        pop.popup();
    }
}

const BASE_SIDE_MARGIN: i32 = 24;

const ALL_FORMAT_TAGS: &[&str] = &[
    "paragraph",
    "image_block",
    "hr_block",
    "h1",
    "h2",
    "h3",
    "h4",
    "blockquote",
    "codeblock",
    "bullet_0",
    "bullet_1",
    "bullet_2",
    "bullet_3",
    "bullet_4",
    "ordered_0",
    "ordered_1",
    "ordered_2",
    "ordered_3",
    "ordered_4",
    "dir_rtl",
    "dir_ltr",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "code",
    "link",
    "code_kw",
    "code_str",
    "code_num",
    "code_cmt",
    "code_type",
];

fn list_tag_name(ordered: bool, depth: u8) -> &'static str {
    let d = depth.min(4);
    match (ordered, d) {
        (false, 0) => "bullet_0",
        (false, 1) => "bullet_1",
        (false, 2) => "bullet_2",
        (false, 3) => "bullet_3",
        (false, _) => "bullet_4",
        (true, 0) => "ordered_0",
        (true, 1) => "ordered_1",
        (true, 2) => "ordered_2",
        (true, 3) => "ordered_3",
        (true, _) => "ordered_4",
    }
}

pub fn create_shared_tag_table(prefs: &Preferences) -> gtk4::TextTagTable {
    let table = gtk4::TextTagTable::new();
    populate_tag_table(&table, prefs);
    table
}

fn setup_tags(buffer: &gtk4::TextBuffer, prefs: &Preferences) {
    let table = buffer.tag_table();
    populate_tag_table(&table, prefs);
}

fn populate_tag_table(table: &gtk4::TextTagTable, prefs: &Preferences) {
    let add = |tag: gtk4::TextTag| {
        table.add(&tag);
    };

    add(gtk4::TextTag::builder().name("paragraph").build());
    add(gtk4::TextTag::builder()
        .name("image_block")
        .justification(gtk4::Justification::Center)
        .left_margin(0)
        .right_margin(0)
        .line_height(1.0)
        .pixels_above_lines(16)
        .pixels_below_lines(18)
        .build());
    add(gtk4::TextTag::builder()
        .name("hr_block")
        .justification(gtk4::Justification::Center)
        .pixels_above_lines(14)
        .pixels_below_lines(14)
        .build());
    add(gtk4::TextTag::builder()
        .name("h1")
        .scale(1.55)
        .weight(pango::Weight::Bold.into_glib())
        .build());
    add(gtk4::TextTag::builder()
        .name("h2")
        .scale(1.30)
        .weight(pango::Weight::Bold.into_glib())
        .build());
    add(gtk4::TextTag::builder()
        .name("h3")
        .scale(1.14)
        .weight(pango::Weight::Bold.into_glib())
        .build());
    add(gtk4::TextTag::builder()
        .name("h4")
        .scale(1.05)
        .weight(pango::Weight::Bold.into_glib())
        .build());
    add(gtk4::TextTag::builder()
        .name("blockquote")
        .accumulative_margin(true)
        .left_margin(28)
        .right_margin(18)
        .style(pango::Style::Italic)
        .build());
    add(gtk4::TextTag::builder()
        .name("codeblock")
        .accumulative_margin(true)
        .left_margin(18)
        .right_margin(18)
        .family("Monospace")
        .scale(0.84)
        .build());

    for d in 0..=4u8 {
        for ordered in [false, true] {
            add(gtk4::TextTag::builder()
                .name(list_tag_name(ordered, d))
                .accumulative_margin(true)
                .left_margin(20 * (i32::from(d) + 1))
                .build());
        }
    }

    add(gtk4::TextTag::builder()
        .name("dir_rtl")
        .justification(gtk4::Justification::Right)
        .build());
    add(gtk4::TextTag::builder()
        .name("dir_ltr")
        .justification(gtk4::Justification::Left)
        .build());

    add(gtk4::TextTag::builder()
        .name("bold")
        .weight(pango::Weight::Bold.into_glib())
        .build());
    add(gtk4::TextTag::builder()
        .name("italic")
        .style(pango::Style::Italic)
        .build());
    add(gtk4::TextTag::builder()
        .name("underline")
        .underline(pango::Underline::Single)
        .build());
    add(gtk4::TextTag::builder()
        .name("strikethrough")
        .strikethrough(true)
        .build());
    add(gtk4::TextTag::builder()
        .name("code")
        .family("Monospace")
        .scale(0.90)
        .build());
    add(gtk4::TextTag::builder()
        .name("link")
        .underline(pango::Underline::Single)
        .build());
    add(gtk4::TextTag::builder().name("code_kw").build());
    add(gtk4::TextTag::builder().name("code_str").build());
    add(gtk4::TextTag::builder().name("code_num").build());
    add(gtk4::TextTag::builder()
        .name("code_cmt")
        .style(pango::Style::Italic)
        .build());
    add(gtk4::TextTag::builder().name("code_type").build());
    add(gtk4::TextTag::builder().name("search_match").build());
    add(gtk4::TextTag::builder().name("search_current").build());
    add(gtk4::TextTag::builder()
        .name("diagnostic")
        .underline(pango::Underline::Error)
        .build());
    add(gtk4::TextTag::builder().name("focus_dim").build());

    update_tag_styles_table(table, prefs);
}

fn update_tag_styles(buffer: &gtk4::TextBuffer, prefs: &Preferences) {
    update_tag_styles_table(&buffer.tag_table(), prefs);
}

pub fn update_tag_styles_table(table: &gtk4::TextTagTable, prefs: &Preferences) {
    let pal = resolve_palette(prefs);
    let ls = prefs.line_spacing.factor();
    let h_ls = prefs.line_spacing.heading_factor();
    let p_below = if prefs.paragraph_margins { 12 } else { 0 };
    if let Some(t) = table.lookup("paragraph") {
        t.set_line_height(ls);
        t.set_pixels_above_lines(0);
        t.set_pixels_below_lines(p_below);
    }
    for (name, above, below) in [("h1", 18, 8), ("h2", 16, 6), ("h3", 12, 5), ("h4", 8, 4)] {
        if let Some(t) = table.lookup(name) {
            t.set_line_height(h_ls);
            t.set_pixels_above_lines(if prefs.paragraph_margins { above } else { 2 });
            t.set_pixels_below_lines(if prefs.paragraph_margins { below } else { 2 });
        }
    }

    if let Some(t) = table.lookup("blockquote") {
        t.set_line_height(ls);
        t.set_pixels_above_lines(if prefs.paragraph_margins { 6 } else { 2 });
        t.set_pixels_below_lines(if prefs.paragraph_margins { 12 } else { 2 });
        let muted = rgba_from_css(&pal.muted_text, gdk::RGBA::new(0.45, 0.45, 0.45, 1.0));
        t.set_foreground_rgba(Some(&muted));
    }

    if let Some(t) = table.lookup("codeblock") {
        t.set_line_height(1.42);
        t.set_pixels_above_lines(12);
        t.set_pixels_below_lines(14);
    }

    for d in 0..=4u8 {
        for ordered in [false, true] {
            if let Some(t) = table.lookup(list_tag_name(ordered, d)) {
                t.set_line_height(ls);
                t.set_pixels_above_lines(0);
                t.set_pixels_below_lines(if prefs.paragraph_margins { 2 } else { 0 });
            }
        }
    }

    if let Some(t) = table.lookup("code") {
        t.set_background_rgba(None);
    }
    if let Some(t) = table.lookup("link") {
        let accent = rgba_from_css(&pal.accent, gdk::RGBA::new(0.16, 0.73, 1.0, 1.0));
        let underline_c = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.72);
        t.set_foreground_rgba(None);
        t.set_underline_rgba(Some(&underline_c));
    }
    let (kw_c, str_c, num_c, cmt_c, type_c) = if pal.is_dark {
        (
            gdk::RGBA::new(0.76, 0.65, 0.90, 1.0),
            gdk::RGBA::new(0.60, 0.78, 0.62, 1.0),
            gdk::RGBA::new(0.85, 0.70, 0.50, 1.0),
            gdk::RGBA::new(0.56, 0.58, 0.61, 0.85),
            gdk::RGBA::new(0.55, 0.74, 0.82, 1.0),
        )
    } else {
        (
            gdk::RGBA::new(0.45, 0.27, 0.58, 1.0),
            gdk::RGBA::new(0.24, 0.47, 0.28, 1.0),
            gdk::RGBA::new(0.64, 0.37, 0.14, 1.0),
            gdk::RGBA::new(0.48, 0.50, 0.53, 0.90),
            gdk::RGBA::new(0.18, 0.44, 0.55, 1.0),
        )
    };
    for (name, color) in [
        ("code_kw", kw_c),
        ("code_str", str_c),
        ("code_num", num_c),
        ("code_cmt", cmt_c),
        ("code_type", type_c),
    ] {
        if let Some(t) = table.lookup(name) {
            t.set_foreground_rgba(Some(&color));
        }
    }
    if let Some(t) = table.lookup("search_match") {
        let bg = rgba_from_css(&pal.search_bg, gdk::RGBA::new(1.0, 0.88, 0.4, 0.55));
        t.set_background_rgba(Some(&bg));
    }
    if let Some(t) = table.lookup("search_current") {
        let bg = rgba_from_css(&pal.search_current_bg, gdk::RGBA::new(1.0, 0.65, 0.2, 0.80));
        t.set_background_rgba(Some(&bg));
    }
    if let Some(t) = table.lookup("diagnostic") {
        let diag_c = if pal.is_dark {
            gdk::RGBA::new(0.95, 0.68, 0.25, 0.88)
        } else {
            gdk::RGBA::new(0.84, 0.44, 0.08, 0.88)
        };
        t.set_underline_rgba(Some(&diag_c));
    }
    if let Some(t) = table.lookup("focus_dim") {
        let base = rgba_from_css(&pal.text, gdk::RGBA::new(0.24, 0.24, 0.24, 1.0));
        let alpha = prefs.focus_mode_opacity.clamp(0.05, 1.0) as f32;
        let dimmed = gdk::RGBA::new(base.red(), base.green(), base.blue(), alpha);
        t.set_foreground_rgba(Some(&dimmed));
        t.set_priority(table.size() - 1);
    }
}

fn block_needs_trailing_sentinel(block: &crate::model::Block) -> bool {
    matches!(block.inlines.last(), Some(Inline::LineBreak))
}

fn iter_at_doc_pos(buffer: &gtk4::TextBuffer, pos: DocPos) -> Option<gtk4::TextIter> {
    let mut iter = buffer.iter_at_line(pos.block as i32)?;
    let mut line_end = iter;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }
    let max_off = line_end.line_offset().max(0);
    let clamped = (pos.offset as i32).min(max_off);
    iter.set_line_offset(clamped);
    Some(iter)
}

fn line_iter_bounds(
    buffer: &gtk4::TextBuffer,
    line: usize,
) -> Option<(gtk4::TextIter, gtk4::TextIter)> {
    let start = buffer.iter_at_line(line as i32)?;
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    Some((start, end))
}

const MAX_CLIPBOARD_HTML_BYTES: usize = 16 * 1024 * 1024;

fn read_stream_to_string_async(
    stream: gio::InputStream,
    on_done: impl FnOnce(Option<String>) + 'static,
) {
    fn read_next(
        stream: gio::InputStream,
        mut acc: Vec<u8>,
        on_done: Box<dyn FnOnce(Option<String>)>,
    ) {
        let stream_c = stream.clone();
        stream.read_bytes_async(
            16 * 1024,
            glib::Priority::DEFAULT,
            gio::Cancellable::NONE,
            move |res| match res {
                Ok(bytes) if bytes.is_empty() => {
                    stream_c.close_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, |_| {});
                    on_done(String::from_utf8(acc).ok());
                }
                Ok(bytes) => {
                    if acc.len().saturating_add(bytes.len()) > MAX_CLIPBOARD_HTML_BYTES {
                        stream_c.close_async(
                            glib::Priority::DEFAULT,
                            gio::Cancellable::NONE,
                            |_| {},
                        );
                        on_done(None);
                        return;
                    }
                    acc.extend_from_slice(&bytes);
                    read_next(stream_c, acc, on_done);
                }
                Err(_) => {
                    stream_c.close_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, |_| {});
                    on_done(None);
                }
            },
        );
    }

    read_next(stream, Vec::new(), Box::new(on_done));
}

pub fn is_image_file_path(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
        )
    })
}

fn is_standalone_image_block(block: &crate::model::Block) -> bool {
    if block.kind != BlockKind::Paragraph {
        return false;
    }
    let mut image_count = 0usize;
    for inline in &block.inlines {
        match inline {
            Inline::Image { .. } => image_count += 1,
            Inline::Text(run) if run.text.trim().is_empty() => {}
            _ => return false,
        }
    }
    image_count == 1
}

fn compute_image_display_size(
    nat_w: f64,
    nat_h: f64,
    raw_vw: i32,
    raw_vh: i32,
    col_px: i32,
    standalone: bool,
) -> (i32, i32) {
    let eff_vw = if raw_vw > 1 { raw_vw } else { 960 };
    let eff_vh = if raw_vh > 1 { raw_vh } else { 680 };
    let (max_w, max_h) = if standalone {
        let side_pad = if eff_vw < 480 { 12.0 } else { 60.0 };
        let mw = (eff_vw as f64 - side_pad)
            .min((eff_vw as f64 - 8.0).max(32.0))
            .max(64.0);
        let mh = (eff_vh as f64 * 0.66).max(160.0);
        (mw, mh)
    } else {
        let mw = (col_px.min(eff_vw - 24)).max(64) as f64;
        let mh = (eff_vh as f64 * 0.55).max(120.0);
        (mw, mh)
    };

    let mut w = nat_w.max(1.0);
    let mut h = nat_h.max(1.0);
    if w > max_w {
        let s = max_w / w;
        w *= s;
        h *= s;
    }
    if h > max_h {
        let s = max_h / h;
        w *= s;
        h *= s;
    }
    (w.round().max(16.0) as i32, h.round().max(16.0) as i32)
}

fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            out.push(((hi << 4) | lo) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn resolve_local_image_path(src: &str, doc_path: Option<&std::path::Path>) -> Option<PathBuf> {
    let raw = if let Some(rest) = src.strip_prefix("file://") {
        let decoded = percent_decode_path(rest);
        Some(PathBuf::from(decoded))
    } else if !src.contains(':') {
        let p = PathBuf::from(src);
        if p.is_absolute() {
            Some(p)
        } else {
            doc_path
                .and_then(|dp| dp.parent())
                .map(|dir| dir.join(src))
                .or(Some(p))
        }
    } else {
        None
    }?;
    if raw.exists() {
        return Some(raw);
    }
    if src.contains('%') && !src.starts_with("file://") {
        let decoded = percent_decode_path(src);
        let alt = if std::path::Path::new(&decoded).is_absolute() {
            PathBuf::from(decoded)
        } else {
            doc_path
                .and_then(|dp| dp.parent())
                .map(|dir| dir.join(&decoded))
                .unwrap_or_else(|| PathBuf::from(decoded))
        };
        if alt.exists() {
            return Some(alt);
        }
    }
    None
}

fn build_inline_image_widget(
    src: &str,
    alt: &str,
    doc_path: Option<&std::path::Path>,
    raw_vw: i32,
    raw_vh: i32,
    col_px: i32,
    standalone: bool,
) -> (gtk4::Widget, Option<(gtk4::Picture, f64, f64)>) {
    let texture: Option<gdk::Texture> = if let Some((_, bytes)) =
        crate::io::docx::decode_data_uri_image(src)
    {
        gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)).ok()
    } else {
        resolve_local_image_path(src, doc_path).and_then(|p| gdk::Texture::from_filename(&p).ok())
    };

    if let Some(tex) = texture {
        let nat_w = tex.width().max(1) as f64;
        let nat_h = tex.height().max(1) as f64;
        let (w, h) = compute_image_display_size(nat_w, nat_h, raw_vw, raw_vh, col_px, standalone);
        let pic = gtk4::Picture::for_paintable(&tex);
        pic.set_can_shrink(true);
        pic.set_content_fit(gtk4::ContentFit::Contain);
        pic.set_size_request(w, h);
        if !alt.is_empty() {
            pic.set_tooltip_text(Some(alt));
        }
        return (pic.clone().upcast(), Some((pic, nat_w, nat_h)));
    }

    let short_src = if src.starts_with("data:") {
        "embedded image"
    } else {
        src
    };
    let label_text = if alt.is_empty() {
        format!("[Image: {short_src}]")
    } else {
        format!("[Image: {alt}]")
    };
    let pill = gtk4::Label::new(Some(&label_text));
    pill.add_css_class("sillpaper-comment-badge");
    (pill.upcast(), None)
}

fn take_next_word(s: &str) -> (&str, &str) {
    let mut seen_non_ws = false;
    for (idx, ch) in s.char_indices() {
        if !ch.is_whitespace() {
            seen_non_ws = true;
        } else if seen_non_ws {
            return (&s[..idx], &s[idx..]);
        }
    }
    (s, "")
}
