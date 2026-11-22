use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static HTTP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

fn shared_agent() -> &'static ureq::Agent {
    HTTP_AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_idle_connections(2)
            .max_idle_connections_per_host(2)
            .max_idle_age(Duration::from_secs(30))
            .build()
            .into()
    })
}

fn tune_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn mallopt(param: i32, value: i32) -> i32;
        }
        const M_TRIM_THRESHOLD: i32 = -1;
        const M_ARENA_MAX: i32 = -8;
        let _ = mallopt(M_ARENA_MAX, 2);
        let _ = mallopt(M_TRIM_THRESHOLD, 64 * 1024);
    }
}

fn trim_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        let _ = malloc_trim(0);
    }
}

#[derive(Default)]
struct SharedState {
    settings: HashMap<String, String>,
    cancelled: HashSet<u64>,
    rate_limited_until: Option<Instant>,
}

#[derive(Debug, Serialize)]
struct DiagnosticOut {
    start: usize,
    end: usize,
    severity: String,
    message: String,
    replacement: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDiagnostic {
    original: Option<String>,
    replacement: Option<String>,
    message: Option<String>,
    severity: Option<String>,
}

fn default_model(provider: &str) -> &'static str {
    match provider {
        "ollama" => "qwen3:4b",
        "groq" => "openai/gpt-oss-20b",
        "openai" => "gpt-4.1-mini",
        "gemini" => "gemini-2.5-flash-lite",
        "openrouter" => "openrouter/free",
        "cerebras" => "gpt-oss-120b",
        "lmstudio" => "local-model",
        "cloudflare" => "@cf/qwen/qwen3-30b-a3b-fp8",
        "vercel" => "openai/gpt-4.1-mini",
        _ => "gpt-4.1-mini",
    }
}

fn preset_url(provider: &str) -> Option<&'static str> {
    match provider {
        "ollama" => Some("http://127.0.0.1:11434"),
        "groq" => Some("https://api.groq.com/openai/v1"),
        "openai" => Some("https://api.openai.com/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "cerebras" => Some("https://api.cerebras.ai/v1"),
        "lmstudio" => Some("http://127.0.0.1:1234/v1"),
        "vercel" => Some("https://ai-gateway.vercel.sh/v1"),
        _ => None,
    }
}

fn env_key_name(provider: &str) -> Option<&'static str> {
    match provider {
        "groq" => Some("GROQ_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "gemini" => Some("GEMINI_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "cerebras" => Some("CEREBRAS_API_KEY"),
        "cloudflare" => Some("CLOUDFLARE_API_KEY"),
        "vercel" => Some("VERCEL_AI_API_KEY"),
        _ => None,
    }
}

fn emit(out_lock: &Mutex<()>, obj: &Value) {
    let _g = out_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut stdout = io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, obj);
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

fn is_cancelled(state: &Mutex<SharedState>, req_id: u64) -> bool {
    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cancelled
        .contains(&req_id)
}

fn get_settings(state: &Mutex<SharedState>) -> HashMap<String, String> {
    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .settings
        .clone()
}

fn strip_think_tags(input: &str) -> String {
    let mut s = input.to_string();
    loop {
        let lower = s.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            break;
        };
        if let Some(rel_end) = lower[start..].find("</think>") {
            let end = start + rel_end + "</think>".len();
            s.replace_range(start..end, "");
        } else {
            s.truncate(start);
            break;
        }
    }
    s
}

fn unwrap_text_tag(input: &str) -> String {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<text>") && lower.ends_with("</text>") && trimmed.len() >= 13 {
        return trimmed[6..trimmed.len() - 7].trim().to_string();
    }
    trimmed.to_string()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn is_reasoning_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.contains("gpt-oss")
        || m.contains("qwen3")
        || m.contains("deepseek-r1")
        || m.contains("qwq")
        || m.contains("gemini-2.5")
        || m.contains("thinking")
        || m.contains("reasoner")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.contains("/o1")
        || m.contains("/o3")
        || m.contains("/o4")
}

fn rate_limit_remaining_secs(state: &Mutex<SharedState>) -> Option<u64> {
    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
    let until = st.rate_limited_until?;
    let now = Instant::now();
    if until > now {
        Some((until - now).as_secs().max(1))
    } else {
        st.rate_limited_until = None;
        None
    }
}

