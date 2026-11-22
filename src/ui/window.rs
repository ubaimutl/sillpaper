use crate::config::{
    AccentSwatch, FontChoice, GhostSuggestionMode, LineSpacing, Preferences, RendererBackend,
    TextWidth, ThemeId, generate_app_css, resolve_palette,
};
use crate::extensions::{
    ExtensionEvent, ExtensionManager, ExtensionRequestKind, default_extensions_dir,
};
use crate::io::backup::{SessionOrder, StorageManager, WindowState, file_mtime_ms};
use crate::io::docx::document_to_docx_bytes_with_options;
use crate::io::print::{export_document_to_pdf_with_base, print_document_native};
use crate::io::{FileFormat, atomic_write, load_document, save_document_with_prefs};
use crate::model::stats::{compute_stats, extract_toc, format_reading_time};
use crate::model::{BlockKind, Document, InlineAttr, TextDirection};
use crate::ui::dialogs::{
    UnsavedChoice, build_shortcut_cheatsheet, show_confirm_unsaved_dialog,
    show_external_change_dialog, show_info_dialog, show_writing_goal_dialog,
};
use crate::ui::editor_tab::{
    EditorTab, EditorTabCallbacks, ExtensionTrigger, create_shared_tag_table, generate_tab_id,
    is_image_file_path, update_tag_styles_table,
};
use crate::ui::extension_overlays::{ActionPreviewCard, ExtensionHintBanner, QuickFixCard};
use crate::ui::floating_toolbar::{FloatingToolbar, ToolbarAction};
use crate::ui::icons::{AppIcon, symbolic_button, symbolic_image};
use crate::ui::preferences_ui::PreferencesPanel;
use crate::ui::push_drawer::PushDrawer;
use crate::ui::search_bar::SearchBar;
use crate::ui::tab_bar::CompactTabBar;
use crate::ui::toc_popover::TocPopover;
use crate::ui::word_counter::{WordCounterWidget, format_with_commas};
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static NEXT_WAKE_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static EXT_WAKE_HANDLERS: RefCell<HashMap<u64, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
}

#[derive(Clone, PartialEq)]
struct EditorVisualKey {
    theme: ThemeId,
    custom_theme_bg: String,
    text_width: TextWidth,
    font: FontChoice,
    font_size_percent: u16,
    text_color: AccentSwatch,
    custom_text_color: String,
    caret_color: AccentSwatch,
    paragraph_margins: bool,
    line_spacing: LineSpacing,
    focus_mode: bool,
    focus_mode_opacity_bits: u64,
    typewriter_mode: bool,
    default_direction: TextDirection,
}

impl EditorVisualKey {
    fn from_prefs(p: &Preferences) -> Self {
        Self {
            theme: p.theme,
            custom_theme_bg: p.custom_theme_bg.clone(),
            text_width: p.text_width,
            font: p.font.clone(),
            font_size_percent: p.font_size_percent,
            text_color: p.text_color,
            custom_text_color: p.custom_text_color.clone(),
            caret_color: p.caret_color,
            paragraph_margins: p.paragraph_margins,
            line_spacing: p.line_spacing,
            focus_mode: p.focus_mode,
            focus_mode_opacity_bits: p.focus_mode_opacity.to_bits(),
            typewriter_mode: p.typewriter_mode,
            default_direction: p.default_direction,
        }
    }
}

#[derive(Clone)]
pub struct SillpaperWindow {
    pub window: adw::ApplicationWindow,
    app: adw::Application,
    is_primary: bool,
    storage: Rc<StorageManager>,
    prefs: Rc<RefCell<Preferences>>,
    css_provider: gtk4::CssProvider,
    last_css: Rc<RefCell<String>>,
    last_editor_visual: Rc<RefCell<Option<EditorVisualKey>>>,
    shared_tag_table: gtk4::TextTagTable,
    tab_view: adw::TabView,
    topbar: gtk4::CenterBox,
    edge_drawer_btn: gtk4::Button,
    canvas_overlay: gtk4::Overlay,
    split_view: PushDrawer,
    sidebar_stack: gtk4::Stack,
    settings_slot: gtk4::Box,
    settings_panel: Rc<RefCell<Option<PreferencesPanel>>>,
    system_fonts_cache: Rc<RefCell<Option<Vec<String>>>>,
    settings_teardown_timer: Rc<RefCell<Option<glib::SourceId>>>,
    tabs: Rc<RefCell<HashMap<String, EditorTab>>>,
    page_to_id: Rc<RefCell<HashMap<glib::Object, String>>>,
    floating_toolbar: FloatingToolbar,
    quick_fix_card: QuickFixCard,
    action_preview_card: ActionPreviewCard,
    ext_hint_banner: ExtensionHintBanner,
    ext_manager: Rc<RefCell<ExtensionManager>>,
    ext_wake_id: u64,
    ext_last_running_ids: Rc<RefCell<Vec<String>>>,
    ext_comp_timer: Rc<RefCell<Option<glib::SourceId>>>,
    ext_diag_timer: Rc<RefCell<Option<glib::SourceId>>>,
    ext_last_auto_diag_at: Rc<RefCell<Option<std::time::Instant>>>,
    ext_last_diag_text: Rc<RefCell<HashMap<(String, usize), String>>>,
    ext_last_bg_error_toast_at: Rc<RefCell<Option<std::time::Instant>>>,
    ext_active_action_label: Rc<RefCell<String>>,
    ext_active_tab_id: Rc<RefCell<Option<String>>>,
    search_bar: SearchBar,
    toc: TocPopover,
    toc_dirty: Rc<Cell<bool>>,
    word_counter: WordCounterWidget,
    drawer_stats_label: gtk4::Label,
    shortcut_sheet: gtk4::Box,
    chrome_hidden: Rc<Cell<bool>>,
    pointer_in_chrome_zone: Rc<Cell<bool>>,
    chrome_hide_timer: Rc<RefCell<Option<glib::SourceId>>>,
    ctrl_hold_timer: Rc<RefCell<Option<glib::SourceId>>>,
    stats_timer: Rc<RefCell<Option<glib::SourceId>>>,
    toolbar_timer: Rc<RefCell<Option<glib::SourceId>>>,
    allow_close_tab: Rc<Cell<bool>>,
    active_renderer: RendererBackend,
    on_spawn_window: Rc<dyn Fn()>,
    on_broadcast_prefs: Rc<dyn Fn(Preferences)>,
    on_request_restart: Rc<dyn Fn()>,
}

