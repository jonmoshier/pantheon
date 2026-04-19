use serde_json::{json, Value};
use tokio::{sync::mpsc, task::JoinHandle};
use tui_textarea::TextArea;
use ratatui::style::{Color, Modifier, Style};

use crate::api::StreamEvent;
use crate::theme::{Theme, THEMES};

#[derive(Clone)]
pub enum Provider {
    Anthropic,
    OpenAiCompat { base_url: String, env_key: String },
}

#[derive(Clone)]
pub struct Model {
    pub label: String,
    pub id: String,
    pub provider: Provider,
}

fn build_models() -> Vec<Model> {
    crate::config::load_model_defs()
        .into_iter()
        .filter_map(|d| {
            let provider = match d.provider.as_str() {
                "anthropic" => Provider::Anthropic,
                "openai-compat" => Provider::OpenAiCompat {
                    base_url: d.base_url?,
                    env_key: d.env_key?,
                },
                _ => return None,
            };
            Some(Model { label: d.label, id: d.id, provider })
        })
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub model_label: Option<String>,
}

pub enum AppMode {
    Normal,
    ModelSelect,
    Help,
    Confirm(String),
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub api_history: Vec<Value>,
    pub textarea: TextArea<'static>,
    pub models: Vec<Model>,
    pub model_idx: usize,
    pub picker_idx: usize,
    pub theme_idx: usize,
    pub streaming: bool,
    pub current_stream: String,
    pub stream_rx: Option<mpsc::Receiver<StreamEvent>>,
    pub stream_handle: Option<JoinHandle<()>>,
    pub api_key: Option<String>,
    pub auto_scroll: bool,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub mode: AppMode,
    pub spinner_tick: u8,
    pub confirm_tx: Option<mpsc::Sender<bool>>,
    pub status_msg: Option<(String, u8)>,
    pub input_history: Vec<String>,
    pub history_idx: Option<usize>,
    pub history_draft: String,
}

impl App {
    pub fn new(api_key: Option<String>) -> Self {
        let mut app = Self {
            messages: vec![],
            api_history: vec![],
            textarea: make_textarea(),
            models: build_models(),
            theme_idx: 0,
            model_idx: 0,
            picker_idx: 0,
            streaming: false,
            current_stream: String::new(),
            stream_rx: None,
            stream_handle: None,
            api_key,
            auto_scroll: true,
            scroll_offset: 0,
            should_quit: false,
            mode: AppMode::Normal,
            spinner_tick: 0,
            confirm_tx: None,
            status_msg: None,
            input_history: load_input_history(),
            history_idx: None,
            history_draft: String::new(),
        };
        if app.api_key.is_none() {
            app.push_system(
                "No API key found. Set ANTHROPIC_API_KEY and restart.".into(),
            );
        }
        app
    }

    pub fn model(&self) -> &Model {
        &self.models[self.model_idx]
    }

