use crate::config::{Preferences, TextWidth};
use crate::io::docx::{decode_data_uri_image, resolve_local_image_path};
use crate::model::{Block, BlockKind, Document, Inline, TextDirection};
use gtk4::cairo;
use gtk4::gdk_pixbuf;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

struct CheckboxMarker {
    byte_idx: i32,
    checked: bool,
}

struct InlineSpan {
    start_byte: i32,
    end_byte: i32,
}

struct LinkSpan {
    start_byte: i32,
    end_byte: i32,
    url: String,
}

struct PrintImage {
    surface: cairo::ImageSurface,
    width_pt: f64,
    height_pt: f64,
}

struct PrintBlock {
    kind: BlockKind,
    is_rtl: bool,
    text: String,
    attrs: pango::AttrList,
    font_desc: pango::FontDescription,
    line_spacing: f32,
    spacing_before: f64,
    spacing_after: f64,
    start_indent: f64,
    end_indent: f64,
    bullet_prefix: Option<String>,
    checkboxes: Vec<CheckboxMarker>,
    code_spans: Vec<InlineSpan>,
    link_spans: Vec<LinkSpan>,
    images: Vec<PrintImage>,
}

#[derive(Clone, Copy)]
struct MeasuredLine {
    band_top: f64,
    band_bottom: f64,
    logical_top: f64,
    logical_bottom: f64,
    baseline: f64,
}

enum PageSlice {
    TextLines {
        block_idx: usize,
        start_line: usize,
        end_line: usize,
        y: f64,
        visual_height: f64,
        is_first_slice: bool,
        is_last_slice: bool,
    },
    HorizontalRule {
        y: f64,
    },
    Image {
        block_idx: usize,
        image_idx: usize,
        y: f64,
        width: f64,
        height: f64,
    },
}

pub fn export_document_to_pdf(
    doc: &Document,
    prefs: &Preferences,
    path: &Path,
) -> Result<(), String> {
    export_document_to_pdf_with_base(doc, prefs, path, None)
}

pub fn export_document_to_pdf_with_base(
    doc: &Document,
    prefs: &Preferences,
    path: &Path,
    doc_dir: Option<&Path>,
) -> Result<(), String> {
    let page_w = 595.28;
    let page_h = 841.89;
    let margin_x = match prefs.text_width {
        TextWidth::Narrow => 82.0,
        TextWidth::Medium => 60.0,
        TextWidth::Wide | TextWidth::ExtraWide | TextWidth::Percent75 | TextWidth::Full => 46.0,
    };
    let margin_top = 58.0;
    let margin_bottom = 62.0;
    let content_w = page_w - margin_x * 2.0;
    let content_h = page_h - margin_top - margin_bottom;

    let surface = cairo::PdfSurface::new(page_w, page_h, path)
        .map_err(|e| format!("Failed creating PDF surface: {e}"))?;
    let title = doc
        .derived_title()
        .unwrap_or_else(|| "Untitled".to_string());
    let _ = surface.set_metadata(cairo::PdfMetadata::Title, &title);
    let _ = surface.set_metadata(cairo::PdfMetadata::Creator, "Sillpaper");

    let cr = cairo::Context::new(&surface).map_err(|e| format!("Cairo context error: {e}"))?;

    let print_blocks = build_print_blocks(doc, prefs, doc_dir, content_w, content_h * 0.55);
    let pages = paginate_blocks(&cr, &print_blocks, content_w, content_h);
    let total_pages = pages.len().max(1);

    for (page_idx, page) in pages.iter().enumerate() {
        if page_idx > 0 {
            cr.show_page().map_err(|e| format!("PDF show_page: {e}"))?;
        }
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();

        render_page(
            &cr,
            &print_blocks,
            page,
            margin_x,
            margin_top,
            content_w,
            true,
        );

        if total_pages > 1 {
            render_page_number(&cr, page_idx + 1, total_pages, page_w, page_h - 30.0);
        }
    }

    surface.finish();
    Ok(())
}

