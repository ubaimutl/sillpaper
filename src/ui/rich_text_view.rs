use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::translate::FromGlib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockDecoration {
    None,
    Blockquote,
    Bullet { depth: u8 },
    Ordered { depth: u8, number: u32 },
    CodeBlock { language: Option<String> },
}

#[derive(Debug, Clone, Copy)]
pub struct DecorationColors {
    pub text: gdk::RGBA,
    pub muted: gdk::RGBA,
    pub quote_bar: gdk::RGBA,
    pub code_bg: gdk::RGBA,
    pub border: gdk::RGBA,
    pub selection_bg: gdk::RGBA,
}

impl Default for DecorationColors {
    fn default() -> Self {
        Self {
            text: gdk::RGBA::new(0.24, 0.24, 0.24, 1.0),
            muted: gdk::RGBA::new(0.48, 0.48, 0.48, 1.0),
            quote_bar: gdk::RGBA::new(0.24, 0.24, 0.24, 0.28),
            code_bg: gdk::RGBA::new(0.0, 0.0, 0.0, 0.05),
            border: gdk::RGBA::new(0.0, 0.0, 0.0, 0.10),
            selection_bg: gdk::RGBA::new(0.16, 0.43, 0.66, 0.55),
        }
    }
}

fn scale_alpha(c: gdk::RGBA, factor: f32) -> gdk::RGBA {
    gdk::RGBA::new(
        c.red(),
        c.green(),
        c.blue(),
        (c.alpha() * factor).clamp(0.02, 1.0),
    )
}

fn line_vertical_margins(view: &gtk4::TextView, line_start: &gtk4::TextIter) -> (i32, i32) {
    let mut above = view.pixels_above_lines();
    let mut below = view.pixels_below_lines();
    for tag in line_start.tags() {
        if tag.is_pixels_above_lines_set() {
            above = tag.pixels_above_lines();
        }
        if tag.is_pixels_below_lines_set() {
            below = tag.pixels_below_lines();
        }
    }
    (above.max(0), below.max(0))
}

fn merge_x_spans(spans: &mut [(i32, i32)]) -> Vec<(i32, i32)> {
    if spans.is_empty() {
        return Vec::new();
    }
    spans.sort_unstable();
    let mut merged: Vec<(i32, i32)> = Vec::with_capacity(spans.len().min(8));
    let (mut cur_l, mut cur_r) = spans[0];
    for &(l, r) in &spans[1..] {
        if l <= cur_r + 1 {
            if r > cur_r {
                cur_r = r;
            }
        } else {
            if cur_r > cur_l {
                merged.push((cur_l, cur_r));
            }
            cur_l = l;
            cur_r = r;
        }
    }
    if cur_r > cur_l {
        merged.push((cur_l, cur_r));
    }
    merged
}

struct DisplayLineInfo {
    start: gtk4::TextIter,
    end: gtk4::TextIter,
    min_y: i32,
    max_bot: i32,
}

