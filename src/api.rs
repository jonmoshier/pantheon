use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;

pub enum StreamEvent {
    Delta(String),
    ApiHistory(Vec<Value>),
    ConfirmRequest(String),
    ModelResolved(String),
    Done,
    Error(String),
}

// ── Anthropic tool call (collected during streaming) ─────────────────────────

struct AnthropicToolCall {
    id: String,
    name: String,
    input_json: String,
}

// ── OpenAI tool call (collected during streaming) ─────────────────────────────

struct OpenAiToolCall {
    id: String,
    name: String,
    arguments: String,
}

// ── Path sandboxing ───────────────────────────────────────────────────────────

/// Resolves `path` and checks it falls inside CWD (or a subdir).
/// For existing paths uses `canonicalize`; for new files canonicalizes the parent.
/// Returns the resolved path string, or an error message to return to the model.
fn sandbox_path(path: &str) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("error: cannot get cwd: {}", e))?;
    let p = std::path::Path::new(path);
    let resolved = if p.exists() {
        p.canonicalize().map_err(|e| format!("error: {}", e))?
    } else {
        let parent = p.parent().unwrap_or(std::path::Path::new("."));
        let canon_parent = if parent == std::path::Path::new("") {
            cwd.clone()
        } else {
            parent.canonicalize().map_err(|e| format!("error: {}", e))?
        };
        canon_parent.join(p.file_name().unwrap_or_default())
    };
    if resolved.starts_with(&cwd) {
        Ok(resolved.to_string_lossy().to_string())
    } else {
        Err(format!(
            "error: path '{}' is outside the working directory ({}). Only paths within {} are accessible. Ask the user if you need access elsewhere.",
            path,
            cwd.display(),
            cwd.display(),
        ))
    }
}

// ── SSRF protection ───────────────────────────────────────────────────────────

fn is_blocked_ipv4(v4: std::net::Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_unspecified()
}

fn is_blocked_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_blocked_ipv4(v4),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00  // fc00::/7 unique-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80  // fe80::/10 link-local
                || v6.to_ipv4_mapped().is_some_and(is_blocked_ipv4)
        }
    }
}

async fn check_ssrf(url_str: &str) -> Result<(), String> {
    let url = url::Url::parse(url_str).map_err(|e| format!("error: invalid URL: {}", e))?;

    match url.scheme() {
        "http" | "https" => {}
        s => {
            return Err(format!(
                "error: scheme '{}' not allowed — only http/https",
                s
            ))
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| "error: URL has no host".to_string())?;

    let ips: Vec<std::net::IpAddr> = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        vec![ip]
    } else {
        let port = url.port_or_known_default().unwrap_or(80);
        tokio::net::lookup_host(format!("{}:{}", host, port))
            .await
            .map_err(|e| format!("error: DNS lookup failed: {}", e))?
            .map(|s| s.ip())
            .collect()
    };

    for ip in ips {
        if is_blocked_ip(ip) {
            return Err(format!(
                "error: '{}' resolves to a private/reserved address — access denied",
                host
            ));
        }
    }

    Ok(())
}

// ── Shared tool execution ─────────────────────────────────────────────────────

fn validate_tool_input(name: &str, input: &Value) -> Result<(), String> {
    let required: &[&str] = match name {
        "read_file" => &["path"],
        "write_file" => &["path", "content"],
        "append_file" => &["path", "content"],
        "list_dir" => &["path"],
        "search_files" => &["path", "pattern"],
        "run_shell" => &["command"],
        "fetch_url" => &["url"],
        "delegate" => &["task"],
        other => return Err(format!("unknown tool: {other}")),
    };
    for field in required {
        if input[field].is_null() {
            return Err(format!("tool '{name}' missing required field '{field}'"));
        }
    }
    Ok(())
}

