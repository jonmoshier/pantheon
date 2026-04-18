from prompt_toolkit.styles import Style

THEMES: dict[str, dict[str, str]] = {
    "default": {
        "user-label":   "bold #569cd6",
        "assistant":    "#d4d4d4",
        "routing":      "italic #555555",
        "tool-pending": "#ce9178",
        "tool-ok":      "#4ec9b0",
        "tool-skip":    "#555555",
        "error":        "#f44747",
        "banner-title": "bold #cccccc",
        "banner-hint":  "#555555",
        "separator":    "#333333",
        "status-model": "#888888",
        "status-hint":  "#4a4a4a",
    },
}

_active = "default"


def get_style() -> Style:
    return Style.from_dict(THEMES[_active])