fn set_rate_limit_cooldown(state: &Mutex<SharedState>, secs: u64) {
    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
    let until = Instant::now() + Duration::from_secs(secs.clamp(2, 60));
    match st.rate_limited_until {
        Some(existing) if existing > until => {}
        _ => st.rate_limited_until = Some(until),
    }
}

fn parse_retry_after_secs(header: Option<&str>, data: &Value) -> u64 {
    if let Some(h) = header
        && let Ok(v) = h.trim().parse::<f64>()
        && v.is_finite()
        && v > 0.0
    {
        return (v.ceil() as u64).clamp(2, 60);
    }
    let detail = data
        .get("error")
        .and_then(|e| {
            e.get("message")
                .and_then(|m| m.as_str())
                .or_else(|| e.as_str())
        })
        .unwrap_or("");
    let lower = detail.to_ascii_lowercase();
    if let Some(idx) = lower.find("try again in ") {
        let rest = &lower[idx + "try again in ".len()..];
        let token = rest
            .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .next()
            .unwrap_or("")
            .trim_end_matches('.');
        let mut total = 0.0f64;
        let mut num = String::new();
        let mut chars = token.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
            } else if !num.is_empty() {
                let v = num.parse::<f64>().unwrap_or(0.0);
                num.clear();
                match c {
                    'm' if chars.peek() == Some(&'s') => {
                        let _ = chars.next();
                        total += v / 1000.0;
                    }
                    'm' => total += v * 60.0,
                    's' => total += v,
                    _ => {}
                }
            }
        }
        if total > 0.0 {
            return (total.ceil() as u64).clamp(2, 60);
        }
    }
    12
}

fn extract_error(provider: &str, status: u16, data: &Value, retry_secs: Option<u64>) -> String {
    let detail = data
        .get("error")
        .and_then(|e| {
            e.get("message")
                .and_then(|m| m.as_str())
                .or_else(|| e.as_str())
        })
        .unwrap_or("");
    match status {
        401 | 403 => format!("{provider} rejected the API key. Check Settings → Extensions."),
        404 => format!("{provider} could not find the selected model or endpoint."),
        429 => {
            if let Some(s) = retry_secs {
                format!("{provider} rate limit reached — try again in ~{s}s.")
            } else {
                format!("{provider} rate limit reached. Try again shortly.")
            }
        }
        _ if !detail.is_empty() => detail.to_string(),
        _ => format!("{provider} returned HTTP {status}."),
    }
}

fn post_json(
    url: &str,
    headers: &[(&str, String)],
    body: &Value,
    timeout_secs: u64,
) -> Result<(u16, Value, Option<String>), String> {
    let mut req = shared_agent()
        .post(url)
        .config()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build()
        .header("Content-Type", "application/json")
        .header("User-Agent", "Sillpaper-AI/0.2");
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }

    let mut resp = req
        .send_json(body)
        .map_err(|e| format!("Connection failed: {e}"))?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let raw = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {e}"))?;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "error": raw }));
    Ok((status, parsed, retry_after))
}

fn extract_choice_text(choice: &Value) -> String {
    let Some(content) = choice.get("message").and_then(|m| m.get("content")) else {
        return String::new();
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
            }
        }
        return out;
    }
    String::new()
}

struct LlmCallOpts {
    max_tokens: u32,
    temperature: f64,
    preserve_leading_space: bool,
    is_completion: bool,
}

