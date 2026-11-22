use crate::model::TocEntry;
use crate::ui::icons::{AppIcon, symbolic_image};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type SelectBlockCallback = Rc<RefCell<Option<Box<dyn Fn(usize)>>>>;

#[derive(Clone)]
pub struct TocPopover {
    pub toggle_button: gtk4::MenuButton,
    popover: gtk4::Popover,
    list_box: gtk4::Box,
    on_select: SelectBlockCallback,
}

impl Default for TocPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl TocPopover {
    pub fn new() -> Self {
        let toggle_button = gtk4::MenuButton::new();
        toggle_button.set_child(Some(&symbolic_image(AppIcon::SidebarRight, 16)));
        toggle_button.set_tooltip_text(Some("Table of Contents"));
        toggle_button.add_css_class("sillpaper-icon-btn");
        toggle_button.set_halign(gtk4::Align::End);
        toggle_button.set_valign(gtk4::Align::Start);
        toggle_button.set_margin_top(46);
        toggle_button.set_margin_end(16);
        toggle_button.set_can_focus(false);
        toggle_button.set_focus_on_click(false);

        let popover = gtk4::Popover::new();
        popover.add_css_class("sillpaper-toc-popover");
        popover.set_size_request(280, -1);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_max_content_height(380);
        scroll.set_propagate_natural_height(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

        let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        scroll.set_child(Some(&list_box));
        popover.set_child(Some(&scroll));
        toggle_button.set_popover(Some(&popover));

        Self {
            toggle_button,
            popover,
            list_box,
            on_select: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set_on_select(&self, f: impl Fn(usize) + 'static) {
        *self.on_select.borrow_mut() = Some(Box::new(f));
    }

    pub fn is_open(&self) -> bool {
        self.popover.is_visible()
    }

    pub fn connect_opened(&self, f: impl Fn() + 'static) {
        self.popover.connect_show(move |_| f());
    }

    pub fn connect_closed(&self, f: impl Fn() + 'static) {
        self.popover.connect_closed(move |_| f());
    }

    pub fn set_entries(&self, entries: &[TocEntry], active_block: usize) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        if entries.is_empty() {
            let empty = gtk4::Label::new(Some(
                "No headings yet\nUse Ctrl+1 / 2 / 3 or # to add headings.",
            ));
            empty.set_wrap(true);
            empty.set_justify(gtk4::Justification::Center);
            empty.add_css_class("sillpaper-shortcut-hint");
            empty.set_margin_top(12);
            empty.set_margin_bottom(12);
            empty.set_margin_start(8);
            empty.set_margin_end(8);
            self.list_box.append(&empty);
            return;
        }

        let active_entry_block = entries
            .iter()
            .rfind(|e| e.block_idx <= active_block)
            .map(|e| e.block_idx);

        for entry in entries {
            let display = if entry.text.is_empty() {
                "Empty heading".to_string()
            } else {
                entry.text.clone()
            };
            let btn = gtk4::Button::new();
            btn.add_css_class("sillpaper-toc-row");
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            let label = gtk4::Label::new(Some(&display));
            label.set_xalign(0.0);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            label.set_margin_start(match entry.level {
                1 => 2,
                2 => 14,
                _ => 26,
            });
            match entry.level {
                1 => label.set_opacity(1.0),
                2 => label.set_opacity(0.84),
                _ => label.set_opacity(0.72),
            }
            btn.set_child(Some(&label));

            if active_entry_block == Some(entry.block_idx) {
                btn.add_css_class("toc-active");
            }

            let cb = self.on_select.clone();
            let pop = self.popover.clone();
            let b_idx = entry.block_idx;
            btn.connect_clicked(move |_| {
                pop.popdown();
                if let Some(f) = cb.borrow().as_ref() {
                    f(b_idx);
                }
            });

            self.list_box.append(&btn);
        }
    }

    pub fn hide(&self) {
        if self.popover.is_visible() {
            self.popover.popdown();
        }
    }
}
