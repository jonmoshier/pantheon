# Pantheon — Improvement Plan

## 🔴 Critical / High Impact

**1. The core promise isn't implemented yet — there's no automatic router**
The README and CLAUDE.md describe a classifier that picks the right model per message. In reality, `app.rs` just uses whatever model the user has manually selected (`MODELS[self.model_idx]`). The `classify(prompt) → (tier, skill)` → `pick_model()` pipeline described in CLAUDE.md doesn't exist. This is the entire point of the project and should be the top priority.

**2. `read_file` has no confirmation prompt** ✅ DONE
~~In `api.rs`, `write_file` and `append_file` go through `prompt_confirm()`, but `read_file` silently reads any file without asking the user. A malicious or confused prompt could exfiltrate `~/.ssh/id_rsa`, `credentials.json`, or any other sensitive file. This is a security hole that should be fixed immediately — at minimum add a confirm for files outside the current working directory.~~

Fixed in `src/api.rs` lines 40-51: `read_file` now calls `prompt_confirm()` before reading any file, matching the security pattern of `write_file` and `append_file`.

**3. History is split-brained between providers**
`api_history` (the Anthropic-format history) is maintained across turns, but when switching to an OpenAI-compat provider, the history is rebuilt from scratch from display messages. If you switch providers mid-conversation, the context is silently truncated. There's no warning to the user.

---

## 🟡 Medium Priority

**4. Model list is hardcoded with stale-looking model IDs**
`claude-haiku-4-5-20251001` and `claude-opus-4-7` look like version strings from the future — likely placeholders. These should either be verified against the real API or loaded from a config file so they don't require a recompile to update. A `~/.pantheon/models.toml` would make the tool much more maintainable.

**5. No cost tracking or display**
The project's whole pitch is cost-awareness, but there's no token counting, no cost estimate, and no running total shown to the user. Even a rough `~$0.003` per message in the status bar would make the value proposition tangible and real.

**6. Credentials stored insecurely**
`~/.pantheon/credentials.json` stores API keys in plaintext JSON. At minimum the file should be created with `0600` permissions. Ideally, integrate with the system keychain (`security` on macOS, `libsecret` on Linux).

**7. `search_files` shells out to `grep` instead of using Rust**
This means it fails silently on Windows and creates a shell injection risk if pattern or path contain shell metacharacters (the current `replace('\'', "'\\''")` escaping is not sufficient). Use the `grep` or `walkdir` + `regex` crates instead.

**8. No `/help` command** ✅ DONE
~~The only discoverable commands are `/model`, `/theme`, and `/quit`. New users have no way to know this without reading the source. A `/help` command (or displaying this on startup) would drastically improve onboarding.~~

Implemented in `src/ui.rs` lines 211-245: Full `/help` command with a comprehensive help dialog showing slash commands and keybindings. Triggeraable via `/help`, `/h`, or `/?`. Help is also discoverable through UI hints and input placeholder text.

---

## 🟢 Quality of Life / Polish

**9. Input box height is fixed at 5 lines**
`Constraint::Length(5)` in `ui.rs`. For multi-line messages this is cramped. The input area should grow dynamically up to a max height based on content, similar to how modern chat UIs work.

**10. Scroll UX is fragile**
`scroll_offset` is a `u16` stored in the app but recalculated as `max_scroll` on every render. This means the scroll position can silently reset when the window is resized or new messages arrive (since `auto_scroll` takes over). A proper scroll model that tracks "lines from the bottom" would be more stable.

**11. No conversation save/load**
There's no way to save a conversation or resume a previous one. Given that `~/.pantheon/` already exists as a config directory, persisting conversations there as JSONL would be straightforward and very useful.

**12. The spinner ticks on stream deltas, not on time**
`spinner_tick` increments on each `StreamEvent::Delta`, so it spins fast on verbose responses and freezes on slow ones (e.g. waiting for the first token). It should be driven by a clock tick, not message volume.

**13. Confirm dialog description can overflow**
In `ui.rs`, the popup width is `desc.len() + 6`, but this doesn't account for Unicode width or terminal width — long tool descriptions (like a full shell command) will wrap or clip unpredictably.

---

## 🔵 Architectural / Future

**14. The `Provider` enum uses `&'static str` for URLs and keys**
This means every provider must be known at compile time. Moving to a runtime-loaded config (e.g. from `~/.pantheon/providers.toml`) would let users add Ollama endpoints, OpenRouter, Together AI, etc. without recompiling — which is especially important for the "many gods" philosophy.

**15. Tool definitions are duplicated**
`anthropic_tool_defs()` and `openai_tool_defs()` define the same 7 tools twice in different JSON formats. A single `ToolDef` struct with a method to render as either format would eliminate the duplication and make adding new tools much less error-prone.

**16. `max_tokens` is hardcoded at 8096**
This is fine for most tasks but should be configurable per model — Claude Opus supports much larger outputs, and Groq models have different limits.
