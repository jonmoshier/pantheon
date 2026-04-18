import asyncio
import json
import logging
import os
from pathlib import Path

import litellm
from rich.text import Text
from textual import work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.screen import ModalScreen
from textual.message import Message
from textual.widgets import Input, Label, RichLog, Static, TextArea

from pantheon.config import load_credentials, PROVIDERS, enabled_providers, CONFIG_DIR
from pantheon.router import pick_model
from pantheon.tools import TOOLS, execute_tool
from pantheon.theme import get_theme, set_theme, theme_names, active_theme

_log = logging.getLogger("pantheon")


def _setup_debug_logging() -> None:
    CONFIG_DIR.mkdir(mode=0o700, exist_ok=True)
    handler = logging.FileHandler(CONFIG_DIR / "debug.log")
    handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(message)s"))
    _log.addHandler(handler)
    _log.setLevel(logging.DEBUG)

litellm.suppress_debug_info = True
litellm.disable_fallbacks = True
litellm.vertex_project = None
litellm.vertex_location = None

_VERTEX_ENV_VARS = (
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_CLOUD_PROJECT",
    "VERTEXAI_PROJECT",
    "VERTEXAI_LOCATION",
    "VERTEX_PROJECT",
    "VERTEX_LOCATION",
)


def _load_creds() -> dict:
    creds = load_credentials()
    for key, val in creds.items():
        os.environ[key] = val
    for var in _VERTEX_ENV_VARS:
        os.environ.pop(var, None)
    return creds


def _api_key_for(provider_id: str, creds: dict) -> str | None:
    return creds.get(PROVIDERS[provider_id]["env_key"])


class ChatInput(TextArea):
    class Submitted(Message):
        def __init__(self, text: str) -> None:
            super().__init__()
            self.text = text

    async def _on_key(self, event) -> None:
        if event.key == "shift+enter":
            event.prevent_default()
            event.stop()
            self.insert("\n")
        elif event.key == "enter":
            event.prevent_default()
            event.stop()
            self.post_message(self.Submitted(self.text))
        else:
            await super()._on_key(event)


class ConfirmModal(ModalScreen[bool]):
    DEFAULT_CSS = """
    ConfirmModal {
        align: center middle;
    }
    #dialog {
        padding: 1 2;
        border: solid #555555;
        background: #1a1a1a;
        width: 70;
        height: auto;
    }
    #confirm-label {
        color: #ce9178;
        margin-bottom: 1;
    }
    """

    def __init__(self, name: str, args: dict) -> None:
        super().__init__()
        self._name = name
        self._args = args

    def compose(self) -> ComposeResult:
        args_str = "  ".join(f"{k}={v}" for k, v in self._args.items())
        with Static(id="dialog"):
            yield Label(f"  {self._name}  {args_str}", id="confirm-label")
            yield Input(placeholder="y/N", id="confirm-input")

    def on_mount(self) -> None:
        self.query_one("#confirm-input", Input).focus()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        self.dismiss(event.value.strip().lower() in ("y", "yes"))

    def on_key(self, event) -> None:
        if event.key == "escape":
            self.dismiss(False)


