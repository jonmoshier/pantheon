import os
from rich.console import Console
from rich.markdown import Markdown
from rich.panel import Panel
from prompt_toolkit import PromptSession
from prompt_toolkit.history import FileHistory
from pathlib import Path
import litellm

from pantheon.config import load_credentials, PROVIDERS
from pantheon.router import pick_model

litellm.suppress_debug_info = True

console = Console()
HISTORY_FILE = Path.home() / ".pantheon" / "history"


def _inject_credentials():
    creds = load_credentials()
    for key, val in creds.items():
        os.environ[key] = val


def run():
    _inject_credentials()
    HISTORY_FILE.parent.mkdir(exist_ok=True)
    session = PromptSession(history=FileHistory(str(HISTORY_FILE)))
    history: list[dict] = []

    console.print(Panel(
        "[bold yellow]Pantheon[/bold yellow]  [dim]— many gods, one interface[/dim]\n"
        "[dim]Type your message. [bold]Ctrl+C[/bold] or [bold]/quit[/bold] to exit.[/dim]",
        border_style="yellow",
        padding=(0, 1),
    ))

    while True:
        try:
            user_input = session.prompt("\n[you] ").strip()
        except (KeyboardInterrupt, EOFError):
            console.print("\n[dim]Goodbye.[/dim]")
            break

        if not user_input:
            continue
        if user_input.lower() in ("/quit", "/exit", "quit", "exit"):
            console.print("[dim]Goodbye.[/dim]")
            break

        history.append({"role": "user", "content": user_input})

        try:
            provider_id, model = pick_model(user_input)
            label = PROVIDERS[provider_id]["label"]
            tier = PROVIDERS[provider_id]["tier"]

            console.print(f"[dim]  → {label} ({tier})[/dim]")

            response = litellm.completion(
                model=model,
                messages=history,
                stream=True,
            )

            full_response = ""
            console.print()
            with console.status("", spinner="dots"):
                pass

            print(f"\033[0m", end="")  # reset any color bleed
            collected = []
            for chunk in response:
                delta = chunk.choices[0].delta.content or ""
                collected.append(delta)
                print(delta, end="", flush=True)

            full_response = "".join(collected)
            print()  # newline after streaming

            history.append({"role": "assistant", "content": full_response})

        except RuntimeError as e:
            console.print(f"[red]{e}[/red]")
            break
        except Exception as e:
            console.print(f"[red]Error: {e}[/red]")
