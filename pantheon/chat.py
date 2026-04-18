import os
from rich.console import Console
from rich.panel import Panel
from prompt_toolkit import PromptSession
from prompt_toolkit.history import FileHistory
from pathlib import Path
import litellm

from pantheon.config import load_credentials, PROVIDERS, enabled_providers
from pantheon.router import pick_model

litellm.suppress_debug_info = True
litellm.disable_fallbacks = True

console = Console()
HISTORY_FILE = Path.home() / ".pantheon" / "history"


def _load_creds() -> dict:
    creds = load_credentials()
    for key, val in creds.items():
        os.environ[key] = val
    return creds


def _api_key_for(provider_id: str, creds: dict) -> str | None:
    return creds.get(PROVIDERS[provider_id]["env_key"])


def _handle_model_command(arg: str, pinned: str | None) -> str | None:
    """Handle /model [id|auto]. Returns new pinned provider_id or None for auto."""
    available = enabled_providers()

    if not arg or arg == "auto":
        if not arg:
            # Show current state + options
            current = f"[cyan]{PROVIDERS[pinned]['label']}[/cyan]" if pinned else "[yellow]auto-route[/yellow]"
            console.print(f"\n[bold]Current:[/bold] {current}")
            console.print("\n[bold]Available:[/bold]")
            for pid in available:
                meta = PROVIDERS[pid]
                marker = " [green]←[/green]" if pid == pinned else ""
                console.print(f"  [bold]{pid:<20}[/bold] {meta['label']} ({meta['tier']}){marker}")
            console.print("\n  [dim]/model auto[/dim]       resume auto-routing")
            console.print(  "  [dim]/model <id>[/dim]       pin to a provider")
        else:
            console.print("[dim]  Switched to auto-routing.[/dim]")
        return pinned if not arg else None

    if arg in available:
        meta = PROVIDERS[arg]
        console.print(f"[dim]  Pinned to {meta['label']} ({meta['tier']}).[/dim]")
        return arg

    console.print(f"[red]Unknown provider '{arg}'. Run /model to see options.[/red]")
    return pinned


def run():
    creds = _load_creds()
    HISTORY_FILE.parent.mkdir(exist_ok=True)
    session = PromptSession(history=FileHistory(str(HISTORY_FILE)))
    history: list[dict] = []
    pinned_provider: str | None = None

    console.print(Panel(
        "[bold yellow]Pantheon[/bold yellow]  [dim]— many gods, one interface[/dim]\n"
        "[dim]Ctrl+C or /quit to exit  ·  /model to switch providers[/dim]",
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

        if user_input.lower().startswith("/model"):
            arg = user_input[6:].strip().lower()
            pinned_provider = _handle_model_command(arg, pinned_provider)
            continue

        history.append({"role": "user", "content": user_input})

        try:
            if pinned_provider:
                provider_id = pinned_provider
                model = PROVIDERS[provider_id]["model"]
            else:
                provider_id, model = pick_model(user_input)

            meta = PROVIDERS[provider_id]
            pin_marker = " [cyan](pinned)[/cyan]" if pinned_provider else ""
            console.print(f"[dim]  → {meta['label']} ({meta['tier']}){pin_marker}[/dim]")

            response = litellm.completion(
                model=model,
                messages=history,
                stream=True,
                api_key=_api_key_for(provider_id, creds),
            )

            collected = []
            print()
            for chunk in response:
                delta = chunk.choices[0].delta.content or ""
                collected.append(delta)
                print(delta, end="", flush=True)

            full_response = "".join(collected)
            print()

            history.append({"role": "assistant", "content": full_response})

        except RuntimeError as e:
            console.print(f"[red]{e}[/red]")
            break
        except Exception as e:
            console.print(f"[red]Error: {e}[/red]")