class PantheonApp(App):
    CSS = """
    Screen {
        background: #0d0d0d;
        layers: base overlay;
    }
    #output {
        height: 1fr;
        min-height: 3;
        padding: 0 1;
        scrollbar-color: #333333;
        scrollbar-background: transparent;
    }
    #streaming {
        height: auto;
        padding: 0 2;
        color: #d4d4d4;
    }
    #status-bar {
        height: 1;
        background: #111111;
        color: #888888;
        padding: 0 2;
    }
    #input-row {
        height: auto;
        border-top: solid #333333;
        background: #0d0d0d;
    }
    #prompt-label {
        width: auto;
        padding: 1 0 1 1;
        color: #569cd6;
        text-style: bold;
    }
    #chat-input {
        width: 1fr;
        height: auto;
        max-height: 10;
        border: none;
        background: transparent;
        color: #d4d4d4;
        padding: 1 0;
        scrollbar-color: #333333;
        scrollbar-background: transparent;
    }
    """

    BINDINGS = [
        Binding("ctrl+c", "quit_app", "Quit", priority=True),
        Binding("ctrl+d", "quit_app", "Quit", priority=True),
        Binding("ctrl+y", "copy_last", "Copy last response"),
    ]

    def __init__(self, creds: dict, tools_enabled: bool, root: Path, debug_mode: bool = False) -> None:
        super().__init__()
        self.creds = creds
        self.tools_enabled = tools_enabled
        self.root = root
        self.debug_mode = debug_mode
        self.history: list[dict] = [{"role": "system", "content": self._build_system_prompt()}]
        self.pinned_provider: str | None = None
        self.is_processing = False
        self.last_response: str = ""

    def _build_system_prompt(self) -> str:
        lines = [
            "You are a helpful terminal assistant running inside Pantheon, a CLI chat tool.",
            "Be concise and direct. Prefer short answers unless detail is genuinely needed.",
            "Do not narrate what you are about to do — just do it.",
        ]
        if self.tools_enabled:
            lines += [
                "",
                f"You have access to the local filesystem within this working directory: {self.root}",
                "Available tools:",
                "  read_file(path)       — read the contents of a file",
                "  write_file(path, content) — write or overwrite a file",
                "  list_directory(path)  — list files and subdirectories",
                "All paths must be relative to the working directory.",
                "Use tools when the user's request clearly requires reading or writing files.",
                "Do not speculatively read files unless asked.",
            ]
        return "\n".join(lines)

    def compose(self) -> ComposeResult:
        yield RichLog(id="output", wrap=True, markup=True, highlight=False)
        yield Static("", id="streaming")
        yield Static("  auto", id="status-bar")
        with Horizontal(id="input-row"):
            yield Label("  you ›  ", id="prompt-label")
            yield ChatInput(id="chat-input")

    def on_mount(self) -> None:
        t = get_theme()
        log = self.query_one(RichLog)
        log.write(Text("  Pantheon", style=t["banner-title"]))
        hints = "/model  ·  /auth  ·  /theme  ·  /quit"
        if self.tools_enabled:
            hints += f"  ·  tools on  ·  {self.root}"
        if self.debug_mode:
            hints += f"  ·  debug → {CONFIG_DIR / 'debug.log'}"
        log.write(Text(f"  {hints}", style=t["banner-hint"]))
        log.write("")
        self.query_one("#chat-input", ChatInput).focus()
        self._apply_theme()

    def action_quit_app(self) -> None:
        self.exit()

    def action_copy_last(self) -> None:
        if self.last_response:
            self.copy_to_clipboard(self.last_response)
            self._log("  copied to clipboard", "#555555")

    def _update_status(self) -> None:
        label = PROVIDERS[self.pinned_provider]["label"] if self.pinned_provider else "auto"
        text = f"  {label}"
        if self.tools_enabled:
            text += "  ·  tools on"
        if self.debug_mode:
            text += "  ·  debug"
        self.query_one("#status-bar", Static).update(text)

    def _apply_theme(self) -> None:
        t = get_theme()
        bg = t["background"]
        surface = t["surface"]
        text = t["text"]
        sep = t["separator"]

        self.screen.styles.background = bg
        self.query_one("#output", RichLog).styles.background = bg
        self.query_one("#streaming", Static).styles.background = bg
        self.query_one("#streaming", Static).styles.color = text
        self.query_one("#status-bar", Static).styles.background = surface
        self.query_one("#status-bar", Static).styles.color = t["status"]
        self.query_one("#input-row", Horizontal).styles.background = bg
        self.query_one("#input-row", Horizontal).styles.border_top = ("solid", sep)
        self.query_one("#prompt-label", Label).styles.color = t["prompt"].split()[0]
        self.query_one("#chat-input", ChatInput).styles.background = bg
        self.query_one("#chat-input", ChatInput).styles.color = text

    def _log(self, content: str | Text, style: str = "") -> None:
        log = self.query_one(RichLog)
        if isinstance(content, str) and style:
            log.write(Text(content, style=style))
        else:
            log.write(content if content != "" else Text(""))

    async def on_chat_input_submitted(self, event: ChatInput.Submitted) -> None:
        user_input = event.text.strip()
        self.query_one("#chat-input", ChatInput).load_text("")
        if not user_input or self.is_processing:
            return
        if await self._handle_command(user_input):
            return
        t = get_theme()
        msg = Text("  you  ", style=t["user-label"])
        msg.append(user_input, style=t["assistant"])
        self._log(msg)
        self.is_processing = True
        self._process_message(user_input)

    async def _handle_command(self, cmd: str) -> bool:
        lower = cmd.lower()
        t = get_theme()

        if lower in ("/quit", "/exit"):
            self.exit()
            return True

        if lower in ("/help", "/h", "/?"):
            self._log("")
            self._log("  /model              list providers", t["banner-hint"])
            self._log("  /model auto         auto-routing", t["banner-hint"])
            self._log("  /model <id>         pin to provider", t["banner-hint"])
            self._log("  /auth add           add a new provider", t["banner-hint"])
            self._log("  /theme              list available themes", t["banner-hint"])
            self._log("  /theme <name>       switch theme", t["banner-hint"])
            self._log("  /quit               exit", t["banner-hint"])
            self._log("")
            return True

        if lower.startswith("/auth"):
            arg = lower[5:].strip()
            self._log("")
            if arg == "add" or not arg:
                from pantheon.auth import _provider_selection_prompt
                added = _provider_selection_prompt()
                if added:
                    self._log(f"[green]✓ Added {len(added)} provider(s).[/green]", "")
                    # Reload enabled providers in case the UI needs to update
                else:
                    self._log("[yellow]No providers added.[/yellow]", "")
            else:
                self._log(f"  Unknown auth command '{arg}'.", t["error"])
            self._log("")
            return True

        if lower.startswith("/model"):
            arg = lower[6:].strip()
            available = enabled_providers()
            self._log("")
            if not arg:
                current = PROVIDERS[self.pinned_provider]["label"] if self.pinned_provider else "auto-route"
                self._log(f"  model: {current}", t["banner-hint"])
                self._log("")
                for pid in available:
                    meta = PROVIDERS[pid]
                    marker = "  ←" if pid == self.pinned_provider else ""
                    self._log(f"    {pid:<22} {meta['label']} ({meta['tier']}){marker}", t["banner-hint"])
                self._log("")
                self._log("  /model auto      resume auto-routing", t["banner-hint"])
                self._log("  /model <id>      pin to a provider", t["banner-hint"])
            elif arg == "auto":
                self.pinned_provider = None
                self._update_status()
                self._log("  Switched to auto-routing.", t["banner-hint"])
            elif arg in available:
                self.pinned_provider = arg
                self._update_status()
                meta = PROVIDERS[arg]
                self._log(f"  Pinned to {meta['label']} ({meta['tier']}).", t["banner-hint"])
            else:
                self._log(f"  Unknown provider '{arg}'.", t["error"])
            self._log("")
            return True

        if lower.startswith("/theme"):
            arg = lower[6:].strip()
            self._log("")
            if not arg:
                current = active_theme()
                self._log(f"  theme: {current}", t["banner-hint"])
                for name in theme_names():
                    marker = "  ←" if name == current else ""
                    self._log(f"    {name}{marker}", t["banner-hint"])
                self._log("")
                self._log("  /theme <name>    switch theme", t["banner-hint"])
            else:
                try:
                    set_theme(arg)
                    self._apply_theme()
                    t = get_theme()
                    self._log(f"  Switched to {arg} theme.", t["banner-hint"])
                except ValueError:
                    self._log(f"  Unknown theme '{arg}'. Available: {', '.join(theme_names())}", t["error"])
            self._log("")
            return True

        return False

    @work
    async def _process_message(self, user_input: str) -> None:
        self.history.append({"role": "user", "content": user_input})
        excluded: set[str] = set()
        t = get_theme()

        try:
            while True:
                try:
                    if self.pinned_provider:
                        provider_id = self.pinned_provider
                        model = PROVIDERS[provider_id]["model"]
                    else:
                        provider_id, model = pick_model(user_input, exclude=excluded)

                    meta = PROVIDERS[provider_id]
                    if self.tools_enabled and not meta.get("supports_tools"):
                        self._log(f"  {meta['label']} doesn't support tools", t["error"])
                        break

                    pin_marker = " (pinned)" if self.pinned_provider else ""
                    self._log(f"  → {meta['label']} ({meta['tier']}){pin_marker}", t["routing"])
                    self._log("")

                    api_key = _api_key_for(provider_id, self.creds)

                    if self.tools_enabled:
                        await asyncio.wait_for(self._run_with_tools(model, api_key), timeout=120)
                    else:
                        await asyncio.wait_for(self._run_streaming(model, api_key), timeout=120)

                    break

                except (litellm.Timeout, asyncio.TimeoutError):
                    t = get_theme()
                    self._log(f"  timeout on {PROVIDERS[provider_id]['label']}", t["error"])
                    if self.debug_mode:
                        _log.error("timeout provider=%s model=%s", provider_id, model)
                    break

                except litellm.RateLimitError:
                    excluded.add(provider_id)
                    t = get_theme()
                    msg = Text(f"  rate limited on {PROVIDERS[provider_id]['label']}", style=t["routing"])
                    if self.debug_mode:
                        _log.warning("rate_limit provider=%s", provider_id)
                    if self.pinned_provider:
                        msg.append(" — unpin to enable fallback", style=t["error"])
                        self._log(msg)
                        break
                    try:
                        pick_model(user_input, exclude=excluded)
                        msg.append(", trying next…", style=t["routing"])
                        self._log(msg)
                    except RuntimeError:
                        msg.append(" — no providers remaining", style=t["error"])
                        self._log(msg)
                        break

                except Exception as e:
                    t = get_theme()
                    self._log(f"  error  {e}", t["error"])
                    if self.debug_mode:
                        _log.exception("provider=%s model=%s", provider_id, model)
                    break
        finally:
            self.is_processing = False
            self.query_one("#streaming", Static).update("")

    async def _run_streaming(self, model: str, api_key: str | None) -> None:
        t = get_theme()
        streaming = self.query_one("#streaming", Static)
        collected: list[str] = []

        response = await litellm.acompletion(
            model=model,
            messages=self.history,
            stream=True,
            api_key=api_key,
            timeout=60,
        )

        async for chunk in response:
            delta = chunk.choices[0].delta.content or ""
            if delta:
                collected.append(delta)
                streaming.update(Text("".join(collected), style=t["assistant"]))

        full_response = "".join(collected)
        streaming.update("")
        self._log(Text(full_response, style=t["assistant"]))
        self._log("")
        self.last_response = full_response
        self.history.append({"role": "assistant", "content": full_response})

    async def _run_with_tools(self, model: str, api_key: str | None) -> None:
        t = get_theme()
        response = await litellm.acompletion(
            model=model,
            messages=self.history,
            tools=TOOLS,
            tool_choice="auto",
            stream=False,
            api_key=api_key,
            timeout=60,
        )

        choice = response.choices[0]
        message = choice.message

        if choice.finish_reason != "tool_calls" or not message.tool_calls:
            content = message.content or ""
            self._log(Text(content, style=t["assistant"]))
            self._log("")
            self.history.append({"role": "assistant", "content": content})
            return

        self.history.append({
            "role": "assistant",
            "content": message.content,
            "tool_calls": [tc.model_dump() for tc in message.tool_calls],
        })

        for tc in message.tool_calls:
            name = tc.function.name
            args = json.loads(tc.function.arguments)
            confirmed = await self.push_screen_wait(ConfirmModal(name, args))

            if confirmed:
                result = execute_tool(name, args, self.root)
                t = get_theme()
                self._log("  ✓ done", t["tool-ok"])
            else:
                result = "User declined tool execution."
                t = get_theme()
                self._log("  ✗ skipped", t["tool-skip"])

            self.history.append({
                "role": "tool",
                "tool_call_id": tc.id,
                "name": name,
                "content": result,
            })

        t = get_theme()
        streaming = self.query_one("#streaming", Static)
        collected: list[str] = []

        follow_up = await litellm.acompletion(
            model=model,
            messages=self.history,
            tools=TOOLS,
            stream=True,
            api_key=api_key,
            timeout=60,
        )

        async for chunk in follow_up:
            delta = chunk.choices[0].delta.content or ""
            if delta:
                collected.append(delta)
                streaming.update(Text("".join(collected), style=t["assistant"]))

        full_response = "".join(collected)
        streaming.update("")
        self._log(Text(full_response, style=t["assistant"]))
        self._log("")
        self.last_response = full_response
        self.history.append({"role": "assistant", "content": full_response})


def run(tools_enabled: bool = False, debug: bool = False) -> None:
    if debug:
        _setup_debug_logging()
    creds = _load_creds()
    PantheonApp(creds=creds, tools_enabled=tools_enabled, root=Path.cwd(), debug_mode=debug).run()
