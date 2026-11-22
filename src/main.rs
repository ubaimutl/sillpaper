use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use sillpaper::config::{Preferences, RendererBackend};
use sillpaper::io::backup::StorageManager;
use sillpaper::ui::window::SillpaperWindow;
use std::cell::{Cell, RefCell};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::rc::Rc;

fn main() -> gtk4::glib::ExitCode {
    sillpaper::tune_allocator_for_low_memory();
    let start_instant = std::time::Instant::now();
    let log_startup = std::env::var_os("SILLPAPER_LOG_STARTUP").is_some();

    let storage = Rc::new(StorageManager::default_xdg());
    let has_saved_prefs = storage.preferences_path().exists();
    let initial_prefs = storage.load_preferences();
    let active_renderer = apply_renderer_env_before_gtk(initial_prefs.renderer);

    let app = adw::Application::builder()
        .application_id("dev.ubai.Sillpaper")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let prefs = Rc::new(RefCell::new(initial_prefs));
    let windows: Rc<RefCell<Vec<SillpaperWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let first_launched = Rc::new(Cell::new(false));
    let restart_requested = Rc::new(Cell::new(false));
    let css_provider: Rc<RefCell<Option<gtk4::CssProvider>>> = Rc::new(RefCell::new(None));

    type SpawnFn = Rc<RefCell<Option<Rc<dyn Fn(Vec<PathBuf>)>>>>;
    let spawn_holder: SpawnFn = Rc::new(RefCell::new(None));

    let spawn_window: Rc<dyn Fn(Vec<PathBuf>)> = {
        let app_c = app.clone();
        let storage_c = storage.clone();
        let prefs_c = prefs.clone();
        let windows_c = windows.clone();
        let first_c = first_launched.clone();
        let restart_c = restart_requested.clone();
        let css_slot = css_provider.clone();
        let spawn_ref = spawn_holder.clone();

        Rc::new(move |files: Vec<PathBuf>| {
            let provider = css_slot
                .borrow_mut()
                .get_or_insert_with(|| {
                    gtk4::Window::set_default_icon_name("dev.ubai.Sillpaper");
                    if !has_saved_prefs && adw::StyleManager::default().is_dark() {
                        prefs_c.borrow_mut().theme = sillpaper::config::ThemeId::Dark;
                    }
                    let p = gtk4::CssProvider::new();
                    if let Some(display) = gdk::Display::default() {
                        gtk4::style_context_add_provider_for_display(
                            &display,
                            &p,
                            gtk4::STYLE_PROVIDER_PRIORITY_USER + 100,
                        );
                    }
                    p
                })
                .clone();

            let is_primary = !first_c.get();
            first_c.set(true);

            let on_broadcast: Rc<dyn Fn(Preferences)> = {
                let wins = windows_c.clone();
                let prefs_shared = prefs_c.clone();
                Rc::new(move |updated| {
                    *prefs_shared.borrow_mut() = updated.clone();
                    for w in wins.borrow().iter() {
                        w.update_preferences(updated.clone(), false);
                    }
                })
            };

            let on_spawn_new: Rc<dyn Fn()> = {
                let sr = spawn_ref.clone();
                Rc::new(move || {
                    if let Some(f) = sr.borrow().as_ref() {
                        f(Vec::new());
                    }
                })
            };

            let on_request_restart: Rc<dyn Fn()> = {
                let app_q = app_c.clone();
                let wins_q = windows_c.clone();
                let flag = restart_c.clone();
                Rc::new(move || {
                    for w in wins_q.borrow().iter() {
                        w.flush_all_state();
                    }
                    flag.set(true);
                    app_q.quit();
                })
            };

            let win = SillpaperWindow::new(
                &app_c,
                storage_c.clone(),
                prefs_c.clone(),
                provider,
                is_primary,
                active_renderer,
                files,
                on_spawn_new,
                on_broadcast,
                on_request_restart,
            );

            let wins_prune = windows_c.clone();
            let raw_win = win.window.clone();
            win.window.connect_destroy(move |_| {
                wins_prune.borrow_mut().retain(|w| w.window != raw_win);
            });

            if log_startup && is_primary {
                win.window.connect_map(move |_| {
                    eprintln!(
                        "SILLPAPER_STARTUP_MS={:.2}",
                        start_instant.elapsed().as_secs_f64() * 1000.0
                    );
                });
            }

            win.present();
            windows_c.borrow_mut().push(win);
        })
    };

    *spawn_holder.borrow_mut() = Some(spawn_window.clone());

    {
        let spawn = spawn_window.clone();
        let wins = windows.clone();
        app.connect_activate(move |_| {
            if let Some(existing) = wins.borrow().first() {
                existing.present();
            } else {
                spawn(Vec::new());
            }
        });
    }

    {
        let spawn = spawn_window;
        let wins = windows;
        app.connect_open(move |_, files, _| {
            let paths: Vec<PathBuf> = files.iter().filter_map(|f| f.path()).collect();
            if let Some(existing) = wins.borrow().first() {
                for p in paths {
                    existing.open_path(&p);
                }
                existing.present();
            } else {
                spawn(paths);
            }
        });
    }

    let exit_code = app.run();

    if restart_requested.get()
        && let Ok(exe) = std::env::current_exe()
    {
        let next_renderer = storage.load_preferences().renderer;
        let mut cmd = std::process::Command::new(exe);
        match next_renderer.gsk_env_value() {
            Some(val) => {
                cmd.env("GSK_RENDERER", val);
            }
            None => {
                cmd.env_remove("GSK_RENDERER");
            }
        }
        let _ = cmd.exec();
    }

    exit_code
}

fn apply_renderer_env_before_gtk(saved: RendererBackend) -> RendererBackend {
    if let Ok(explicit) = std::env::var("SILLPAPER_GSK_RENDERER")
        .or_else(|_| std::env::var("STILLWRITE_GSK_RENDERER"))
    {
        let lower = explicit.to_ascii_lowercase();
        if !lower.is_empty() {
            unsafe {
                std::env::set_var("GSK_RENDERER", &lower);
            }
            return match lower.as_str() {
                "cairo" => RendererBackend::Cairo,
                "vulkan" => RendererBackend::Vulkan,
                _ => RendererBackend::Auto,
            };
        }
    }

    if let Some(val) = saved.gsk_env_value() {
        unsafe {
            std::env::set_var("GSK_RENDERER", val);
        }
    }
    saved
}
