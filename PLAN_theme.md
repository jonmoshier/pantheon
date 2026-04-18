# Plan: /theme command + light mode

## Goal
Add a `/theme` slash command that switches between `default` (dark) and `light` themes at runtime without restarting.

## Files to change

### 1. `pantheon/theme.py`

Add a `light` theme entry to `THEMES` and a `set_theme()` function.

```python
THEMES: dict[str, dict[str, str]] = {
    "default": {
        "user-label":   "#569cd6 bold",
        "assistant":    "#d4d4d4",
        "routing":      "#555555 italic",
        "tool-pending": "#ce9178",
        "tool-ok":      "#4ec9b0",
        "tool-skip":    "#555555",
        "error":        "#f44747",
        "banner-title": "#cccccc bold",
        "banner-hint":  "#555555",
        "separator":    "#333333",
        "status":       "#888888",
        "background":   "#0d0d0d",
        "surface":      "#111111",
        "text":         "#d4d4d4",
        "prompt":       "#569cd6 bold",
    },
    "light": {
        "user-label":   "#0070c1 bold",
        "assistant":    "#1a1a1a",
        "routing":      "#888888 italic",
        "tool-pending": "#b5520a",
        "tool-ok":      "#177347",
        "tool-skip":    "#999999",
        "error":        "#cc0000",
        "banner-title": "#333333 bold",
        "banner-hint":  "#888888",
        "separator":    "#cccccc",
        "status":       "#666666",
        "background":   "#ffffff",
        "surface":      "#f5f5f5",
        "text":         "#1a1a1a",
        "prompt":       "#0070c1 bold",
    },
}

_active = "default"


def get_theme() -> dict[str, str]:
    return THEMES[_active]


def set_theme(name: str) -> None:
    global _active
    if name not in THEMES:
        raise ValueError(f"Unknown theme: {name}")
    _active = name


def theme_names() -> list[str]:
    return list(THEMES.keys())
```

### 2. `pantheon/chat.py`

#### Import changes
Add `set_theme` and `theme_names` to the import from `pantheon.theme`:
```python
from pantheon.theme import get_theme, set_theme, theme_names
```

#### Add `_apply_theme()` method to `PantheonApp`
This method updates live widget styles after a theme change. Add it after `_update_status`:

```python
def _apply_theme(self) -> None:
    t = get_theme()
    bg = t["background"]
    surface = t["surface"]
    text = t["text"]
    sep = t["separator"]

    self.query_one("Screen").styles.background = bg
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
```

#### Add `/theme` handling inside `_handle_command()`
In the `_handle_command` method, add a new block before `return False`. Place it after the `/model` block:

```python
if lower.startswith("/theme"):
    arg = lower[6:].strip()
    self._log("")
    if not arg:
        current = _active  # import _active from pantheon.theme, or use a getter
        self._log(f"  theme: {get_theme() and _active}", "#555555")
        for name in theme_names():
            marker = "  ←" if name == _active else ""
            self._log(f"    {name}{marker}", "#555555")
        self._log("")
        self._log("  /theme <name>    switch theme", "#555555")
    else:
        try:
            set_theme(arg)
            self._apply_theme()
            self._log(f"  Switched to {arg} theme.", "#555555")
        except ValueError:
            self._log(f"  Unknown theme '{arg}'. Available: {', '.join(theme_names())}", "#f44747")
    self._log("")
    return True
```

**Note:** To show the active theme name in `/theme` with no args, also export `_active` via a getter. Add to `theme.py`:
```python
def active_theme() -> str:
    return _active
```
Then import and use `active_theme()` in the `/theme` handler instead of referencing `_active` directly.

#### Update `_log()` to use theme colors
The `_log` method currently accepts arbitrary style strings. No changes needed — callers pass style strings from `get_theme()` values. Existing hardcoded color strings in the codebase can stay as-is for now; this plan does not require migrating every call.

#### Update `on_mount` to call `_apply_theme()`
At the end of `on_mount`, after writing the banner:
```python
self._apply_theme()
```

This ensures the initial theme (default dark) is applied consistently via the same code path as theme switching.

## `/theme` command behavior

```
/theme              — list available themes, show current
/theme light        — switch to light
/theme default      — switch back to dark
```

## Verification checklist
1. `pan` starts in dark mode — no visual change from today
2. `/theme light` switches background, text, and accent colors without restart
3. `/theme default` switches back
4. `/theme bogus` shows an error message
5. `/theme` with no args lists themes and marks the active one
6. Status bar, input area, and output area all update on switch
7. Routing/error/tool messages use colors from the active theme after switch
