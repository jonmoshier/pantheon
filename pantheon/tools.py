from pathlib import Path

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read the contents of a file. Path must be relative to the working directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the working directory.",
                    }
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write content to a file, creating it or overwriting it. Path must be relative to the working directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the working directory.",
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file.",
                    },
                },
                "required": ["path", "content"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "list_directory",
            "description": "List files and directories at a path. Use '.' for the working directory root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to the working directory.",
                    }
                },
                "required": ["path"],
            },
        },
    },
]


def execute_tool(name: str, args: dict, root: Path) -> str:
    try:
        rel = args.get("path", ".")
        target = (root / rel).resolve()

        if not target.is_relative_to(root.resolve()):
            return "Error: path is outside the working directory"

        if name == "read_file":
            if not target.exists():
                return f"Error: file not found: {rel}"
            if not target.is_file():
                return f"Error: not a file: {rel}"
            return target.read_text(errors="replace")

        if name == "write_file":
            content = args.get("content", "")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
            return f"wrote {len(content)} bytes to {rel}"

        if name == "list_directory":
            if not target.exists():
                return f"Error: directory not found: {rel}"
            if not target.is_dir():
                return f"Error: not a directory: {rel}"
            entries = sorted(target.iterdir(), key=lambda p: (p.is_file(), p.name))
            lines = []
            for entry in entries:
                suffix = "/" if entry.is_dir() else ""
                lines.append(f"{entry.name}{suffix}")
            return "\n".join(lines) if lines else "(empty)"

        return f"Error: unknown tool '{name}'"

    except PermissionError as e:
        return f"Error: permission denied — {e}"
    except Exception as e:
        return f"Error: {e}"
