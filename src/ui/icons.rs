use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppIcon {
    #[default]
    SidebarLeft,
    SidebarRight,
    Link,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ListBullet,
    ListOrdered,
    Check,
    Close,
    Plus,
    Search,
    FileNew,
    WindowNew,
    FolderOpen,
    Save,
    SaveAs,
    ExportDocx,
    ExportPdf,
    Fullscreen,
    Print,
    Settings,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SymbolicIconPaintable {
        pub icon: Cell<AppIcon>,
        pub size: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SymbolicIconPaintable {
        const NAME: &'static str = "SillpaperSymbolicIconPaintable";
        type Type = super::SymbolicIconPaintable;
        type Interfaces = (gdk::Paintable, gtk4::SymbolicPaintable);
    }

    impl ObjectImpl for SymbolicIconPaintable {}

    impl PaintableImpl for SymbolicIconPaintable {
        fn intrinsic_width(&self) -> i32 {
            self.size.get().max(12)
        }

        fn intrinsic_height(&self) -> i32 {
            self.size.get().max(12)
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let fallback = gdk::RGBA::new(0.24, 0.24, 0.24, 1.0);
            draw_symbolic_icon(self.icon.get(), snapshot, width, height, fallback);
        }
    }

    impl SymbolicPaintableImpl for SymbolicIconPaintable {
        fn snapshot_symbolic(
            &self,
            snapshot: &gdk::Snapshot,
            width: f64,
            height: f64,
            colors: &[gdk::RGBA],
        ) {
            let fg = colors
                .first()
                .copied()
                .unwrap_or_else(|| gdk::RGBA::new(0.24, 0.24, 0.24, 1.0));
            draw_symbolic_icon(self.icon.get(), snapshot, width, height, fg);
        }
    }
}

glib::wrapper! {
    pub struct SymbolicIconPaintable(ObjectSubclass<imp::SymbolicIconPaintable>)
        @implements gdk::Paintable, gtk4::SymbolicPaintable;
}

impl SymbolicIconPaintable {
    pub fn new(icon: AppIcon, pixel_size: i32) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().icon.set(icon);
        obj.imp().size.set(pixel_size.max(12));
        obj
    }
}

pub fn symbolic_image(icon: AppIcon, pixel_size: i32) -> gtk4::Image {
    let paintable = SymbolicIconPaintable::new(icon, pixel_size);
    let img = gtk4::Image::from_paintable(Some(&paintable));
    img.set_pixel_size(pixel_size);
    img
}

pub fn symbolic_button(icon: AppIcon) -> gtk4::Button {
    symbolic_button_sized(icon, 16)
}

pub fn symbolic_button_sized(icon: AppIcon, pixel_size: i32) -> gtk4::Button {
    let btn = gtk4::Button::new();
    btn.add_css_class("image-button");
    btn.set_child(Some(&symbolic_image(icon, pixel_size)));
    btn
}

fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

