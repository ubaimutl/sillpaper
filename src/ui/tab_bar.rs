use crate::ui::icons::{AppIcon, symbolic_button_sized};
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

struct TabEntry {
    wrap: gtk4::Overlay,
    button: gtk4::Button,
}

#[derive(Clone)]
pub struct CompactTabBar {
    pub root: gtk4::ScrolledWindow,
    tabs_box: gtk4::Box,
    entries: Rc<RefCell<HashMap<glib::Object, TabEntry>>>,
}

impl CompactTabBar {
    pub fn new(tab_view: &adw::TabView) -> Self {
        let root = gtk4::ScrolledWindow::new();
        root.add_css_class("sillpaper-tabbar-scroll");
        root.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        root.set_propagate_natural_width(true);
        root.set_min_content_width(0);
        root.set_hexpand(false);
        root.set_vexpand(false);

        let tabs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        tabs_box.add_css_class("sillpaper-tabbar");
        tabs_box.set_valign(gtk4::Align::Center);
        root.set_child(Some(&tabs_box));

        {
            let scroll_ctrl = gtk4::EventControllerScroll::new(
                gtk4::EventControllerScrollFlags::VERTICAL
                    | gtk4::EventControllerScrollFlags::HORIZONTAL,
            );
            let sw = root.clone();
            scroll_ctrl.connect_scroll(move |_, dx, dy| {
                let adj = sw.hadjustment();
                let max = (adj.upper() - adj.page_size()).max(0.0);
                if max <= 0.5 {
                    return glib::Propagation::Proceed;
                }
                let delta = if dx.abs() > dy.abs() { dx } else { dy };
                adj.set_value((adj.value() + delta * 36.0).clamp(0.0, max));
                glib::Propagation::Stop
            });
            root.add_controller(scroll_ctrl);
        }

        let bar = Self {
            root,
            tabs_box,
            entries: Rc::new(RefCell::new(HashMap::new())),
        };

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            bar.insert_page(tab_view, &page, i);
        }
        bar.sync_selected(tab_view);

        {
            let b = bar.clone();
            tab_view.connect_page_attached(move |tv, page, pos| {
                b.insert_page(tv, page, pos);
                b.sync_selected(tv);
            });
        }
        {
            let b = bar.clone();
            tab_view.connect_page_detached(move |tv, page, _| {
                b.remove_page(page);
                b.sync_selected(tv);
            });
        }
        {
            let b = bar.clone();
            tab_view.connect_page_reordered(move |tv, page, pos| {
                b.reorder_page(tv, page, pos);
            });
        }
        {
            let b = bar.clone();
            tab_view.connect_selected_page_notify(move |tv| {
                b.sync_selected(tv);
            });
        }