pub fn print_document_native(
    parent: &impl IsA<gtk4::Window>,
    doc: &Document,
    prefs: &Preferences,
    doc_dir: Option<&Path>,
) {
    let op = gtk4::PrintOperation::new();
    op.set_use_full_page(false);
    op.set_unit(gtk4::Unit::Points);

    let doc_owned = doc.clone();
    let prefs_owned = prefs.clone();
    let dir_owned = doc_dir.map(Path::to_path_buf);
    let blocks_holder: Rc<RefCell<Vec<PrintBlock>>> = Rc::new(RefCell::new(Vec::new()));
    let pages_holder: Rc<RefCell<Vec<Vec<PageSlice>>>> = Rc::new(RefCell::new(Vec::new()));

    let bh_begin = blocks_holder.clone();
    let ph_begin = pages_holder.clone();
    op.connect_begin_print(move |op, ctx| {
        let cr = ctx.cairo_context();
        let w = ctx.width();
        let h = ctx.height();
        let blocks =
            build_print_blocks(&doc_owned, &prefs_owned, dir_owned.as_deref(), w, h * 0.55);
        let layout_pages = paginate_blocks(&cr, &blocks, w, h);
        op.set_n_pages(layout_pages.len().max(1) as i32);
        *bh_begin.borrow_mut() = blocks;
        *ph_begin.borrow_mut() = layout_pages;
    });

    let bh_draw = blocks_holder;
    let ph_draw = pages_holder;
    op.connect_draw_page(move |_op, ctx, page_nr| {
        let cr = ctx.cairo_context();
        let w = ctx.width();
        let blocks = bh_draw.borrow();
        let pages = ph_draw.borrow();
        if let Some(page) = pages.get(page_nr as usize) {
            render_page(&cr, &blocks, page, 0.0, 0.0, w, false);
        }
    });

    let _ = op.run(gtk4::PrintOperationAction::PrintDialog, Some(parent));
}

