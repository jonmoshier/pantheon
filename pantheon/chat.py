import json
import os
import threading
from pathlib import Path

import litellm
from prompt_toolkit import Application
from prompt_toolkit.data_structures import Point
from prompt_toolkit.layout import Layout, HSplit, Window
from prompt_toolkit.layout.controls import FormattedTextControl
from prompt_toolkit.layout.margins import ScrollbarMargin
from prompt_toolkit.widgets import TextArea
from prompt_toolkit.key_binding import KeyBindings

from pantheon.config import load_credentials, PROVIDERS, enabled_providers
from pantheon.router import pick_model
from pantheon.theme import get_style
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


def run(tools_enabled: bool = False):
    creds = _load_creds()
    root = Path.cwd()
    (Path.home() / ".pantheon").mkdir(exist_ok=True)

    history: list[dict] = []
    pinned_provider: str | None = None
    is_processing = False
    awaiting_confirmation = False
    _confirm_event = threading.Event()
    _confirm_result = [False]

    # Output as styled fragments — (style_class, text) pairs
    # List append is atomic in CPython so background-thread writes are safe
    output_fragments: list[tuple[str, str]] = []

    def get_output():
        return output_fragments

    def get_cursor_position():
        # Always point to the last line so the window stays scrolled to the bottom
        row = sum(t.count("\n") for _, t in output_fragments)
        return Point(x=0, y=row)

    output_window = Window(
        content=FormattedTextControl(
            text=get_output,
            get_cursor_position=get_cursor_position,
            focusable=False,
        ),
        wrap_lines=True,
        right_margins=[ScrollbarMargin(display_arrows=False)],
    )

    def get_status():
        label = PROVIDERS[pinned_provider]["label"] if pinned_provider else "auto"
        frags = [("class:status-model", f"  {label}")]
        if tools_enabled:
            frags.append(("class:status-hint", "  ·  tools on"))
        return frags

    status_window = Window(
        content=FormattedTextControl(get_status, focusable=False),
        height=1,
    )

    input_field = TextArea(
        prompt="  you ›  ",
        multiline=False,
        wrap_lines=True,
        height=1,
    )

    def append(text: str, style: str = "class:assistant") -> None:
        if text:
            output_fragments.append((style, text))
            app.invalidate()

    # Banner — populate directly since app doesn't exist yet
    hints = "/model  ·  /quit"
    if tools_enabled:
        hints += f"  ·  tools on  ·  {root}"
    output_fragments.append(("class:banner-title", "  Pantheon\n"))
    output_fragments.append(("class:banner-hint", f"  {hints}\n\n"))

    def request_confirmation(name: str, args: dict) -> bool:
        nonlocal awaiting_confirmation
        args_str = "  ".join(f"{k}={v}" for k, v in args.items())
        append(f"\n  {name}  {args_str}\n", "class:tool-pending")
        append("  run? [y/N]  \n", "class:tool-pending")
        _confirm_event.clear()
        awaiting_confirmation = True
        app.invalidate()
        _confirm_event.wait()
        awaiting_confirmation = False
        return _confirm_result[0]

    def handle_command(cmd: str) -> bool:
        nonlocal pinned_provider
        lower = cmd.lower()

        if lower in ("/quit", "/exit"):
            app.exit()
            return True

        if lower.startswith("/model"):
            arg = lower[6:].strip()
            available = enabled_providers()

            if not arg:
                current = PROVIDERS[pinned_provider]["label"] if pinned_provider else "auto-route"
                append(f"\n  model: {current}\n\n", "class:banner-hint")
                for pid in available:
                    meta = PROVIDERS[pid]
                    marker = "  ←" if pid == pinned_provider else ""
                    append(f"    {pid:<22} {meta['label']} ({meta['tier']}){marker}\n", "class:banner-hint")
                append("\n  /model auto      resume auto-routing\n", "class:banner-hint")
                append("  /model <id>      pin to a provider\n\n", "class:banner-hint")
            elif arg == "auto":
                pinned_provider = None
                append("\n  Switched to auto-routing.\n\n", "class:banner-hint")
            elif arg in available:
                pinned_provider = arg
                meta = PROVIDERS[arg]
                append(f"\n  Pinned to {meta['label']} ({meta['tier']}).\n\n", "class:banner-hint")
            else:
                append(f"\n  Unknown provider '{arg}'.\n\n", "class:error")
            return True

        return False

    def process_message(user_input: str) -> None:
        nonlocal is_processing, pinned_provider

        history.append({"role": "user", "content": user_input})
        excluded: set[str] = set()

        while True:
            try:
                if pinned_provider:
                    provider_id = pinned_provider
                    model = PROVIDERS[provider_id]["model"]
                else:
                    provider_id, model = pick_model(user_input, exclude=excluded)

                meta = PROVIDERS[provider_id]
                pin_marker = " (pinned)" if pinned_provider else ""
                append(f"  → {meta['label']} ({meta['tier']}){pin_marker}\n\n", "class:routing")

                api_key = _api_key_for(provider_id, creds)

                if tools_enabled:
                    _run_with_tools(model, api_key)
                else:
                    _run_streaming(model, api_key)

                break

            except litellm.RateLimitError:
                excluded.add(provider_id)
                append(f"  rate limited on {PROVIDERS[provider_id]['label']}", "class:routing")
                if pinned_provider:
                    append(" — unpin a provider to enable fallback\n\n", "class:error")
                    break
                try:
                    pick_model(user_input, exclude=excluded)  # probe: any left?
                    append(", trying next…\n\n", "class:routing")
                except RuntimeError:
                    append(" — no providers remaining\n\n", "class:error")
                    break

            except Exception as e:
                append(f"\n  error  {e}\n\n", "class:error")
                break

        is_processing = False

    def _run_streaming(model: str, api_key: str | None) -> None:
        response = litellm.completion(
            model=model,
            messages=history,
            stream=True,
            api_key=api_key,
        )
        collected = []
        for chunk in response:
            delta = chunk.choices[0].delta.content or ""
            collected.append(delta)
            append(delta, "class:assistant")
        history.append({"role": "assistant", "content": "".join(collected)})
        append("\n\n", "class:assistant")

    def _run_with_tools(model: str, api_key: str | None) -> None:
        response = litellm.completion(
            model=model,
            messages=history,
            tools=TOOLS,
            tool_choice="auto",
            stream=False,
            api_key=api_key,
        )

        choice = response.choices[0]
        message = choice.message

        if choice.finish_reason != "tool_calls" or not message.tool_calls:
            content = message.content or ""
            append(content, "class:assistant")
            history.append({"role": "assistant", "content": content})
            append("\n\n", "class:assistant")
            return

        history.append({
            "role": "assistant",
            "content": message.content,
            "tool_calls": [tc.model_dump() for tc in message.tool_calls],
        })

        for tc in message.tool_calls:
            name = tc.function.name
            args = json.loads(tc.function.arguments)
            confirmed = request_confirmation(name, args)

            if confirmed:
                result = execute_tool(name, args, root)
                append("  ✓ done\n", "class:tool-ok")
            else:
                result = "User declined tool execution."
                append("  ✗ skipped\n", "class:tool-skip")

            history.append({
                "role": "tool",
                "tool_call_id": tc.id,
                "name": name,
                "content": result,
            })

        append("\n", "class:assistant")
        follow_up = litellm.completion(
            model=model,
            messages=history,
            stream=True,
            api_key=api_key,
        )
        collected = []
        for chunk in follow_up:
            delta = chunk.choices[0].delta.content or ""
            collected.append(delta)
            append(delta, "class:assistant")
        history.append({"role": "assistant", "content": "".join(collected)})
        append("\n\n", "class:assistant")

    kb = KeyBindings()

    @kb.add("enter")
    def on_enter(event):
        nonlocal is_processing, awaiting_confirmation

        if awaiting_confirmation:
            answer = input_field.text.strip().lower()
            input_field.text = ""
            _confirm_result[0] = answer in ("y", "yes")
            _confirm_event.set()
            return

        user_input = input_field.text.strip()
        if not user_input or is_processing:
            return
        input_field.text = ""
        if handle_command(user_input):
            return
        append("  you  ", "class:user-label")
        append(f"{user_input}\n", "class:assistant")
        is_processing = True
        threading.Thread(target=process_message, args=(user_input,), daemon=True).start()

    @kb.add("c-c")
    @kb.add("c-d")
    def on_quit(event):
        app.exit()

    layout = Layout(
        HSplit([
            output_window,
            Window(height=1, char="─", style="class:separator"),
            status_window,
            input_field,
        ]),
        focused_element=input_field,
    )

    app = Application(
        layout=layout,
        key_bindings=kb,
        style=get_style(),
        full_screen=True,
        mouse_support=True,
    )

    app.run()