        bar
    }

    fn insert_page(&self, tab_view: &adw::TabView, page: &adw::TabPage, pos: i32) {
        let key: glib::Object = page.clone().upcast();
        if self.entries.borrow().contains_key(&key) {
            return;
        }

        let wrap = gtk4::Overlay::new();
        wrap.add_css_class("sillpaper-tab-wrap");
        wrap.set_valign(gtk4::Align::Center);
        wrap.set_hexpand(true);

        let btn = gtk4::Button::new();
        btn.add_css_class("sillpaper-tab");
        btn.set_can_focus(false);
        btn.set_focus_on_click(false);

        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        inner.set_valign(gtk4::Align::Center);
        inner.set_margin_end(14);

        let title_label = gtk4::Label::new(Some(page.title().as_str()));
        title_label.add_css_class("sillpaper-tab-title");
        title_label.set_ellipsize(pango::EllipsizeMode::End);
        title_label.set_single_line_mode(true);
        title_label.set_width_chars(1);
        title_label.set_max_width_chars(16);
        title_label.set_xalign(0.0);
        title_label.set_hexpand(true);

        let dirty_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dirty_dot.add_css_class("sillpaper-tab-dot");
        dirty_dot.set_valign(gtk4::Align::Center);
        dirty_dot.set_opacity(if page.needs_attention() { 1.0 } else { 0.0 });

        inner.append(&dirty_dot);
        inner.append(&title_label);
        btn.set_child(Some(&inner));
        wrap.set_child(Some(&btn));

        let close_btn = symbolic_button_sized(AppIcon::Close, 12);
        close_btn.add_css_class("sillpaper-tab-close");
        close_btn.set_halign(gtk4::Align::End);
        close_btn.set_valign(gtk4::Align::Center);
        close_btn.set_margin_end(5);
        close_btn.set_can_focus(false);
        close_btn.set_focus_on_click(false);
        close_btn.set_tooltip_text(Some("Close tab"));
        close_btn.set_visible(false);
        wrap.add_overlay(&close_btn);

        let tip = page.tooltip();
        if let Some(t) = tip.as_deref().filter(|s| !s.is_empty()) {
            wrap.set_tooltip_text(Some(t));
        } else {
            wrap.set_tooltip_text(Some(page.title().as_str()));
        }

        {
            let motion = gtk4::EventControllerMotion::new();
            let close = close_btn.clone();
            motion.connect_enter(move |_, _, _| close.set_visible(true));
            let close = close_btn.clone();
            motion.connect_leave(move |_| close.set_visible(false));
            wrap.add_controller(motion);
        }

        {
            let tv = tab_view.clone();
            let pg = page.clone();
            btn.connect_clicked(move |_| {
                tv.set_selected_page(&pg);
            });
        }

        {
            let tv = tab_view.clone();
            let pg = page.clone();
            close_btn.connect_clicked(move |_| {
                tv.close_page(&pg);
            });
        }

        {
            let mid = gtk4::GestureClick::builder().button(2).build();
            let tv = tab_view.clone();
            let pg = page.clone();
            mid.connect_released(move |_, _, _, _| {
                tv.close_page(&pg);
            });
            wrap.add_controller(mid);
        }

        {
            let lbl = title_label.clone();
            let w = wrap.clone();
            page.connect_title_notify(move |p| {
                let t = p.title();
                lbl.set_text(t.as_str());
                if p.tooltip().as_deref().is_none_or(str::is_empty) {
                    w.set_tooltip_text(Some(t.as_str()));
                }
            });
        }
        {
            let w = wrap.clone();
            page.connect_tooltip_notify(move |p| {
                let tip = p.tooltip();
                if let Some(t) = tip.as_deref().filter(|s| !s.is_empty()) {
                    w.set_tooltip_text(Some(t));
                } else {
                    w.set_tooltip_text(Some(p.title().as_str()));
                }
            });
        }
        {
            let dot = dirty_dot.clone();
            page.connect_needs_attention_notify(move |p| {
                dot.set_opacity(if p.needs_attention() { 1.0 } else { 0.0 });
            });
        }

        if pos <= 0 {
            self.tabs_box.prepend(&wrap);
        } else {
            let mut prev = self.tabs_box.first_child();
            for _ in 1..pos {
                if let Some(p) = prev {
                    prev = p.next_sibling();
                }
            }
            self.tabs_box.insert_child_after(&wrap, prev.as_ref());
        }

        self.entries
            .borrow_mut()
            .insert(key, TabEntry { wrap, button: btn });
    }

    fn remove_page(&self, page: &adw::TabPage) {
        let key: glib::Object = page.clone().upcast();
        if let Some(entry) = self.entries.borrow_mut().remove(&key) {
            self.tabs_box.remove(&entry.wrap);
        }
    }

    fn reorder_page(&self, tab_view: &adw::TabView, page: &adw::TabPage, pos: i32) {
        let key: glib::Object = page.clone().upcast();
        let entries = self.entries.borrow();
        let Some(entry) = entries.get(&key) else {
            return;
        };
        if pos <= 0 {
            self.tabs_box
                .reorder_child_after(&entry.wrap, gtk4::Widget::NONE);
        } else if pos - 1 < tab_view.n_pages() {
            let prev_page = tab_view.nth_page(pos - 1);
            let prev_key: glib::Object = prev_page.upcast();
            if let Some(prev_entry) = entries.get(&prev_key) {
                self.tabs_box
                    .reorder_child_after(&entry.wrap, Some(&prev_entry.wrap));
            }
        }
    }

    fn sync_selected(&self, tab_view: &adw::TabView) {
        let sel_key: Option<glib::Object> = tab_view.selected_page().map(|p| p.upcast());
        let mut active_wrap: Option<gtk4::Overlay> = None;
        for (k, entry) in self.entries.borrow().iter() {
            let is_sel = sel_key.as_ref() == Some(k);
            if is_sel {
                entry.button.add_css_class("tab-active");
                active_wrap = Some(entry.wrap.clone());
            } else {
                entry.button.remove_css_class("tab-active");
            }
        }

        if let Some(wrap) = active_wrap {
            let sw = self.root.clone();
            let tb = self.tabs_box.clone();
            glib::idle_add_local_once(move || {
                let adj = sw.hadjustment();
                let page_sz = adj.page_size();
                if adj.upper() <= page_sz + 1.0 {
                    return;
                }
                if let Some(pt) = wrap.compute_point(&tb, &gtk4::graphene::Point::zero()) {
                    let x0 = pt.x() as f64;
                    let x1 = x0 + wrap.width() as f64;
                    let cur = adj.value();
                    if x0 < cur {
                        adj.set_value(x0.max(0.0));
                    } else if x1 > cur + page_sz {
                        adj.set_value((x1 - page_sz).max(0.0));
                    }
                }
            });
        }
    }
}