fn build_print_blocks(
    doc: &Document,
    prefs: &Preferences,
    doc_dir: Option<&Path>,
    max_img_w: f64,
    max_img_h: f64,
) -> Vec<PrintBlock> {
    let pct = (prefs.font_size_percent as f64 / 100.0).clamp(0.75, 1.5);
    let base_pt = 11.2 * pct;
    let family = prefs.font.pango_family();
    let body_ls = prefs.line_spacing.factor();
    let head_ls = prefs.line_spacing.heading_factor();
    let p_margins = prefs.paragraph_margins;

    doc.blocks
        .iter()
        .map(|block| {
            build_single_print_block(
                block, family, base_pt, body_ls, head_ls, p_margins, doc_dir, max_img_w, max_img_h,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_single_print_block(
    block: &Block,
    family: &str,
    base_pt: f64,
    body_ls: f32,
    head_ls: f32,
    p_margins: bool,
    doc_dir: Option<&Path>,
    max_img_w: f64,
    max_img_h: f64,
) -> PrintBlock {
    let mut text = String::new();
    let attrs = pango::AttrList::new();
    let mut checkboxes = Vec::new();
    let mut code_spans = Vec::new();
    let mut link_spans = Vec::new();
    let mut images = Vec::new();
    let is_code_block = matches!(block.kind, BlockKind::CodeBlock { .. });

    for inline in &block.inlines {
        match inline {
            Inline::Text(run) => {
                let start = text.len() as u32;
                text.push_str(&run.text);
                let end = text.len() as u32;
                if run.style.bold {
                    let mut a = pango::AttrInt::new_weight(pango::Weight::Bold);
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a);
                }
                if run.style.italic {
                    let mut a = pango::AttrInt::new_style(pango::Style::Italic);
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a);
                }
                if run.style.underline || run.style.link.is_some() {
                    let mut a = pango::AttrInt::new_underline(pango::Underline::Single);
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a);
                }
                if let Some(url) = &run.style.link {
                    let mut a = pango::AttrColor::new_foreground(0x1A00, 0x6F00, 0xB500);
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a);
                    link_spans.push(LinkSpan {
                        start_byte: start as i32,
                        end_byte: end as i32,
                        url: url.clone(),
                    });
                }
                if run.style.strikethrough {
                    let mut a = pango::AttrInt::new_strikethrough(true);
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a);
                }
                if run.style.code && !is_code_block {
                    let mut fam = pango::AttrString::new_family("Monospace");
                    fam.set_start_index(start);
                    fam.set_end_index(end);
                    attrs.insert(fam);
                    let mut sc = pango::AttrFloat::new_scale(0.90);
                    sc.set_start_index(start);
                    sc.set_end_index(end);
                    attrs.insert(sc);
                    code_spans.push(InlineSpan {
                        start_byte: start as i32,
                        end_byte: end as i32,
                    });
                }
            }
            Inline::LineBreak => text.push('\n'),
            Inline::Checkbox { checked } => {
                let byte_idx = text.len() as i32;
                text.push('\u{2003}');
                checkboxes.push(CheckboxMarker {
                    byte_idx,
                    checked: *checked,
                });
            }
            Inline::Image { src, .. } => {
                if let Some(img) = load_print_image(src, doc_dir, max_img_w, max_img_h) {
                    images.push(img);
                }
            }
            Inline::Comment { .. } => {}
        }
    }

    let is_rtl = match block.direction {
        TextDirection::Rtl => true,
        TextDirection::Ltr => false,
        TextDirection::Auto => pango::find_base_dir(&text) == pango::Direction::Rtl,
    };

    let mut font_desc = pango::FontDescription::new();
    font_desc.set_family(family);

    let (
        scale,
        weight,
        style,
        line_spacing,
        spacing_before,
        spacing_after,
        start_indent,
        end_indent,
        bullet_prefix,
    ) = match &block.kind {
        BlockKind::Paragraph => (
            1.0,
            pango::Weight::Normal,
            pango::Style::Normal,
            body_ls,
            0.0,
            if p_margins { 10.0 } else { 3.0 },
            0.0,
            0.0,
            None,
        ),
        BlockKind::Heading(1) => (
            1.85,
            pango::Weight::Bold,
            pango::Style::Normal,
            head_ls,
            if p_margins { 18.0 } else { 8.0 },
            if p_margins { 9.0 } else { 4.0 },
            0.0,
            0.0,
            None,
        ),
        BlockKind::Heading(2) => (
            1.45,
            pango::Weight::Bold,
            pango::Style::Normal,
            head_ls,
            if p_margins { 15.0 } else { 6.0 },
            if p_margins { 7.0 } else { 4.0 },
            0.0,
            0.0,
            None,
        ),
        BlockKind::Heading(3) => (
            1.22,
            pango::Weight::Bold,
            pango::Style::Normal,
            head_ls,
            if p_margins { 12.0 } else { 5.0 },
            if p_margins { 6.0 } else { 3.0 },
            0.0,
            0.0,
            None,
        ),
        BlockKind::Heading(_) => (
            1.10,
            pango::Weight::Bold,
            pango::Style::Normal,
            head_ls,
            10.0,
            5.0,
            0.0,
            0.0,
            None,
        ),
        BlockKind::Blockquote => (
            1.0,
            pango::Weight::Normal,
            pango::Style::Italic,
            body_ls,
            if p_margins { 8.0 } else { 3.0 },
            if p_margins { 10.0 } else { 4.0 },
            22.0,
            14.0,
            None,
        ),
        BlockKind::BulletListItem { depth } => {
            let glyph = match depth % 3 {
                0 => "•",
                1 => "◦",
                _ => "▪",
            };
            (
                1.0,
                pango::Weight::Normal,
                pango::Style::Normal,
                body_ls,
                1.5,
                if p_margins { 4.5 } else { 2.0 },
                24.0 * (*depth as f64 + 1.0),
                0.0,
                Some(glyph.to_string()),
            )
        }
        BlockKind::OrderedListItem { depth, number } => (
            1.0,
            pango::Weight::Normal,
            pango::Style::Normal,
            body_ls,
            1.5,
            if p_margins { 4.5 } else { 2.0 },
            24.0 * (*depth as f64 + 1.0),
            0.0,
            Some(format!("{number}.")),
        ),
        BlockKind::CodeBlock { .. } => {
            font_desc.set_family("Monospace");
            (
                0.88,
                pango::Weight::Normal,
                pango::Style::Normal,
                1.38,
                8.0,
                12.0,
                12.0,
                12.0,
                None,
            )
        }
        BlockKind::HorizontalRule => (
            1.0,
            pango::Weight::Normal,
            pango::Style::Normal,
            body_ls,
            10.0,
            12.0,
            0.0,
            0.0,
            None,
        ),
    };

    font_desc.set_size(((base_pt * scale) * pango::SCALE as f64) as i32);
    font_desc.set_weight(weight);
    font_desc.set_style(style);

    PrintBlock {
        kind: block.kind.clone(),
        is_rtl,
        text,
        attrs,
        font_desc,
        line_spacing,
        spacing_before,
        spacing_after,
        start_indent,
        end_indent,
        bullet_prefix,
        checkboxes,
        code_spans,
        link_spans,
        images,
    }
}

