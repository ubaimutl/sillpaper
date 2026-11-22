use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionActionDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub tooltip: String,
    #[serde(default)]
    pub prompt_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingFieldKind {
    Text,
    Secret,
    Select,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionSettingDef {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: SettingFieldKind,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub visible_when: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtensionCapabilities {
    pub completions: bool,
    pub diagnostics: bool,
    pub actions: Vec<ExtensionActionDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub capabilities: ExtensionCapabilities,
    #[serde(default)]
    pub settings: Vec<ExtensionSettingDef>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    pub manifest: ExtensionManifest,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionDiagnostic {
    pub start: usize,
    pub end: usize,
    #[serde(default = "default_severity")]
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub replacement: Option<String>,
}

fn default_severity() -> String {
    "warning".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionRequestKind {
    Completion,
    Diagnostics,
    Action,
    Other,
}

#[derive(Debug, Clone)]
pub enum ExtensionEvent {
    Completion {
        request_id: u64,
        block: usize,
        offset: usize,
        suggestion: String,
        manual: bool,
    },
    Diagnostics {
        request_id: u64,
        block: usize,
        items: Vec<ExtensionDiagnostic>,
        manual: bool,
    },
    ActionResult {
        request_id: u64,
        replacement: String,
    },
    Error {
        request_id: Option<u64>,
        extension_id: String,
        kind: ExtensionRequestKind,
        manual: bool,
        message: String,
    },
}

enum PendingKind {
    Completion {
        block: usize,
        offset: usize,
        manual: bool,
    },
    Diagnostics {
        block: usize,
        manual: bool,
    },
    Action,
}

struct RunningExtension {
    manifest: ExtensionManifest,
    child: Child,
    stdin: ChildStdin,
}

impl Drop for RunningExtension {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct ExtensionManager {
    root_dir: PathBuf,
    discovered: Vec<DiscoveredExtension>,
    running: HashMap<String, RunningExtension>,
    tx: Sender<(String, Value)>,
    rx: Receiver<(String, Value)>,
    on_wake: Option<Arc<dyn Fn() + Send + Sync>>,
    next_id: u64,
    pending: HashMap<u64, PendingKind>,
    last_completion_req: Option<(String, u64)>,
    last_diagnostics_req: Option<(String, u64)>,
    last_action_req: Option<(String, u64)>,
}

pub fn default_extensions_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("SILLPAPER_EXTENSIONS_DIR")
        .or_else(|_| std::env::var("STILLWRITE_EXTENSIONS_DIR"))
        && !override_dir.trim().is_empty()
    {
        return PathBuf::from(override_dir);
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home =
                std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
            home.join(".config")
        });
    let old_extensions = base.join("stillwrite/extensions");
    let extensions = base.join("sillpaper/extensions");
    if !extensions.exists() && old_extensions.is_dir() {
        old_extensions
    } else {
        extensions
    }
}

impl ExtensionManager {
    pub fn new(root_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut mgr = Self {
            root_dir,
            discovered: Vec::new(),
            running: HashMap::new(),
            tx,
            rx,
            on_wake: None,
            next_id: 1,
            pending: HashMap::new(),
            last_completion_req: None,
            last_diagnostics_req: None,
            last_action_req: None,
        };
        mgr.rescan();
        mgr
    }

    pub fn set_wake_callback(&mut self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.on_wake = Some(cb);
    }

    pub fn running_manifests(&self) -> Vec<ExtensionManifest> {
        let mut out = Vec::new();
        for ext in &self.discovered {
            if let Some(run) = self.running.get(&ext.manifest.id) {
                out.push(run.manifest.clone());
            }
        }
        out
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn discovered(&self) -> &[DiscoveredExtension] {
        &self.discovered
    }

    pub fn is_running(&self, ext_id: &str) -> bool {
        self.running.contains_key(ext_id)
    }

    pub fn has_any_running(&self) -> bool {
        !self.running.is_empty()
    }

    pub fn rescan(&mut self) {
        self.discovered.clear();
        let Ok(entries) = fs::read_dir(&self.root_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("manifest.json");
            let Ok(raw) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            if let Ok(manifest) = serde_json::from_str::<ExtensionManifest>(&raw)
                && !manifest.id.trim().is_empty()
                && !manifest.command.trim().is_empty()
            {
                self.discovered.push(DiscoveredExtension { manifest, dir });
            }
        }
        self.discovered
            .sort_by_key(|a| a.manifest.name.to_lowercase());
    }

    pub fn load_config(&self, ext_id: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Some(ext) = self.discovered.iter().find(|e| e.manifest.id == ext_id) else {
            return out;
        };
        for s in &ext.manifest.settings {
            out.insert(s.key.clone(), s.default.clone());
        }
        let cfg_path = ext.dir.join("config.json");
        if let Ok(raw) = fs::read_to_string(&cfg_path)
            && let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw)
        {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    out.insert(k, s.to_string());
                }
            }
        }
        out
    }

    pub fn save_config(
        &mut self,
        ext_id: &str,
        values: &HashMap<String, String>,
    ) -> Result<(), String> {
        let Some(ext) = self.discovered.iter().find(|e| e.manifest.id == ext_id) else {
            return Err(format!("Extension '{ext_id}' not found"));
        };
        let cfg_path = ext.dir.join("config.json");
        let mut obj = Map::new();
        for (k, v) in values {
            obj.insert(k.clone(), Value::String(v.clone()));
        }
        let serialized = serde_json::to_string_pretty(&Value::Object(obj))
            .map_err(|e| format!("Failed to serialize extension config: {e}"))?;
        let tmp = ext.dir.join("config.json.tmp");
        fs::write(&tmp, serialized.as_bytes())
            .map_err(|e| format!("Failed to write extension config: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, &cfg_path)
            .map_err(|e| format!("Failed to commit extension config: {e}"))?;

        if self.running.contains_key(ext_id) {
            let id = self.alloc_id();
            let msg = json!({
                "id": id,
                "method": "config/update",
                "params": { "settings": values }
            });
            let _ = self.send_json(ext_id, &msg);
        }
        Ok(())
    }

    pub fn sync_enabled(&mut self, enabled_ids: &[String]) {
        let to_stop: Vec<String> = self
            .running
            .keys()
            .filter(|id| !enabled_ids.iter().any(|e| e == *id))
            .cloned()
            .collect();
        for id in to_stop {
            self.running.remove(&id);
        }

        for ext_id in enabled_ids {
            if self.running.contains_key(ext_id) {
                continue;
            }
            let _ = self.start_extension(ext_id);
        }
    }

    pub fn stop_all(&mut self) {
        self.running.clear();
        self.pending.clear();
        self.last_completion_req = None;
        self.last_diagnostics_req = None;
        self.last_action_req = None;
    }

    fn start_extension(&mut self, ext_id: &str) -> Result<(), String> {
        let Some(ext) = self
            .discovered
            .iter()
            .find(|e| e.manifest.id == ext_id)
            .cloned()
        else {
            return Err(format!("Extension '{ext_id}' not found"));
        };

        let program =
            if ext.manifest.command.starts_with("./") || ext.manifest.command.starts_with("../") {
                ext.dir.join(&ext.manifest.command)
            } else {
                PathBuf::from(&ext.manifest.command)
            };
        let mut cmd = Command::new(&program);
        cmd.args(&ext.manifest.args)
            .current_dir(&ext.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn '{}': {e}", ext.manifest.name))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Extension stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Extension stdout unavailable".to_string())?;

        let tx = self.tx.clone();
        let wake = self.on_wake.clone();
        let eid = ext.manifest.id.clone();
        let _ = thread::Builder::new()
            .name(format!("ext-read-{eid}"))
            .stack_size(256 * 1024)
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                        if tx.send((eid.clone(), val)).is_err() {
                            break;
                        }
                        if let Some(ref cb) = wake {
                            cb();
                        }
                    }
                }
            });

        self.running.insert(
            ext.manifest.id.clone(),
            RunningExtension {
                manifest: ext.manifest.clone(),
                child,
                stdin,
            },
        );

        let cfg = self.load_config(&ext.manifest.id);
        let init_id = self.alloc_id();
        let init_msg = json!({
            "id": init_id,
            "method": "config/update",
            "params": { "settings": cfg }
        });
        let _ = self.send_json(&ext.manifest.id, &init_msg);
        Ok(())
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn send_json(&mut self, ext_id: &str, value: &Value) -> Result<(), String> {
        let Some(run) = self.running.get_mut(ext_id) else {
            return Err(format!("Extension '{ext_id}' is not running"));
        };
        let mut line =
            serde_json::to_string(value).map_err(|e| format!("JSON encode error: {e}"))?;
        line.push('\n');
        if let Err(e) = run
            .stdin
            .write_all(line.as_bytes())
            .and_then(|_| run.stdin.flush())
        {
            self.running.remove(ext_id);
            return Err(format!("Extension pipe closed: {e}"));
        }
        Ok(())
    }

    fn cancel_prev(&mut self, slot: &mut Option<(String, u64)>) {
        if let Some((ext_id, prev_id)) = slot.take() {
            self.pending.remove(&prev_id);
            let msg = json!({
                "method": "cancel",
                "params": { "id": prev_id }
            });
            let _ = self.send_json(&ext_id, &msg);
        }
    }

    pub fn has_completions(&self) -> bool {
        self.running
            .values()
            .any(|r| r.manifest.capabilities.completions)
    }

    pub fn has_diagnostics(&self) -> bool {
        self.running
            .values()
            .any(|r| r.manifest.capabilities.diagnostics)
    }

    pub fn available_actions(&self) -> Vec<(String, ExtensionActionDef)> {
        let mut out = Vec::new();
        for ext in &self.discovered {
            if let Some(run) = self.running.get(&ext.manifest.id) {
                for act in &run.manifest.capabilities.actions {
                    out.push((ext.manifest.id.clone(), act.clone()));
                }
            }
        }
        out
    }

    pub fn request_completion(
        &mut self,
        block: usize,
        offset: usize,
        prefix: &str,
        suffix: &str,
        paragraph: &str,
        manual: bool,
    ) -> Option<u64> {
        let ext_id = self
            .running
            .iter()
            .find(|(_, r)| r.manifest.capabilities.completions)
            .map(|(k, _)| k.clone())?;

        let mut prev = self.last_completion_req.take();
        self.cancel_prev(&mut prev);

        let id = self.alloc_id();
        let msg = json!({
            "id": id,
            "method": "completion/request",
            "params": {
                "block": block,
                "offset": offset,
                "prefix": prefix,
                "suffix": suffix,
                "paragraph": paragraph,
                "manual": manual
            }
        });
        if self.send_json(&ext_id, &msg).is_ok() {
            self.pending.insert(
                id,
                PendingKind::Completion {
                    block,
                    offset,
                    manual,
                },
            );
            self.last_completion_req = Some((ext_id, id));
            Some(id)
        } else {
            None
        }
    }

    pub fn cancel_completion(&mut self) {
        let mut prev = self.last_completion_req.take();
        self.cancel_prev(&mut prev);
    }

    pub fn request_diagnostics(&mut self, block: usize, text: &str, manual: bool) -> Option<u64> {
        let ext_id = self
            .running
            .iter()
            .find(|(_, r)| r.manifest.capabilities.diagnostics)
            .map(|(k, _)| k.clone())?;

        let mut prev = self.last_diagnostics_req.take();
        self.cancel_prev(&mut prev);

        let id = self.alloc_id();
        let msg = json!({
            "id": id,
            "method": "diagnostics/request",
            "params": {
                "block": block,
                "text": text,
                "manual": manual
            }
        });
        if self.send_json(&ext_id, &msg).is_ok() {
            self.pending
                .insert(id, PendingKind::Diagnostics { block, manual });
            self.last_diagnostics_req = Some((ext_id, id));
            Some(id)
        } else {
            None
        }
    }

    pub fn cancel_diagnostics(&mut self) {
        let mut prev = self.last_diagnostics_req.take();
        self.cancel_prev(&mut prev);
    }

    pub fn run_action(
        &mut self,
        ext_id: &str,
        action_id: &str,
        text: &str,
        custom_prompt: Option<&str>,
    ) -> Option<u64> {
        let mut prev = self.last_action_req.take();
        self.cancel_prev(&mut prev);

        let id = self.alloc_id();
        let msg = json!({
            "id": id,
            "method": "action/run",
            "params": {
                "action_id": action_id,
                "text": text,
                "prompt": custom_prompt.unwrap_or("")
            }
        });
        if self.send_json(ext_id, &msg).is_ok() {
            self.pending.insert(id, PendingKind::Action);
            self.last_action_req = Some((ext_id.to_string(), id));
            Some(id)
        } else {
            None
        }
    }

    pub fn cancel_action(&mut self) {
        let mut prev = self.last_action_req.take();
        self.cancel_prev(&mut prev);
    }

    pub fn poll_events(&mut self) -> Vec<ExtensionEvent> {
        let mut events = Vec::new();
        while let Ok((ext_id, msg)) = self.rx.try_recv() {
            let req_id = msg.get("id").and_then(|v| v.as_u64());
            if let Some(err) = msg.get("error").and_then(|v| v.as_str()) {
                let (kind, manual) = if let Some(id) = req_id {
                    let Some(pending_kind) = self.pending.remove(&id) else {
                        continue;
                    };
                    match pending_kind {
                        PendingKind::Completion { manual, .. } => {
                            (ExtensionRequestKind::Completion, manual)
                        }
                        PendingKind::Diagnostics { manual, .. } => {
                            (ExtensionRequestKind::Diagnostics, manual)
                        }
                        PendingKind::Action => (ExtensionRequestKind::Action, true),
                    }
                } else {
                    (ExtensionRequestKind::Other, true)
                };
                events.push(ExtensionEvent::Error {
                    request_id: req_id,
                    extension_id: ext_id,
                    kind,
                    manual,
                    message: err.to_string(),
                });
                continue;
            }
            let Some(id) = req_id else { continue };
            let Some(kind) = self.pending.remove(&id) else {
                continue;
            };
            let Some(result) = msg.get("result") else {
                continue;
            };
            match kind {
                PendingKind::Completion {
                    block,
                    offset,
                    manual,
                } => {
                    let s = result
                        .get("suggestion")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    events.push(ExtensionEvent::Completion {
                        request_id: id,
                        block,
                        offset,
                        suggestion: s.to_string(),
                        manual,
                    });
                }
                PendingKind::Diagnostics { block, manual } => {
                    if let Some(arr) = result.get("diagnostics")
                        && let Ok(items) =
                            serde_json::from_value::<Vec<ExtensionDiagnostic>>(arr.clone())
                    {
                        events.push(ExtensionEvent::Diagnostics {
                            request_id: id,
                            block,
                            items,
                            manual,
                        });
                    }
                }
                PendingKind::Action => {
                    if let Some(rep) = result.get("replacement").and_then(|v| v.as_str()) {
                        events.push(ExtensionEvent::ActionResult {
                            request_id: id,
                            replacement: rep.to_string(),
                        });
                    }
                }
            }
        }
        events
    }
}