    pub fn theme(&self) -> &'static Theme {
        &THEMES[self.theme_idx]
    }

    pub fn cycle_theme(&mut self) {
        self.theme_idx = (self.theme_idx + 1) % THEMES.len();
        self.push_info(format!("theme: {}", self.theme().name));
    }

    pub fn poll_stream(&mut self) {
        if let Some((_, ref mut ticks)) = self.status_msg {
            if *ticks == 0 {
                self.status_msg = None;
            } else {
                *ticks = ticks.saturating_sub(1);
            }
        }

        let mut rx = match self.stream_rx.take() {
            Some(r) => r,
            None => return,
        };

        let mut finished = false;
        let mut error: Option<String> = None;
        let mut confirm: Option<String> = None;

        loop {
            match rx.try_recv() {
                Ok(StreamEvent::Delta(text)) => {
                    self.current_stream.push_str(&text);
                    self.spinner_tick = self.spinner_tick.wrapping_add(1);
                }
                Ok(StreamEvent::ApiHistory(new_msgs)) => {
                    self.api_history.extend(new_msgs);
                }
                Ok(StreamEvent::ConfirmRequest(desc)) => {
                    confirm = Some(desc);
                    break;
                }
                Ok(StreamEvent::Done) => {
                    finished = true;
                    break;
                }
                Ok(StreamEvent::Error(e)) => {
                    error = Some(e);
                    break;
                }
                Err(_) => break,
            }
        }

        if finished {
            let content = std::mem::take(&mut self.current_stream);
            if !content.is_empty() {
                self.messages.push(ChatMessage {
                    role: Role::Assistant,
                    content,
                    model_label: Some(self.model().label.to_string()),
                });
            }
            self.streaming = false;
            self.stream_handle = None;
            self.confirm_tx = None;
            self.auto_scroll = true;
        } else if let Some(e) = error {
            self.push_system(format!("error: {}", e));
            self.current_stream.clear();
            self.streaming = false;
            self.stream_handle = None;
            self.confirm_tx = None;
            self.auto_scroll = true;
        } else if let Some(desc) = confirm {
            self.mode = AppMode::Confirm(desc);
            self.stream_rx = Some(rx);
        } else {
            self.stream_rx = Some(rx);
        }
    }

    pub fn submit(&mut self) {
        let text = self.textarea.lines().join("\n");
        let text = text.trim().to_string();
        if text.is_empty() || self.streaming {
            return;
        }
        self.textarea = make_textarea();
        self.history_idx = None;
        self.history_draft = String::new();
        self.input_history.push(text.clone());
        append_input_history(&text);

        if let Some(cmd) = text.strip_prefix('/') {
            self.handle_command(cmd);
            return;
        }

        let provider = self.model().provider.clone();

        // Resolve the API key for whichever provider we're using
        let api_key = match &provider {
            Provider::Anthropic => self.api_key.clone(),
            Provider::OpenAiCompat { env_key, .. } => crate::config::load_api_key(env_key),
        };
        let api_key = match api_key {
            Some(k) => k,
            None => {
                let hint = match &provider {
                    Provider::Anthropic => "ANTHROPIC_API_KEY".to_string(),
                    Provider::OpenAiCompat { env_key, .. } => env_key.clone(),
                };
                self.push_system(format!("No API key — set {}", hint));
                return;
            }
        };

        self.messages.push(ChatMessage {
            role: Role::User,
            content: text.clone(),
            model_label: None,
        });
        self.auto_scroll = true;

        // Anthropic uses api_history (preserves tool-use turns in native format).
        // OpenAI-compat providers get simple role+content pairs rebuilt from display messages
        // since they don't share the same history format.
        let msgs: Vec<Value> = match &provider {
            Provider::Anthropic => {
                self.api_history.push(json!({"role": "user", "content": text}));
                self.api_history.clone()
            }
            Provider::OpenAiCompat { .. } => {
                self.messages.iter()
                    .filter(|m| matches!(m.role, Role::User | Role::Assistant))
                    .map(|m| json!({
                        "role": if matches!(m.role, Role::User) { "user" } else { "assistant" },
                        "content": m.content,
                    }))
                    .collect()
            }
        };

        let (tx, rx) = mpsc::channel(256);
        let (confirm_tx, confirm_rx) = mpsc::channel(1);
        self.stream_rx = Some(rx);
        self.confirm_tx = Some(confirm_tx);
        self.streaming = true;

        let model_id = self.model().id.clone();
        let handle = match provider {
            Provider::Anthropic => tokio::spawn(async move {
                crate::api::stream_anthropic(api_key, model_id, msgs, tx, confirm_rx).await;
            }),
            Provider::OpenAiCompat { base_url, .. } => {
                tokio::spawn(async move {
                    crate::api::stream_openai_compat(base_url, api_key, model_id, msgs, tx, confirm_rx).await;
                })
            }
        };
        self.stream_handle = Some(handle);
    }

    pub fn cancel_stream(&mut self) {
        if let Some(handle) = self.stream_handle.take() {
            handle.abort();
        }
        self.streaming = false;
        self.stream_rx = None;
        self.confirm_tx = None;
        self.mode = AppMode::Normal;
        self.current_stream.clear();
        self.push_info("Request cancelled.".into());
    }

    pub fn open_model_picker(&mut self) {
        self.picker_idx = self.model_idx;
        self.mode = AppMode::ModelSelect;
    }

    pub fn close_model_picker(&mut self) {
        self.mode = AppMode::Normal;
    }

    pub fn confirm_model_select(&mut self) {
        self.model_idx = self.picker_idx;
        self.push_info(format!("Switched to {}.", self.model().label));
        self.mode = AppMode::Normal;
    }

    pub fn picker_up(&mut self) {
        if self.picker_idx > 0 {
            self.picker_idx -= 1;
        }
    }

    pub fn picker_down(&mut self) {
        if self.picker_idx < self.models.len() - 1 {
            self.picker_idx += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }

    pub fn open_help(&mut self) {
        self.mode = AppMode::Help;
    }

    pub fn close_help(&mut self) {
        self.mode = AppMode::Normal;
    }

    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() { return; }
        let idx = match self.history_idx {
            None => {
                self.history_draft = self.textarea.lines().join("\n");
                self.input_history.len() - 1
            }
            Some(i) => i.saturating_sub(1),
        };
        self.history_idx = Some(idx);
        self.textarea = make_textarea();
        self.textarea.insert_str(&self.input_history[idx]);
    }

    pub fn history_next(&mut self) {
        let Some(idx) = self.history_idx else { return };
        if idx + 1 >= self.input_history.len() {
            self.history_idx = None;
            let draft = std::mem::take(&mut self.history_draft);
            self.textarea = make_textarea();
            if !draft.is_empty() { self.textarea.insert_str(&draft); }
        } else {
            self.history_idx = Some(idx + 1);
            self.textarea = make_textarea();
            self.textarea.insert_str(&self.input_history[idx + 1]);
        }
    }

    pub fn save_conversation(&mut self, name: &str) {
        let dir = crate::config::conversations_dir();
        let _ = std::fs::create_dir_all(&dir);
        let filename = if name.is_empty() { timestamp_name() } else { name.to_string() };
        let path = dir.join(format!("{}.json", filename));
        let data = serde_json::json!({
            "version": 1,
            "model": self.model().label,
            "messages": self.messages,
            "api_history": self.api_history,
        });
        match std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap_or_default()) {
            Ok(_) => self.push_info(format!("Saved to {}.json", filename)),
            Err(e) => self.push_system(format!("error saving: {}", e)),
        }
    }

    pub fn load_conversation(&mut self, name: &str) {
        let dir = crate::config::conversations_dir();
        if name.is_empty() {
            match std::fs::read_dir(&dir) {
                Ok(entries) => {
                    let mut names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| {
                            let n = e.file_name().to_string_lossy().to_string();
                            n.ends_with(".json").then(|| n.trim_end_matches(".json").to_string())
                        })
                        .collect();
                    names.sort();
                    if names.is_empty() {
                        self.push_system("no saved conversations — use /save [name]".into());
                    } else {
                        self.push_system(format!("saved conversations:\n{}", names.iter().map(|n| format!("  {}", n)).collect::<Vec<_>>().join("\n")));
                    }
                }
                Err(_) => self.push_system("no saved conversations — use /save [name]".into()),
            }
            return;
        }
        let path = dir.join(format!("{}.json", name));
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => { self.push_system(format!("error: '{}' not found — use /load to list saves", name)); return; }
        };
        let v: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => { self.push_system(format!("error parsing save: {}", e)); return; }
        };
        self.messages = serde_json::from_value(v["messages"].clone()).unwrap_or_default();
        self.api_history = v["api_history"].as_array().cloned().unwrap_or_default();
        self.current_stream.clear();
        self.auto_scroll = true;
        self.push_info(format!("Loaded '{}'", name));
    }

    fn handle_command(&mut self, cmd: &str) {
        let mut parts = cmd.splitn(2, ' ');
        let verb = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().map(str::trim).unwrap_or("");

        match verb.as_str() {
            "quit" | "exit" | "q" => {
                self.should_quit = true;
            }
            "save" => {
                self.save_conversation(arg);
            }
            "load" => {
                self.load_conversation(arg);
            }
            "context" | "ctx" => {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string());

                let mut lines = vec![
                    format!("model:    {}", self.model().label),
                    format!("cwd:      {}", cwd),
                    format!("messages: {}  (api history: {})", self.messages.len(), self.api_history.len()),
                ];

                lines.push(String::new());
                lines.push("context files:".into());
                let ctx_path = std::path::Path::new(&cwd);
                for name in &["CLAUDE.md", "README.md", ".pantheon/context.md"] {
                    let path = ctx_path.join(name);
                    if path.exists() {
                        let size = std::fs::metadata(&path)
                            .map(|m| format!("{} bytes", m.len()))
                            .unwrap_or_else(|_| "?".into());
                        lines.push(format!("  ✓ {} ({})", name, size));
                    } else {
                        lines.push(format!("  · {} (not found)", name));
                    }
                }

                self.push_system(lines.join("\n"));
            }
            "clear" | "reset" => {
                self.messages.clear();
                self.api_history.clear();
                self.current_stream.clear();
                self.push_info("Conversation cleared.".into());
            }
            "help" | "h" | "?" => {
                self.open_help();
            }
            "model" => {
                if arg.is_empty() {
                    self.open_model_picker();
                } else if let Some(idx) = self.models.iter().position(|m| {
                    m.label.to_lowercase().contains(&arg.to_lowercase())
                        || m.id.to_lowercase().contains(&arg.to_lowercase())
                }) {
                    self.model_idx = idx;
                    self.push_info(format!("Switched to {}.", self.models[idx].label));
                } else {
                    self.push_system(format!(
                        "unknown model '{}' — try haiku, sonnet, or opus",
                        arg
                    ));
                }
            }
            "theme" => {
                if arg.is_empty() {
                    let list = THEMES.iter().enumerate()
                        .map(|(i, t)| {
                            if i == self.theme_idx {
                                format!("  {} ←", t.name)
                            } else {
                                format!("  {}", t.name)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.push_system(format!(
                        "theme: {}\n\navailable:\n{}\n\nuse /theme <name> or Ctrl+T to cycle",
                        self.theme().name, list
                    ));
                } else if let Some(idx) = THEMES.iter().position(|t| t.name == arg) {
                    self.theme_idx = idx;
                    self.push_info(format!("theme: {}", THEMES[idx].name));
                } else {
                    let names = THEMES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ");
                    self.push_system(format!("unknown theme '{}' — try: {}", arg, names));
                }
            }
            _ => {
                self.push_system(format!(
                    "unknown command /{} — try /help, /model, /theme, or /quit",
                    verb
                ));
            }
        }
    }

    pub fn push_info(&mut self, msg: String) {
        self.status_msg = Some((msg, 60));
    }

    fn push_system(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: Role::System,
            content,
            model_label: None,
        });
        self.auto_scroll = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app() -> App {
        App::new(Some("test-key".into()))
    }

    #[test]
    fn clear_command_empties_messages() {
        let mut app = make_app();
        app.messages.push(ChatMessage { role: Role::User, content: "hi".into(), model_label: None });
        app.handle_command("clear");
        assert!(app.messages.is_empty());
    }

    #[test]
    fn clear_sets_status_msg() {
        let mut app = make_app();
        app.handle_command("clear");
        assert!(app.status_msg.is_some());
        let (msg, _) = app.status_msg.as_ref().unwrap();
        assert!(msg.contains("cleared"));
    }

    #[test]
    fn unknown_command_adds_system_message() {
        let mut app = make_app();
        app.handle_command("nope");
        let last = app.messages.last().unwrap();
        assert!(matches!(last.role, Role::System));
        assert!(last.content.contains("unknown command"));
    }

    #[test]
    fn model_command_with_arg_switches_model() {
        let mut app = make_app();
        let initial_idx = app.model_idx;
        if app.models.len() > 1 {
            let other_label = app.models[(initial_idx + 1) % app.models.len()].label.clone();
            app.handle_command(&format!("model {}", other_label));
            assert_ne!(app.model_idx, initial_idx);
        }
    }

    #[test]
    fn model_command_unknown_shows_error() {
        let mut app = make_app();
        app.handle_command("model zzz_nonexistent_model_zzz");
        let last = app.messages.last().unwrap();
        assert!(matches!(last.role, Role::System));
        assert!(last.content.contains("unknown model"));
    }

    #[test]
    fn theme_command_with_arg_switches_theme() {
        let mut app = make_app();
        let new_name = if app.theme_idx == 0 { THEMES[1].name } else { THEMES[0].name };
        app.handle_command(&format!("theme {}", new_name));
        assert_eq!(app.theme().name, new_name);
    }

    #[test]
    fn cycle_theme_wraps_around() {
        let mut app = make_app();
        let total = THEMES.len();
        for _ in 0..total {
            app.cycle_theme();
        }
        assert_eq!(app.theme_idx, 0);
    }

    #[test]
    fn push_info_sets_status_msg_with_ticks() {
        let mut app = make_app();
        app.push_info("hello".into());
        let (msg, ticks) = app.status_msg.as_ref().unwrap();
        assert_eq!(msg, "hello");
        assert!(*ticks > 0);
    }

    #[test]
    fn help_command_opens_help_mode() {
        let mut app = make_app();
        app.handle_command("help");
        assert!(matches!(app.mode, AppMode::Help));
    }

    #[test]
    fn quit_command_sets_should_quit() {
        let mut app = make_app();
        app.handle_command("quit");
        assert!(app.should_quit);
    }
}

fn load_input_history() -> Vec<String> {
    let path = crate::config::history_file();
    let Ok(content) = std::fs::read_to_string(path) else { return vec![] };
    content.lines()
        .filter(|l| l.starts_with('+'))
        .map(|l| l[1..].to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn append_input_history(entry: &str) {
    use std::io::Write as _;
    let path = crate::config::history_file();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(f, "\n# {}\n+{}", secs, entry);
    }
}

fn timestamp_name() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn make_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(Color::Rgb(212, 212, 212)).bg(Color::Rgb(24, 24, 24)));
    ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    ta.set_placeholder_text("Message… (Enter to send · Alt+Enter for newline · Ctrl+P for model · /help for commands)");
    ta.set_placeholder_style(Style::default().fg(Color::Rgb(85, 85, 85)));
    ta
}