fn call_llm(
    state: &Mutex<SharedState>,
    cfg: &HashMap<String, String>,
    system_prompt: &str,
    user_prompt: &str,
    opts: LlmCallOpts,
) -> Result<String, String> {
    let LlmCallOpts {
        max_tokens: base_max_tokens,
        temperature,
        preserve_leading_space,
        is_completion,
    } = opts;
    let provider = cfg
        .get("provider")
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ollama".to_string());

    let model = cfg
        .get("model")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_model(&provider))
        .to_string();

    let reasoning = is_reasoning_model(&model);
    let max_tokens = if reasoning {
        if is_completion {
            base_max_tokens.max(512)
        } else {
            base_max_tokens.max(1024)
        }
    } else {
        base_max_tokens
    };

    let mut api_key = cfg
        .get("api_key")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if api_key.is_empty()
        && let Some(env_name) = env_key_name(&provider)
        && let Ok(v) = std::env::var(env_name)
    {
        api_key = v.trim().to_string();
    }

    let base_url = cfg
        .get("base_url")
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .unwrap_or_default();
    let account_id = cfg
        .get("account_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let finish = |raw: &str| -> String {
        let cleaned = strip_think_tags(raw);
        if preserve_leading_space {
            cleaned.trim_end().to_string()
        } else {
            cleaned.trim().to_string()
        }
    };

    if provider == "ollama" {
        let root = if base_url.is_empty() {
            preset_url("ollama").unwrap()
        } else {
            base_url.as_str()
        };
        let url = format!("{root}/api/chat");
        let mut body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": false,
            "options": {
                "num_predict": max_tokens,
                "temperature": temperature
            }
        });
        if is_completion
            && model.to_ascii_lowercase().contains("qwen3")
            && let Some(obj) = body.as_object_mut()
        {
            obj.insert("think".into(), json!(false));
        }
        let (status, data, retry_after) = post_json(&url, &[], &body, 45)?;
        if status == 429 {
            let wait = parse_retry_after_secs(retry_after.as_deref(), &data);
            set_rate_limit_cooldown(state, wait);
            return Err(extract_error("Ollama", status, &data, Some(wait)));
        }
        if status >= 400 {
            return Err(extract_error("Ollama", status, &data, None));
        }
        let content = data
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");
        let out = finish(content);
        if out.trim().is_empty()
            && data.get("done_reason").and_then(|v| v.as_str()) == Some("length")
        {
            return Err("Model reached its output token limit while thinking. Try again or pick a faster model.".into());
        }
        return Ok(out);
    }

    if provider == "gemini" {
        if api_key.is_empty() {
            return Err(
                "Add a Gemini API key in Settings → Extensions (or set GEMINI_API_KEY).".into(),
            );
        }
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            percent_encode(&model),
            percent_encode(&api_key)
        );
        let mut gen_cfg = json!({
            "maxOutputTokens": max_tokens,
            "temperature": temperature
        });
        if is_completion
            && model.to_ascii_lowercase().contains("gemini-2.5")
            && let Some(obj) = gen_cfg.as_object_mut()
        {
            obj.insert("thinkingConfig".into(), json!({ "thinkingBudget": 0 }));
        }
        let body = json!({
            "systemInstruction": { "parts": [{ "text": system_prompt }] },
            "contents": [{ "role": "user", "parts": [{ "text": user_prompt }] }],
            "generationConfig": gen_cfg
        });
        let (status, data, retry_after) = post_json(&url, &[], &body, 35)?;
        if status == 429 {
            let wait = parse_retry_after_secs(retry_after.as_deref(), &data);
            set_rate_limit_cooldown(state, wait);
            return Err(extract_error("Gemini", status, &data, Some(wait)));
        }
        if status >= 400 {
            return Err(extract_error("Gemini", status, &data, None));
        }
        let mut combined = String::new();
        if let Some(parts) = data
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
        {
            for p in parts {
                if p.get("thought").and_then(|v| v.as_bool()) == Some(true) {
                    continue;
                }
                if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                    combined.push_str(t);
                }
            }
        }
        return Ok(finish(&combined));
    }

    let (url, mut headers): (String, Vec<(&str, String)>) = if provider == "cloudflare" {
        if api_key.is_empty() || account_id.is_empty() {
            return Err("Add Cloudflare API key and Account ID in Settings → Extensions.".into());
        }
        (
            format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1/chat/completions",
                percent_encode(&account_id)
            ),
            vec![("Authorization", format!("Bearer {api_key}"))],
        )
    } else {
        let resolved_base = if provider == "custom" || provider == "lmstudio" {
            if !base_url.is_empty() {
                base_url.as_str()
            } else {
                preset_url(&provider).unwrap_or("")
            }
        } else {
            preset_url(&provider).unwrap_or(base_url.as_str())
        };
        if resolved_base.is_empty() {
            return Err(format!(
                "Set a Server URL for '{provider}' in Settings → Extensions."
            ));
        }
        if matches!(
            provider.as_str(),
            "groq" | "openai" | "openrouter" | "cerebras" | "vercel"
        ) && api_key.is_empty()
        {
            return Err(format!(
                "Add a {provider} API key in Settings → Extensions."
            ));
        }
        let mut h = Vec::new();
        if !api_key.is_empty() {
            h.push(("Authorization", format!("Bearer {api_key}")));
        }
        if provider == "openrouter" {
            h.push(("X-Title", "Sillpaper".to_string()));
        }
        (format!("{resolved_base}/chat/completions"), h)
    };

    let mut body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": temperature
    });
    if let Some(obj) = body.as_object_mut() {
        let use_completion_tokens = provider == "cerebras"
            || (provider == "groq" && model.starts_with("openai/gpt-oss-"))
            || (provider == "openai" && reasoning);
        if use_completion_tokens {
            obj.insert("max_completion_tokens".into(), json!(max_tokens));
        } else {
            obj.insert("max_tokens".into(), json!(max_tokens));
        }
        if provider == "groq" && model.starts_with("openai/gpt-oss-") {
            obj.insert("reasoning_effort".into(), json!("low"));
            obj.insert("include_reasoning".into(), json!(false));
        }
    }

    let (status, data, retry_after) = post_json(&url, &headers, &body, 35)?;
    let _ = &mut headers;
    if status == 429 {
        let wait = parse_retry_after_secs(retry_after.as_deref(), &data);
        set_rate_limit_cooldown(state, wait);
        return Err(extract_error(&provider, status, &data, Some(wait)));
    }
    if status >= 400 {
        return Err(extract_error(&provider, status, &data, None));
    }
    let choice = data.get("choices").and_then(|c| c.get(0));
    let content = choice.map(extract_choice_text).unwrap_or_default();
    let out = finish(&content);
    if out.trim().is_empty()
        && choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|v| v.as_str())
            == Some("length")
    {
        return Err(
            "Model spent its output limit on reasoning. Retry or choose a faster non-reasoning model."
                .into(),
        );
    }
    Ok(out)
}

