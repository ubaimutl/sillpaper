use adw::prelude::AnimationExt;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type SidebarNotifyCb = Rc<dyn Fn(&PushDrawer)>;

mod imp {
    use super::*;

    pub struct PushDrawer {
        pub sidebar: RefCell<Option<gtk4::Widget>>,
        pub content: RefCell<Option<gtk4::Widget>>,
        pub scrim: RefCell<Option<gtk4::Box>>,
        pub sidebar_width: Cell<f64>,
        pub show_sidebar: Cell<bool>,
        pub progress: Cell<f64>,
        pub anim: RefCell<Option<adw::TimedAnimation>>,
        pub listeners: RefCell<Vec<SidebarNotifyCb>>,
    }

    impl Default for PushDrawer {
        fn default() -> Self {
            Self {
                sidebar: RefCell::new(None),
                content: RefCell::new(None),
                scrim: RefCell::new(None),
                sidebar_width: Cell::new(350.0),
                show_sidebar: Cell::new(false),
                progress: Cell::new(0.0),
                anim: RefCell::new(None),
                listeners: RefCell::new(Vec::new()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PushDrawer {
        const NAME: &'static str = "SillpaperPushDrawer";
        type Type = super::PushDrawer;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for PushDrawer {
        fn dispose(&self) {
            if let Some(anim) = self.anim.borrow_mut().take() {
                anim.pause();
            }
            if let Some(w) = self.sidebar.borrow_mut().take() {
                w.unparent();
            }
            if let Some(w) = self.scrim.borrow_mut().take() {
                w.unparent();
            }
            if let Some(w) = self.content.borrow_mut().take() {
                w.unparent();
            }
        }
    }

    impl WidgetImpl for PushDrawer {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if let Some(sb) = self.sidebar.borrow().as_ref()
                && sb.is_child_visible()
            {
                let _ = sb.measure(orientation, for_size);
            }
            if let Some(scrim) = self.scrim.borrow().as_ref()
                && scrim.is_child_visible()
            {
                let _ = scrim.measure(orientation, for_size);
            }
            if let Some(content) = self.content.borrow().as_ref() {
                return content.measure(orientation, for_size);
            }
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if width <= 0 || height <= 0 {
                return;
            }
            let target_w = self.sidebar_width.get().round() as i32;
            let sw = target_w.min((width - 48).max(200)).min(width).max(0);
            let p = self.progress.get().clamp(0.0, 1.0);
            let offset = (p * sw as f64).round() as f32;

            if let Some(content) = self.content.borrow().as_ref() {
                let transform = if offset > 0.0 {
                    Some(gsk::Transform::new().translate(&graphene::Point::new(offset, 0.0)))
                } else {
                    None
                };
                content.allocate(width, height, baseline, transform);
            }

            if let Some(scrim) = self.scrim.borrow().as_ref()
                && scrim.is_child_visible()
            {
                let scrim_w = (width - offset as i32).max(0);
                let transform = if offset > 0.0 {
                    Some(gsk::Transform::new().translate(&graphene::Point::new(offset, 0.0)))
                } else {
                    None
                };
                scrim.allocate(scrim_w, height, baseline, transform);
            }

            if let Some(sb) = self.sidebar.borrow().as_ref()
                && sb.is_child_visible()
            {
                let sb_x = offset - sw as f32;
                let transform = if sb_x.abs() > 0.01 {
                    Some(gsk::Transform::new().translate(&graphene::Point::new(sb_x, 0.0)))
                } else {
                    None
                };
                sb.allocate(sw, height, baseline, transform);
            }
        }
    }
}

glib::wrapper! {
              pub struct PushDrawer(ObjectSubclass<imp::PushDrawer>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PushDrawer {
    fn default() -> Self {
        Self::new()
    }
}

impl PushDrawer {
    pub fn new() -> Self {
        let obj: Self = glib::Object::new();
        obj.add_css_class("sillpaper-push-drawer");
        obj.set_overflow(gtk4::Overflow::Hidden);
        obj.set_hexpand(true);
        obj.set_vexpand(true);

        let scrim = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        scrim.set_child_visible(false);
        scrim.set_can_target(true);
        scrim.set_can_focus(false);
        scrim.set_parent(&obj);

        let click = gtk4::GestureClick::new();
        click.set_button(0);
        let drawer_c = obj.clone();
        click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if drawer_c.shows_sidebar() {
                drawer_c.set_show_sidebar(false);
            }
        });
        scrim.add_controller(click);
        *obj.imp().scrim.borrow_mut() = Some(scrim);

        obj
    }

    pub fn set_sidebar_width(&self, width: f64) {
        self.imp().sidebar_width.set(width.max(200.0));
        self.queue_allocate();
    }

    pub fn set_content(&self, widget: Option<&impl IsA<gtk4::Widget>>) {
        let imp = self.imp();
        if let Some(old) = imp.content.borrow_mut().take() {
            old.unparent();
        }
        if let Some(w) = widget {
            let w: gtk4::Widget = w.clone().upcast();
            if let Some(scrim) = imp.scrim.borrow().as_ref() {
                w.insert_before(self, Some(scrim));
            } else if let Some(sb) = imp.sidebar.borrow().as_ref() {
                w.insert_before(self, Some(sb));
            } else {
                w.set_parent(self);
            }
            *imp.content.borrow_mut() = Some(w);
        }
        self.queue_resize();
    }

    pub fn set_sidebar(&self, widget: Option<&impl IsA<gtk4::Widget>>) {
        let imp = self.imp();
        if let Some(old) = imp.sidebar.borrow_mut().take() {
            old.unparent();
        }
        if let Some(w) = widget {
            let w: gtk4::Widget = w.clone().upcast();
            w.set_child_visible(imp.show_sidebar.get() || imp.progress.get() > 0.0);
            if let Some(scrim) = imp.scrim.borrow().as_ref() {
                w.insert_after(self, Some(scrim));
            } else {
                w.set_parent(self);
            }
            *imp.sidebar.borrow_mut() = Some(w);
        }
        self.queue_resize();
    }

    pub fn shows_sidebar(&self) -> bool {
        self.imp().show_sidebar.get()
    }

    pub fn sidebar_progress(&self) -> f64 {
        self.imp().progress.get()
    }

    pub fn scrim_visible(&self) -> bool {
        self.imp()
            .scrim
            .borrow()
            .as_ref()
            .is_some_and(|s| s.is_child_visible())
    }

    pub fn set_show_sidebar(&self, show: bool) {
        let imp = self.imp();
        if imp.show_sidebar.get() == show {
            return;
        }
        imp.show_sidebar.set(show);

        if show {
            if let Some(sb) = imp.sidebar.borrow().as_ref() {
                sb.set_child_visible(true);
            }
            if let Some(scrim) = imp.scrim.borrow().as_ref() {
                scrim.set_child_visible(true);
            }
        }

        if let Some(prev) = imp.anim.borrow_mut().take() {
            prev.pause();
        }

        let start = imp.progress.get();
        let target = if show { 1.0 } else { 0.0 };
        let drawer_step = self.clone();
        let cb_target = adw::CallbackAnimationTarget::new(move |val| {
            drawer_step.imp().progress.set(val);
            drawer_step.queue_allocate();
        });

        let anim = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(start)
            .value_to(target)
            .duration(240)
            .easing(adw::Easing::EaseOutCubic)
            .target(&cb_target)
            .build();

        let drawer_done = self.clone();
        anim.connect_done(move |finished| {
            let is_current = drawer_done
                .imp()
                .anim
                .borrow()
                .as_ref()
                .is_some_and(|cur| cur == finished);
            if is_current {
                *drawer_done.imp().anim.borrow_mut() = None;
            }
            if !drawer_done.imp().show_sidebar.get() && drawer_done.imp().progress.get() <= 0.001 {
                if let Some(sb) = drawer_done.imp().sidebar.borrow().as_ref() {
                    sb.set_child_visible(false);
                }
                let drawer_idle = drawer_done.clone();
                glib::idle_add_local_once(move || {
                    if !drawer_idle.imp().show_sidebar.get()
                        && drawer_idle.imp().progress.get() <= 0.001
                        && let Some(scrim) = drawer_idle.imp().scrim.borrow().as_ref()
                    {
                        scrim.set_child_visible(false);
                    }
                });
            }
            drawer_done.queue_allocate();
        });

        *imp.anim.borrow_mut() = Some(anim.clone());
        anim.play();

        let cbs: Vec<SidebarNotifyCb> = imp.listeners.borrow().clone();
        for cb in cbs {
            cb(self);
        }
    }

    pub fn connect_show_sidebar_notify(&self, f: impl Fn(&PushDrawer) + 'static) {
        self.imp().listeners.borrow_mut().push(Rc::new(f));
    }
}
