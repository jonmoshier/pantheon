import getpass
from rich.console import Console
from rich.table import Table
import typer

from pantheon.config import PROVIDERS, TIER_ORDER, save_credential, save_config, load_config

console = Console()


def onboard() -> bool:
    """First-run setup wizard. Returns True if at least one provider was added."""
    console.print("\n[bold yellow]Welcome to Pantheon.[/bold yellow]")
    console.print("Let's add your first provider.\n")
    console.print("Pantheon routes chats to the cheapest capable model.")
    console.print("Starting with a [bold]free tier[/bold] provider is recommended.\n")

    added = _provider_selection_prompt()

    if not added:
        console.print("\n[red]No providers added. Run [bold]pan auth add[/bold] to set one up.[/red]")
        return False

    console.print(f"\n[bold green]✓ You're set up with {len(added)} provider(s).[/bold green]")
    console.print("Add more anytime with: [bold]pan auth add[/bold]\n")
    return True


def add_provider():
    """Interactively add a provider."""
    added = _provider_selection_prompt()
    if added:
        console.print(f"\n[bold green]✓ Added {len(added)} provider(s).[/bold green]")


def _provider_selection_prompt() -> list[str]:
    sorted_providers = sorted(PROVIDERS.items(), key=lambda x: TIER_ORDER.index(x[1]["tier"]))

    console.print("[bold]Available providers:[/bold]\n")
    for i, (pid, meta) in enumerate(sorted_providers, 1):
        console.print(f"  [{i}] {meta['label']:<25} ({meta['tier']})")

    console.print()
    raw = input("Select providers (e.g. 1 or 1,3): ").strip()

    if not raw:
        return []

    selected_indices = []
    for part in raw.replace(",", " ").split():
        try:
            idx = int(part) - 1
            if 0 <= idx < len(sorted_providers):
                selected_indices.append(idx)
        except ValueError:
            pass

    added = []
    for idx in selected_indices:
        provider_id, meta = sorted_providers[idx]
        console.print(f"\n[bold]{meta['label']}[/bold]")
        console.print(f"  Get your API key at: {meta['key_url']}")
        key = input(f"  Paste your {meta['env_key']}: ").strip()
        if len(key) > 20:
            save_credential(meta["env_key"], key)
            _enable_provider(provider_id)
            added.append(provider_id)
            console.print(f"  [green]✓ Saved[/green]")
        else:
            console.print(f"  [dim]Skipped (key too short — not saved)[/dim]")

    return added


def _enable_provider(provider_id: str):
    config = load_config()
    enabled = config.get("enabled_providers", [])
    if provider_id not in enabled:
        enabled.append(provider_id)
    config["enabled_providers"] = enabled
    save_config(config)


def list_providers():
    from pantheon.config import load_credentials, enabled_providers

    creds = load_credentials()
    active = set(enabled_providers())

    console.print("\n[bold]Configured providers:[/bold]\n")
    for pid, meta in PROVIDERS.items():
        has_key = bool(creds.get(meta["env_key"]))
        is_active = pid in active
        if is_active:
            status = "[green]✓ active[/green]"
        elif has_key:
            status = "[yellow]key set[/yellow]"
        else:
            status = "[dim]not configured[/dim]"
        console.print(f"  {meta['label']:<25} {meta['tier']:<8} {status}")
    console.print()