impl SillpaperWindow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: &adw::Application,
        storage: Rc<StorageManager>,
        prefs: Rc<RefCell<Preferences>>,
        css_provider: gtk4::CssProvider,
        is_primary: bool,
        active_renderer: RendererBackend,
        initial_files: Vec<PathBuf>,
        on_spawn_window: Rc<dyn Fn()>,
        on_broadcast_prefs: Rc<dyn Fn(Preferences)>,
        on_request_restart: Rc<dyn Fn()>,
    ) -> Self {
        let win_state = storage.load_window_state();

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Sillpaper")
            .default_width(win_state.width.max(480))
            .default_height(win_state.height.max(320))
            .build();
        window.set_size_request(480, 320);
        window.add_css_class("sillpaper-window");

        let tab_view = adw::TabView::new();
        tab_view.set_vexpand(true);
        tab_view.set_hexpand(true);

        let tab_bar = CompactTabBar::new(&tab_view);

        let topbar = gtk4::CenterBox::new();
        topbar.set_shrink_center_last(false);
        topbar.add_css_class("sillpaper-topbar");

        let win_controls_start = gtk4::WindowControls::new(gtk4::PackType::Start);
        let win_controls_end = gtk4::WindowControls::new(gtk4::PackType::End);

        let btn_menu = symbolic_button(AppIcon::SidebarLeft);
        btn_menu.add_css_class("sillpaper-icon-btn");
        btn_menu.set_tooltip_text(Some("Open menu"));
        btn_menu.set_can_focus(false);
        btn_menu.set_focus_on_click(false);

        let btn_add_tab = symbolic_button(AppIcon::Plus);
        btn_add_tab.add_css_class("sillpaper-icon-btn");
        btn_add_tab.set_tooltip_text(Some("New Tab (Ctrl+N)"));
        btn_add_tab.set_can_focus(false);
        btn_add_tab.set_focus_on_click(false);

        let leading_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        leading_actions.append(&win_controls_start);
        leading_actions.append(&btn_menu);

        let documents = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        documents.append(&tab_bar.root);
        documents.append(&btn_add_tab);

        topbar.set_start_widget(Some(&leading_actions));
        topbar.set_center_widget(Some(&documents));
        topbar.set_end_widget(Some(&win_controls_end));

        let topbar_handle = gtk4::WindowHandle::new();
        topbar_handle.add_css_class("sillpaper-topbar-handle");
        topbar_handle.set_child(Some(&topbar));

        let edge_drawer_btn = gtk4::Button::new();
        edge_drawer_btn.add_css_class("sillpaper-edge-drawer-trigger");
        edge_drawer_btn.set_halign(gtk4::Align::Start);
        edge_drawer_btn.set_valign(gtk4::Align::Fill);
        edge_drawer_btn.set_margin_top(40);
        edge_drawer_btn.set_width_request(26);
        edge_drawer_btn.set_can_focus(false);
        edge_drawer_btn.set_focus_on_click(false);
        edge_drawer_btn.set_cursor_from_name(Some("pointer"));
        edge_drawer_btn.set_tooltip_text(Some("Open menu"));
        let edge_arrow = symbolic_image(AppIcon::ChevronRight, 16);
        edge_arrow.set_halign(gtk4::Align::Center);
        edge_arrow.set_valign(gtk4::Align::Center);
        edge_drawer_btn.set_child(Some(&edge_arrow));

        let floating_toolbar = FloatingToolbar::new();
        let quick_fix_card = QuickFixCard::new();
        let action_preview_card = ActionPreviewCard::new();
        let ext_hint_banner = ExtensionHintBanner::new();
        let ext_manager = Rc::new(RefCell::new(
            ExtensionManager::new(default_extensions_dir()),
        ));
        let ext_wake_id = NEXT_WAKE_WINDOW_ID.fetch_add(1, Ordering::Relaxed);
        let shared_tag_table = create_shared_tag_table(&prefs.borrow());
        let search_bar = SearchBar::new();
        let toc = TocPopover::new();
        let word_counter = WordCounterWidget::new();
        let shortcut_sheet = build_shortcut_cheatsheet();

        let sidebar_stack = gtk4::Stack::new();
        sidebar_stack.add_css_class("sillpaper-sidebar-shell");
        sidebar_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        sidebar_stack.set_transition_duration(150);
        sidebar_stack.set_hhomogeneous(false);

        let drawer_box = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        drawer_box.add_css_class("sillpaper-drawer");

        let settings_slot = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        sidebar_stack.add_named(&drawer_box, Some("menu"));
        sidebar_stack.add_named(&settings_slot, Some("settings"));
        sidebar_stack.set_visible_child_name("menu");

        let drawer_stats_label = gtk4::Label::new(None);
        drawer_stats_label.add_css_class("sillpaper-drawer-stats");
        drawer_stats_label.set_use_markup(true);
        drawer_stats_label.set_xalign(0.0);
        drawer_stats_label.set_margin_start(14);
        drawer_stats_label.set_margin_top(8);

        let split_view = PushDrawer::new();
        split_view.set_sidebar_width(350.0);
        split_view.set_sidebar(Some(&sidebar_stack));
        split_view.set_content(Some(&tab_view));

        let main_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        main_vbox.add_css_class("sillpaper-canvas-host");
        main_vbox.set_hexpand(true);
        main_vbox.set_vexpand(true);
        main_vbox.append(&topbar_handle);
        main_vbox.append(&split_view);

        let canvas_overlay = gtk4::Overlay::new();
        canvas_overlay.set_hexpand(true);
        canvas_overlay.set_vexpand(true);
        canvas_overlay.set_child(Some(&main_vbox));
        canvas_overlay.add_overlay(&edge_drawer_btn);
        canvas_overlay.add_overlay(&toc.toggle_button);
        canvas_overlay.add_overlay(&search_bar.container);
        canvas_overlay.add_overlay(&word_counter.pill);
        canvas_overlay.add_overlay(&word_counter.toast);
        canvas_overlay.add_overlay(&shortcut_sheet);
        canvas_overlay.add_overlay(&ext_hint_banner.container);
        canvas_overlay.add_overlay(&quick_fix_card.container);
        canvas_overlay.add_overlay(&action_preview_card.container);
        canvas_overlay.add_overlay(&floating_toolbar.container);

        window.set_content(Some(&canvas_overlay));

        let sw = Self {
            window: window.clone(),
            app: app.clone(),
            is_primary,
            storage,
            prefs,
            css_provider,
            last_css: Rc::new(RefCell::new(String::new())),
            last_editor_visual: Rc::new(RefCell::new(None)),
            shared_tag_table,
            tab_view,
            topbar,
            edge_drawer_btn,
            canvas_overlay,
            split_view,
            sidebar_stack,
            settings_slot,
            settings_panel: Rc::new(RefCell::new(None)),
            system_fonts_cache: Rc::new(RefCell::new(None)),
            settings_teardown_timer: Rc::new(RefCell::new(None)),
            tabs: Rc::new(RefCell::new(HashMap::new())),
            page_to_id: Rc::new(RefCell::new(HashMap::new())),
            floating_toolbar,
            quick_fix_card,
            action_preview_card,
            ext_hint_banner,
            ext_manager,
            ext_wake_id,
            ext_last_running_ids: Rc::new(RefCell::new(Vec::new())),
            ext_comp_timer: Rc::new(RefCell::new(None)),
            ext_diag_timer: Rc::new(RefCell::new(None)),
            ext_last_auto_diag_at: Rc::new(RefCell::new(None)),
            ext_last_diag_text: Rc::new(RefCell::new(HashMap::new())),
            ext_last_bg_error_toast_at: Rc::new(RefCell::new(None)),
            ext_active_action_label: Rc::new(RefCell::new("Suggestion".into())),
            ext_active_tab_id: Rc::new(RefCell::new(None)),
            search_bar,
            toc,
            toc_dirty: Rc::new(Cell::new(true)),
            word_counter,
            drawer_stats_label,
            shortcut_sheet,
            chrome_hidden: Rc::new(Cell::new(false)),
            pointer_in_chrome_zone: Rc::new(Cell::new(false)),
            chrome_hide_timer: Rc::new(RefCell::new(None)),
            ctrl_hold_timer: Rc::new(RefCell::new(None)),
            stats_timer: Rc::new(RefCell::new(None)),
            toolbar_timer: Rc::new(RefCell::new(None)),
            allow_close_tab: Rc::new(Cell::new(false)),
            active_renderer,
            on_spawn_window,
            on_broadcast_prefs,
            on_request_restart,
        };

        sw.wire_extension_wake_handler();
        sw.wire_floating_toolbar();
        sw.wire_extension_overlays();
        sw.populate_drawer(&drawer_box);
        sw.wire_topbar_buttons(&btn_menu, &btn_add_tab);
        sw.wire_tab_view();
        sw.wire_search_and_toc();
        sw.wire_counter_click();
        sw.wire_window_controllers();
        sw.wire_drag_and_drop();
        sw.apply_preferences_visuals();

        let mut opened_any = false;
        if is_primary {
            let (restored, active_id) = sw.storage.restore_all_backups();
            let mut active_page: Option<adw::TabPage> = None;
            for (backup, doc) in restored {
                let page = sw.add_tab(
                    backup.id.clone(),
                    doc,
                    backup.file_path.clone(),
                    !backup.is_saved,
                    backup.last_file_mtime_ms,
                    backup.cursor_flat_offset,
                    false,
                );
                if active_id.as_deref() == Some(backup.id.as_str()) {
                    active_page = Some(page);
                }
                opened_any = true;
            }
            if let Some(p) = active_page {
                sw.tab_view.set_selected_page(&p);
            }
        }

        for path in initial_files {
            sw.open_path(&path);
            opened_any = true;
        }

        if !opened_any {
            sw.new_empty_tab();
        }

        if win_state.maximized {
            window.maximize();
        }
        if win_state.fullscreen {
            window.fullscreen();
        }

        sw.refresh_stats_now();
        sw.refresh_toc();
        sw.focus_active_editor();

        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), || {
            crate::trim_allocator_memory();
        });

        sw
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn active_tab(&self) -> Option<EditorTab> {
        let page = self.tab_view.selected_page()?;
        let id = self.page_to_id.borrow().get(&page.upcast())?.clone();
        self.tabs.borrow().get(&id).cloned()
    }

    pub fn focus_active_editor(&self) {
        if let Some(tab) = self.active_tab() {
            tab.focus_editor();
        }
    }

    pub fn update_preferences(&self, new_prefs: Preferences, broadcast: bool) {
        *self.prefs.borrow_mut() = new_prefs.clone();
        let _ = self.storage.save_preferences(&new_prefs);
        self.apply_preferences_visuals();
        if let Some(panel) = self.settings_panel.borrow().as_ref() {
            panel.sync_external(&new_prefs);
        }
        if broadcast {
            (self.on_broadcast_prefs)(new_prefs);
        }
    }

    fn ensure_settings_panel_built(&self) {
        if let Some(src) = self.settings_teardown_timer.borrow_mut().take() {
            src.remove();
        }
        if self.settings_panel.borrow().is_some() {
            return;
        }
        let system_fonts = {
            let mut cache = self.system_fonts_cache.borrow_mut();
            cache
                .get_or_insert_with(|| {
                    let mut fonts: Vec<String> = self
                        .window
                        .pango_context()
                        .list_families()
                        .into_iter()
                        .map(|f| f.name().to_string())
                        .filter(|n| {
                            !matches!(n.as_str(), "Serif" | "Sans" | "Monospace" | "System-ui")
                        })
                        .collect();
                    fonts.sort_by_key(|s| s.to_ascii_lowercase());
                    fonts.dedup();
                    fonts
                })
                .clone()
        };

        let sw_change = self.clone();
        let sw_restart = self.clone();
        let panel = PreferencesPanel::new(
            &self.prefs.borrow(),
            &system_fonts,
            self.active_renderer,
            self.ext_manager.clone(),
            move |new_p| {
                sw_change.update_preferences(new_p, true);
            },
            move || {
                sw_restart.flush_all_state();
                (sw_restart.on_request_restart)();
            },
        );
        let back = symbolic_button(AppIcon::ChevronLeft);
        back.add_css_class("sillpaper-settings-nav");
        back.set_tooltip_text(Some("Back to Menu"));
        let sw = self.clone();
        back.connect_clicked(move |_| sw.open_menu_drawer());

        let close = symbolic_button(AppIcon::Close);
        close.add_css_class("sillpaper-settings-nav");
        close.set_tooltip_text(Some("Close Settings (Ctrl+,)"));
        let sw = self.clone();
        close.connect_clicked(move |_| {
            sw.split_view.set_show_sidebar(false);
            sw.focus_active_editor();
        });

        panel.header.prepend(&back);
        panel.header.append(&close);
        self.settings_slot.append(&panel.root);
        *self.settings_panel.borrow_mut() = Some(panel);
    }

    fn schedule_settings_panel_teardown(&self) {
        if self.settings_panel.borrow().is_none() {
            return;
        }
        if let Some(src) = self.settings_teardown_timer.borrow_mut().take() {
            src.remove();
        }
        let sw = self.clone();
        let src = glib::timeout_add_local_once(std::time::Duration::from_millis(260), move || {
            *sw.settings_teardown_timer.borrow_mut() = None;
            if sw.split_view.shows_sidebar() {
                return;
            }
            if let Some(panel) = sw.settings_panel.borrow_mut().take() {
                sw.settings_slot.remove(&panel.root);
            }
            crate::trim_allocator_memory();
        });
        *self.settings_teardown_timer.borrow_mut() = Some(src);
    }

    pub fn flush_all_state(&self) {
        let win = &self.window;
        let (w, h) = win.default_size();
        let ws = WindowState {
            width: if w > 0 { w } else { win.width().max(480) },
            height: if h > 0 { h } else { win.height().max(320) },
            maximized: win.is_maximized(),
            fullscreen: win.is_fullscreen(),
        };
        let _ = self.storage.save_window_state(&ws);
        if self.is_primary {
            for tab in self.tabs.borrow().values() {
                tab.flush_backup_now();
            }
            self.save_session_order();
        }
    }

    fn open_menu_drawer(&self) {
        self.floating_toolbar.hide();
        self.sidebar_stack.set_visible_child_name("menu");
        self.split_view.set_show_sidebar(true);
        self.reveal_chrome();
        self.refresh_stats_now();
    }

    fn open_settings_drawer(&self) {
        self.floating_toolbar.hide();
        self.ensure_settings_panel_built();
        self.sidebar_stack.set_visible_child_name("settings");
        self.split_view.set_show_sidebar(true);
        self.reveal_chrome();
    }

    fn toggle_settings_panel(&self) {
        if self.split_view.shows_sidebar()
            && self.sidebar_stack.visible_child_name().as_deref() == Some("settings")
        {
            self.split_view.set_show_sidebar(false);
            self.focus_active_editor();
        } else {
            self.open_settings_drawer();
        }
    }

    pub fn apply_preferences_visuals(&self) {
        let prefs = self.prefs.borrow().clone();
        let pal = resolve_palette(&prefs);

        let style_mgr = adw::StyleManager::default();
        let desired_scheme = if pal.is_dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        };
        if style_mgr.color_scheme() != desired_scheme {
            style_mgr.set_color_scheme(desired_scheme);
        }

        let css = generate_app_css(&prefs);
        if *self.last_css.borrow() != css {
            self.css_provider.load_from_string(&css);
            *self.last_css.borrow_mut() = css;
        }

        let vis_key = EditorVisualKey::from_prefs(&prefs);
        if self.last_editor_visual.borrow().as_ref() != Some(&vis_key) {
            update_tag_styles_table(&self.shared_tag_table, &prefs);
            for tab in self.tabs.borrow().values() {
                tab.apply_preferences_with_tags(false);
            }
            *self.last_editor_visual.borrow_mut() = Some(vis_key);
        }

        self.toc.toggle_button.set_visible(prefs.show_toc_button);
        if prefs.hide_chrome_unless_hovered {
            self.sync_chrome_hover_visibility();
        } else if !prefs.hide_chrome_while_typing {
            self.reveal_chrome();
        }
        if !prefs.floating_toolbar {
            self.hide_floating_toolbar();
        }
        if prefs.ghost_suggestion_mode == GhostSuggestionMode::Off {
            if let Some(src) = self.ext_comp_timer.borrow_mut().take() {
                src.remove();
            }
            self.ext_manager.borrow_mut().cancel_completion();
            for tab in self.tabs.borrow().values() {
                tab.clear_ghost_suggestion();
            }
        }
        self.sync_extensions(&prefs.enabled_extensions);
        self.refresh_stats_now();
    }

    pub fn new_empty_tab(&self) -> adw::TabPage {
        let default_dir = self.prefs.borrow().default_direction;
        let doc = Document::new_empty(default_dir);
        let page = self.add_tab(generate_tab_id(), doc, None, false, None, 0, true);
        self.save_session_order();
        page
    }

    #[allow(clippy::too_many_arguments)]
    fn add_tab(
        &self,
        id: String,
        doc: Document,
        file_path: Option<PathBuf>,
        is_dirty: bool,
        last_mtime: Option<u64>,
        cursor_flat: usize,
        select_it: bool,
    ) -> adw::TabPage {
        let sw_state = self.clone();
        let sw_sel = self.clone();
        let sw_hide = self.clone();
        let sw_type = self.clone();
        let sw_ext = self.clone();
        let sw_link = self.clone();
        let sw_ext_trig = self.clone();
        let sw_toast = self.clone();
        let win_ref = self.window.clone();

        let callbacks = EditorTabCallbacks {
            on_state_changed: Rc::new(move || {
                sw_state.sync_active_tab_title();
                sw_state.schedule_stats_refresh();
                sw_state.refresh_toc();
            }),
            on_selection_changed: Rc::new(move |immediate| {
                if immediate {
                    sw_sel.update_floating_toolbar();
                } else {
                    sw_sel.schedule_floating_toolbar();
                }
                sw_sel.schedule_stats_refresh();
                sw_sel.refresh_toc();
            }),
            on_hide_toolbar: Rc::new(move || {
                sw_hide.hide_floating_toolbar();
                sw_hide.quick_fix_card.hide();
                sw_hide.schedule_stats_refresh();
                sw_hide.refresh_toc();
            }),
            on_typing_activity: Rc::new(move || {
                sw_type.hide_floating_toolbar();
                sw_type.quick_fix_card.hide();
                sw_type.schedule_chrome_autohide();
            }),
            on_external_file_changed: Rc::new(move |tab_id| {
                sw_ext.handle_external_file_change(&tab_id);
            }),
            on_open_link_input: Rc::new(move || {
                sw_link.open_toolbar_link_editor();
            }),
            on_extension_trigger: Rc::new(move |trig| {
                sw_ext_trig.handle_extension_trigger(trig);
            }),
            on_toast: Rc::new(move |msg| {
                sw_toast.word_counter.show_toast(msg);
            }),
            get_window: Rc::new(move || Some(win_ref.clone().upcast())),
        };

        let tab = EditorTab::new_with_tag_table(
            id.clone(),
            doc,
            file_path.clone(),
            is_dirty,
            last_mtime,
            cursor_flat,
            self.prefs.clone(),
            self.storage.clone(),
            callbacks,
            Some(&self.shared_tag_table),
        );

        let page = self.tab_view.append(&tab.root);
        self.update_page_header(&page, &tab);

        self.tabs.borrow_mut().insert(id.clone(), tab.clone());
        self.page_to_id
            .borrow_mut()
            .insert(page.clone().upcast(), id);

        if select_it {
            self.tab_view.set_selected_page(&page);
            tab.focus_editor();
        }

        if let (Some(p), Some(saved_mtime)) = (file_path.as_ref(), last_mtime)
            && let Some(disk_mtime) = file_mtime_ms(p)
            && disk_mtime > saved_mtime
        {
            let sw_c = self.clone();
            let tid = tab.id.clone();
            glib::idle_add_local_once(move || {
                sw_c.handle_external_file_change(&tid);
            });
        }

        page
    }

    fn update_page_header(&self, page: &adw::TabPage, tab: &EditorTab) {
        let base_title = tab.display_title();
        page.set_title(&base_title);
        page.set_needs_attention(tab.is_dirty());
        if let Some(p) = tab.file_path.borrow().as_ref() {
            page.set_tooltip(&p.to_string_lossy());
        } else {
            page.set_tooltip(&base_title);
        }
    }

    fn sync_active_tab_title(&self) {
        if let Some(page) = self.tab_view.selected_page()
            && let Some(id) = self
                .page_to_id
                .borrow()
                .get(&page.clone().upcast())
                .cloned()
            && let Some(tab) = self.tabs.borrow().get(&id)
        {
            self.update_page_header(&page, tab);
            self.window
                .set_title(Some(&format!("{} — Sillpaper", tab.display_title())));
        }
    }

    fn save_session_order(&self) {
        if !self.is_primary {
            return;
        }
        let mut tab_ids = Vec::new();
        let n = self.tab_view.n_pages();
        let map = self.page_to_id.borrow();
        for i in 0..n {
            let p = self.tab_view.nth_page(i);
            if let Some(id) = map.get(&p.upcast()) {
                tab_ids.push(id.clone());
            }
        }
        let active_tab_id = self
            .tab_view
            .selected_page()
            .and_then(|p| map.get(&p.upcast()).cloned());
        let _ = self.storage.save_session_order(&SessionOrder {
            active_tab_id,
            tab_ids,
        });
    }

    pub fn open_file_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("Open Document")
            .modal(true)
            .build();

        let filter_supported = gtk4::FileFilter::new();
        filter_supported.set_name(Some("Supported Documents (.html, .md, .txt, .docx)"));
        for pat in [
            "*.html",
            "*.htm",
            "*.md",
            "*.markdown",
            "*.txt",
            "*.text",
            "*.mdown",
            "*.mkd",
            "*.docx",
        ] {
            filter_supported.add_pattern(pat);
        }
        let filter_all = gtk4::FileFilter::new();
        filter_all.set_name(Some("All Files"));
        filter_all.add_pattern("*");

        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter_supported);
        filters.append(&filter_all);
        dialog.set_filters(Some(&filters));

        if let Some(folder) = self.prefs.borrow().default_save_folder.as_ref() {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let sw_c = self.clone();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res
                && let Some(path) = file.path()
            {
                sw_c.open_path(&path);
            }
        });
    }

    pub fn open_path(&self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let existing_page = {
            let map = self.page_to_id.borrow();
            let tabs = self.tabs.borrow();
            let mut found = None;
            for i in 0..self.tab_view.n_pages() {
                let page = self.tab_view.nth_page(i);
                if let Some(id) = map.get(&page.clone().upcast())
                    && let Some(tab) = tabs.get(id)
                    && let Some(tp) = tab.file_path.borrow().as_ref()
                    && tp.canonicalize().unwrap_or_else(|_| tp.clone()) == canonical
                {
                    found = Some((page, tab.clone()));
                    break;
                }
            }
            found
        };

        if let Some((page, _tab)) = existing_page {
            self.tab_view.set_selected_page(&page);
            self.focus_active_editor();
            return;
        }

        match load_document(path) {
            Ok((doc, _fmt)) => {
                let mtime = file_mtime_ms(path);
                if let Some(active) = self.active_tab()
                    && active.is_pristine_empty()
                {
                    let mut st = active.state.borrow_mut();
                    st.doc = doc;
                    st.selection = crate::model::Selection::default();
                    st.undo.clear();
                    drop(st);
                    *active.file_path.borrow_mut() = Some(path.to_path_buf());
                    *active.last_file_mtime_ms.borrow_mut() = mtime;
                    active.full_render_buffer();
                    active.watch_file(path.to_path_buf());
                    active.flush_backup_now();
                    self.sync_active_tab_title();
                    self.refresh_stats_now();
                    self.refresh_toc();
                    active.focus_editor();
                    return;
                }

                let page = self.add_tab(
                    generate_tab_id(),
                    doc,
                    Some(path.to_path_buf()),
                    false,
                    mtime,
                    0,
                    true,
                );
                if let Some(tab) = self.active_tab() {
                    tab.flush_backup_now();
                    self.update_page_header(&page, &tab);
                }
                self.save_session_order();
                self.sync_active_tab_title();
                self.refresh_stats_now();
                self.refresh_toc();
            }
            Err(err) => {
                show_info_dialog(&self.window, "Could not open file", &err);
            }
        }
    }

    pub fn save_active(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let path_opt = tab.file_path.borrow().clone();
        if let Some(path) = path_opt
            && FileFormat::from_path(&path).supports_direct_save()
        {
            self.perform_save(&tab, &path);
        } else {
            self.save_active_as(None);
        }
    }

    pub fn save_active_as(&self, after_save: Option<Box<dyn FnOnce()>>) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let prefs = self.prefs.borrow().clone();
        let suggested = suggested_filename(&tab, &prefs);

        let dialog = gtk4::FileDialog::builder()
            .title("Save Document")
            .modal(true)
            .initial_name(&suggested)
            .build();

        let f_html = gtk4::FileFilter::new();
        f_html.set_name(Some("Rich HTML (*.html, *.htm)"));
        f_html.add_pattern("*.html");
        f_html.add_pattern("*.htm");
        let f_md = gtk4::FileFilter::new();
        f_md.set_name(Some("Markdown (*.md, *.markdown)"));
        f_md.add_pattern("*.md");
        f_md.add_pattern("*.markdown");
        let f_txt = gtk4::FileFilter::new();
        f_txt.set_name(Some("Plain Text (*.txt)"));
        f_txt.add_pattern("*.txt");
        f_txt.add_pattern("*.text");

        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        let default_filter = match prefs.default_save_format {
            crate::config::DefaultSaveFormat::Html => {
                filters.append(&f_html);
                filters.append(&f_md);
                filters.append(&f_txt);
                &f_html
            }
            crate::config::DefaultSaveFormat::Markdown => {
                filters.append(&f_md);
                filters.append(&f_txt);
                filters.append(&f_html);
                &f_md
            }
            crate::config::DefaultSaveFormat::PlainText => {
                filters.append(&f_txt);
                filters.append(&f_md);
                filters.append(&f_html);
                &f_txt
            }
        };
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(default_filter));

        if let Some(existing) = tab.file_path.borrow().as_ref().and_then(|p| p.parent()) {
            dialog.set_initial_folder(Some(&gio::File::for_path(existing)));
        } else if let Some(folder) = prefs.default_save_folder.as_ref() {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let sw_c = self.clone();
        let default_ext = prefs.default_save_format.extension().to_string();
        let after_cell = RefCell::new(after_save);
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res
                && let Some(raw_path) = file.path()
            {
                let path = ensure_extension(raw_path, &default_ext, false);
                if sw_c.perform_save(&tab, &path)
                    && let Some(cb) = after_cell.borrow_mut().take()
                {
                    cb();
                }
            }
        });
    }

    fn perform_save(&self, tab: &EditorTab, path: &Path) -> bool {
        let doc = tab.state.borrow().doc.clone();
        let prefs = self.prefs.borrow().clone();
        match save_document_with_prefs(&doc, path, &prefs) {
            Ok(outcome) => {
                *tab.file_path.borrow_mut() = Some(path.to_path_buf());
                *tab.last_file_mtime_ms.borrow_mut() = file_mtime_ms(path);
                tab.state.borrow_mut().mark_saved();
                tab.watch_file(path.to_path_buf());
                tab.flush_backup_now();
                self.sync_active_tab_title();
                self.word_counter.show_toast("Saved");
                if let Some(warn) = outcome.warning {
                    show_info_dialog(&self.window, "Format Conversion Notice", &warn);
                }
                true
            }
            Err(err) => {
                show_info_dialog(&self.window, "Could not save file", &err);
                false
            }
        }
    }

    pub fn export_docx_dialog(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let base = sanitize_stem(&tab.display_title());
        let dialog = gtk4::FileDialog::builder()
            .title("Export as Word Document (.docx)")
            .modal(true)
            .initial_name(format!("{base}.docx"))
            .build();
        let f_docx = gtk4::FileFilter::new();
        f_docx.set_name(Some("Word Document (*.docx)"));
        f_docx.add_pattern("*.docx");
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&f_docx);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&f_docx));

        if let Some(existing) = tab.file_path.borrow().as_ref().and_then(|p| p.parent()) {
            dialog.set_initial_folder(Some(&gio::File::for_path(existing)));
        } else if let Some(folder) = self.prefs.borrow().default_save_folder.as_ref() {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let sw_c = self.clone();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res
                && let Some(raw_path) = file.path()
            {
                let path = ensure_extension(raw_path, "docx", true);
                let doc = tab.state.borrow().doc.clone();
                let prefs = sw_c.prefs.borrow().clone();
                let doc_dir = tab
                    .file_path
                    .borrow()
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf));
                match document_to_docx_bytes_with_options(&doc, doc_dir.as_deref(), Some(&prefs))
                    .and_then(|b| atomic_write(&path, &b))
                {
                    Ok(()) => sw_c.word_counter.show_toast("Exported .docx"),
                    Err(e) => show_info_dialog(&sw_c.window, "DOCX Export Failed", &e),
                }
            }
        });
    }

    pub fn export_pdf_dialog(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let base = sanitize_stem(&tab.display_title());
        let dialog = gtk4::FileDialog::builder()
            .title("Export as PDF (.pdf)")
            .modal(true)
            .initial_name(format!("{base}.pdf"))
            .build();
        let f_pdf = gtk4::FileFilter::new();
        f_pdf.set_name(Some("PDF Document (*.pdf)"));
        f_pdf.add_pattern("*.pdf");
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&f_pdf);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&f_pdf));

        if let Some(existing) = tab.file_path.borrow().as_ref().and_then(|p| p.parent()) {
            dialog.set_initial_folder(Some(&gio::File::for_path(existing)));
        } else if let Some(folder) = self.prefs.borrow().default_save_folder.as_ref() {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let sw_c = self.clone();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res
                && let Some(raw_path) = file.path()
            {
                let path = ensure_extension(raw_path, "pdf", true);
                let doc = tab.state.borrow().doc.clone();
                let prefs = sw_c.prefs.borrow().clone();
                let doc_dir = tab
                    .file_path
                    .borrow()
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf));
                match export_document_to_pdf_with_base(&doc, &prefs, &path, doc_dir.as_deref()) {
                    Ok(()) => sw_c.word_counter.show_toast("Exported PDF"),
                    Err(e) => show_info_dialog(&sw_c.window, "PDF Export Failed", &e),
                }
            }
        });
    }

    pub fn print_active(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let doc = tab.state.borrow().doc.clone();
        let prefs = self.prefs.borrow().clone();
        let doc_dir = tab
            .file_path
            .borrow()
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        print_document_native(&self.window, &doc, &prefs, doc_dir.as_deref());
    }

    fn open_writing_goal_dialog(&self) {
        if let Some(tab) = self.active_tab() {
            let current = *tab.goal.borrow();
            let sw_inner = self.clone();
            show_writing_goal_dialog(&self.window, current, move |new_goal| {
                *tab.goal.borrow_mut() = new_goal;
                tab.goal_announced.set(false);
                sw_inner.refresh_stats_now();
            });
        }
    }

    fn handle_external_file_change(&self, tab_id: &str) {
        let Some(tab) = self.tabs.borrow().get(tab_id).cloned() else {
            return;
        };
        if !tab.is_dirty() {
            if tab.reload_from_disk().is_ok() {
                self.word_counter
                    .show_toast("Reloaded updated file from disk");
                self.sync_active_tab_title();
                self.refresh_stats_now();
                self.refresh_toc();
            }
        } else {
            let name = tab.display_title();
            let sw_c = self.clone();
            show_external_change_dialog(&self.window, &name, move |reload| {
                if reload {
                    let _ = tab.reload_from_disk();
                    sw_c.sync_active_tab_title();
                    sw_c.refresh_stats_now();
                    sw_c.refresh_toc();
                } else if let Some(p) = tab.file_path.borrow().as_ref() {
                    *tab.last_file_mtime_ms.borrow_mut() = file_mtime_ms(p);
                }
            });
        }
    }

    fn wire_floating_toolbar(&self) {
        let sw_c = self.clone();
        self.floating_toolbar.set_on_action(move |act| {
            let Some(active) = sw_c.active_tab() else {
                return;
            };
            match act {
                ToolbarAction::Paragraph => active.toggle_block(BlockKind::Paragraph),
                ToolbarAction::Heading(lvl) => active.toggle_block(BlockKind::Heading(lvl)),
                ToolbarAction::Bold => active.toggle_inline(InlineAttr::Bold),
                ToolbarAction::Italic => active.toggle_inline(InlineAttr::Italic),
                ToolbarAction::Underline => active.toggle_inline(InlineAttr::Underline),
                ToolbarAction::Strikethrough => active.toggle_inline(InlineAttr::Strikethrough),
                ToolbarAction::OpenLink => {
                    sw_c.open_toolbar_link_editor();
                    return;
                }
                ToolbarAction::SetLink(url) => {
                    active.commit_pending_link(url);
                    sw_c.hide_floating_toolbar();
                    sw_c.focus_active_editor();
                    return;
                }
                ToolbarAction::Blockquote => active.toggle_block(BlockKind::Blockquote),
                ToolbarAction::BulletList => {
                    active.toggle_block(BlockKind::BulletListItem { depth: 0 })
                }
                ToolbarAction::OrderedList => active.toggle_block(BlockKind::OrderedListItem {
                    depth: 0,
                    number: 1,
                }),
                ToolbarAction::OpenExtensionPrompt {
                    ext_id,
                    action_id,
                    label,
                } => {
                    sw_c.open_toolbar_extension_prompt(ext_id, action_id, label);
                    return;
                }
                ToolbarAction::RunExtensionAction {
                    ext_id,
                    action_id,
                    label,
                    prompt,
                } => {
                    sw_c.run_extension_selection_action(
                        &ext_id,
                        &action_id,
                        &label,
                        prompt.as_deref(),
                    );
                    return;
                }
            }
            sw_c.update_floating_toolbar();
        });
        let sw_f = self.clone();
        self.floating_toolbar.set_on_return_focus(move || {
            if let Some(active) = sw_f.active_tab()
                && active.has_pending_link()
            {
                active.cancel_pending_link(true);
            }
            sw_f.focus_active_editor();
            sw_f.update_floating_toolbar();
        });
    }

    fn schedule_floating_toolbar(&self) {
        if let Some(src) = self.toolbar_timer.borrow_mut().take() {
            src.remove();
        }
        if !self.prefs.borrow().floating_toolbar {
            self.hide_floating_toolbar();
            return;
        }
        let sw_c = self.clone();
        let src = glib::timeout_add_local_once(std::time::Duration::from_millis(180), move || {
            *sw_c.toolbar_timer.borrow_mut() = None;
            sw_c.update_floating_toolbar();
        });
        *self.toolbar_timer.borrow_mut() = Some(src);
    }

    fn update_floating_toolbar(&self) {
        if let Some(src) = self.toolbar_timer.borrow_mut().take() {
            src.remove();
        }
        if !self.prefs.borrow().floating_toolbar {
            self.hide_floating_toolbar();
            return;
        }
        let Some(tab) = self.active_tab() else {
            return;
        };
        if self.floating_toolbar.is_input_mode() {
            return;
        }
        if let Some((l, r, t, b)) = tab.selection_bounds_in(&self.canvas_overlay) {
            let state = tab.toolbar_state();
            if !state.can_inline && !state.can_heading && !state.can_blockquote && !state.can_list {
                self.hide_floating_toolbar();
                return;
            }
            self.floating_toolbar.update_state(&state);
            let ow = self.canvas_overlay.width().max(400) as f32;
            let oh = self.canvas_overlay.height().max(260) as f32;
            self.floating_toolbar.show_clamped(l, r, t, b, ow, oh);
        } else {
            self.hide_floating_toolbar();
        }
    }

    fn open_toolbar_link_editor(&self) {
        if let Some(src) = self.toolbar_timer.borrow_mut().take() {
            src.remove();
        }
        let Some(tab) = self.active_tab() else {
            return;
        };
        let Some((l, r, t, b)) = tab.selection_bounds_in(&self.canvas_overlay) else {
            return;
        };
        let state = tab.toolbar_state();
        if !state.can_inline {
            return;
        }
        if tab.begin_pending_link().is_none() {
            return;
        }
        self.floating_toolbar.update_state(&state);
        self.floating_toolbar.open_link_input(state.link.as_deref());
        let ow = self.canvas_overlay.width().max(400) as f32;
        let oh = self.canvas_overlay.height().max(260) as f32;
        self.floating_toolbar.show_clamped(l, r, t, b, ow, oh);
    }

    fn hide_floating_toolbar(&self) {
        if let Some(src) = self.toolbar_timer.borrow_mut().take() {
            src.remove();
        }
        if let Some(tab) = self.active_tab()
            && tab.has_pending_link()
        {
            tab.cancel_pending_link(false);
        }
        self.floating_toolbar.hide();
    }

    fn open_toolbar_extension_prompt(&self, ext_id: String, action_id: String, label: String) {
        if let Some(src) = self.toolbar_timer.borrow_mut().take() {
            src.remove();
        }
        let Some(tab) = self.active_tab() else {
            return;
        };
        let Some((l, r, t, b)) = tab.selection_bounds_in(&self.canvas_overlay) else {
            return;
        };
        if tab.begin_pending_link().is_none() {
            return;
        }
        self.floating_toolbar
            .open_extension_prompt(ext_id, action_id, label);
        let ow = self.canvas_overlay.width().max(400) as f32;
        let oh = self.canvas_overlay.height().max(260) as f32;
        self.floating_toolbar.show_clamped(l, r, t, b, ow, oh);
    }

    fn wire_extension_wake_handler(&self) {
        let wake_id = self.ext_wake_id;
        let wake_queued = Arc::new(AtomicBool::new(false));
        let sw = self.clone();
        let queued_drain = wake_queued.clone();
        let handler: Rc<dyn Fn()> = Rc::new(move || {
            queued_drain.store(false, Ordering::Release);
            sw.drain_extension_events();
        });
        EXT_WAKE_HANDLERS.with(|map| {
            map.borrow_mut().insert(wake_id, handler);
        });

        self.ext_manager
            .borrow_mut()
            .set_wake_callback(Arc::new(move || {
                if !wake_queued.swap(true, Ordering::AcqRel) {
                    glib::MainContext::default().invoke(move || {
                        EXT_WAKE_HANDLERS.with(|map| {
                            if let Some(cb) = map.borrow().get(&wake_id).cloned() {
                                cb();
                            }
                        });
                    });
                }
            }));

        self.window.connect_destroy(move |_| {
            EXT_WAKE_HANDLERS.with(|map| {
                map.borrow_mut().remove(&wake_id);
            });
        });
    }

    fn drain_extension_events(&self) {
        let events = self.ext_manager.borrow_mut().poll_events();
        for ev in events {
            self.dispatch_extension_event(ev);
        }
    }

    fn wire_extension_overlays(&self) {
        {
            let sw = self.clone();
            self.quick_fix_card.set_on_apply(move |block, diag| {
                if let Some(tab) = sw.active_tab() {
                    tab.apply_diagnostic_fix(block, &diag);
                    tab.focus_editor();
                }
            });
        }
        {
            let sw = self.clone();
            self.quick_fix_card.set_on_ignore(move |block, start, end| {
                if let Some(tab) = sw.active_tab() {
                    tab.dismiss_diagnostic(block, start, end);
                    tab.focus_editor();
                }
            });
        }
        {
            let sw = self.clone();
            self.action_preview_card
                .set_on_commit(move |sel, text, insert_below| {
                    if let Some(tab) = sw.active_tab() {
                        tab.replace_range_with_markdown(sel, &text, insert_below);
                        tab.focus_editor();
                    }
                });
        }
        {
            let sw = self.clone();
            self.action_preview_card.set_on_discard(move || {
                sw.ext_manager.borrow_mut().cancel_action();
                sw.focus_active_editor();
            });
        }
        {
            let sw = self.clone();
            self.ext_hint_banner.set_on_dismiss(move |ext_id| {
                let mut p = sw.prefs.borrow().clone();
                if !p.dismissed_extension_hints.contains(&ext_id) {
                    p.dismissed_extension_hints.push(ext_id);
                    sw.update_preferences(p, true);
                }
                sw.focus_active_editor();
            });
        }
    }

    fn sync_extension_hint_banner(&self) {
        let running = self.ext_manager.borrow().running_manifests();
        let prefs = self.prefs.borrow();
        let next_hint = running
            .iter()
            .find(|m| !prefs.dismissed_extension_hints.contains(&m.id));
        if let Some(manifest) = next_hint {
            self.ext_hint_banner.show_for(
                manifest,
                prefs.floating_toolbar,
                prefs.ghost_suggestion_mode,
            );
        } else if self.ext_hint_banner.is_visible() {
            self.ext_hint_banner.hide();
        }
    }

    fn sync_extensions(&self, enabled_ids: &[String]) {
        let running_ids: Vec<String> = {
            let mut mgr = self.ext_manager.borrow_mut();
            mgr.sync_enabled(enabled_ids);
            mgr.running_manifests().into_iter().map(|m| m.id).collect()
        };

        if *self.ext_last_running_ids.borrow() != running_ids {
            let actions = self.ext_manager.borrow().available_actions();
            self.floating_toolbar.set_extension_actions(&actions);
            *self.ext_last_running_ids.borrow_mut() = running_ids.clone();
        }

        if running_ids.is_empty() {
            if let Some(src) = self.ext_comp_timer.borrow_mut().take() {
                src.remove();
            }
            if let Some(src) = self.ext_diag_timer.borrow_mut().take() {
                src.remove();
            }
            self.quick_fix_card.hide();
            self.action_preview_card.hide();
            self.ext_hint_banner.hide();
            for tab in self.tabs.borrow().values() {
                tab.clear_ghost_suggestion();
                tab.clear_all_diagnostics();
            }
            return;
        }

        self.sync_extension_hint_banner();
    }

    fn handle_extension_trigger(&self, trigger: ExtensionTrigger) {
        if !self.ext_manager.borrow().has_any_running() {
            return;
        }
        match trigger {
            ExtensionTrigger::CancelSuggestions => {
                if let Some(src) = self.ext_comp_timer.borrow_mut().take() {
                    src.remove();
                }
                self.ext_manager.borrow_mut().cancel_completion();
                self.quick_fix_card.hide();
            }
            ExtensionTrigger::RequestCompletion { manual } => {
                if let Some(src) = self.ext_comp_timer.borrow_mut().take() {
                    src.remove();
                }
                if !self.ext_manager.borrow().has_completions() {
                    return;
                }
                let mode = self.prefs.borrow().ghost_suggestion_mode;
                if mode == GhostSuggestionMode::Off {
                    return;
                }
                if !manual && mode != GhostSuggestionMode::AutoWhileTyping {
                    return;
                }
                if manual {
                    if let Some(src) = self.ext_diag_timer.borrow_mut().take() {
                        src.remove();
                    }
                    self.fire_completion_request(true);
                    return;
                }
                let sw = self.clone();
                let src = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(900),
                    move || {
                        *sw.ext_comp_timer.borrow_mut() = None;
                        sw.fire_completion_request(false);
                    },
                );
                *self.ext_comp_timer.borrow_mut() = Some(src);
            }
            ExtensionTrigger::RequestDiagnostics { block, manual } => {
                if let Some(src) = self.ext_diag_timer.borrow_mut().take() {
                    src.remove();
                }
                if !self.ext_manager.borrow().has_diagnostics() {
                    return;
                }
                if !manual && !self.prefs.borrow().auto_inline_diagnostics {
                    return;
                }
                let delay = if manual { 10 } else { 3500 };
                let sw = self.clone();
                let src = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(delay),
                    move || {
                        *sw.ext_diag_timer.borrow_mut() = None;
                        sw.fire_diagnostics_request(block, manual);
                    },
                );
                *self.ext_diag_timer.borrow_mut() = Some(src);
            }
            ExtensionTrigger::ShowQuickFix { block, diagnostic } => {
                let Some(tab) = self.active_tab() else {
                    return;
                };
                if let Some(bounds) = tab.char_range_bounds_in(
                    block,
                    diagnostic.start,
                    diagnostic.end,
                    &self.canvas_overlay,
                ) {
                    let ow = self.canvas_overlay.width().max(400) as f32;
                    let oh = self.canvas_overlay.height().max(260) as f32;
                    self.quick_fix_card
                        .show_at(block, diagnostic, bounds, (ow, oh));
                }
            }
        }
    }

    fn fire_completion_request(&self, manual: bool) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let Some((block, offset, prefix, suffix, paragraph)) = tab.completion_context() else {
            if manual {
                self.word_counter
                    .show_toast("Clear selection first, then press Ctrl+Space");
            }
            return;
        };
        if manual && prefix.trim().is_empty() {
            self.word_counter
                .show_toast("Type a few words first, then press Ctrl+Space");
            return;
        }
        if !manual && (paragraph.trim().chars().count() < 6 || !suffix.is_empty()) {
            return;
        }
        if manual {
            tab.set_ghost_loading(block, offset);
        }
        *self.ext_active_tab_id.borrow_mut() = Some(tab.id.clone());
        let mut sent = self
            .ext_manager
            .borrow_mut()
            .request_completion(block, offset, &prefix, &suffix, &paragraph, manual);
        if sent.is_none() && manual {
            let enabled = self.prefs.borrow().enabled_extensions.clone();
            self.ext_manager.borrow_mut().sync_enabled(&enabled);
            sent = self
                .ext_manager
                .borrow_mut()
                .request_completion(block, offset, &prefix, &suffix, &paragraph, manual);
        }
        if sent.is_none() && manual {
            tab.clear_ghost_suggestion();
            self.word_counter
                .show_toast("Extension is not running — check Settings → Extensions");
        }
    }

    fn fire_diagnostics_request(&self, block: usize, manual: bool) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let text = {
            let st = tab.state.borrow();
            let Some(blk) = st.doc.blocks.get(block) else {
                return;
            };
            if matches!(
                blk.kind,
                BlockKind::CodeBlock { .. } | BlockKind::HorizontalRule
            ) {
                return;
            }
            blk.plain_text()
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if !manual {
            if trimmed.chars().count() < 16 {
                return;
            }
            if let Some(prev_at) = *self.ext_last_auto_diag_at.borrow()
                && prev_at.elapsed() < std::time::Duration::from_secs(8)
            {
                return;
            }
            if let Some(prev_txt) = self
                .ext_last_diag_text
                .borrow()
                .get(&(tab.id.clone(), block))
            {
                if prev_txt == trimmed {
                    return;
                }
                let delta = trimmed.chars().count().abs_diff(prev_txt.chars().count());
                let ends_sentence = trimmed
                    .chars()
                    .last()
                    .is_some_and(|c| matches!(c, '.' | '!' | '?' | '؟' | '。'));
                if delta < 4 && !ends_sentence {
                    return;
                }
            }
            *self.ext_last_auto_diag_at.borrow_mut() = Some(std::time::Instant::now());
        }
        self.ext_last_diag_text
            .borrow_mut()
            .insert((tab.id.clone(), block), trimmed.to_string());
        *self.ext_active_tab_id.borrow_mut() = Some(tab.id.clone());
        self.ext_manager
            .borrow_mut()
            .request_diagnostics(block, &text, manual);
    }

    fn run_extension_selection_action(
        &self,
        ext_id: &str,
        action_id: &str,
        label: &str,
        custom_prompt: Option<&str>,
    ) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        if tab.has_pending_link() {
            tab.cancel_pending_link(true);
        }
        let Some((sel, md)) = tab.selected_markdown() else {
            return;
        };
        self.hide_floating_toolbar();
        *self.ext_active_action_label.borrow_mut() = label.to_string();
        *self.ext_active_tab_id.borrow_mut() = Some(tab.id.clone());
        self.action_preview_card.show_loading(label, sel);
        if self
            .ext_manager
            .borrow_mut()
            .run_action(ext_id, action_id, &md, custom_prompt)
            .is_none()
        {
            self.action_preview_card
                .show_error("Extension is not running. Check Settings → Extensions.");
        }
    }

    fn dispatch_extension_event(&self, event: ExtensionEvent) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        if self
            .ext_active_tab_id
            .borrow()
            .as_deref()
            .is_some_and(|tid| tid != tab.id)
        {
            return;
        }
        match event {
            ExtensionEvent::Completion {
                block,
                offset,
                suggestion,
                manual,
                ..
            } => {
                if suggestion.is_empty() {
                    tab.clear_ghost_suggestion();
                    if manual {
                        self.word_counter
                            .show_toast("No suggestion returned — press Ctrl+Space to retry");
                    }
                } else {
                    tab.set_ghost_suggestion(block, offset, suggestion);
                }
            }
            ExtensionEvent::Diagnostics { block, items, .. } => {
                tab.set_block_diagnostics(block, items);
            }
            ExtensionEvent::ActionResult { replacement, .. } => {
                let label = self.ext_active_action_label.borrow().clone();
                self.action_preview_card.show_result(&label, replacement);
            }
            ExtensionEvent::Error {
                kind,
                manual,
                message,
                ..
            } => {
                if kind == ExtensionRequestKind::Completion {
                    tab.clear_ghost_suggestion();
                }
                if kind == ExtensionRequestKind::Action && self.action_preview_card.is_visible() {
                    self.action_preview_card.show_error(&message);
                } else if manual {
                    self.word_counter.show_toast(&message);
                } else {
                    let allow = self
                        .ext_last_bg_error_toast_at
                        .borrow()
                        .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(20));
                    if allow {
                        *self.ext_last_bg_error_toast_at.borrow_mut() =
                            Some(std::time::Instant::now());
                        self.word_counter.show_toast(&message);
                    }
                }
            }
        }
    }

    fn schedule_stats_refresh(&self) {
        if let Some(src) = self.stats_timer.borrow_mut().take() {
            src.remove();
        }
        let sw_c = self.clone();
        let src = glib::timeout_add_local_once(std::time::Duration::from_millis(160), move || {
            *sw_c.stats_timer.borrow_mut() = None;
            sw_c.refresh_stats_now();
        });
        *self.stats_timer.borrow_mut() = Some(src);
    }

    fn refresh_stats_now(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let prefs = self.prefs.borrow();
        let pal = resolve_palette(&prefs);
        let st = tab.state.borrow();
        let sel = if st.selection.is_collapsed() {
            None
        } else {
            Some(st.selection)
        };
        let stats = compute_stats(&st.doc, sel, prefs.words_per_minute);
        drop(st);

        let goal = *tab.goal.borrow();
        let reached = self.word_counter.update(stats, &prefs, goal);
        if reached && !tab.goal_announced.get() {
            tab.goal_announced.set(true);
            self.word_counter.show_toast("Target achieved!");
        } else if !reached {
            tab.goal_announced.set(false);
        }

        if self.split_view.shows_sidebar() {
            let chars = if prefs.count_spaces_in_chars {
                stats.chars_with_spaces
            } else {
                stats.chars_no_spaces
            };
            let num_c = pal.text;
            let rt = format_reading_time(stats.reading_seconds);
            let words_fmt = format_with_commas(stats.words);
            let chars_fmt = format_with_commas(chars);
            self.drawer_stats_label.set_markup(&format!(
                "<span foreground=\"{num_c}\" weight=\"semibold\">{words_fmt}</span> words\n\
                 <span foreground=\"{num_c}\" weight=\"semibold\">{chars_fmt}</span> characters\n\
                 <span foreground=\"{num_c}\" weight=\"semibold\">{rt}</span> reading time"
            ));
        }
    }

    fn refresh_toc(&self) {
        if self.toc.is_open() {
            self.rebuild_toc_now();
        } else {
            self.toc_dirty.set(true);
        }
    }

    fn rebuild_toc_now(&self) {
        self.toc_dirty.set(false);
        let Some(tab) = self.active_tab() else {
            return;
        };
        let st = tab.state.borrow();
        let entries = extract_toc(&st.doc);
        let active_block = st.selection.focus.block;
        drop(st);
        self.toc.set_entries(&entries, active_block);
    }

    fn chrome_blocked_from_hiding(&self) -> bool {
        self.split_view.shows_sidebar() || self.search_bar.is_open() || self.toc.is_open()
    }

    fn apply_chrome_hidden(&self) {
        self.chrome_hidden.set(true);
        self.topbar.add_css_class("chrome-hidden");
        self.toc.toggle_button.add_css_class("chrome-hidden");
    }

    fn schedule_chrome_autohide(&self) {
        let p = self.prefs.borrow();
        let should_hide = p.hide_chrome_while_typing || p.hide_chrome_unless_hovered;
        drop(p);
        if !should_hide || self.chrome_blocked_from_hiding() {
            return;
        }
        if self.chrome_hidden.get() || self.chrome_hide_timer.borrow().is_some() {
            return;
        }
        self.schedule_chrome_hide_delay(750, false);
    }

    fn schedule_chrome_hide_delay(&self, delay_ms: u64, require_outside_hover: bool) {
        if self.chrome_blocked_from_hiding() {
            return;
        }
        if require_outside_hover && self.pointer_in_chrome_zone.get() {
            return;
        }
        if let Some(src) = self.chrome_hide_timer.borrow_mut().take() {
            src.remove();
        }
        let sw_c = self.clone();
        let src =
            glib::timeout_add_local_once(std::time::Duration::from_millis(delay_ms), move || {
                *sw_c.chrome_hide_timer.borrow_mut() = None;
                if sw_c.chrome_blocked_from_hiding() {
                    return;
                }
                if require_outside_hover && sw_c.pointer_in_chrome_zone.get() {
                    return;
                }
                sw_c.apply_chrome_hidden();
            });
        *self.chrome_hide_timer.borrow_mut() = Some(src);
    }

    fn sync_chrome_hover_visibility(&self) {
        if !self.prefs.borrow().hide_chrome_unless_hovered {
            return;
        }
        if self.chrome_blocked_from_hiding() || self.pointer_in_chrome_zone.get() {
            self.reveal_chrome();
        } else if !self.chrome_hidden.get() {
            self.schedule_chrome_hide_delay(300, true);
        }
    }

    fn reveal_chrome(&self) {
        if let Some(src) = self.chrome_hide_timer.borrow_mut().take() {
            src.remove();
        }
        self.chrome_hidden.set(false);
        self.topbar.remove_css_class("chrome-hidden");
        self.toc.toggle_button.remove_css_class("chrome-hidden");
    }

    fn populate_drawer(&self, drawer: &gtk4::Box) {
        let back_btn = gtk4::Button::new();
        back_btn.add_css_class("sillpaper-drawer-item");
        back_btn.set_can_focus(false);
        back_btn.set_focus_on_click(false);
        let back_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let back_ico = symbolic_image(AppIcon::ChevronLeft, 16);
        let title_lbl = gtk4::Label::new(Some("SILLPAPER"));
        title_lbl.add_css_class("sillpaper-brand-title");
        let ver_lbl = gtk4::Label::new(Some("0.1"));
        ver_lbl.add_css_class("sillpaper-brand-ver");
        back_row.append(&back_ico);
        back_row.append(&title_lbl);
        back_row.append(&ver_lbl);
        back_btn.set_child(Some(&back_row));
        {
            let sw = self.clone();
            back_btn.connect_clicked(move |_| {
                sw.split_view.set_show_sidebar(false);
                sw.focus_active_editor();
            });
        }
        drawer.append(&back_btn);

        let add_item =
            |icon: AppIcon, label: &str, shortcut: &str, close_drawer: bool, cb: Box<dyn Fn()>| {
                let btn = gtk4::Button::new();
                btn.add_css_class("sillpaper-drawer-item");
                btn.set_can_focus(false);
                btn.set_focus_on_click(false);
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
                let img = symbolic_image(icon, 16);
                let l = gtk4::Label::new(Some(label));
                l.set_xalign(0.0);
                l.set_hexpand(true);
                row.append(&img);
                row.append(&l);
                if !shortcut.is_empty() {
                    let s = gtk4::Label::new(Some(shortcut));
                    s.add_css_class("sillpaper-shortcut-hint");
                    row.append(&s);
                }
                btn.set_child(Some(&row));
                let split = self.split_view.clone();
                btn.connect_clicked(move |_| {
                    if close_drawer {
                        split.set_show_sidebar(false);
                    }
                    cb();
                });
                drawer.append(&btn);
            };

        let sw = self.clone();
        add_item(
            AppIcon::FileNew,
            "New",
            "CTRL + N",
            true,
            Box::new(move || {
                sw.new_empty_tab();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::WindowNew,
            "New Window",
            "SHIFT + CTRL + N",
            true,
            Box::new(move || {
                (sw.on_spawn_window)();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::FolderOpen,
            "Open",
            "CTRL + O",
            true,
            Box::new(move || {
                sw.open_file_dialog();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::Save,
            "Save",
            "CTRL + S",
            true,
            Box::new(move || {
                sw.save_active();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::SaveAs,
            "Save As",
            "",
            true,
            Box::new(move || {
                sw.save_active_as(None);
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::ExportDocx,
            "Export Word (.docx)",
            "",
            true,
            Box::new(move || {
                sw.export_docx_dialog();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::ExportPdf,
            "Export PDF (.pdf)",
            "",
            true,
            Box::new(move || {
                sw.export_pdf_dialog();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::Fullscreen,
            "Toggle full screen",
            "F11",
            true,
            Box::new(move || {
                if sw.window.is_fullscreen() {
                    sw.window.unfullscreen();
                } else {
                    sw.window.fullscreen();
                }
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::Print,
            "Print",
            "CTRL + P",
            true,
            Box::new(move || {
                sw.print_active();
            }),
        );

        let sw = self.clone();
        add_item(
            AppIcon::Settings,
            "Settings",
            "CTRL + ,",
            false,
            Box::new(move || {
                sw.open_settings_drawer();
            }),
        );

        let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        drawer.append(&spacer);

        let target_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        target_row.set_margin_start(12);
        target_row.set_margin_top(6);
        let btn_target = gtk4::Button::with_label("set target");
        btn_target.add_css_class("sillpaper-target-btn");
        btn_target.set_can_focus(false);
        btn_target.set_focus_on_click(false);
        {
            let sw = self.clone();
            btn_target.connect_clicked(move |_| {
                sw.split_view.set_show_sidebar(false);
                sw.open_writing_goal_dialog();
            });
        }
        target_row.append(&btn_target);
        drawer.append(&target_row);
        drawer.append(&self.drawer_stats_label);
    }

    fn wire_topbar_buttons(&self, btn_menu: &gtk4::Button, btn_add_tab: &gtk4::Button) {
        let sw = self.clone();
        btn_menu.connect_clicked(move |_| {
            if sw.split_view.shows_sidebar() {
                sw.split_view.set_show_sidebar(false);
                sw.focus_active_editor();
            } else {
                sw.open_menu_drawer();
            }
        });

        let sw = self.clone();
        btn_add_tab.connect_clicked(move |_| {
            sw.new_empty_tab();
        });

        let sw = self.clone();
        let menu = btn_menu.clone();
        self.split_view.connect_show_sidebar_notify(move |sv| {
            if sv.shows_sidebar() {
                menu.add_css_class("sillpaper-menu-open");
                menu.set_tooltip_text(Some("Close sidebar"));
            } else {
                menu.remove_css_class("sillpaper-menu-open");
                menu.set_tooltip_text(Some("Open menu"));
                sw.focus_active_editor();
            }
        });
    }

    fn wire_counter_click(&self) {
        let sw = self.clone();
        self.word_counter.pill.connect_clicked(move |_| {
            sw.open_writing_goal_dialog();
        });
    }

    fn wire_tab_view(&self) {
        {
            let sw = self.clone();
            self.tab_view.connect_selected_page_notify(move |_| {
                sw.hide_floating_toolbar();
                sw.sync_active_tab_title();
                sw.refresh_stats_now();
                sw.refresh_toc();
                sw.save_session_order();
                sw.focus_active_editor();
            });
        }

        {
            let sw = self.clone();
            self.tab_view.connect_page_reordered(move |_, _, _| {
                sw.save_session_order();
            });
        }

        {
            let sw = self.clone();
            self.tab_view.connect_close_page(move |tv, page| {
                if sw.allow_close_tab.get() {
                    sw.allow_close_tab.set(false);
                    sw.finalize_closed_page(page);
                    return glib::Propagation::Proceed;
                }

                let page_obj: glib::Object = page.clone().upcast();
                let tab_opt = sw
                    .page_to_id
                    .borrow()
                    .get(&page_obj)
                    .and_then(|id| sw.tabs.borrow().get(id).cloned());

                let Some(tab) = tab_opt else {
                    return glib::Propagation::Proceed;
                };

                if !tab.is_dirty() {
                    tab.remove_backup();
                    sw.finalize_closed_page(page);
                    tv.close_page_finish(page, true);
                    if tv.n_pages() == 0 {
                        sw.new_empty_tab();
                    }
                    return glib::Propagation::Stop;
                }

                let title = tab.display_title();
                let tv_c = tv.clone();
                let page_c = page.clone();
                let sw_c = sw.clone();
                show_confirm_unsaved_dialog(&sw.window, &title, move |choice| match choice {
                    UnsavedChoice::Cancel => {
                        tv_c.close_page_finish(&page_c, false);
                    }
                    UnsavedChoice::Discard => {
                        tab.remove_backup();
                        sw_c.finalize_closed_page(&page_c);
                        tv_c.close_page_finish(&page_c, true);
                        if tv_c.n_pages() == 0 {
                            sw_c.new_empty_tab();
                        }
                    }
                    UnsavedChoice::Save => {
                        let path_opt = tab.file_path.borrow().clone();
                        if let Some(p) = path_opt
                            && FileFormat::from_path(&p).supports_direct_save()
                        {
                            if sw_c.perform_save(&tab, &p) {
                                tab.remove_backup();
                                sw_c.finalize_closed_page(&page_c);
                                tv_c.close_page_finish(&page_c, true);
                                if tv_c.n_pages() == 0 {
                                    sw_c.new_empty_tab();
                                }
                            } else {
                                tv_c.close_page_finish(&page_c, false);
                            }
                        } else {
                            tv_c.set_selected_page(&page_c);
                            let tv_s = tv_c.clone();
                            let page_s = page_c.clone();
                            let sw_s = sw_c.clone();
                            let tab_s = tab.clone();
                            tv_c.close_page_finish(&page_c, false);
                            sw_c.save_active_as(Some(Box::new(move || {
                                tab_s.remove_backup();
                                sw_s.allow_close_tab.set(true);
                                tv_s.close_page(&page_s);
                                if tv_s.n_pages() == 0 {
                                    sw_s.new_empty_tab();
                                }
                            })));
                        }
                    }
                });
                glib::Propagation::Stop
            });
        }
    }

    fn finalize_closed_page(&self, page: &adw::TabPage) {
        let key: glib::Object = page.clone().upcast();
        if let Some(id) = self.page_to_id.borrow_mut().remove(&key) {
            self.tabs.borrow_mut().remove(&id);
            self.ext_last_diag_text
                .borrow_mut()
                .retain(|(tid, _), _| tid != &id);
        }
        self.save_session_order();
        crate::trim_allocator_memory();
    }

    fn wire_search_and_toc(&self) {
        {
            let sw = self.clone();
            self.search_bar.set_on_query(move |q| {
                if let Some(tab) = sw.active_tab() {
                    let (cur, total) = tab.run_search(q);
                    sw.search_bar.set_counter(cur, total);
                }
            });
        }
        {
            let sw = self.clone();
            self.search_bar.set_on_step(move |fwd| {
                if let Some(tab) = sw.active_tab() {
                    let (cur, total) = tab.step_search(fwd);
                    sw.search_bar.set_counter(cur, total);
                }
            });
        }
        {
            let sw = self.clone();
            self.search_bar.set_on_close(move || {
                if let Some(tab) = sw.active_tab() {
                    tab.clear_search();
                }
                sw.search_bar.close();
                sw.focus_active_editor();
                sw.sync_chrome_hover_visibility();
            });
        }
        {
            let sw = self.clone();
            self.toc.set_on_select(move |block_idx| {
                if let Some(tab) = sw.active_tab() {
                    tab.scroll_to_block(block_idx);
                    tab.focus_editor();
                }
            });
        }
        {
            let sw = self.clone();
            self.toc.connect_opened(move || {
                if sw.toc_dirty.get() {
                    sw.rebuild_toc_now();
                }
            });
        }
        {
            let sw = self.clone();
            self.toc.connect_closed(move || {
                sw.sync_chrome_hover_visibility();
            });
        }
        {
            let sw = self.clone();
            self.edge_drawer_btn.connect_clicked(move |_| {
                sw.open_menu_drawer();
            });
        }
        {
            let sw = self.clone();
            self.split_view.connect_show_sidebar_notify(move |sv| {
                let shown = sv.shows_sidebar();
                sw.edge_drawer_btn.set_visible(!shown);
                sw.sync_chrome_hover_visibility();
                if shown {
                    if let Some(src) = sw.settings_teardown_timer.borrow_mut().take() {
                        src.remove();
                    }
                } else {
                    sw.schedule_settings_panel_teardown();
                }
            });
        }
    }

    fn wire_window_controllers(&self) {
        {
            let motion = gtk4::EventControllerMotion::new();
            motion.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let sw = self.clone();
            motion.connect_motion(move |_, x, y| {
                let w = sw.window.width() as f64;
                let in_topbar = y < 58.0;
                let in_toc = y < 96.0 && w > 100.0 && x > w - 68.0;
                let in_zone = in_topbar || in_toc;
                let was_in = sw.pointer_in_chrome_zone.replace(in_zone);
                if in_zone {
                    sw.reveal_chrome();
                } else if sw.prefs.borrow().hide_chrome_unless_hovered
                    && (was_in || !sw.chrome_hidden.get())
                    && sw.chrome_hide_timer.borrow().is_none()
                {
                    sw.schedule_chrome_hide_delay(320, true);
                }
            });
            let sw_leave = self.clone();
            motion.connect_leave(move |_| {
                sw_leave.pointer_in_chrome_zone.set(false);
                if sw_leave.prefs.borrow().hide_chrome_unless_hovered {
                    sw_leave.schedule_chrome_hide_delay(320, true);
                }
            });
            self.window.add_controller(motion);
        }

        {
            let scroll =
                gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
            scroll.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let sw = self.clone();
            scroll.connect_scroll(move |ctrl, _dx, dy| {
                if ctrl
                    .current_event_state()
                    .contains(gdk::ModifierType::CONTROL_MASK)
                {
                    let delta = if dy < 0.0 { 4 } else { -4 };
                    let mut p = sw.prefs.borrow().clone();
                    p.bump_font_size(delta);
                    sw.update_preferences(p, true);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            self.window.add_controller(scroll);
        }

        {
            let key = gtk4::EventControllerKey::new();
            key.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let sw = self.clone();
            key.connect_key_pressed(move |_, keyval, _, mods| {
                let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
                let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
                let alt = mods.contains(gdk::ModifierType::ALT_MASK);

                if matches!(keyval, gdk::Key::Control_L | gdk::Key::Control_R) {
                    if sw.ctrl_hold_timer.borrow().is_none() {
                        let sheet = sw.shortcut_sheet.clone();
                        let timer_slot = sw.ctrl_hold_timer.clone();
                        let id = glib::timeout_add_local_once(
                            std::time::Duration::from_millis(1200),
                            move || {
                                *timer_slot.borrow_mut() = None;
                                sheet.set_visible(true);
                            },
                        );
                        *sw.ctrl_hold_timer.borrow_mut() = Some(id);
                    }
                    return glib::Propagation::Proceed;
                } else {
                    if let Some(id) = sw.ctrl_hold_timer.borrow_mut().take() {
                        id.remove();
                    }
                    sw.shortcut_sheet.set_visible(false);
                }

                if keyval == gdk::Key::Escape {
                    if sw.floating_toolbar.is_input_mode() {
                        sw.floating_toolbar.cancel_link_input();
                        return glib::Propagation::Stop;
                    }
                    if sw.quick_fix_card.is_visible() {
                        sw.quick_fix_card.hide();
                        sw.focus_active_editor();
                        return glib::Propagation::Stop;
                    }
                    if sw.action_preview_card.is_visible() {
                        sw.action_preview_card.hide();
                        sw.ext_manager.borrow_mut().cancel_action();
                        sw.focus_active_editor();
                        return glib::Propagation::Stop;
                    }
                    if sw.search_bar.is_open() {
                        if let Some(tab) = sw.active_tab() {
                            tab.clear_search();
                        }
                        sw.search_bar.close();
                        sw.focus_active_editor();
                        sw.sync_chrome_hover_visibility();
                        return glib::Propagation::Stop;
                    }
                    if sw.split_view.shows_sidebar() {
                        sw.split_view.set_show_sidebar(false);
                        sw.focus_active_editor();
                        sw.sync_chrome_hover_visibility();
                        return glib::Propagation::Stop;
                    }
                    sw.toc.hide();
                    sw.reveal_chrome();
                    if sw.prefs.borrow().hide_chrome_unless_hovered {
                        sw.schedule_chrome_hide_delay(1600, true);
                    }
                    if sw.window.is_fullscreen() {
                        sw.window.unfullscreen();
                        return glib::Propagation::Stop;
                    }
                    return glib::Propagation::Proceed;
                }

                if keyval == gdk::Key::F11 {
                    if sw.window.is_fullscreen() {
                        sw.window.unfullscreen();
                    } else {
                        sw.window.fullscreen();
                    }
                    return glib::Propagation::Stop;
                }

                if keyval == gdk::Key::F3 && !ctrl {
                    if sw.search_bar.is_open() {
                        if let Some(tab) = sw.active_tab() {
                            let (c, t) = tab.step_search(!shift);
                            sw.search_bar.set_counter(c, t);
                        }
                    } else {
                        sw.open_search_bar();
                    }
                    return glib::Propagation::Stop;
                }

                if ctrl && !alt {
                    match keyval {
                        gdk::Key::n | gdk::Key::N => {
                            if shift {
                                (sw.on_spawn_window)();
                            } else {
                                sw.new_empty_tab();
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::o | gdk::Key::O if !shift => {
                            sw.open_file_dialog();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::s | gdk::Key::S => {
                            if shift {
                                sw.save_active_as(None);
                            } else {
                                sw.save_active();
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::w | gdk::Key::W if !shift => {
                            if let Some(p) = sw.tab_view.selected_page() {
                                sw.tab_view.close_page(&p);
                            }
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::p | gdk::Key::P if !shift => {
                            sw.print_active();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::comma => {
                            sw.toggle_settings_panel();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::f | gdk::Key::F if !shift => {
                            sw.open_search_bar();
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::m | gdk::Key::M => {
                            let mut p = sw.prefs.borrow().clone();
                            p.focus_mode = !p.focus_mode;
                            let msg = if p.focus_mode {
                                "Focus Mode on"
                            } else {
                                "Focus Mode off"
                            };
                            sw.update_preferences(p, true);
                            sw.word_counter.show_toast(msg);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::t | gdk::Key::T if shift => {
                            let mut p = sw.prefs.borrow().clone();
                            p.typewriter_mode = !p.typewriter_mode;
                            let msg = if p.typewriter_mode {
                                "Typewriter Mode on"
                            } else {
                                "Typewriter Mode off"
                            };
                            sw.update_preferences(p, true);
                            sw.word_counter.show_toast(msg);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::c | gdk::Key::C if shift => {
                            let mut p = sw.prefs.borrow().clone();
                            p.word_counter_always_visible = !p.word_counter_always_visible;
                            sw.update_preferences(p, true);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => {
                            let mut p = sw.prefs.borrow().clone();
                            p.bump_font_size(4);
                            sw.update_preferences(p, true);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::minus | gdk::Key::KP_Subtract => {
                            let mut p = sw.prefs.borrow().clone();
                            p.bump_font_size(-4);
                            sw.update_preferences(p, true);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::_0 | gdk::Key::KP_0 => {
                            let mut p = sw.prefs.borrow().clone();
                            p.font_size_percent = 100;
                            sw.update_preferences(p, true);
                            return glib::Propagation::Stop;
                        }
                        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                            let n = sw.tab_view.n_pages();
                            if n > 1
                                && let Some(sel) = sw.tab_view.selected_page()
                            {
                                let idx = sw.tab_view.page_position(&sel);
                                let next = if shift || keyval == gdk::Key::ISO_Left_Tab {
                                    (idx + n - 1) % n
                                } else {
                                    (idx + 1) % n
                                };
                                sw.tab_view.set_selected_page(&sw.tab_view.nth_page(next));
                            }
                            return glib::Propagation::Stop;
                        }
                        _ => {}
                    }
                }

                glib::Propagation::Proceed
            });

            let sw_rel = self.clone();
            key.connect_key_released(move |_, keyval, _, _| {
                if matches!(keyval, gdk::Key::Control_L | gdk::Key::Control_R) {
                    if let Some(id) = sw_rel.ctrl_hold_timer.borrow_mut().take() {
                        id.remove();
                    }
                    sw_rel.shortcut_sheet.set_visible(false);
                }
            });

            self.window.add_controller(key);
        }

        {
            let sw = self.clone();
            self.window.connect_close_request(move |_| {
                sw.flush_all_state();
                sw.hide_floating_toolbar();
                sw.ext_manager.borrow_mut().stop_all();
                glib::Propagation::Proceed
            });
        }
    }

    fn wire_drag_and_drop(&self) {
        let target = gtk4::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
        target.set_types(&[gdk::FileList::static_type(), gio::File::static_type()]);
        target.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let sw = self.clone();
        target.connect_drop(move |_, value, _, _| {
            let paths: Vec<PathBuf> = if let Ok(file_list) = value.get::<gdk::FileList>() {
                file_list
                    .files()
                    .into_iter()
                    .filter_map(|f| f.path())
                    .collect()
            } else if let Ok(file) = value.get::<gio::File>() {
                file.path().into_iter().collect()
            } else {
                Vec::new()
            };
            if paths.is_empty() {
                return false;
            }

            let mut inserted_images = 0usize;
            for path in paths {
                if is_image_file_path(&path) {
                    if let Some(tab) = sw.active_tab() {
                        tab.insert_image_from_path(&path);
                        tab.focus_editor();
                        inserted_images += 1;
                    }
                } else {
                    sw.open_path(&path);
                }
            }
            if inserted_images == 1 {
                sw.word_counter.show_toast("Inserted image");
            } else if inserted_images > 1 {
                sw.word_counter
                    .show_toast(&format!("Inserted {inserted_images} images"));
            }
            true
        });
        self.window.add_controller(target);
    }

    fn open_search_bar(&self) {
        self.reveal_chrome();
        let prefill = self
            .active_tab()
            .and_then(|t| t.state.borrow().copy_fragment(false))
            .map(|f| f.plain_text(false));
        self.search_bar.open(prefill.as_deref());
    }

    pub fn application(&self) -> &adw::Application {
        &self.app
    }
}

fn suggested_filename(tab: &EditorTab, prefs: &Preferences) -> String {
    let ext = prefs.default_save_format.extension();
    if let Some(p) = tab.file_path.borrow().as_ref() {
        if FileFormat::from_path(p).supports_direct_save()
            && let Some(name) = p.file_name().and_then(|n| n.to_str())
        {
            return name.to_string();
        }
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            return format!("{}.{ext}", sanitize_stem(stem));
        }
    }
    let stem = sanitize_stem(&tab.display_title());
    if prefs.include_date_in_filenames {
        let date = glib::DateTime::now_local()
            .ok()
            .and_then(|dt| dt.format("%Y-%m-%d").ok())
            .map(|g| g.to_string())
            .unwrap_or_default();
        if !date.is_empty() {
            return format!("{date} {stem}.{ext}");
        }
    }
    format!("{stem}.{ext}")
}

fn ensure_extension(path: PathBuf, ext: &str, force: bool) -> PathBuf {
    let has_ext = path.extension().and_then(|e| e.to_str()).is_some();
    if !has_ext
        || (force
            && !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext)))
    {
        let mut os = path.into_os_string();
        os.push(".");
        os.push(ext);
        PathBuf::from(os)
    } else {
        path
    }
}

fn sanitize_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}