fn load_print_image(
    src: &str,
    doc_dir: Option<&Path>,
    max_w: f64,
    max_h: f64,
) -> Option<PrintImage> {
    let pixbuf = if let Some((_, bytes)) = decode_data_uri_image(src) {
        let loader = gdk_pixbuf::PixbufLoader::new();
        loader.write(&bytes).ok()?;
        loader.close().ok()?;
        loader.pixbuf()?
    } else {
        let path = resolve_local_image_path(src, doc_dir)?;
        gdk_pixbuf::Pixbuf::from_file(&path).ok()?
    };
    let pw = pixbuf.width().max(1);
    let ph = pixbuf.height().max(1);
    let has_alpha = pixbuf.has_alpha();
    let rowstride = pixbuf.rowstride() as usize;
    let n_channels = pixbuf.n_channels() as usize;
    let pixel_bytes = pixbuf.read_pixel_bytes();
    let pixels: &[u8] = pixel_bytes.as_ref();

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).ok()?;
    {
        let stride = surface.stride() as usize;
        let mut data = surface.data().ok()?;
        for y in 0..(ph as usize) {
            for x in 0..(pw as usize) {
                let src_i = y * rowstride + x * n_channels;
                if src_i + 2 >= pixels.len() {
                    continue;
                }
                let r = pixels[src_i] as u32;
                let g = pixels[src_i + 1] as u32;
                let b = pixels[src_i + 2] as u32;
                let a = if has_alpha && n_channels >= 4 && src_i + 3 < pixels.len() {
                    pixels[src_i + 3] as u32
                } else {
                    255
                };
                let pr = (r * a + 127) / 255;
                let pg = (g * a + 127) / 255;
                let pb = (b * a + 127) / 255;
                let argb = (a << 24) | (pr << 16) | (pg << 8) | pb;
                let dst_i = y * stride + x * 4;
                data[dst_i..dst_i + 4].copy_from_slice(&argb.to_ne_bytes());
            }
        }
    }

    let mut w_pt = pw as f64 * 0.75;
    let mut h_pt = ph as f64 * 0.75;
    if w_pt > max_w {
        let s = max_w / w_pt;
        w_pt *= s;
        h_pt *= s;
    }
    if h_pt > max_h {
        let s = max_h / h_pt;
        w_pt *= s;
        h_pt *= s;
    }

    Some(PrintImage {
        surface,
        width_pt: w_pt,
        height_pt: h_pt,
    })
}