fn handle_completion(
    state: &Mutex<SharedState>,
    out_lock: &Mutex<()>,
    req_id: u64,
    params: &Value,
) -> Result<(), String> {
    let prefix = params.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
    let suffix = params.get("suffix").and_then(|v| v.as_str()).unwrap_or("");
    let manual = params
        .get("manual")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if prefix.trim().is_empty() {
        emit(
            out_lock,
            &json!({ "id": req_id, "result": { "suggestion": "" } }),
        );
        return Ok(());
    }
    if let Some(rem) = rate_limit_remaining_secs(state) {
        if manual {
            return Err(format!("Rate limit cooling down — try again in {rem}s."));
        }
        emit(
            out_lock,
            &json!({ "id": req_id, "result": { "suggestion": "" } }),
        );
        return Ok(());
    }
    let cfg = get_settings(state);
    let hint = cfg.get("language_hint").map(|s| s.trim()).unwrap_or("");
    let hint_line = if hint.is_empty() {
        String::new()
    } else {
        format!(" User preference: {hint}.")
    };
    let system = format!(
        "You are an unobtrusive inline writing autocomplete assistant. \
         Continue naturally from the exact end of <prefix> with one concise phrase (4 to 12 words). \
         Preserve the exact language and tone. Do NOT repeat any part of <prefix>. \
         Do NOT wrap in quotes, tags, or explanations. Return ONLY the continuation.{hint_line}"
    );
    let mut user = format!("<prefix>{prefix}</prefix>");
    if !suffix.trim().is_empty() {
        user.push_str(&format!("\n<suffix>{suffix}</suffix>"));
    }

    let mut out = call_llm(
        state,
        &cfg,
        &system,
        &user,
        LlmCallOpts {
            max_tokens: 96,
            temperature: 0.25,
            preserve_leading_space: true,
            is_completion: true,
        },
    )?;
    if is_cancelled(state, req_id) {
        return Ok(());
    }
    if out.trim().is_empty() {
        emit(
            out_lock,
            &json!({ "id": req_id, "result": { "suggestion": "" } }),
        );
        return Ok(());
    }

    for tag in ["text", "continuation", "completion"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let lower = out.trim().to_ascii_lowercase();
        if lower.starts_with(&open)
            && lower.ends_with(&close)
            && out.trim().len() >= open.len() + close.len()
        {
            let t = out.trim();
            out = t[open.len()..t.len() - close.len()].to_string();
        }
    }
    out = out
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_string();

    let trimmed = out.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        let leading_ws = out.starts_with(char::is_whitespace);
        out = trimmed[1..trimmed.len() - 1].to_string();
        if leading_ws {
            out.insert(0, ' ');
        }
    }

    let tail: String = prefix
        .chars()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if !tail.is_empty() && out.starts_with(&tail) {
        out = out[tail.len()..].to_string();
    } else if let Some(last_line) = prefix.lines().last()
        && !last_line.trim().is_empty()
        && out.starts_with(last_line)
    {
        out = out[last_line.len()..].to_string();
    }
    if out.trim().is_empty() {
        if !is_cancelled(state, req_id) {
            emit(
                out_lock,
                &json!({ "id": req_id, "result": { "suggestion": "" } }),
            );
        }
        return Ok(());
    }

    let last_ch = prefix.chars().last();
    let first_ch = out.chars().next();
    match (last_ch, first_ch) {
        (Some(l), Some(f)) if l.is_whitespace() && f.is_whitespace() => {
            out = out.trim_start().to_string();
        }
        (Some(l), Some(f))
            if matches!(l, '.' | '!' | '?' | ',' | ';' | ':' | ')' | ']' | '”' | '»')
                && (f.is_alphanumeric() || matches!(f, '(' | '"' | '„' | '“' | '«')) =>
        {
            out.insert(0, ' ');
        }
        _ => {}
    }

    if !is_cancelled(state, req_id) {
        emit(
            out_lock,
            &json!({ "id": req_id, "result": { "suggestion": out } }),
        );
    }
    Ok(())
}

