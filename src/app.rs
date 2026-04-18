use serde_json::{json, Value};
use tokio::{sync::mpsc, task::JoinHandle};
use tui_textarea::TextArea;
use ratatui::style::{Color, Modifier, Style};

use crate::api::StreamEvent;

/// Which API format and endpoint a model uses.
/// OpenAiCompat covers Groq, Together, Ollama, OpenAI — same wire format, different URL + key.
#[derive(Clone, Copy)]
pub enum Provider {
    Anthropic,
    OpenAiCompat { base_url: &'static str, env_key: &'static str },
}

pub struct Model {
    pub label: &'static str,
    pub id: &'static str,
    pub provider: Provider,
}

pub const MODELS: &[Model] = &[
    Model { label: "Claude Haiku",   id: "claude-haiku-4-5-20251001",    provider: Provider::Anthropic },
    Model { label: "Claude Sonnet",  id: "claude-sonnet-4-6",            provider: Provider::Anthropic },
    Model { label: "Claude Opus",    id: "claude-opus-4-7",              provider: Provider::Anthropic },
    Model {
        label: "Groq Llama 3.3 70B",
        id: "llama-3.3-70b-versatile",
        provider: Provider::OpenAiCompat {
            base_url: "https://api.groq.com/openai/v1",
            env_key:  "GROQ_API_KEY",
        },
    },
    Model {
        label: "Groq Llama 3.1 8B",
        id: "llama-3.1-8b-instant",
        provider: Provider::OpenAiCompat {
            base_url: "https://api.groq.com/openai/v1",
            env_key:  "GROQ_API_KEY",
        },
    },
];

pub enum Role {
    User,
    Assistant,
    System,
}

pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub model_label: Option<String>,
}

pub enum AppMode {
    Normal,
    ModelSelect,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub api_history: Vec<Value>,
    pub textarea: TextArea<'static>,
    pub model_idx: usize,
    pub picker_idx: usize,
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
}

impl App {
    pub fn new(api_key: Option<String>) -> Self {
        let mut app = Self {
            messages: vec![],
            api_history: vec![],
            textarea: make_textarea(),
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
        };
        if app.api_key.is_none() {
            app.push_system(
                "No API key found. Set ANTHROPIC_API_KEY and restart.".into(),
            );
        }
        app
    }

    pub fn model(&self) -> &'static Model {
        &MODELS[self.model_idx]
    }

    pub fn poll_stream(&mut self) {
        let mut rx = match self.stream_rx.take() {
            Some(r) => r,
            None => return,
        };

        let mut finished = false;
        let mut error: Option<String> = None;

        loop {
            match rx.try_recv() {
                Ok(StreamEvent::Delta(text)) => {
                    self.current_stream.push_str(&text);
                    self.spinner_tick = self.spinner_tick.wrapping_add(1);
                }
                Ok(StreamEvent::ApiHistory(new_msgs)) => {
                    self.api_history.extend(new_msgs);
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
            self.auto_scroll = true;
        } else if let Some(e) = error {
            self.push_system(format!("error: {}", e));
            self.current_stream.clear();
            self.streaming = false;
            self.stream_handle = None;
            self.auto_scroll = true;
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

        if let Some(cmd) = text.strip_prefix('/') {
            self.handle_command(cmd);
            return;
        }

        let provider = self.model().provider;

        // Resolve the API key for whichever provider we're using
        let api_key = match provider {
            Provider::Anthropic => self.api_key.clone(),
            Provider::OpenAiCompat { env_key, .. } => crate::config::load_api_key(env_key),
        };
        let api_key = match api_key {
            Some(k) => k,
            None => {
                let hint = match provider {
                    Provider::Anthropic => "ANTHROPIC_API_KEY",
                    Provider::OpenAiCompat { env_key, .. } => env_key,
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
        let msgs: Vec<Value> = match provider {
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
        self.stream_rx = Some(rx);
        self.streaming = true;

        let model = self.model().id.to_string();
        let handle = match provider {
            Provider::Anthropic => tokio::spawn(async move {
                crate::api::stream_anthropic(api_key, model, msgs, tx).await;
            }),
            Provider::OpenAiCompat { base_url, .. } => {
                let base_url = base_url.to_string();
                tokio::spawn(async move {
                    crate::api::stream_openai_compat(base_url, api_key, model, msgs, tx).await;
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
        self.current_stream.clear();
        self.push_system("Request cancelled.".into());
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
        self.push_system(format!("Switched to {}.", self.model().label));
        self.mode = AppMode::Normal;
    }

    pub fn picker_up(&mut self) {
        if self.picker_idx > 0 {
            self.picker_idx -= 1;
        }
    }

    pub fn picker_down(&mut self) {
        if self.picker_idx < MODELS.len() - 1 {
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

    fn handle_command(&mut self, cmd: &str) {
        let mut parts = cmd.splitn(2, ' ');
        let verb = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().map(str::trim).unwrap_or("");

        match verb.as_str() {
            "quit" | "exit" | "q" => {
                self.should_quit = true;
            }
            "model" => {
                if arg.is_empty() {
                    self.open_model_picker();
                } else if let Some(idx) = MODELS.iter().position(|m| {
                    m.label.to_lowercase().contains(&arg.to_lowercase())
                        || m.id.to_lowercase().contains(&arg.to_lowercase())
                }) {
                    self.model_idx = idx;
                    self.push_system(format!("Switched to {}.", MODELS[idx].label));
                } else {
                    self.push_system(format!(
                        "unknown model '{}' — try haiku, sonnet, or opus",
                        arg
                    ));
                }
            }
            _ => {
                self.push_system(format!(
                    "unknown command /{} — try /model or /quit",
                    verb
                ));
            }
        }
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

fn make_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_style(Style::default().fg(Color::Rgb(212, 212, 212)).bg(Color::Rgb(24, 24, 24)));
    ta.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
    ta.set_placeholder_text("Message… (Enter to send · Alt+Enter for newline · Ctrl+P for model)");
    ta.set_placeholder_style(Style::default().fg(Color::Rgb(85, 85, 85)));
    ta
}