fn compute_selection_rects(
    view: &gtk4::TextView,
    sel_start: gtk4::TextIter,
    sel_end: gtk4::TextIter,
    col_inset: i32,
) -> Vec<graphene::Rect> {
    if sel_start >= sel_end {
        return Vec::new();
    }
    let buffer = view.buffer();
    let vis = view.visible_rect();
    let has_vis = vis.height() > 0;
    let vis_top = vis.y().max(0);
    let vis_bottom = if has_vis {
        vis_top.saturating_add(vis.height())
    } else {
        i32::MAX
    };

    let (first_vis_line, last_vis_line) = if has_vis {
        let (first_it, _) = view.line_at_y(vis_top);
        let (last_it, _) = view.line_at_y(vis_bottom);
        (first_it.line().max(0), last_it.line().max(0))
    } else {
        (sel_start.line(), sel_end.line())
    };

    let start_line = sel_start.line().max(first_vis_line);
    let end_line = sel_end.line().min(last_vis_line);
    if start_line > end_line {
        return Vec::new();
    }

    let mut rects = Vec::new();
    let mut spans_scratch: Vec<(i32, i32)> = Vec::with_capacity(64);
    let left_edge = col_inset.max(view.left_margin());
    let right_edge = (view.width() - col_inset.max(view.right_margin())).max(left_edge + 16);

    for line_idx in start_line..=end_line {
        let Some(line_start) = buffer.iter_at_line(line_idx) else {
            continue;
        };
        let (yr_y, yr_h) = view.line_yrange(&line_start);
        if has_vis && (yr_y + yr_h < vis_top || yr_y > vis_bottom) {
            continue;
        }

        let (above, below) = line_vertical_margins(view, &line_start);
        let content_top = yr_y + above;
        let content_bottom = (yr_y + yr_h - below).max(content_top + 1);

        let mut cur = line_start;
        if has_vis
            && yr_y < vis_top
            && let Some(vis_it) = view.iter_at_location(left_edge, vis_top)
            && vis_it.line() == line_idx
            && vis_it > cur
        {
            cur = vis_it;
        }
        if sel_start.line() == line_idx && sel_start > cur {
            cur = sel_start;
        }
        if !view.starts_display_line(&cur) {
            view.backward_display_line_start(&mut cur);
        }
        if cur > line_start {
            let mut prev = cur;
            if view.backward_display_line(&mut prev) && prev.line() == line_idx {
                cur = prev;
            } else {
                cur = line_start;
            }
        }

        let mut dlines: Vec<DisplayLineInfo> = Vec::new();
        loop {
            let mut d_end = cur;
            view.forward_display_line_end(&mut d_end);

            let loc_s = view.iter_location(&cur);
            let mut min_y = loc_s.y();
            let mut max_bot = loc_s.y() + loc_s.height();
            if cur < d_end {
                let mut scan = cur;
                while scan < d_end {
                    let loc = view.iter_location(&scan);
                    if loc.y() < min_y {
                        min_y = loc.y();
                    }
                    let bot = loc.y() + loc.height();
                    if bot > max_bot {
                        max_bot = bot;
                    }
                    if !scan.forward_char() {
                        break;
                    }
                }
            }

            let ended_buf_line = d_end.ends_line();
            let after_vis_or_sel = min_y > vis_bottom || cur > sel_end;
            dlines.push(DisplayLineInfo {
                start: cur,
                end: d_end,
                min_y,
                max_bot,
            });

            if ended_buf_line || after_vis_or_sel {
                break;
            }
            let mut nxt = cur;
            if !view.forward_display_line(&mut nxt) || nxt.line() != line_idx || nxt <= cur {
                break;
            }
            cur = nxt;
        }

        let num_d = dlines.len();
        for di in 0..num_d {
            let d = &dlines[di];
            let is_first_of_para = d.start == line_start;
            let is_last_of_para = d.end.ends_line();

            let mut box_top = if is_first_of_para {
                content_top
            } else if di > 0 {
                (dlines[di - 1].max_bot + d.min_y) / 2
            } else {
                d.min_y
            };
            let mut box_bot = if is_last_of_para {
                content_bottom
            } else if di + 1 < num_d {
                (d.max_bot + dlines[di + 1].min_y) / 2
            } else {
                d.max_bot
            };
            box_top = box_top.max(content_top);
            box_bot = box_bot.min(content_bottom).max(box_top + 1);

            let logical_end = if di + 1 < num_d {
                dlines[di + 1].start
            } else {
                d.end
            };

            if d.start == d.end {
                if sel_end.line() > line_idx {
                    let loc = view.iter_location(&d.start);
                    let stub_x = if loc.x() >= right_edge - 6 {
                        (loc.x() - 8).max(left_edge)
                    } else {
                        loc.x()
                    };
                    rects.push(graphene::Rect::new(
                        stub_x as f32,
                        box_top as f32,
                        8.0,
                        (box_bot - box_top) as f32,
                    ));
                }
                continue;
            }

            let seg_s = if sel_start > d.start {
                sel_start
            } else {
                d.start
            };
            let seg_e = if sel_end < logical_end {
                sel_end
            } else {
                logical_end
            };
            if seg_s >= seg_e {
                continue;
            }

            spans_scratch.clear();
            let mut it = seg_s;
            while it < seg_e {
                let loc = view.iter_location(&it);
                if it >= d.end && (loc.y() < d.min_y - 4 || loc.y() > d.max_bot + 4) {
                    if !it.forward_char() {
                        break;
                    }
                    continue;
                }
                if loc.width() > 0 {
                    spans_scratch.push((loc.x(), loc.x() + loc.width()));
                } else if loc.width() < 0 {
                    spans_scratch.push((loc.x() + loc.width(), loc.x()));
                }
                if !it.forward_char() {
                    break;
                }
            }

            for (x0, x1) in merge_x_spans(&mut spans_scratch) {
                rects.push(graphene::Rect::new(
                    x0 as f32,
                    box_top as f32,
                    (x1 - x0) as f32,
                    (box_bot - box_top) as f32,
                ));
            }
        }
    }

    rects
}

