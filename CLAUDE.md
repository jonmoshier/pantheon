# Pantheon

A lightweight multi-model terminal chat with tool use, streaming, and a TUI you fully control. One interface to many models.

## What it is

Most LLM CLIs lock you to one provider. Pantheon lets you switch between Gemini, Groq, Claude, OpenRouter, or any OpenAI-compatible endpoint mid-conversation, with a clean TUI, file tools, and streaming — all in one place.

The model selection is manual. For automatic routing, use OpenRouter Auto (`openrouter/auto`) — it picks the best model per request and Pantheon will display which model it routed to.

## Architecture

```
user message
    │
    ▼
App (app.rs)          ← state, input, message history
    │
    ▼
stream_anthropic()    ← Anthropic native API
stream_openai_compat() ← OpenAI-compatible API (Gemini, Groq, OpenRouter, etc.)
    │
    ▼
StreamEvent channel   ← tokens, tool calls, errors, model resolution
    │
    ▼
TUI (ui.rs)           ← ratatui rendering, status bar, markdown
```

## Providers

Configured in `~/.pantheon/models.toml` (written on first run). Any OpenAI-compatible endpoint works — Ollama, Together AI, local models, etc.

| Provider | Notes |
|---|---|
| Anthropic | Native API with tool use |
| Google Gemini | Via OpenAI-compat endpoint |
| Groq | Fast inference |
| OpenRouter | Access to many models; `openrouter/auto` for automatic routing |

## Tool use

Models can read, write, and append files (sandboxed to cwd), run shell commands, search files, and make HTTP requests. All destructive actions require user confirmation.

## Key files

| File | Purpose |
|---|---|
| `src/main.rs` | Entry point, event loop, CLI args (`-s` for system prompt) |
| `src/app.rs` | App struct, state management, command handling |
| `src/api.rs` | Streaming for Anthropic and OpenAI-compat, tool execution |
| `src/ui.rs` | ratatui rendering, status bar, message display |
| `src/config.rs` | Model definitions, settings persistence |
| `src/markdown.rs` | Markdown → terminal rendering |
| `src/theme.rs` | Color themes |

## System prompts

Pantheon loads system prompts in order (project on top, hardcoded default at the bottom):

1. `./.pantheon/system_prompt.md` — project-specific context
2. `~/.pantheon/system_prompt.md` — global user context (or `-s <path>` override)
3. Hardcoded default — tool sandboxing instructions

## What this is not

- Not an automatic router (use OR Auto for that)
- Not a load balancer or agent framework
- Not a proxy — local CLI tool only
