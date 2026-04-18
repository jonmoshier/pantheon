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


def active_theme() -> str:
    return _active