fn layout_for_block(cr: &cairo::Context, pb: &PrintBlock, content_w: f64) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pb.font_desc));
    layout.set_text(&pb.text);
    layout.set_attributes(Some(&pb.attrs));
    layout.set_wrap(pango::WrapMode::WordChar);
    let avail = (content_w - pb.start_indent - pb.end_indent).max(40.0);
    layout.set_width((avail * pango::SCALE as f64) as i32);
    layout.set_line_spacing(pb.line_spacing);
    layout.set_auto_dir(false);
    layout.context().set_base_dir(if pb.is_rtl {
        pango::Direction::Rtl
    } else {
        pango::Direction::Ltr
    });
    layout.set_alignment(if pb.is_rtl {
        pango::Alignment::Right
    } else {
        pango::Alignment::Left
    });
    layout
}

fn measure_layout_lines(layout: &pango::Layout) -> Vec<MeasuredLine> {
    let mut iter = layout.iter();
    let mut lines = Vec::new();
    let scale = pango::SCALE as f64;
    loop {
        let (y0, y1) = iter.line_yrange();
        let (_ink, logical) = iter.line_extents();
        let baseline = iter.baseline() as f64 / scale;
        let logical_top = logical.y() as f64 / scale;
        let logical_bottom = (logical.y() + logical.height().max(1)) as f64 / scale;
        lines.push(MeasuredLine {
            band_top: y0 as f64 / scale,
            band_bottom: y1.max(y0 + 1) as f64 / scale,
            logical_top,
            logical_bottom,
            baseline,
        });
        if !iter.next_line() {
            break;
        }
    }
    lines
}

fn paginate_blocks(
    cr: &cairo::Context,
    blocks: &[PrintBlock],
    content_w: f64,
    content_h: f64,
) -> Vec<Vec<PageSlice>> {
    let mut pages: Vec<Vec<PageSlice>> = vec![Vec::new()];
    let mut y = 0.0_f64;

    for (idx, pb) in blocks.iter().enumerate() {
        let top_pad = if y > 0.0 { pb.spacing_before } else { 0.0 };

        if pb.kind == BlockKind::HorizontalRule {
            let h = 14.0;
            if y + top_pad + h > content_h && !pages.last().unwrap().is_empty() {
                pages.push(Vec::new());
                y = 0.0;
            } else {
                y += top_pad;
            }
            pages
                .last_mut()
                .unwrap()
                .push(PageSlice::HorizontalRule { y });
            y += h + pb.spacing_after;
            continue;
        }

        let is_code_block = matches!(pb.kind, BlockKind::CodeBlock { .. });
        let box_pad_v = if is_code_block { 7.0 } else { 0.0 };

        let has_text_or_empty = !pb.text.is_empty() || pb.images.is_empty();
        if has_text_or_empty {
            let layout = layout_for_block(cr, pb, content_w);
            let lines = measure_layout_lines(&layout);
            let total_lines = lines.len();

            if pb.kind.is_heading() {
                let total_h = lines.last().map(|l| l.logical_bottom).unwrap_or(16.0);
                if y + top_pad + total_h + 36.0 > content_h && !pages.last().unwrap().is_empty() {
                    pages.push(Vec::new());
                    y = 0.0;
                }
            }

            let mut line_start = 0usize;
            while line_start < total_lines {
                let is_first_slice = line_start == 0;
                let slice_pad = if is_first_slice && y > 0.0 {
                    pb.spacing_before
                } else {
                    0.0
                };
                let pad_top = if is_first_slice { box_pad_v } else { 3.0 };
                let base_y0 = lines[line_start].band_top;

                let mut line_end = line_start;
                while line_end < total_lines {
                    let is_last = line_end + 1 == total_lines;
                    let pad_bot = if is_last { box_pad_v } else { 3.0 };
                    let vis_h = (lines[line_end].logical_bottom - base_y0).max(12.0);
                    if y + slice_pad + pad_top + vis_h + pad_bot <= content_h {
                        line_end += 1;
                    } else {
                        break;
                    }
                }

                if line_end == line_start {
                    if !pages.last().unwrap().is_empty() {
                        pages.push(Vec::new());
                        y = 0.0;
                        continue;
                    }
                    line_end = line_start + 1;
                }

                let is_last_slice = line_end == total_lines;
                let pad_bot = if is_last_slice { box_pad_v } else { 3.0 };
                y += slice_pad + pad_top;
                let visual_height = (lines[line_end - 1].logical_bottom - base_y0).max(12.0);
                let advance_height = if is_last_slice {
                    visual_height
                } else {
                    (lines[line_end - 1].band_bottom - base_y0).max(visual_height)
                };

                pages.last_mut().unwrap().push(PageSlice::TextLines {
                    block_idx: idx,
                    start_line: line_start,
                    end_line: line_end,
                    y,
                    visual_height,
                    is_first_slice,
                    is_last_slice,
                });
                y += advance_height + pad_bot;
                if line_end < total_lines {
                    pages.push(Vec::new());
                    y = 0.0;
                }
                line_start = line_end;
            }
            y += pb.spacing_after;
        }

        for (img_idx, img) in pb.images.iter().enumerate() {
            let pad = if y > 0.0 { 6.0 } else { 0.0 };
            if y + pad + img.height_pt > content_h && !pages.last().unwrap().is_empty() {
                pages.push(Vec::new());
                y = 0.0;
            } else {
                y += pad;
            }
            pages.last_mut().unwrap().push(PageSlice::Image {
                block_idx: idx,
                image_idx: img_idx,
                y,
                width: img.width_pt,
                height: img.height_pt,
            });
            y += img.height_pt + 10.0;
        }
    }

    pages
}

