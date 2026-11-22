use crate::ui::icons::{AppIcon, symbolic_button};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type QueryCallback = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;
type StepCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;
type CloseCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

#[derive(Clone)]
pub struct SearchBar {
    pub container: gtk4::Box,
    pub entry: gtk4::Entry,
    counter_label: gtk4::Label,
    on_query: QueryCallback,
    on_step: StepCallback,
    on_close: CloseCallback,
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBar {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        container.add_css_class("sillpaper-search-bar");
        container.set_halign(gtk4::Align::End);
        container.set_valign(gtk4::Align::Start);
        container.set_margin_top(48);
        container.set_margin_end(16);
        container.set_visible(false);

        let entry = gtk4::Entry::new();
        entry.set_placeholder_text(Some("Find in document..."));
        entry.set_width_chars(22);

        let counter_label = gtk4::Label::new(Some("0 / 0"));
        counter_label.add_css_class("sillpaper-shortcut-hint");
        counter_label.set_width_chars(7);

        let btn_prev = symbolic_button(AppIcon::ChevronUp);
        btn_prev.add_css_class("sillpaper-icon-btn");
        btn_prev.set_tooltip_text(Some("Previous Match (Shift+Enter)"));

        let btn_next = symbolic_button(AppIcon::ChevronDown);
        btn_next.add_css_class("sillpaper-icon-btn");
        btn_next.set_tooltip_text(Some("Next Match (Enter / F3)"));

        let btn_close = symbolic_button(AppIcon::Close);
        btn_close.add_css_class("sillpaper-icon-btn");
        btn_close.set_tooltip_text(Some("Close Search (Esc)"));

        container.append(&entry);
        container.append(&counter_label);
        container.append(&btn_prev);
        container.append(&btn_next);
        container.append(&btn_close);

        let on_query: QueryCallback = Rc::new(RefCell::new(None));
        let on_step: StepCallback = Rc::new(RefCell::new(None));
        let on_close: CloseCallback = Rc::new(RefCell::new(None));

        {
            let cb = on_query.clone();
            entry.connect_changed(move |e| {
                if let Some(f) = cb.borrow().as_ref() {
                    f(&e.text());
                }
            });
        }

        {
            let cb = on_step.clone();
            btn_next.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() {
                    f(true);
                }
            });
        }
        {
            let cb = on_step.clone();
            btn_prev.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() {
                    f(false);
                }
            });
        }
        {
            let cb = on_close.clone();
            btn_close.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() {
                    f();
                }
            });
        }

        {
            let key = gtk4::EventControllerKey::new();
            let step_cb = on_step.clone();
            let close_cb = on_close.clone();
            key.connect_key_pressed(move |_, keyval, _, mods| {
                if keyval == gdk::Key::Escape {
                    if let Some(f) = close_cb.borrow().as_ref() {
                        f();
                    }
                    return glib::Propagation::Stop;
                }
                if keyval == gdk::Key::Return
                    || keyval == gdk::Key::KP_Enter
                    || keyval == gdk::Key::F3
                {
                    let forward = !mods.contains(gdk::ModifierType::SHIFT_MASK);
                    if let Some(f) = step_cb.borrow().as_ref() {
                        f(forward);
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            entry.add_controller(key);
        }

        Self {
            container,
            entry,
            counter_label,
            on_query,
            on_step,
            on_close,
        }
    }

    pub fn set_on_query(&self, f: impl Fn(&str) + 'static) {
        *self.on_query.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_step(&self, f: impl Fn(bool) + 'static) {
        *self.on_step.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_close(&self, f: impl Fn() + 'static) {
        *self.on_close.borrow_mut() = Some(Box::new(f));
    }

    pub fn open(&self, initial_text: Option<&str>) {
        self.container.set_visible(true);
        if let Some(t) = initial_text
            && !t.is_empty()
            && t.len() <= 200
        {
            self.entry.set_text(t);
        }
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    pub fn close(&self) {
        self.container.set_visible(false);
        self.counter_label.set_text("0 / 0");
    }

    pub fn is_open(&self) -> bool {
        self.container.is_visible()
    }

    pub fn set_counter(&self, current: usize, total: usize) {
        if total == 0 {
            self.counter_label.set_text("0 / 0");
        } else {
            self.counter_label
                .set_text(&format!("{} / {}", current + 1, total));
        }
    }
}
