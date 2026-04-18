use serde_json::json;
use tokio::sync::mpsc;

use crate::api::StreamEvent;

pub struct Model {
    pub label: &'static str,
    pub id: &'static str,
}

pub const MODELS: &[Model] = &[
    Model { label: "Claude Haiku", id: "claude-haiku-4-5-20251001" },
    Model { label: "Claude Sonnet", id: "claude-sonnet-4-6" },
];

pub enum Role {
    User,
    Assistant,
    System,
}

pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub cursor: usize,
    pub model_idx: usize,
    pub streaming: bool,
    pub current_stream: String,
    pub stream_rx: Option<mpsc::Receiver<StreamEvent>>,
    pub api_key: Option<String>,
    pub auto_scroll: bool,
    pub scroll_offset: u16,
    pub should_quit: bool,
}

impl App {
    pub fn new(api_key: Option<String>) -> Self {
        let mut app = Self {
            messages: vec![],
            input: String::new(),
            cursor: 0,
            model_idx: 0,
            streaming: false,
            current_stream: String::new(),
            stream_rx: None,
            api_key,
            auto_scroll: true,
            scroll_offset: 0,
            should_quit: false,
        };
        if app.api_key.is_none() {
            app.push_system("No API key found. Set ANTHROPIC_API_KEY or run the Python `pan auth add` to store credentials.".into());
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
                self.messages.push(ChatMessage { role: Role::Assistant, content });
            }
            self.streaming = false;
            self.auto_scroll = true;
        } else if let Some(e) = error {
            self.push_system(format!("error: {}", e));
            self.current_stream.clear();
            self.streaming = false;
            self.auto_scroll = true;
        } else {
            self.stream_rx = Some(rx);
        }
    }

    pub fn submit(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() || self.streaming {
            return;
        }
        self.input.clear();
        self.cursor = 0;

        if let Some(cmd) = text.strip_prefix('/') {
            self.handle_command(cmd);
            return;
        }

        let api_key = match self.api_key.clone() {
            Some(k) => k,
            None => {
                self.push_system("No API key — set ANTHROPIC_API_KEY".into());
                return;
            }
        };

        self.messages.push(ChatMessage { role: Role::User, content: text });
        self.auto_scroll = true;

        let (tx, rx) = mpsc::channel(256);
        self.stream_rx = Some(rx);
        self.streaming = true;

        let model = self.model().id.to_string();
        let msgs: Vec<_> = self.messages.iter()
            .filter(|m| matches!(m.role, Role::User | Role::Assistant))
            .map(|m| json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant", Role::System => "user" },
                "content": m.content,
            }))
            .collect();

        tokio::spawn(async move {
            crate::api::stream_anthropic(api_key, model, msgs, tx).await;
        });
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
                    let list = MODELS.iter().enumerate()
                        .map(|(i, m)| {
                            let marker = if i == self.model_idx { " ←" } else { "" };
                            format!("  {}{}", m.label, marker)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.push_system(format!(
                        "model: {}\n\navailable:\n{}\n\nuse /model <name> to switch",
                        self.model().label, list
                    ));
                } else if let Some(idx) = MODELS.iter().position(|m| {
                    m.label.to_lowercase().contains(&arg.to_lowercase())
                        || m.id.to_lowercase().contains(&arg.to_lowercase())
                }) {
                    self.model_idx = idx;
                    self.push_system(format!("switched to {}", MODELS[idx].label));
                } else {
                    self.push_system(format!(
                        "unknown model '{}' — try haiku or sonnet",
                        arg
                    ));
                }
            }
            _ => {
                self.push_system(format!("unknown command /{} — try /model or /quit", verb));
            }
        }
    }

    fn push_system(&mut self, content: String) {
        self.messages.push(ChatMessage { role: Role::System, content });
        self.auto_scroll = true;
    }

    // ── input editing ──────────────────────────────────────────────────────

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.input[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input.remove(prev);
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            let c = self.input[self.cursor..].chars().next().unwrap();
            self.cursor += c.len_utf8();
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.input.len();
    }

    pub fn cursor_col(&self) -> u16 {
        self.input[..self.cursor].chars().count() as u16
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 3;
    }
}
