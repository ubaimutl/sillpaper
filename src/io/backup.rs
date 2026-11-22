use crate::config::Preferences;
use crate::io::atomic_write;
use crate::io::html::{document_to_html, html_to_document};
use crate::model::Document;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabBackup {
    pub id: String,
    pub updated_epoch_ms: u64,
    pub file_path: Option<PathBuf>,
    pub is_saved: bool,
    pub last_file_mtime_ms: Option<u64>,
    pub cursor_flat_offset: usize,
    pub html_content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionOrder {
    pub active_tab_id: Option<String>,
    pub tab_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    pub fullscreen: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1000,
            height: 680,
            maximized: false,
            fullscreen: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageManager {
    pub data_dir: PathBuf,
}

impl StorageManager {
    pub fn default_xdg() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let old_data = base.join("stillwrite");
        let data = base.join("sillpaper");
        Self::new(if !data.exists() && old_data.is_dir() {
            old_data
        } else {
            data
        })
    }

    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        Self { data_dir }
    }

    pub fn backup_json_path(&self, tab_id: &str) -> PathBuf {
        self.data_dir
            .join(format!("stillwrite_backup_{tab_id}.json"))
    }

    pub fn fallback_html_path(&self, tab_id: &str) -> PathBuf {
        self.data_dir
            .join(format!("previous_stillwrite_backup_{tab_id}.html"))
    }

    pub fn session_path(&self) -> PathBuf {
        self.data_dir.join("session.json")
    }

    pub fn preferences_path(&self) -> PathBuf {
        self.data_dir.join("preferences.json")
    }

    pub fn window_state_path(&self) -> PathBuf {
        self.data_dir.join("window-state.json")
    }

    pub fn save_tab_backup(&self, backup: &TabBackup, write_count: u32) -> Result<(), String> {
        let json =
            serde_json::to_string(backup).map_err(|e| format!("Serialize backup failed: {e}"))?;
        atomic_write(&self.backup_json_path(&backup.id), json.as_bytes())?;

        let fallback = self.fallback_html_path(&backup.id);
        if !fallback.exists() || write_count.is_multiple_of(31) {
            let _ = atomic_write(&fallback, backup.html_content.as_bytes());
        }
        Ok(())
    }

    pub fn build_backup(
        tab_id: &str,
        doc: &Document,
        file_path: Option<PathBuf>,
        is_saved: bool,
        last_file_mtime_ms: Option<u64>,
        cursor_flat_offset: usize,
    ) -> TabBackup {
        TabBackup {
            id: tab_id.to_string(),
            updated_epoch_ms: now_epoch_ms(),
            file_path,
            is_saved,
            last_file_mtime_ms,
            cursor_flat_offset,
            html_content: document_to_html(doc),
        }
    }

    pub fn remove_tab_backup(&self, tab_id: &str) {
        let _ = fs::remove_file(self.backup_json_path(tab_id));
        let _ = fs::remove_file(self.fallback_html_path(tab_id));
    }

    pub fn save_session_order(&self, order: &SessionOrder) -> Result<(), String> {
        let json = serde_json::to_string_pretty(order)
            .map_err(|e| format!("Serialize session failed: {e}"))?;
        atomic_write(&self.session_path(), json.as_bytes())
    }

    pub fn load_session_order(&self) -> SessionOrder {
        fs::read_to_string(self.session_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn restore_all_backups(&self) -> (Vec<(TabBackup, Document)>, Option<String>) {
        let order = self.load_session_order();
        let mut found: Vec<(TabBackup, Document)> = Vec::new();

        let Ok(entries) = fs::read_dir(&self.data_dir) else {
            return (found, order.active_tab_id);
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if let Some(rest) = fname.strip_prefix("stillwrite_backup_")
                && let Some(tab_id) = rest.strip_suffix(".json")
            {
                if let Ok(raw) = fs::read_to_string(&path)
                    && let Ok(backup) = serde_json::from_str::<TabBackup>(&raw)
                {
                    let doc = html_to_document(&backup.html_content);
                    found.push((backup, doc));
                } else {
                    let html_path = self.fallback_html_path(tab_id);
                    if let Ok(html) = fs::read_to_string(&html_path) {
                        let doc = html_to_document(&html);
                        let backup = TabBackup {
                            id: tab_id.to_string(),
                            updated_epoch_ms: now_epoch_ms(),
                            file_path: None,
                            is_saved: false,
                            last_file_mtime_ms: None,
                            cursor_flat_offset: 0,
                            html_content: html,
                        };
                        found.push((backup, doc));
                    }
                }
            }
        }

        found.sort_by_key(|(b, _)| {
            order
                .tab_ids
                .iter()
                .position(|id| id == &b.id)
                .unwrap_or(usize::MAX)
        });

        (found, order.active_tab_id)
    }

    pub fn load_preferences(&self) -> Preferences {
        fs::read_to_string(self.preferences_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_preferences(&self, prefs: &Preferences) -> Result<(), String> {
        let json = serde_json::to_string_pretty(prefs)
            .map_err(|e| format!("Serialize preferences failed: {e}"))?;
        atomic_write(&self.preferences_path(), json.as_bytes())
    }

    pub fn load_window_state(&self) -> WindowState {
        fs::read_to_string(self.window_state_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_window_state(&self, state: &WindowState) -> Result<(), String> {
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| format!("Serialize window state failed: {e}"))?;
        atomic_write(&self.window_state_path(), json.as_bytes())
    }
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn file_mtime_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}
