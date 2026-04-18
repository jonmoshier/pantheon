import typer
import sys
import os
from rich.console import Console

from pantheon.config import is_configured

app = typer.Typer(
    name="pan",
    help="Pantheon — cost-aware LLM router for your terminal.",
    add_completion=False,
)
auth_app = typer.Typer(help="Manage provider credentials.")
app.add_typer(auth_app, name="auth")

console = Console()


@app.callback(invoke_without_command=True)
def default(
    ctx: typer.Context,
    tools: bool = typer.Option(False, "--tools", "-t", help="Enable filesystem tools (read-only, cwd-scoped)."),
    debug: bool = typer.Option(False, "--debug", "-d", help="Log errors and API details to ~/.pantheon/debug.log."),
    autoreload: bool = typer.Option(False, "--autoreload", help="Enable autoreload on file changes."),
):
    """Start a chat session (default command)."""
    if ctx.invoked_subcommand is not None:
        return

    if not is_configured():
        from pantheon.auth import onboard
        if not onboard():
            raise typer.Exit(1)

    from pantheon.chat import run
    
    if autoreload:
        run_with_autoreload(tools_enabled=tools, debug=debug)
    else:
        run(tools_enabled=tools, debug=debug)


def run_with_autoreload(tools_enabled: bool, debug: bool):
    """Run the chat with autoreload on file changes."""
    import time
    import importlib
    
    watched_dir = os.path.dirname(os.path.abspath(__file__))
    last_modified = {}
    
    console.print("[yellow]Autoreload enabled. Watching for changes...[/yellow]")
    
    while True:
        try:
            # Check for file changes
            has_changes = False
            for root, dirs, files in os.walk(watched_dir):
                for file in files:
                    if file.endswith('.py'):
                        filepath = os.path.join(root, file)
                        mtime = os.path.getmtime(filepath)
                        if filepath not in last_modified or last_modified[filepath] != mtime:
                            last_modified[filepath] = mtime
                            has_changes = True
                            console.print(f"[green]File changed: {filepath}[/green]")
            
            if has_changes:
                # Reload modules
                for module_name in list(sys.modules.keys()):
                    if module_name.startswith('pantheon'):
                        try:
                            importlib.reload(sys.modules[module_name])
                        except Exception as e:
                            console.print(f"[red]Error reloading {module_name}: {e}[/red]")
                console.print("[yellow]Modules reloaded. Restarting chat...[/yellow]")
            
            from pantheon.chat import run
            run(tools_enabled=tools_enabled, debug=debug)
            break
            
        except KeyboardInterrupt:
            console.print("[yellow]Autoreload stopped.[/yellow]")
            break
        except Exception as e:
            console.print(f"[red]Error: {e}[/red]")
            time.sleep(1)


@auth_app.command("add")
def auth_add():
    """Add a new provider."""
    from pantheon.auth import add_provider
    add_provider()


@auth_app.command("list")
def auth_list():
    """List configured providers."""
    from pantheon.auth import list_providers
    list_providers()


if __name__ == "__main__":
    app()