fn draw_symbolic_icon(
    icon: AppIcon,
    snapshot: &gdk::Snapshot,
    width: f64,
    height: f64,
    fg: gdk::RGBA,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let Some(snap) = snapshot.downcast_ref::<gtk4::Snapshot>() else {
        return;
    };
    let rect = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let cr = snap.append_cairo(&rect);

    let s = width.min(height) / 16.0;
    let ox = (width - 16.0 * s) * 0.5;
    let oy = (height - 16.0 * s) * 0.5;
    cr.translate(ox, oy);
    cr.scale(s, s);

    cr.set_source_rgba(
        fg.red() as f64,
        fg.green() as f64,
        fg.blue() as f64,
        fg.alpha() as f64,
    );
    cr.set_line_width(1.45);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);

    match icon {
        AppIcon::SidebarLeft => {
            let _ = cr.save();
            rounded_rect(&cr, 1.5, 2.5, 13.0, 11.0, 2.2);
            cr.clip();
            cr.set_source_rgba(
                fg.red() as f64,
                fg.green() as f64,
                fg.blue() as f64,
                fg.alpha() as f64 * 0.28,
            );
            cr.rectangle(1.5, 2.5, 4.0, 11.0);
            let _ = cr.fill();
            let _ = cr.restore();

            cr.set_line_width(1.25);
            rounded_rect(&cr, 1.5, 2.5, 13.0, 11.0, 2.2);
            let _ = cr.stroke();
            cr.move_to(5.5, 2.5);
            cr.line_to(5.5, 13.5);
            let _ = cr.stroke();
        }
        AppIcon::SidebarRight => {
            let _ = cr.save();
            rounded_rect(&cr, 1.5, 2.5, 13.0, 11.0, 2.2);
            cr.clip();
            cr.set_source_rgba(
                fg.red() as f64,
                fg.green() as f64,
                fg.blue() as f64,
                fg.alpha() as f64 * 0.28,
            );
            cr.rectangle(10.5, 2.5, 4.0, 11.0);
            let _ = cr.fill();
            let _ = cr.restore();

            cr.set_line_width(1.25);
            rounded_rect(&cr, 1.5, 2.5, 13.0, 11.0, 2.2);
            let _ = cr.stroke();
            cr.move_to(10.5, 2.5);
            cr.line_to(10.5, 13.5);
            let _ = cr.stroke();
        }
        AppIcon::Link => {
            let _ = cr.save();
            cr.translate(8.0, 8.0);
            cr.rotate(-FRAC_PI_4);
            cr.new_sub_path();
            cr.move_to(-1.2, -2.35);
            cr.line_to(-4.0, -2.35);
            cr.arc_negative(-4.0, 0.0, 2.35, -FRAC_PI_2, FRAC_PI_2);
            cr.line_to(-1.2, 2.35);
            let _ = cr.stroke();
            cr.new_sub_path();
            cr.move_to(1.2, -2.35);
            cr.line_to(4.0, -2.35);
            cr.arc(4.0, 0.0, 2.35, -FRAC_PI_2, FRAC_PI_2);
            cr.line_to(1.2, 2.35);
            let _ = cr.stroke();
            cr.move_to(-2.4, 0.0);
            cr.line_to(2.4, 0.0);
            let _ = cr.stroke();
            let _ = cr.restore();
        }
        AppIcon::ChevronDown => {
            cr.move_to(4.25, 6.25);
            cr.line_to(8.0, 10.0);
            cr.line_to(11.75, 6.25);
            let _ = cr.stroke();
        }
        AppIcon::ChevronUp => {
            cr.move_to(4.25, 9.75);
            cr.line_to(8.0, 6.0);
            cr.line_to(11.75, 9.75);
            let _ = cr.stroke();
        }
        AppIcon::ChevronLeft => {
            cr.move_to(9.75, 3.75);
            cr.line_to(5.5, 8.0);
            cr.line_to(9.75, 12.25);
            let _ = cr.stroke();
        }
        AppIcon::ChevronRight => {
            cr.move_to(6.25, 3.75);
            cr.line_to(10.5, 8.0);
            cr.line_to(6.25, 12.25);
            let _ = cr.stroke();
        }
        AppIcon::ListBullet => {
            for y in [4.0, 8.0, 12.0] {
                cr.arc(2.75, y, 1.05, 0.0, TAU);
                let _ = cr.fill();
                cr.move_to(5.75, y);
                cr.line_to(13.5, y);
                let _ = cr.stroke();
            }
        }
        AppIcon::ListOrdered => {
            for y in [4.0, 8.0, 12.0] {
                cr.move_to(6.75, y);
                cr.line_to(13.5, y);
                let _ = cr.stroke();
            }
            cr.set_line_width(1.15);
            cr.move_to(2.6, 3.15);
            cr.line_to(3.35, 2.55);
            cr.line_to(3.35, 5.2);
            let _ = cr.stroke();
            cr.move_to(2.25, 7.15);
            cr.curve_to(2.25, 6.45, 4.35, 6.45, 4.35, 7.35);
            cr.line_to(2.25, 9.25);
            cr.line_to(4.45, 9.25);
            let _ = cr.stroke();
            cr.move_to(2.35, 10.9);
            cr.line_to(4.2, 10.9);
            cr.line_to(3.15, 11.95);
            cr.curve_to(4.55, 11.95, 4.55, 13.45, 2.35, 13.25);
            let _ = cr.stroke();
        }
        AppIcon::Check => {
            cr.set_line_width(1.6);
            cr.move_to(3.25, 8.35);
            cr.line_to(6.4, 11.5);
            cr.line_to(13.0, 4.75);
            let _ = cr.stroke();
        }
        AppIcon::Close => {
            cr.move_to(4.1, 4.1);
            cr.line_to(11.9, 11.9);
            cr.move_to(11.9, 4.1);
            cr.line_to(4.1, 11.9);
            let _ = cr.stroke();
        }
        AppIcon::Plus => {
            cr.move_to(8.0, 3.25);
            cr.line_to(8.0, 12.75);
            cr.move_to(3.25, 8.0);
            cr.line_to(12.75, 8.0);
            let _ = cr.stroke();
        }
        AppIcon::Search => {
            cr.arc(6.8, 6.8, 4.3, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(10.0, 10.0);
            cr.line_to(13.5, 13.5);
            let _ = cr.stroke();
        }
        AppIcon::FileNew => {
            cr.move_to(3.25, 1.9);
            cr.line_to(9.25, 1.9);
            cr.line_to(12.75, 5.4);
            cr.line_to(12.75, 14.1);
            cr.line_to(3.25, 14.1);
            cr.close_path();
            let _ = cr.stroke();
            cr.move_to(9.25, 1.9);
            cr.line_to(9.25, 5.4);
            cr.line_to(12.75, 5.4);
            let _ = cr.stroke();
            cr.set_line_width(1.3);
            cr.move_to(8.0, 7.6);
            cr.line_to(8.0, 11.8);
            cr.move_to(5.9, 9.7);
            cr.line_to(10.1, 9.7);
            let _ = cr.stroke();
        }
        AppIcon::WindowNew => {
            rounded_rect(&cr, 1.85, 2.5, 12.3, 11.0, 1.8);
            let _ = cr.stroke();
            cr.move_to(1.85, 5.75);
            cr.line_to(14.15, 5.75);
            let _ = cr.stroke();
            cr.set_line_width(1.3);
            cr.move_to(8.0, 7.6);
            cr.line_to(8.0, 11.4);
            cr.move_to(6.1, 9.5);
            cr.line_to(9.9, 9.5);
            let _ = cr.stroke();
        }
        AppIcon::FolderOpen => {
            cr.move_to(2.0, 4.25);
            cr.line_to(6.0, 4.25);
            cr.line_to(7.4, 5.75);
            cr.line_to(14.0, 5.75);
            cr.line_to(14.0, 12.75);
            cr.line_to(2.0, 12.75);
            cr.close_path();
            let _ = cr.stroke();
        }
        AppIcon::Save => {
            cr.move_to(2.5, 2.25);
            cr.line_to(11.1, 2.25);
            cr.line_to(13.5, 4.65);
            cr.line_to(13.5, 13.75);
            cr.line_to(2.5, 13.75);
            cr.close_path();
            let _ = cr.stroke();
            cr.rectangle(5.0, 2.25, 5.0, 3.6);
            let _ = cr.stroke();
            cr.rectangle(4.75, 9.0, 6.5, 4.75);
            let _ = cr.stroke();
        }
        AppIcon::SaveAs => {
            cr.move_to(8.0, 2.25);
            cr.line_to(8.0, 10.25);
            cr.move_to(4.85, 7.35);
            cr.line_to(8.0, 10.5);
            cr.line_to(11.15, 7.35);
            let _ = cr.stroke();
            cr.move_to(2.75, 12.75);
            cr.line_to(13.25, 12.75);
            let _ = cr.stroke();
        }
        AppIcon::ExportDocx => {
            cr.move_to(8.25, 3.0);
            cr.line_to(3.0, 3.0);
            cr.line_to(3.0, 13.0);
            cr.line_to(13.0, 13.0);
            cr.line_to(13.0, 7.75);
            let _ = cr.stroke();
            cr.move_to(7.5, 8.5);
            cr.line_to(13.25, 2.75);
            cr.move_to(9.75, 2.75);
            cr.line_to(13.25, 2.75);
            cr.line_to(13.25, 6.25);
            let _ = cr.stroke();
        }
        AppIcon::ExportPdf => {
            cr.move_to(3.25, 1.9);
            cr.line_to(9.25, 1.9);
            cr.line_to(12.75, 5.4);
            cr.line_to(12.75, 14.1);
            cr.line_to(3.25, 14.1);
            cr.close_path();
            let _ = cr.stroke();
            cr.move_to(9.25, 1.9);
            cr.line_to(9.25, 5.4);
            cr.line_to(12.75, 5.4);
            let _ = cr.stroke();
            cr.set_line_width(1.25);
            cr.move_to(5.4, 8.0);
            cr.line_to(10.6, 8.0);
            cr.move_to(5.4, 10.5);
            cr.line_to(10.6, 10.5);
            cr.move_to(5.4, 12.4);
            cr.line_to(8.6, 12.4);
            let _ = cr.stroke();
        }
        AppIcon::Fullscreen => {
            cr.move_to(2.5, 5.75);
            cr.line_to(2.5, 2.5);
            cr.line_to(5.75, 2.5);
            cr.move_to(10.25, 2.5);
            cr.line_to(13.5, 2.5);
            cr.line_to(13.5, 5.75);
            cr.move_to(13.5, 10.25);
            cr.line_to(13.5, 13.5);
            cr.line_to(10.25, 13.5);
            cr.move_to(5.75, 13.5);
            cr.line_to(2.5, 13.5);
            cr.line_to(2.5, 10.25);
            let _ = cr.stroke();
        }
        AppIcon::Print => {
            cr.rectangle(4.5, 2.0, 7.0, 3.6);
            let _ = cr.stroke();
            rounded_rect(&cr, 2.0, 5.6, 12.0, 5.4, 1.4);
            let _ = cr.stroke();
            cr.rectangle(4.5, 9.5, 7.0, 4.5);
            let _ = cr.stroke();
        }
        AppIcon::Settings => {
            cr.arc(8.0, 8.0, 2.15, 0.0, TAU);
            let _ = cr.stroke();
            cr.arc(8.0, 8.0, 4.4, 0.0, TAU);
            let _ = cr.stroke();
            cr.set_line_width(1.6);
            for i in 0..8 {
                let a = (i as f64) * FRAC_PI_4;
                cr.move_to(8.0 + 4.5 * a.cos(), 8.0 + 4.5 * a.sin());
                cr.line_to(8.0 + 6.25 * a.cos(), 8.0 + 6.25 * a.sin());
            }
            let _ = cr.stroke();
        }
    }
}
