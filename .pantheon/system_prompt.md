This is the Pantheon project — a cost-aware, skill-aware LLM router for the terminal, written in Rust.

The binary is `pan`. Source lives in `src/`. Key modules: `main.rs` (event loop, CLI args), `app.rs` (app state), `api.rs` (Anthropic/OpenAI streaming), `ui.rs` (ratatui rendering), `config.rs` (model definitions).

When suggesting code changes, use idiomatic Rust. Prefer `anyhow::Result` for error handling. The TUI uses ratatui 0.29 with crossterm. Async runtime is tokio.