mod imp {
    use super::*;

    pub struct RichTextView {
        pub decorations: RefCell<Vec<BlockDecoration>>,
        pub has_decorations: Cell<bool>,
        pub colors: RefCell<DecorationColors>,
        pub focus_line: Cell<Option<usize>>,
        pub focus_opacity: Cell<f32>,
        pub override_selection: Cell<Option<(i32, i32)>>,
        pub ghost_text: RefCell<Option<(i32, String)>>,
        pub column_inset: Cell<i32>,
    }

    impl Default for RichTextView {
        fn default() -> Self {
            Self {
                decorations: RefCell::new(Vec::new()),
                has_decorations: Cell::new(false),
                colors: RefCell::new(DecorationColors::default()),
                focus_line: Cell::new(None),
                focus_opacity: Cell::new(1.0),
                override_selection: Cell::new(None),
                ghost_text: RefCell::new(None),
                column_inset: Cell::new(24),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RichTextView {
        const NAME: &'static str = "SillpaperRichTextView";
        type Type = super::RichTextView;
        type ParentType = gtk4::TextView;
    }

    impl ObjectImpl for RichTextView {}
    impl WidgetImpl for RichTextView {}

    impl TextViewImpl for RichTextView {
        fn snapshot_layer(&self, layer: gtk4::TextViewLayer, snapshot: gtk4::Snapshot) {
            self.parent_snapshot_layer(layer, snapshot.clone());

            let view = self.obj();
            let colors = *self.colors.borrow();

            if layer == gtk4::TextViewLayer::AboveText {
                let ghost = self.ghost_text.borrow();
                if let Some((off, ref text)) = *ghost
                    && !text.is_empty()
                {
                    let buffer = view.buffer();
                    let clamped = off.clamp(0, buffer.char_count());
                    let iter = buffer.iter_at_offset(clamped);
                    let loc = view.iter_location(&iter);

                    let mut font_desc = view.pango_context().font_description().unwrap_or_default();
                    let tag_iter = if clamped > 0 && iter.ends_line() {
                        let mut prev = iter;
                        prev.backward_char();
                        prev
                    } else {
                        iter
                    };
                    for tag in tag_iter.tags() {
                        if tag.is_family_set()
                            && let Some(fam) = tag.family()
                        {
                            font_desc.set_family(&fam);
                        }
                        if tag.is_scale_set() {
                            let base_sz = font_desc.size();
                            if base_sz > 0 {
                                let scaled = (base_sz as f64 * tag.scale()).round();
                                if font_desc.is_size_absolute() {
                                    font_desc.set_absolute_size(scaled);
                                } else {
                                    font_desc.set_size(scaled as i32);
                                }
                            }
                        }
                        if tag.is_weight_set() {
                            let w = unsafe { pango::Weight::from_glib(tag.weight()) };
                            font_desc.set_weight(w);
                        }
                        if tag.is_style_set() {
                            font_desc.set_style(tag.style());
                        }
                    }

                    let mut d_start = iter;
                    if !view.starts_display_line(&d_start) {
                        view.backward_display_line_start(&mut d_start);
                    }
                    let mut d_end = d_start;
                    view.forward_display_line_end(&mut d_end);
                    let line_sample = if d_start < d_end {
                        buffer.text(&d_start, &d_end, false).to_string()
                    } else {
                        "Ag".to_string()
                    };
                    let ref_layout = view.create_pango_layout(Some(&line_sample));
                    ref_layout.set_font_description(Some(&font_desc));
                    let line_baseline = ref_layout.baseline() as f32 / pango::SCALE as f32;

                    let display_text = text.replace('\n', " ");
                    let layout = view.create_pango_layout(Some(&display_text));
                    layout.set_font_description(Some(&font_desc));
                    layout.set_single_paragraph_mode(true);
                    layout.set_ellipsize(pango::EllipsizeMode::End);

                    let col_left = self.column_inset.get().max(view.left_margin());
                    let col_right = self.column_inset.get().max(view.right_margin());
                    let right_edge = (view.width() - col_right).max(col_left + 80);
                    let avail_inline = right_edge - loc.x() - 4;
                    let ghost_color = scale_alpha(colors.text, 0.42);

                    if iter.ends_line() && avail_inline >= 90 {
                        layout.set_width(avail_inline * pango::SCALE);
                        let ghost_baseline = layout.baseline() as f32 / pango::SCALE as f32;
                        let draw_x = loc.x() as f32 + 2.5;
                        let draw_y = loc.y() as f32 + (line_baseline - ghost_baseline);
                        snapshot.save();
                        snapshot.translate(&graphene::Point::new(draw_x, draw_y));
                        snapshot.append_layout(&layout, &ghost_color);
                        snapshot.restore();
                    } else {
                        let max_w = (right_edge - col_left).clamp(140, 440);
                        layout.set_single_paragraph_mode(false);
                        layout.set_wrap(pango::WrapMode::WordChar);
                        layout.set_ellipsize(pango::EllipsizeMode::None);
                        layout.set_width(max_w * pango::SCALE);
                        let (pw, ph) = layout.pixel_size();
                        let pill_w = (pw as f32 + 16.0).min(max_w as f32 + 16.0);
                        let pill_h = ph as f32 + 8.0;
                        let px = (loc.x() as f32)
                            .min((right_edge as f32 - pill_w).max(col_left as f32))
                            .max(col_left as f32);
                        let py = (loc.y() + loc.height() + 4) as f32;
                        let rect = graphene::Rect::new(px, py, pill_w, pill_h);
                        let rounded = gsk::RoundedRect::from_rect(rect, 6.0);
                        let bg = scale_alpha(colors.code_bg, 1.0);
                        let bdr = scale_alpha(colors.border, 1.0);
                        snapshot.push_rounded_clip(&rounded);
                        snapshot.append_color(&bg, &rect);
                        snapshot.pop();
                        snapshot.append_border(&rounded, &[1.0; 4], &[bdr; 4]);
                        snapshot.save();
                        snapshot.translate(&graphene::Point::new(px + 8.0, py + 4.0));
                        snapshot.append_layout(&layout, &scale_alpha(colors.text, 0.72));
                        snapshot.restore();
                    }
                }
                return;
            }

            if layer != gtk4::TextViewLayer::BelowText {
                return;
            }

            if self.has_decorations.get() {
                let decorations = self.decorations.borrow();
                let focus_line = self.focus_line.get();
                let focus_opacity = self.focus_opacity.get();

                let buffer = view.buffer();
                let vis = view.visible_rect();
                let (first_iter, _) = view.line_at_y(vis.y().max(0));
                let (last_iter, _) = view.line_at_y((vis.y() + vis.height()).max(0));

                let first_line = first_iter.line().max(0) as usize;
                let last_line =
                    (last_iter.line().max(0) as usize).min(decorations.len().saturating_sub(1));

                let view_width = view.width() as f32;
                let base_left = self.column_inset.get().max(view.left_margin()) as f32;
                let base_right = self.column_inset.get().max(view.right_margin()) as f32;
                let content_width = (view_width - base_left - base_right).max(120.0);

                for (line_idx, deco) in decorations
                    .iter()
                    .enumerate()
                    .take(last_line + 1)
                    .skip(first_line)
                {
                    if *deco == BlockDecoration::None {
                        continue;
                    }
                    let Some(line_start) = buffer.iter_at_line(line_idx as i32) else {
                        continue;
                    };
                    let start_rect = view.iter_location(&line_start);
                    let mut line_end = line_start;
                    if !line_end.ends_line() {
                        line_end.forward_to_line_end();
                    }
                    let end_rect = view.iter_location(&line_end);

                    let top_y = start_rect.y() as f32;
                    let bottom_y = (end_rect.y() + end_rect.height())
                        .max(start_rect.y() + start_rect.height())
                        as f32;
                    let block_h = (bottom_y - top_y).max(start_rect.height() as f32);
                    let text_x = start_rect.x() as f32;

                    let is_dimmed = focus_line.is_some_and(|active| active != line_idx);
                    let alpha_mul = if is_dimmed { focus_opacity } else { 1.0 };

                    match deco {
                        BlockDecoration::None => {}
                        BlockDecoration::Blockquote => {
                            let bar_x = (text_x - 16.0).max(4.0);
                            let rect = graphene::Rect::new(
                                bar_x,
                                top_y + 2.0,
                                2.5,
                                (block_h - 4.0).max(8.0),
                            );
                            let bar_color = scale_alpha(colors.quote_bar, alpha_mul);
                            let rounded = gsk::RoundedRect::from_rect(rect, 1.5);
                            snapshot.push_rounded_clip(&rounded);
                            snapshot.append_color(&bar_color, &rect);
                            snapshot.pop();
                        }
                        BlockDecoration::Bullet { depth } => {
                            let glyph = match depth % 3 {
                                0 => "•",
                                1 => "◦",
                                _ => "▪",
                            };
                            let layout = view.create_pango_layout(Some(glyph));
                            let (pw, _ph) = layout.pixel_size();
                            let draw_x = (text_x - pw as f32 - 8.0).max(4.0);
                            let marker_color = scale_alpha(colors.text, 0.65 * alpha_mul);
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(draw_x, top_y));
                            snapshot.append_layout(&layout, &marker_color);
                            snapshot.restore();
                        }
                        BlockDecoration::Ordered { number, .. } => {
                            let label = format!("{number}.");
                            let layout = view.create_pango_layout(Some(&label));
                            let (pw, _ph) = layout.pixel_size();
                            let draw_x = (text_x - pw as f32 - 7.0).max(4.0);
                            let marker_color = scale_alpha(colors.text, 0.65 * alpha_mul);
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(draw_x, top_y));
                            snapshot.append_layout(&layout, &marker_color);
                            snapshot.restore();
                        }
                        BlockDecoration::CodeBlock { language } => {
                            let pad_v = 10.0;
                            let bg_x = base_left.max(4.0);
                            let bg_w = content_width.max(100.0);
                            let rect = graphene::Rect::new(
                                bg_x,
                                top_y - pad_v,
                                bg_w,
                                block_h + pad_v * 2.0,
                            );
                            let rounded = gsk::RoundedRect::from_rect(rect, 8.0);
                            let tint_alpha = if is_dimmed { 0.022 } else { 0.045 };
                            let bg = gdk::RGBA::new(
                                colors.text.red(),
                                colors.text.green(),
                                colors.text.blue(),
                                tint_alpha,
                            );
                            snapshot.push_rounded_clip(&rounded);
                            snapshot.append_color(&bg, &rect);
                            snapshot.pop();

                            if let Some(lang) = language.as_deref().filter(|s| !s.is_empty()) {
                                let layout = view.create_pango_layout(Some(lang));
                                let mut fd = pango::FontDescription::from_string("Monospace");
                                fd.set_absolute_size(10.5 * pango::SCALE as f64);
                                layout.set_font_description(Some(&fd));
                                let (lw, _lh) = layout.pixel_size();
                                let lx = (bg_x + bg_w - lw as f32 - 12.0).max(bg_x + 12.0);
                                let ly = top_y - pad_v + 6.0;
                                let label_c = scale_alpha(colors.muted, 0.36 * alpha_mul);
                                snapshot.save();
                                snapshot.translate(&graphene::Point::new(lx, ly));
                                snapshot.append_layout(&layout, &label_c);
                                snapshot.restore();
                            }
                        }
                    }
                }
            }

            for rect in view.selection_rects() {
                snapshot.append_color(&colors.selection_bg, &rect);
            }
        }
    }
}

glib::wrapper! {
    pub struct RichTextView(ObjectSubclass<imp::RichTextView>)
        @extends gtk4::TextView, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Scrollable;
}

impl Default for RichTextView {
    fn default() -> Self {
        Self::new()
    }
}

impl RichTextView {
    pub fn new() -> Self {
        let obj: Self = glib::Object::new();
        obj.set_wrap_mode(gtk4::WrapMode::WordChar);
        obj.set_accepts_tab(true);
        obj.set_pixels_above_lines(0);
        obj.set_pixels_below_lines(0);
        obj.add_css_class("sillpaper-editor");
        obj
    }