async fn run_tool(
    name: &str,
    input: &Value,
    tx: &mpsc::Sender<StreamEvent>,
    confirm_rx: &mut mpsc::Receiver<bool>,
) -> String {
    if let Err(msg) = validate_tool_input(name, input) {
        return format!("Error: {msg}. Check the required fields and try again.");
    }
    match name {
        "read_file" => {
            let path = input["path"].as_str().unwrap_or("");
            let path = match sandbox_path(path) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let desc = format!("read file: {}", path);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    tx.send(StreamEvent::Delta(format!(
                        "← _read {} bytes_\n\n",
                        content.len()
                    )))
                    .await
                    .ok();
                    content
                }
                Err(e) => {
                    let msg = format!("error: {}", e);
                    tx.send(StreamEvent::Delta(format!("← _{}_\n\n", msg)))
                        .await
                        .ok();
                    msg
                }
            }
        }
        "write_file" => {
            let path = input["path"].as_str().unwrap_or("");
            let path = match sandbox_path(path) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let content = input["content"].as_str().unwrap_or("");
            let desc = format!("write {} bytes → {}", content.len(), path);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            let old_content = std::fs::read_to_string(&path).unwrap_or_default();
            match std::fs::write(&path, content) {
                Ok(_) => {
                    let diff = diff_files(&old_content, content);
                    let summary = if diff.is_empty() {
                        format!("← _wrote {} bytes (no changes)_\n\n", content.len())
                    } else {
                        format!("← _wrote {} bytes_\n\n{}\n\n", content.len(), diff)
                    };
                    tx.send(StreamEvent::Delta(summary)).await.ok();
                    "ok".to_string()
                }
                Err(e) => {
                    let msg = format!("error: {}", e);
                    tx.send(StreamEvent::Delta(format!("← _{}_\n\n", msg)))
                        .await
                        .ok();
                    msg
                }
            }
        }
        "append_file" => {
            let path = input["path"].as_str().unwrap_or("");
            let path = match sandbox_path(path) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let content = input["content"].as_str().unwrap_or("");
            let desc = format!("append {} bytes → {}", content.len(), path);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            use std::io::Write as _;
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(mut file) => match file.write_all(content.as_bytes()) {
                    Ok(_) => {
                        tx.send(StreamEvent::Delta(format!(
                            "← _appended {} bytes_\n\n",
                            content.len()
                        )))
                        .await
                        .ok();
                        "ok".to_string()
                    }
                    Err(e) => format!("error: {}", e),
                },
                Err(e) => format!("error: {}", e),
            }
        }
        "list_dir" => {
            let path = input["path"].as_str().unwrap_or(".");
            let path = match sandbox_path(path) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let desc = format!("list dir: {}", path);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                format!("{}/", name)
                            } else {
                                name
                            }
                        })
                        .collect();
                    names.sort();
                    tx.send(StreamEvent::Delta(format!(
                        "← _listed {} entries_\n\n",
                        names.len()
                    )))
                    .await
                    .ok();
                    names.join("\n")
                }
                Err(e) => format!("error: {}", e),
            }
        }
        "search_files" => {
            let path = input["path"].as_str().unwrap_or(".");
            let path = match sandbox_path(path) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let pattern = input["pattern"].as_str().unwrap_or("");
            let desc = format!("search '{}' in {}", pattern, path);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            let cmd = format!(
                "grep -rn --include='*' '{}' '{}' 2>/dev/null | head -200",
                pattern.replace('\'', "'\\''"),
                path.replace('\'', "'\\''"),
            );
            match tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .await
            {
                Ok(out) => {
                    let result = String::from_utf8_lossy(&out.stdout).to_string();
                    tx.send(StreamEvent::Delta(format!(
                        "← _search: {} bytes_\n\n",
                        result.len()
                    )))
                    .await
                    .ok();
                    if result.is_empty() {
                        "no matches".into()
                    } else {
                        result
                    }
                }
                Err(e) => format!("error: {}", e),
            }
        }
        "run_shell" => {
            let command = input["command"].as_str().unwrap_or("");
            let desc = format!("$ {}", command);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }

            let mut child = match tokio::process::Command::new("sh")
                .arg("-c")
                .arg(format!("({}) 2>&1", command))
                .stdout(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => return format!("error: {}", e),
            };

            let stdout = child.stdout.take().unwrap();
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            let mut output = String::new();
            let mut truncated = false;

            while let Ok(Some(line)) = lines.next_line().await {
                if output.len() >= 20_000 {
                    truncated = true;
                    break;
                }
                tx.send(StreamEvent::Delta(format!("{}\n", line)))
                    .await
                    .ok();
                output.push_str(&line);
                output.push('\n');
            }

            let exit_code = child
                .wait()
                .await
                .map(|s| s.code().unwrap_or(-1))
                .unwrap_or(-1);

            let suffix = if truncated { " (truncated)" } else { "" };
            tx.send(StreamEvent::Delta(format!(
                "\n← _exit {}{}_\n\n",
                exit_code, suffix
            )))
            .await
            .ok();

            output
        }
        "fetch_url" => {
            let url = input["url"].as_str().unwrap_or("");
            if let Err(e) = check_ssrf(url).await {
                return e;
            }
            let desc = format!("fetch {}", url);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }
            let client = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default();
            match client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            let truncated = truncate(&body, 50_000);
                            tx.send(StreamEvent::Delta(format!(
                                "← _HTTP {}, {} bytes_\n\n",
                                status,
                                truncated.len()
                            )))
                            .await
                            .ok();
                            truncated
                        }
                        Err(e) => format!("error reading body: {}", e),
                    }
                }
                Err(e) => format!("error: {}", e),
            }
        }
        "delegate" => {
            let task = input["task"].as_str().unwrap_or("");
            let directory = input["directory"].as_str();
            let desc = format!("delegate to Claude Code: {}", task);
            if !prompt_confirm(&desc, tx, confirm_rx).await {
                return "Action denied by user. Ask what they would like you to do instead, or try a different approach.".into();
            }

            let mut cmd = tokio::process::Command::new("claude");
            cmd.arg("--output-format")
                .arg("stream-json")
                .arg("--verbose")
                .arg("--print")
                .arg(task)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            if let Some(dir) = directory {
                cmd.current_dir(dir);
            }

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => return format!("error: could not start claude CLI: {}", e),
            };

            let stdout = child.stdout.take().unwrap();
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            let mut final_result = String::new();

            while let Ok(Some(line)) = lines.next_line().await {
                let v: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match v["type"].as_str().unwrap_or("") {
                    "assistant" => {
                        if let Some(content) = v["message"]["content"].as_array() {
                            for block in content {
                                match block["type"].as_str().unwrap_or("") {
                                    "text" => {
                                        if let Some(text) = block["text"].as_str() {
                                            tx.send(StreamEvent::Delta(text.to_string()))
                                                .await
                                                .ok();
                                        }
                                    }
                                    "tool_use" => {
                                        let name = block["name"].as_str().unwrap_or("tool");
                                        tx.send(StreamEvent::Delta(format!("\n→ **{}**\n", name)))
                                            .await
                                            .ok();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "result" => {
                        if let Some(r) = v["result"].as_str() {
                            final_result = r.to_string();
                        }
                    }
                    _ => {}
                }
            }

            let status = child.wait().await;
            let exit_ok = status.map(|s| s.success()).unwrap_or(false);
            if !exit_ok && final_result.is_empty() {
                // Read stderr for error info
                "claude CLI exited with an error".to_string()
            } else if final_result.is_empty() {
                "done".to_string()
            } else {
                final_result
            }
        }
        _ => format!("unknown tool: {}", name),
    }
}

// ── Claude Code provider ──────────────────────────────────────────────────────

pub async fn stream_claude_code(msgs: Vec<Value>, tx: mpsc::Sender<StreamEvent>) {
    // Build a plain transcript from the message history and pass it as the prompt.
    // claude --print is stateless, so we reconstruct context each turn.
    let mut transcript = String::new();
    let n = msgs.len();
    for (i, msg) in msgs.iter().enumerate() {
        let role = msg["role"].as_str().unwrap_or("user");
        let content = msg["content"].as_str().unwrap_or("");
        if i < n - 1 {
            // Prior turns: include as transcript context
            let label = if role == "user" { "User" } else { "Assistant" };
            transcript.push_str(&format!("{}: {}\n\n", label, content));
        } else {
            // Final (current) user message: append after transcript
            if !transcript.is_empty() {
                transcript.push_str(&format!("User: {}", content));
            } else {
                transcript = content.to_string();
            }
        }
    }

    if transcript.is_empty() {
        let _ = tx
            .send(StreamEvent::Error("no message to send".into()))
            .await;
        return;
    }

    let mut child = match tokio::process::Command::new("claude")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--permission-mode")
        .arg("default")
        .arg("--print")
        .arg(&transcript)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx
                .send(StreamEvent::Error(format!(
                    "could not start claude CLI: {}",
                    e
                )))
                .await;
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v["type"].as_str().unwrap_or("") {
            "assistant" => {
                if let Some(content) = v["message"]["content"].as_array() {
                    for block in content {
                        if block["type"].as_str() == Some("text") {
                            if let Some(text) = block["text"].as_str() {
                                if tx.send(StreamEvent::Delta(text.to_string())).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            "result" => {
                // result event marks completion; the text was already streamed above
            }
            _ => {}
        }
    }

    let _ = child.wait().await;
    let _ = tx.send(StreamEvent::Done).await;
}

async fn prompt_confirm(
    desc: &str,
    tx: &mpsc::Sender<StreamEvent>,
    confirm_rx: &mut mpsc::Receiver<bool>,
) -> bool {
    tx.send(StreamEvent::ConfirmRequest(desc.to_string()))
        .await
        .ok();
    confirm_rx.recv().await.unwrap_or(false)
}

fn diff_files(old: &str, new: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    let mut changed = false;
    for group in diff.grouped_ops(3) {
        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    ChangeTag::Delete => {
                        out.push_str(&format!("- {}", change.value()));
                        changed = true;
                    }
                    ChangeTag::Insert => {
                        out.push_str(&format!("+ {}", change.value()));
                        changed = true;
                    }
                    ChangeTag::Equal => {
                        out.push_str(&format!("  {}", change.value()));
                    }
                }
            }
        }
    }
    if !changed {
        return String::new();
    }
    let out = truncate(&out, 4_000);
    format!("```diff\n{}\n```", out.trim_end())
}

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        format!("{}\n... (truncated)", &s[..max_bytes])
    }
}

fn tool_hint(name: &str, input: &Value) -> String {
    match name {
        "read_file" | "write_file" | "append_file" | "list_dir" => {
            input["path"].as_str().unwrap_or("?").to_string()
        }
        "run_shell" => input["command"].as_str().unwrap_or("?").to_string(),
        "fetch_url" => input["url"].as_str().unwrap_or("?").to_string(),
        "search_files" => format!(
            "'{}' in {}",
            input["pattern"].as_str().unwrap_or("?"),
            input["path"].as_str().unwrap_or("?")
        ),
        "delegate" => input["task"].as_str().unwrap_or("?").to_string(),
        _ => String::new(),
    }
}

// ── Tool definitions ──────────────────────────────────────────────────────────

fn anthropic_tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "read_file",
            "description": "Read the contents of a file at the given path.",
            "input_schema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "write_file",
            "description": "Write content to a file, creating or overwriting it.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path":    { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "append_file",
            "description": "Append content to the end of a file, creating it if it doesn't exist.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path":    { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "list_dir",
            "description": "List files and directories at the given path.",
            "input_schema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        }),
        json!({
            "name": "search_files",
            "description": "Search for a text pattern in files under a directory. Returns matching lines with file paths and line numbers.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path":    { "type": "string" },
                    "pattern": { "type": "string" }
                },
                "required": ["path", "pattern"]
            }
        }),
        json!({
            "name": "run_shell",
            "description": "Execute a shell command and return stdout and stderr. Requires user approval.",
            "input_schema": {
                "type": "object",
                "properties": { "command": { "type": "string" } },
                "required": ["command"]
            }
        }),
        json!({
            "name": "fetch_url",
            "description": "Fetch the contents of a URL via HTTP GET. Requires user approval.",
            "input_schema": {
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"]
            }
        }),
        json!({
            "name": "delegate",
            "description": "Delegate a complex task to Claude Code, a capable sub-agent with full file system access, shell execution, and multi-step reasoning. Use this for coding tasks, refactors, debugging, or anything requiring many tool calls. Claude Code will work autonomously and return a summary when done.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "A clear description of the task for Claude Code to complete."
                    },
                    "directory": {
                        "type": "string",
                        "description": "Working directory for the task. Defaults to the current directory if omitted."
                    }
                },
                "required": ["task"]
            }
        }),
    ]
}

