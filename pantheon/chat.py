import json
import os
from pathlib import Path

import litellm
from rich.text import Text
from textual import work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.screen import ModalScreen
from textual.widgets import Input, Label, RichLog, Static

from pantheon.config import load_credentials, PROVIDERS, enabled_providers
from pantheon.router import pick_model
from pantheon.tools import TOOLS, execute_tool

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
        height: 3;
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
        border: none;
        background: transparent;
        color: #d4d4d4;
        padding: 1 0;
    }
    """

    BINDINGS = [
        Binding("ctrl+c", "quit_app", "Quit", priority=True),
        Binding("ctrl+d", "quit_app", "Quit", priority=True),
    ]

    def __init__(self, creds: dict, tools_enabled: bool, root: Path) -> None:
        super().__init__()
        self.creds = creds
        self.tools_enabled = tools_enabled
        self.root = root
        self.history: list[dict] = []
        self.pinned_provider: str | None = None
        self.is_processing = False

    def compose(self) -> ComposeResult:
        yield RichLog(id="output", wrap=True, markup=True, highlight=False)
        yield Static("", id="streaming")
        yield Static("  auto", id="status-bar")
        with Horizontal(id="input-row"):
            yield Label("  you ›  ", id="prompt-label")
            yield Input(id="chat-input")

    def on_mount(self) -> None:
        log = self.query_one(RichLog)
        log.write(Text("  Pantheon", style="bold #cccccc"))
        hints = "/model  ·  /quit"
        if self.tools_enabled:
            hints += f"  ·  tools on  ·  {self.root}"
        log.write(Text(f"  {hints}", style="#555555"))
        log.write("")
        self.query_one("#chat-input", Input).focus()

    def action_quit_app(self) -> None:
        self.exit()

    def _update_status(self) -> None:
        label = PROVIDERS[self.pinned_provider]["label"] if self.pinned_provider else "auto"
        text = f"  {label}"
        if self.tools_enabled:
            text += "  ·  tools on"
        self.query_one("#status-bar", Static).update(text)

    def _log(self, content: str | Text, style: str = "") -> None:
        log = self.query_one(RichLog)
        if isinstance(content, str) and style:
            log.write(Text(content, style=style))
        else:
            log.write(content if content != "" else Text(""))

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        if event.input.id != "chat-input":
            return
        user_input = event.value.strip()
        event.input.clear()
        if not user_input or self.is_processing:
            return
        if await self._handle_command(user_input):
            return
        msg = Text("  you  ", style="bold #569cd6")
        msg.append(user_input, style="#d4d4d4")
        self._log(msg)
        self.is_processing = True
        self._process_message(user_input)

    async def _handle_command(self, cmd: str) -> bool:
        lower = cmd.lower()

        if lower in ("/quit", "/exit"):
            self.exit()
            return True

        if lower in ("/help", "/h", "/?"):
            self._log("")
            self._log("  /model              list providers", "#555555")
            self._log("  /model auto         auto-routing", "#555555")
            self._log("  /model <id>         pin to provider", "#555555")
            self._log("  /quit               exit", "#555555")
            self._log("")
            return True

        if lower.startswith("/model"):
            arg = lower[6:].strip()
            available = enabled_providers()
            self._log("")
            if not arg:
                current = PROVIDERS[self.pinned_provider]["label"] if self.pinned_provider else "auto-route"
                self._log(f"  model: {current}", "#555555")
                self._log("")
                for pid in available:
                    meta = PROVIDERS[pid]
                    marker = "  ←" if pid == self.pinned_provider else ""
                    self._log(f"    {pid:<22} {meta['label']} ({meta['tier']}){marker}", "#555555")
                self._log("")
                self._log("  /model auto      resume auto-routing", "#555555")
                self._log("  /model <id>      pin to a provider", "#555555")
            elif arg == "auto":
                self.pinned_provider = None
                self._update_status()
                self._log("  Switched to auto-routing.", "#555555")
            elif arg in available:
                self.pinned_provider = arg
                self._update_status()
                meta = PROVIDERS[arg]
                self._log(f"  Pinned to {meta['label']} ({meta['tier']}).", "#555555")
            else:
                self._log(f"  Unknown provider '{arg}'.", "#f44747")
            self._log("")
            return True

        return False

    @work
    async def _process_message(self, user_input: str) -> None:
        self.history.append({"role": "user", "content": user_input})
        excluded: set[str] = set()

        while True:
            try:
                if self.pinned_provider:
                    provider_id = self.pinned_provider
                    model = PROVIDERS[provider_id]["model"]
                else:
                    provider_id, model = pick_model(user_input, exclude=excluded)

                meta = PROVIDERS[provider_id]
                pin_marker = " (pinned)" if self.pinned_provider else ""
                self._log(f"  → {meta['label']} ({meta['tier']}){pin_marker}", "#555555 italic")
                self._log("")

                api_key = _api_key_for(provider_id, self.creds)

                if self.tools_enabled:
                    await self._run_with_tools(model, api_key)
                else:
                    await self._run_streaming(model, api_key)

                break

            except litellm.RateLimitError:
                excluded.add(provider_id)
                msg = Text(f"  rate limited on {PROVIDERS[provider_id]['label']}", style="#555555 italic")
                if self.pinned_provider:
                    msg.append(" — unpin to enable fallback", style="#f44747")
                    self._log(msg)
                    break
                try:
                    pick_model(user_input, exclude=excluded)
                    msg.append(", trying next…", style="#555555 italic")
                    self._log(msg)
                except RuntimeError:
                    msg.append(" — no providers remaining", style="#f44747")
                    self._log(msg)
                    break

            except Exception as e:
                self._log(f"  error  {e}", "#f44747")
                break

        self.is_processing = False

    async def _run_streaming(self, model: str, api_key: str | None) -> None:
        streaming = self.query_one("#streaming", Static)
        collected: list[str] = []

        response = await litellm.acompletion(
            model=model,
            messages=self.history,
            stream=True,
            api_key=api_key,
        )

        async for chunk in response:
            delta = chunk.choices[0].delta.content or ""
            if delta:
                collected.append(delta)
                streaming.update(Text("".join(collected), style="#d4d4d4"))

        full_response = "".join(collected)
        streaming.update("")
        self._log(Text(full_response, style="#d4d4d4"))
        self._log("")
        self.history.append({"role": "assistant", "content": full_response})

    async def _run_with_tools(self, model: str, api_key: str | None) -> None:
        response = await litellm.acompletion(
            model=model,
            messages=self.history,
            tools=TOOLS,
            tool_choice="auto",
            stream=False,
            api_key=api_key,
        )

        choice = response.choices[0]
        message = choice.message

        if choice.finish_reason != "tool_calls" or not message.tool_calls:
            content = message.content or ""
            self._log(Text(content, style="#d4d4d4"))
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
                self._log("  ✓ done", "#4ec9b0")
            else:
                result = "User declined tool execution."
                self._log("  ✗ skipped", "#555555")

            self.history.append({
                "role": "tool",
                "tool_call_id": tc.id,
                "name": name,
                "content": result,
            })

        streaming = self.query_one("#streaming", Static)
        collected: list[str] = []

        follow_up = await litellm.acompletion(
            model=model,
            messages=self.history,
            tools=TOOLS,
            stream=True,
            api_key=api_key,
        )

        async for chunk in follow_up:
            delta = chunk.choices[0].delta.content or ""
            if delta:
                collected.append(delta)
                streaming.update(Text("".join(collected), style="#d4d4d4"))

        full_response = "".join(collected)
        streaming.update("")
        self._log(Text(full_response, style="#d4d4d4"))
        self._log("")
        self.history.append({"role": "assistant", "content": full_response})


def run(tools_enabled: bool = False) -> None:
    creds = _load_creds()
    (Path.home() / ".pantheon").mkdir(exist_ok=True)
    PantheonApp(creds=creds, tools_enabled=tools_enabled, root=Path.cwd()).run()