    pub fn set_decorations(&self, decorations: Vec<BlockDecoration>) {
        let imp = self.imp();
        let any = decorations.iter().any(|d| *d != BlockDecoration::None);
        imp.has_decorations.set(any);
        *imp.decorations.borrow_mut() = decorations;
        self.queue_draw();
    }

    pub fn set_colors(&self, colors: DecorationColors) {
        *self.imp().colors.borrow_mut() = colors;
        self.queue_draw();
    }

    pub fn set_focus_state(&self, active_line: Option<usize>, dim_opacity: f32) {
        let imp = self.imp();
        if imp.focus_line.get() != active_line
            || (imp.focus_opacity.get() - dim_opacity).abs() > 0.005
        {
            imp.focus_line.set(active_line);
            imp.focus_opacity.set(dim_opacity);
            self.queue_draw();
        }
    }

    pub fn set_override_selection(&self, range: Option<(i32, i32)>) {
        let imp = self.imp();
        if imp.override_selection.get() != range {
            imp.override_selection.set(range);
            self.queue_draw();
        }
    }

    pub fn set_ghost_text(&self, ghost: Option<(i32, String)>) {
        let mut slot = self.imp().ghost_text.borrow_mut();
        if *slot != ghost {
            *slot = ghost;
            drop(slot);
            self.queue_draw();
        }
    }