fn openai_tool_defs() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"read_file","description":"Read the contents of a file at the given path.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}),
        json!({"type":"function","function":{"name":"write_file","description":"Write content to a file, creating or overwriting it.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}}}),
        json!({"type":"function","function":{"name":"append_file","description":"Append content to the end of a file.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}}}),
        json!({"type":"function","function":{"name":"list_dir","description":"List files and directories at the given path.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}),
        json!({"type":"function","function":{"name":"search_files","description":"Search for a text pattern in files under a directory.","parameters":{"type":"object","properties":{"path":{"type":"string"},"pattern":{"type":"string"}},"required":["path","pattern"]}}}),
        json!({"type":"function","function":{"name":"run_shell","description":"Execute a shell command and return stdout and stderr.","parameters":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}}}),
        json!({"type":"function","function":{"name":"fetch_url","description":"Fetch the contents of a URL via HTTP GET.","parameters":{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}}}),
        json!({"type":"function","function":{"name":"delegate","description":"Delegate a complex task to Claude Code, a capable sub-agent with full file system access and multi-step reasoning.","parameters":{"type":"object","properties":{"task":{"type":"string"},"directory":{"type":"string"}},"required":["task"]}}}),
    ]
}

// ── System prompt ─────────────────────────────────────────────────────────────

fn system_prompt(custom_path: Option<&str>) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let home = std::env::var("HOME").unwrap_or_default();

    let default = format!(
        "You are a helpful assistant running in a terminal.\n\
         File and directory tools are sandboxed to the current working directory: {cwd}\n\
         Do not attempt to access paths outside {cwd}. If a task requires files outside this \
         directory, ask the user to change directory or grant access rather than trying anyway.\n\n\
         Tool use guidelines:\n\
         - Every tool call requires explicit user confirmation before it executes. Minimize \
           the number of tool calls you make — batch related work into a single call where \
           possible, and avoid redundant reads or exploratory calls you don't strictly need.\n\
         - For multi-step coding tasks (edit multiple files, run tests, apply fixes), prefer \
           the `delegate` tool over issuing many individual tool calls. `delegate` lets you \
           describe the full task and handles it end-to-end with fewer interruptions.\n\
         - Do not call tools speculatively. If you are unsure whether a file exists or a \
           command will succeed, say so and ask the user rather than probing with tool calls."
    );

    // Load global user prompt: -s override or ~/.pantheon/system_prompt.md
    let global_path = custom_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(&home).join(".pantheon/system_prompt.md"));
    let global = std::fs::read_to_string(&global_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Load project prompt: ./.pantheon/system_prompt.md
    let project_path = std::path::Path::new(&cwd).join(".pantheon/system_prompt.md");
    let project = std::fs::read_to_string(&project_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Build prompt: project (most specific) → global → default
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = project {
        parts.push(p);
    }
    if let Some(g) = global {
        parts.push(g);
    }
    parts.push(default);
    let mut prompt = parts.join("\n\n");

    // Append project context files
    let cwd_path = std::path::Path::new(&cwd);
    for name in &["CLAUDE.md", "README.md", ".pantheon/context.md"] {
        let path = cwd_path.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let truncated = truncate(&content, 8_000);
            prompt.push_str(&format!("\n\n## {}\n{}", name, truncated));
        }
    }

    prompt
}

// ── Anthropic streaming ───────────────────────────────────────────────────────

pub async fn stream_anthropic(
    api_key: String,
    model: String,
    messages: Vec<Value>,
    tx: mpsc::Sender<StreamEvent>,
    mut confirm_rx: mpsc::Receiver<bool>,
    system_prompt_path: Option<String>,
) {
    if let Err(e) = anthropic_loop(
        api_key,
        model,
        messages,
        &tx,
        &mut confirm_rx,
        system_prompt_path.as_deref(),
    )
    .await
    {
        tx.send(StreamEvent::Error(e.to_string())).await.ok();
    }
}

async fn anthropic_loop(
    api_key: String,
    model: String,
    initial_messages: Vec<Value>,
    tx: &mpsc::Sender<StreamEvent>,
    confirm_rx: &mut mpsc::Receiver<bool>,
    system_prompt_path: Option<&str>,
) -> anyhow::Result<()> {
    let client = Client::new();
    let mut all_messages = initial_messages;
    let mut new_msgs: Vec<Value> = Vec::new();
    let system = system_prompt(system_prompt_path);
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > 10 {
            tx.send(StreamEvent::Error(
                "tool call loop exceeded 10 iterations — aborting".to_string(),
            ))
            .await
            .ok();
            break;
        }

        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": model,
                "max_tokens": 8096,
                "system": system,
                "messages": all_messages,
                "tools": anthropic_tool_defs(),
                "stream": true,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tx.send(StreamEvent::Error(format!("{}: {}", status, body)))
                .await
                .ok();
            return Ok(());
        }

        let (text, calls) = collect_anthropic_stream(resp, tx).await?;

        if calls.is_empty() {
            if text.is_empty() {
                tx.send(StreamEvent::Error(
                    "model returned empty response".to_string(),
                ))
                .await
                .ok();
            } else {
                new_msgs.push(json!({"role": "assistant", "content": text}));
            }
            break;
        }

        let mut content: Vec<Value> = Vec::new();
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
        for tc in &calls {
            let input: Value = serde_json::from_str(&tc.input_json).unwrap_or(Value::Null);
            content.push(json!({"type": "tool_use", "id": tc.id, "name": tc.name, "input": input}));
        }
        let assistant_msg = json!({"role": "assistant", "content": content});
        all_messages.push(assistant_msg.clone());
        new_msgs.push(assistant_msg);

        let mut results: Vec<Value> = Vec::new();
        for tc in &calls {
            let input: Value = serde_json::from_str(&tc.input_json).unwrap_or(Value::Null);
            let output = run_tool(&tc.name, &input, tx, confirm_rx).await;
            results.push(json!({"type": "tool_result", "tool_use_id": tc.id, "content": output}));
        }
        let results_msg = json!({"role": "user", "content": results});
        all_messages.push(results_msg.clone());
        new_msgs.push(results_msg);
    }

    tx.send(StreamEvent::ApiHistory(new_msgs)).await.ok();
    tx.send(StreamEvent::Done).await.ok();
    Ok(())
}

async fn collect_anthropic_stream(
    resp: reqwest::Response,
    tx: &mpsc::Sender<StreamEvent>,
) -> anyhow::Result<(String, Vec<AnthropicToolCall>)> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut text = String::new();
    let mut calls: Vec<AnthropicToolCall> = Vec::new();
    let mut cur_tool: Option<usize> = None;

    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos + 1..].to_string();

            let data = match line.strip_prefix("data: ") {
                Some(d) => d.to_string(),
                None => continue,
            };
            let v: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            match v["type"].as_str().unwrap_or("") {
                "content_block_start" => {
                    let block = &v["content_block"];
                    if block["type"] == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        tx.send(StreamEvent::Delta(format!("\n→ **{}**", name)))
                            .await
                            .ok();
                        cur_tool = Some(calls.len());
                        calls.push(AnthropicToolCall {
                            id,
                            name,
                            input_json: String::new(),
                        });
                    } else {
                        cur_tool = None;
                    }
                }
                "content_block_delta" => {
                    let delta = &v["delta"];
                    match delta["type"].as_str().unwrap_or("") {
                        "text_delta" => {
                            if let Some(t) = delta["text"].as_str() {
                                text.push_str(t);
                                tx.send(StreamEvent::Delta(t.to_string())).await.ok();
                            }
                        }
                        "input_json_delta" => {
                            if let Some(i) = cur_tool {
                                if let Some(p) = delta["partial_json"].as_str() {
                                    calls[i].input_json.push_str(p);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    if let Some(i) = cur_tool {
                        let input: Value =
                            serde_json::from_str(&calls[i].input_json).unwrap_or(Value::Null);
                        tx.send(StreamEvent::Delta(format!(
                            " `{}`\n",
                            tool_hint(&calls[i].name, &input)
                        )))
                        .await
                        .ok();
                    }
                    cur_tool = None;
                }
                _ => {}
            }
        }
    }

    Ok((text, calls))
}

// ── OpenAI-compatible streaming ───────────────────────────────────────────────

pub async fn stream_openai_compat(
    base_url: String,
    api_key: String,
    model: String,
    messages: Vec<Value>,
    tx: mpsc::Sender<StreamEvent>,
    mut confirm_rx: mpsc::Receiver<bool>,
    system_prompt_path: Option<String>,
) {
    if let Err(e) = openai_loop(
        base_url,
        api_key,
        model,
        messages,
        &tx,
        &mut confirm_rx,
        system_prompt_path.as_deref(),
    )
    .await
    {
        tx.send(StreamEvent::Error(e.to_string())).await.ok();
    }
}

async fn openai_loop(
    base_url: String,
    api_key: String,
    model: String,
    initial_messages: Vec<Value>,
    tx: &mpsc::Sender<StreamEvent>,
    confirm_rx: &mut mpsc::Receiver<bool>,
    system_prompt_path: Option<&str>,
) -> anyhow::Result<()> {
    let client = Client::new();
    let mut all_messages =
        vec![json!({"role": "system", "content": system_prompt(system_prompt_path)})];
    all_messages.extend(initial_messages);
    let mut new_msgs: Vec<Value> = Vec::new();
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > 10 {
            tx.send(StreamEvent::Error(
                "tool call loop exceeded 10 iterations — aborting".to_string(),
            ))
            .await
            .ok();
            break;
        }

        let resp = client
            .post(format!("{}/chat/completions", base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&json!({
                "model": model,
                "messages": all_messages,
                "tools": openai_tool_defs(),
                "stream": true,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tx.send(StreamEvent::Error(format!("{}: {}", status, body)))
                .await
                .ok();
            return Ok(());
        }

        let (text, calls) = collect_openai_stream(resp, tx).await?;

        if calls.is_empty() {
            if text.is_empty() {
                tx.send(StreamEvent::Error(
                    "model returned empty response".to_string(),
                ))
                .await
                .ok();
            } else {
                new_msgs.push(json!({"role": "assistant", "content": text}));
            }
            break;
        }

        let tool_calls_json: Vec<Value> = calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments }
                })
            })
            .collect();

        let assistant_msg = json!({
            "role": "assistant",
            "content": if text.is_empty() { Value::Null } else { Value::String(text) },
            "tool_calls": tool_calls_json,
        });
        all_messages.push(assistant_msg.clone());
        new_msgs.push(assistant_msg);

        for tc in &calls {
            let args: Value = match serde_json::from_str(&tc.arguments) {
                Ok(v) => v,
                Err(e) => {
                    tx.send(StreamEvent::Error(format!(
                        "failed to parse tool arguments for '{}': {} — raw: {}",
                        tc.name, e, tc.arguments
                    )))
                    .await
                    .ok();
                    break;
                }
            };
            let output = run_tool(&tc.name, &args, tx, confirm_rx).await;
            let tool_msg = json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": output,
            });
            all_messages.push(tool_msg.clone());
            new_msgs.push(tool_msg);
        }
    }

    tx.send(StreamEvent::ApiHistory(new_msgs)).await.ok();
    tx.send(StreamEvent::Done).await.ok();
    Ok(())
}

