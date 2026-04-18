from rich.console import Console
from rich.prompt import Prompt, Confirm
from rich import print as rprint
import typer

from pantheon.config import PROVIDERS, TIER_ORDER, save_credential, save_config, load_config

console = Console()


def onboard():
    """First-run setup wizard."""
    console.print("\n[bold yellow]Welcome to Pantheon.[/bold yellow]")
    console.print("Let's add your first provider.\n")
    console.print("Pantheon routes your chats to the cheapest capable model.")
    console.print("Starting with a [bold]free tier[/bold] provider is recommended.\n")

    added = _provider_selection_prompt()

    if not added:
        console.print("\n[red]No providers added. Run [bold]pan auth add[/bold] to set one up.[/red]")
        raise typer.Exit(1)

    console.print(f"\n[bold green]✓ You're set up with {len(added)} provider(s).[/bold green]")
    console.print("Add more anytime with: [bold]pan auth add[/bold]\n")


def add_provider():
    """Interactively add a provider."""
    added = _provider_selection_prompt()
    if added:
        console.print(f"\n[bold green]✓ Added {len(added)} provider(s).[/bold green]")


def _provider_selection_prompt() -> list[str]:
    from InquirerPy import inquirer

    choices = [
        {
            "name": f"{meta['label']}  ({meta['tier']})",
            "value": pid,
            "enabled": meta["tier"] == "free",
        }
        for pid, meta in sorted(
            PROVIDERS.items(), key=lambda x: TIER_ORDER.index(x[1]["tier"])
        )
    ]

    selected = inquirer.checkbox(
        message="Which providers do you want to add? (space to select, enter to confirm)",
        choices=choices,
    ).execute()

    added = []
    for provider_id in selected:
        meta = PROVIDERS[provider_id]
        console.print(f"\n[bold]{meta['label']}[/bold]")
        console.print(f"Get your API key at: [link]{meta['key_url']}[/link]")
        key = Prompt.ask(f"Paste your {meta['env_key']}", password=True)
        if key.strip():
            save_credential(meta["env_key"], key.strip())
            _enable_provider(provider_id)
            added.append(provider_id)
            console.print(f"[green]✓ Saved[/green]")

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
        status = "[green]✓ active[/green]" if is_active else ("[yellow]key set[/yellow]" if has_key else "[dim]not configured[/dim]")
        console.print(f"  {meta['label']:<25} {meta['tier']:<8} {status}")
    console.print()