    pub fn ghost_text(&self) -> Option<(i32, String)> {
        self.imp().ghost_text.borrow().clone()
    }

    pub fn set_column_inset(&self, inset: i32) {
        let clamped = inset.max(0);
        if self.imp().column_inset.get() != clamped {
            self.imp().column_inset.set(clamped);
            self.queue_draw();
        }
    }

    pub fn column_inset(&self) -> i32 {
        self.imp().column_inset.get().max(self.left_margin())
    }

    pub fn selection_rects(&self) -> Vec<graphene::Rect> {
        let buffer = self.buffer();
        let tv: &gtk4::TextView = self.upcast_ref();
        let col_inset = self.column_inset();
        if let Some((a, b)) = self.imp().override_selection.get() {
            let max_off = buffer.char_count();
            let lo = a.min(b).clamp(0, max_off);
            let hi = a.max(b).clamp(0, max_off);
            if lo < hi {
                let s = buffer.iter_at_offset(lo);
                let e = buffer.iter_at_offset(hi);
                return compute_selection_rects(tv, s, e, col_inset);
            }
            return Vec::new();
        }
        if let Some((s, e)) = buffer.selection_bounds() {
            return compute_selection_rects(tv, s, e, col_inset);
        }
        Vec::new()
    }
}

pub fn rgba_from_css(css: &str, fallback: gdk::RGBA) -> gdk::RGBA {
    gdk::RGBA::parse(css).unwrap_or(fallback)
}