fn render_page(
    cr: &cairo::Context,
    blocks: &[PrintBlock],
    slices: &[PageSlice],
    margin_x: f64,
    margin_y: f64,
    content_w: f64,
    emit_pdf_links: bool,
) {
    let scale = pango::SCALE as f64;

    for slice in slices {
        match *slice {
            PageSlice::HorizontalRule { y } => {
                cr.set_source_rgb(0.78, 0.78, 0.78);
                cr.set_line_width(0.8);
                cr.move_to(margin_x, margin_y + y + 7.0);
                cr.line_to(margin_x + content_w, margin_y + y + 7.0);
                let _ = cr.stroke();
            }
            PageSlice::Image {
                block_idx,
                image_idx,
                y,
                width,
                height,
            } => {
                if let Some(img) = blocks[block_idx].images.get(image_idx) {
                    let img_x = margin_x + (content_w - width) * 0.5;
                    let img_y = margin_y + y;
                    let sx = width / img.surface.width().max(1) as f64;
                    let sy = height / img.surface.height().max(1) as f64;
                    let _ = cr.save();
                    cr.translate(img_x, img_y);
                    cr.scale(sx, sy);
                    let _ = cr.set_source_surface(&img.surface, 0.0, 0.0);
                    let _ = cr.paint();
                    let _ = cr.restore();
                }
            }
            PageSlice::TextLines {
                block_idx,
                start_line,
                end_line,
                y,
                visual_height,
                is_first_slice,
                is_last_slice,
            } => {
                let pb = &blocks[block_idx];
                let left_pad = if pb.is_rtl {
                    pb.end_indent
                } else {
                    pb.start_indent
                };
                let x = margin_x + left_pad;
                let draw_y = margin_y + y;

                let layout = layout_for_block(cr, pb, content_w);
                let lines = measure_layout_lines(&layout);
                let base_y0 = lines.get(start_line).map(|l| l.band_top).unwrap_or(0.0);
                let first_vis_offset = lines
                    .get(start_line)
                    .map(|l| (l.logical_top - base_y0).max(0.0))
                    .unwrap_or(0.0);
                let box_top = draw_y + first_vis_offset;
                let box_h = (visual_height - first_vis_offset).max(10.0);

                if pb.kind == BlockKind::Blockquote {
                    cr.set_source_rgb(0.72, 0.72, 0.72);
                    let bar_x = if pb.is_rtl {
                        margin_x + content_w - 9.5
                    } else {
                        margin_x + 7.0
                    };
                    cr.rectangle(bar_x, box_top + 1.0, 2.5, (box_h - 2.0).max(8.0));
                    let _ = cr.fill();
                } else if matches!(pb.kind, BlockKind::CodeBlock { .. }) {
                    let pad_top = if is_first_slice { 7.0 } else { 3.0 };
                    let pad_bot = if is_last_slice { 7.0 } else { 3.0 };
                    cr.set_source_rgb(0.955, 0.96, 0.97);
                    draw_rounded_rect(
                        cr,
                        margin_x,
                        box_top - pad_top,
                        content_w,
                        box_h + pad_top + pad_bot,
                        4.5,
                    );
                    let _ = cr.fill_preserve();
                    cr.set_source_rgb(0.86, 0.88, 0.91);
                    cr.set_line_width(0.6);
                    let _ = cr.stroke();
                }

                if is_first_slice && let Some(prefix) = &pb.bullet_prefix {
                    cr.set_source_rgb(0.35, 0.35, 0.35);
                    let bl = pangocairo::functions::create_layout(cr);
                    bl.set_font_description(Some(&pb.font_desc));
                    bl.set_text(prefix);
                    let (bw, _) = bl.pixel_size();
                    let bl_baseline = bl.baseline() as f64 / scale;
                    let first_baseline_y = draw_y
                        + lines
                            .get(start_line)
                            .map(|l| l.baseline - base_y0)
                            .unwrap_or(bl_baseline);
                    let bx = if pb.is_rtl {
                        (margin_x + content_w - pb.start_indent + 6.0)
                            .min(margin_x + content_w - bw as f64)
                    } else {
                        (x - bw as f64 - 6.0).max(margin_x)
                    };
                    cr.move_to(bx, first_baseline_y - bl_baseline);
                    pangocairo::functions::show_layout(cr, &bl);
                }

                let mut iter = layout.iter();
                let mut line_idx = 0usize;
                loop {
                    if line_idx >= start_line && line_idx < end_line {
                        let (_ink, logical) = iter.line_extents();
                        let line_x = x + logical.x() as f64 / scale;
                        let line_top_y = draw_y + (logical.y() as f64 / scale) - base_y0;
                        let line_h = (logical.height() as f64 / scale).max(10.0);
                        let line_baseline_y = draw_y + (iter.baseline() as f64 / scale) - base_y0;

                        if let Some(line) = iter.line_readonly() {
                            let ls = line.start_index();
                            let le = ls + line.length();

                            if !pb.code_spans.is_empty() {
                                cr.set_source_rgb(0.925, 0.935, 0.945);
                                for span in &pb.code_spans {
                                    let ov_s = span.start_byte.max(ls);
                                    let ov_e = span.end_byte.min(le);
                                    if ov_s < ov_e {
                                        let ranges = line.x_ranges(ov_s, ov_e);
                                        for chunk in ranges.as_chunks::<2>().0 {
                                            let rx0 = line_x + chunk[0] as f64 / scale;
                                            let rx1 = line_x + chunk[1] as f64 / scale;
                                            let rw = (rx1 - rx0).abs();
                                            if rw > 0.5 {
                                                draw_rounded_rect(
                                                    cr,
                                                    rx0.min(rx1) - 1.5,
                                                    line_top_y + 0.5,
                                                    rw + 3.0,
                                                    (line_h - 1.0).max(8.0),
                                                    2.5,
                                                );
                                                let _ = cr.fill();
                                            }
                                        }
                                    }
                                }
                            }

                            if pb.kind == BlockKind::Blockquote {
                                cr.set_source_rgb(0.32, 0.32, 0.32);
                            } else {
                                cr.set_source_rgb(0.14, 0.14, 0.14);
                            }
                            cr.move_to(line_x, line_baseline_y);
                            pangocairo::functions::show_layout_line(cr, &line);

                            if emit_pdf_links && !pb.link_spans.is_empty() {
                                for link in &pb.link_spans {
                                    let ov_s = link.start_byte.max(ls);
                                    let ov_e = link.end_byte.min(le);
                                    if ov_s < ov_e {
                                        let ranges = line.x_ranges(ov_s, ov_e);
                                        let safe_uri = escape_cairo_tag_uri(&link.url);
                                        for chunk in ranges.as_chunks::<2>().0 {
                                            let rx0 = line_x + chunk[0] as f64 / scale;
                                            let rx1 = line_x + chunk[1] as f64 / scale;
                                            let rw = (rx1 - rx0).abs();
                                            if rw > 0.5 {
                                                let attrs = format!(
                                                    "rect=[{:.2} {:.2} {:.2} {:.2}] uri='{safe_uri}'",
                                                    rx0.min(rx1),
                                                    line_top_y,
                                                    rw,
                                                    line_h
                                                );
                                                cr.tag_begin("Link", &attrs);
                                                cr.tag_end("Link");
                                            }
                                        }
                                    }
                                }
                            }

                            for cb in &pb.checkboxes {
                                if cb.byte_idx >= ls && cb.byte_idx < le {
                                    let rect = layout.index_to_pos(cb.byte_idx);
                                    let glyph_left =
                                        x + rect.x().min(rect.x() + rect.width()) as f64 / scale;
                                    let glyph_w = (rect.width().abs() as f64 / scale).max(9.0);
                                    let box_sz = (line_h * 0.68).clamp(8.5, 11.5);
                                    let bx = glyph_left + (glyph_w - box_sz) * 0.5;
                                    let by = line_baseline_y - box_sz * 0.84;
                                    draw_vector_checkbox(cr, bx, by, box_sz, cb.checked);
                                }
                            }
                        }
                    }

                    line_idx += 1;
                    if line_idx >= end_line || !iter.next_line() {
                        break;
                    }
                }
            }
        }
    }
}