fn handle_diagnostics(
    state: &Mutex<SharedState>,
    out_lock: &Mutex<()>,
    req_id: u64,
    params: &Value,
) -> Result<(), String> {
    let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let manual = params
        .get("manual")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if text.trim().chars().count() < 8 {
        emit(
            out_lock,
            &json!({ "id": req_id, "result": { "diagnostics": [] } }),
        );
        return Ok(());
    }
    if let Some(rem) = rate_limit_remaining_secs(state) {
        if manual {
            return Err(format!("Rate limit cooling down — try again in {rem}s."));
        }
        return Ok(());
    }
    let cfg = get_settings(state);
    let hint = cfg.get("language_hint").map(|s| s.trim()).unwrap_or("");
    let hint_line = if hint.is_empty() {
        String::new()
    } else {
        format!(" Language/style hint: {hint}.")
    };
    let system = format!(
        "You are a careful copy editor checking a single paragraph for clear spelling, grammar, \
         or punctuation errors. Keep the author's language and voice; only flag genuine issues \
         (maximum 5). Return ONLY a valid JSON array of objects with keys: \
         \"original\" (exact verbatim substring from the paragraph), \
         \"replacement\" (corrected substring), \
         \"message\" (brief 3-10 word explanation), \
         \"severity\" (\"warning\"). If there are no clear issues, return [].{hint_line}"
    );
    let user = format!("<text>\n{text}\n</text>");
    let raw = call_llm(
        state,
        &cfg,
        &system,
        &user,
        LlmCallOpts {
            max_tokens: 512,
            temperature: 0.1,
            preserve_leading_space: false,
            is_completion: false,
        },
    )?;
    if is_cancelled(state, req_id) {
        return Ok(());
    }

    let diagnostics = parse_diagnostics_aligned(text, &raw);
    emit(
        out_lock,
        &json!({ "id": req_id, "result": { "diagnostics": diagnostics } }),
    );
    Ok(())
}

