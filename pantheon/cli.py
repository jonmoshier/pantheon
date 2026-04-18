import typer
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
):
    """Start a chat session (default command)."""
    if ctx.invoked_subcommand is not None:
        return

    if not is_configured():
        from pantheon.auth import onboard
        if not onboard():
            raise typer.Exit(1)

    from pantheon.chat import run
    run(tools_enabled=tools)


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