fn escape_cairo_tag_uri(uri: &str) -> String {
    uri.replace('\\', "\\\\").replace('\'', "\\'")
}

fn draw_vector_checkbox(cr: &cairo::Context, x: f64, y: f64, sz: f64, checked: bool) {
    let _ = cr.save();
    draw_rounded_rect(cr, x, y, sz, sz, 2.0);
    if checked {
        cr.set_source_rgb(0.10, 0.44, 0.71);
        let _ = cr.fill();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_line_width(1.3);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.move_to(x + sz * 0.22, y + sz * 0.54);
        cr.line_to(x + sz * 0.43, y + sz * 0.75);
        cr.line_to(x + sz * 0.78, y + sz * 0.28);
        let _ = cr.stroke();
    } else {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(0.50, 0.50, 0.50);
        cr.set_line_width(0.9);
        let _ = cr.stroke();
    }
    let _ = cr.restore();
}

fn draw_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5);
    let deg = std::f64::consts::PI / 180.0;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -90.0 * deg, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, 90.0 * deg);
    cr.arc(x + r, y + h - r, r, 90.0 * deg, 180.0 * deg);
    cr.arc(x + r, y + r, r, 180.0 * deg, 270.0 * deg);
    cr.close_path();
}

fn render_page_number(cr: &cairo::Context, page: usize, total: usize, page_w: f64, y: f64) {
    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family("Sans");
    fd.set_size(9 * pango::SCALE);
    layout.set_font_description(Some(&fd));
    layout.set_text(&format!("{page} / {total}"));
    let (lw, _) = layout.pixel_size();
    cr.set_source_rgb(0.55, 0.55, 0.55);
    cr.move_to((page_w - lw as f64) * 0.5, y);
    pangocairo::functions::show_layout(cr, &layout);
}