fn parse_diagnostics_aligned(text: &str, raw_json: &str) -> Vec<DiagnosticOut> {
    let Some(lb) = raw_json.find('[') else {
        return Vec::new();
    };
    let Some(rb) = raw_json.rfind(']') else {
        return Vec::new();
    };
    if rb <= lb {
        return Vec::new();
    }
    let Ok(items) = serde_json::from_str::<Vec<RawDiagnostic>>(&raw_json[lb..=rb]) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut used_ranges: Vec<(usize, usize)> = Vec::new();

    for item in items {
        let Some(orig) = item.original.filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(msg) = item
            .message
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if item.replacement.as_deref() == Some(orig.as_str()) {
            continue;
        }

        let mut search_from = 0usize;
        let mut found_chars: Option<(usize, usize)> = None;
        while search_from < text.len() {
            let Some(rel) = text[search_from..].find(&orig) else {
                break;
            };
            let byte_start = search_from + rel;
            let char_start = text[..byte_start].chars().count();
            let char_end = char_start + orig.chars().count();
            let overlaps = used_ranges
                .iter()
                .any(|&(a, b)| !(char_end <= a || char_start >= b));
            if !overlaps {
                found_chars = Some((char_start, char_end));
                break;
            }
            let step = text[byte_start..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            search_from = byte_start + step;
        }

        if let Some((start, end)) = found_chars {
            used_ranges.push((start, end));
            out.push(DiagnosticOut {
                start,
                end,
                severity: item.severity.unwrap_or_else(|| "warning".to_string()),
                message: msg,
                replacement: item.replacement,
            });
            if out.len() >= 5 {
                break;
            }
        }
    }
    out
}

fn handle_action(
    state: &Mutex<SharedState>,
    out_lock: &Mutex<()>,
    req_id: u64,
    params: &Value,
) -> Result<(), String> {
    let action_id = params
        .get("action_id")
        .and_then(|v| v.as_str())
        .unwrap_or("correct");
    let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let custom_prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if text.trim().is_empty() {
        return Ok(());
    }

    let cfg = get_settings(state);
    let hint = cfg.get("language_hint").map(|s| s.trim()).unwrap_or("");
    let hint_line = if hint.is_empty() {
        String::new()
    } else {
        format!("\nUser preference: {hint}")
    };
    let md_guidance = "\nPreserve any existing Markdown formatting (bold, italic, links, lists) \
        unless asked to change it, and do not wrap the output in code fences.";

    let base_prompt = match action_id {
        "correct" => "Correct grammar, spelling, punctuation, clarity, and style. \
             Preserve the language, meaning, and tone. Return only the corrected text, \
             unchanged if already correct."
            .to_string(),
        "rewrite" => {
            "Rewrite for clarity and natural flow. Preserve the language, meaning, and tone. \
             Add no new ideas or commentary. Return only the improved text."
                .to_string()
        }
        "concise" => {
            "Tighten the text to be clearer and more concise while preserving all key facts, \
             language, and tone. Return only the tightened text."
                .to_string()
        }
        _ => {
            let inst = if custom_prompt.is_empty() {
                "Improve this text."
            } else {
                custom_prompt
            };
            format!(
                "Follow this instruction precisely: {inst}\n\
                 Return only the resulting text without introductory commentary."
            )
        }
    };

    let system = format!("{base_prompt}{md_guidance}{hint_line}");
    let user = format!(
        "Transform only the text inside the tags.\nReturn only the transformed text.\n<text>\n{text}\n</text>"
    );
    if let Some(rem) = rate_limit_remaining_secs(state) {
        return Err(format!("Rate limit cooling down — try again in {rem}s."));
    }
    let est_tokens = ((text.len() / 2) + 256).clamp(512, 2048) as u32;
    let out = call_llm(
        state,
        &cfg,
        &system,
        &user,
        LlmCallOpts {
            max_tokens: est_tokens,
            temperature: 0.35,
            preserve_leading_space: false,
            is_completion: false,
        },
    )?;
    if is_cancelled(state, req_id) {
        return Ok(());
    }
    emit(
        out_lock,
        &json!({ "id": req_id, "result": { "replacement": unwrap_text_tag(&out) } }),
    );
    Ok(())
}

fn main() {
    tune_allocator();
    let state = Arc::new(Mutex::new(SharedState::default()));
    let out_lock = Arc::new(Mutex::new(()));
    let mut handles: Vec<thread::JoinHandle<()>> = Vec::new();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let method = msg
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let req_id = msg.get("id").and_then(|v| v.as_u64());
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

        if method == "config/update" {
            if let Some(obj) = params.get("settings").and_then(|v| v.as_object()) {
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                st.settings.clear();
                st.rate_limited_until = None;
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        st.settings.insert(k.clone(), s.to_string());
                    }
                }
            }
            if let Some(id) = req_id {
                emit(&out_lock, &json!({ "id": id, "result": { "ok": true } }));
            }
            continue;
        }

        if method == "cancel" {
            if let Some(cid) = params.get("id").and_then(|v| v.as_u64()) {
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                if st.cancelled.len() > 256 {
                    st.cancelled.clear();
                }
                st.cancelled.insert(cid);
            }
            continue;
        }

        let Some(id) = req_id else { continue };
        handles.retain(|h| !h.is_finished());
        let st_c = state.clone();
        let out_c = out_lock.clone();
        if let Ok(handle) = thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                if is_cancelled(&st_c, id) {
                    return;
                }
                let res = match method.as_str() {
                    "completion/request" => handle_completion(&st_c, &out_c, id, &params),
                    "diagnostics/request" => handle_diagnostics(&st_c, &out_c, id, &params),
                    "action/run" => handle_action(&st_c, &out_c, id, &params),
                    _ => Ok(()),
                };
                if let Err(err) = res
                    && !is_cancelled(&st_c, id)
                {
                    emit(&out_c, &json!({ "id": id, "error": err }));
                }
                trim_allocator();
            })
        {
            handles.push(handle);
        }
    }

    for h in handles {
        let _ = h.join();
    }
}
