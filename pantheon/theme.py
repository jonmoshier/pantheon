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
    },
}

_active = "default"


def get_theme() -> dict[str, str]:
    return THEMES[_active]