async fn collect_openai_stream(
    resp: reqwest::Response,
    tx: &mpsc::Sender<StreamEvent>,
) -> anyhow::Result<(String, Vec<OpenAiToolCall>)> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut text = String::new();
    let mut calls: Vec<OpenAiToolCall> = Vec::new();
    let mut model_resolved = false;

    'outer: while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos + 1..].to_string();

            let data = match line.strip_prefix("data: ") {
                Some(d) => d,
                None => continue,
            };

            if data == "[DONE]" {
                break 'outer;
            }

            let v: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(e) => {
                    tx.send(StreamEvent::Error(format!(
                        "stream parse error: {}: {}",
                        e, data
                    )))
                    .await
                    .ok();
                    continue;
                }
            };

            if !model_resolved {
                if let Some(model_id) = v["model"].as_str() {
                    tx.send(StreamEvent::ModelResolved(model_id.to_string()))
                        .await
                        .ok();
                    model_resolved = true;
                }
            }

            let choice = &v["choices"][0];

            if let Some(content) = choice["delta"]["content"].as_str() {
                if !content.is_empty() {
                    text.push_str(content);
                    tx.send(StreamEvent::Delta(content.to_string())).await.ok();
                }
            }

            if let Some(tc_deltas) = choice["delta"]["tool_calls"].as_array() {
                for delta in tc_deltas {
                    let idx = delta["index"].as_u64().unwrap_or(0) as usize;
                    while calls.len() <= idx {
                        calls.push(OpenAiToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                        });
                    }
                    if let Some(id) = delta["id"].as_str() {
                        calls[idx].id = id.to_string();
                    }
                    if let Some(name) = delta["function"]["name"].as_str() {
                        calls[idx].name = name.to_string();
                        tx.send(StreamEvent::Delta(format!("\n→ **{}**", name)))
                            .await
                            .ok();
                    }
                    if let Some(args) = delta["function"]["arguments"].as_str() {
                        calls[idx].arguments.push_str(args);
                    }
                }
            }

            if choice["finish_reason"] == "tool_calls" {
                for tc in &calls {
                    let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
                    tx.send(StreamEvent::Delta(format!(
                        " `{}`\n",
                        tool_hint(&tc.name, &args)
                    )))
                    .await
                    .ok();
                }
            }
        }
    }

    Ok((text, calls))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── truncate ─────────────────────────────────────────────────────────────

    #[test]
    fn truncate_under_limit() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_limit() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_over_limit() {
        let result = truncate("hello world", 5);
        assert!(result.starts_with("hello"));
        assert!(result.contains("truncated"));
    }

    // ── is_blocked_ip ────────────────────────────────────────────────────────

    #[test]
    fn blocks_loopback_v4() {
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_loopback_v6() {
        let ip: std::net::IpAddr = "::1".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_private_10() {
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_private_192_168() {
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_private_172_16() {
        let ip: std::net::IpAddr = "172.16.0.1".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_link_local_metadata() {
        let ip: std::net::IpAddr = "169.254.169.254".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn blocks_ipv4_mapped_in_ipv6() {
        let ip: std::net::IpAddr = "::ffff:169.254.169.254".parse().unwrap();
        assert!(is_blocked_ip(ip));
    }

    #[test]
    fn allows_public_ip() {
        let ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
        assert!(!is_blocked_ip(ip));
    }

    #[test]
    fn allows_public_ipv6() {
        let ip: std::net::IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(!is_blocked_ip(ip));
    }

    // ── sandbox_path ─────────────────────────────────────────────────────────

    #[test]
    fn sandbox_allows_existing_file_in_cwd() {
        let result = sandbox_path("Cargo.toml");
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[test]
    fn sandbox_allows_new_file_in_cwd() {
        let result = sandbox_path("src/does_not_exist_yet.rs");
        assert!(
            result.is_ok(),
            "expected Ok for new file in src/, got {:?}",
            result
        );
    }

    #[test]
    fn sandbox_blocks_absolute_outside_cwd() {
        let result = sandbox_path("/etc/passwd");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("outside the working directory"));
    }

    #[test]
    fn sandbox_blocks_traversal_outside_cwd() {
        let result = sandbox_path("../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_blocks_home_dir() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let result = sandbox_path(&format!("{}/.ssh/id_rsa", home));
        assert!(result.is_err());
    }

    // ── diff_files ───────────────────────────────────────────────────────────

    #[test]
    fn diff_no_changes_returns_empty() {
        let result = diff_files("hello\nworld\n", "hello\nworld\n");
        assert!(result.is_empty());
    }

    #[test]
    fn diff_added_line_shows_plus() {
        let result = diff_files("hello\n", "hello\nworld\n");
        assert!(result.contains('+'), "expected '+' in diff: {}", result);
    }

    #[test]
    fn diff_removed_line_shows_minus() {
        let result = diff_files("hello\nworld\n", "hello\n");
        assert!(result.contains('-'), "expected '-' in diff: {}", result);
    }

    #[test]
    fn diff_new_file_all_additions() {
        let result = diff_files("", "line one\nline two\n");
        assert!(result.contains('+'));
        assert!(!result.contains('-'));
    }

    #[test]
    fn diff_wraps_in_code_block() {
        let result = diff_files("a\n", "b\n");
        assert!(result.starts_with("```diff"));
        assert!(result.ends_with("```"));
    }

    // ── validate_tool_input ──────────────────────────────────────────────────

    #[test]
    fn validate_read_file_valid() {
        let input = json!({"path": "foo.txt"});
        assert!(validate_tool_input("read_file", &input).is_ok());
    }

    #[test]
    fn validate_read_file_missing_path() {
        let input = json!({});
        let err = validate_tool_input("read_file", &input).unwrap_err();
        assert!(err.contains("path"), "expected 'path' in error: {err}");
    }

    #[test]
    fn validate_read_file_null_path() {
        let input = json!({"path": null});
        let err = validate_tool_input("read_file", &input).unwrap_err();
        assert!(err.contains("path"), "expected 'path' in error: {err}");
    }

    #[test]
    fn validate_write_file_missing_content() {
        let input = json!({"path": "foo.txt"});
        let err = validate_tool_input("write_file", &input).unwrap_err();
        assert!(
            err.contains("content"),
            "expected 'content' in error: {err}"
        );
    }

    #[test]
    fn validate_write_file_missing_path() {
        let input = json!({"content": "hello"});
        let err = validate_tool_input("write_file", &input).unwrap_err();
        assert!(err.contains("path"), "expected 'path' in error: {err}");
    }

    #[test]
    fn validate_write_file_valid() {
        let input = json!({"path": "foo.txt", "content": "hello"});
        assert!(validate_tool_input("write_file", &input).is_ok());
    }

    #[test]
    fn validate_run_shell_missing_command() {
        let input = json!({});
        let err = validate_tool_input("run_shell", &input).unwrap_err();
        assert!(
            err.contains("command"),
            "expected 'command' in error: {err}"
        );
    }

    #[test]
    fn validate_run_shell_valid() {
        let input = json!({"command": "ls"});
        assert!(validate_tool_input("run_shell", &input).is_ok());
    }

    #[test]
    fn validate_search_files_missing_pattern() {
        let input = json!({"path": "."});
        let err = validate_tool_input("search_files", &input).unwrap_err();
        assert!(
            err.contains("pattern"),
            "expected 'pattern' in error: {err}"
        );
    }

    #[test]
    fn validate_unknown_tool_returns_error() {
        let input = json!({});
        let err = validate_tool_input("does_not_exist", &input).unwrap_err();
        assert!(
            err.contains("unknown tool"),
            "expected 'unknown tool' in error: {err}"
        );
    }

    #[test]
    fn silent_null_fallback_is_the_current_bug() {
        // Documents the existing behavior we're fixing:
        // malformed JSON silently becomes null, tool receives empty string for path
        let bad_json = "not valid json {{{";
        let input: Value = serde_json::from_str(bad_json).unwrap_or(Value::Null);
        assert_eq!(input["path"].as_str().unwrap_or(""), "");
        // After the fix, validate_tool_input would catch this before run_tool sees it
    }
}
