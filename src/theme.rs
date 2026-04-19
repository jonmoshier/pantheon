use ratatui::style::Color;

pub struct Theme {
    pub name: &'static str,
    // Layout
    pub bg: Color,
    pub surface: Color,
    pub border: Color,
    pub border_active: Color,
    pub sep: Color,
    // Text
    pub text: Color,
    pub title: Color,
    pub dim: Color,
    pub status_fg: Color,
    // Accents
    pub user_accent: Color,
    pub error: Color,
    // Markdown
    pub code_fg: Color,
    pub code_bg: Color,
    pub heading: Color,
}

pub static THEMES: &[Theme] = &[MOCHA, LATTE, TOKYO_NIGHT, DRACULA];

// ── Catppuccin Mocha ──────────────────────────────────────────────────────────

pub const MOCHA: Theme = Theme {
    name: "mocha",
    bg: Color::Rgb(30, 30, 46),
    surface: Color::Rgb(24, 24, 37),
    border: Color::Rgb(49, 50, 68),
    border_active: Color::Rgb(69, 71, 90),
    sep: Color::Rgb(49, 50, 68),
    text: Color::Rgb(205, 214, 244),
    title: Color::Rgb(186, 194, 222),
    dim: Color::Rgb(108, 112, 134),
    status_fg: Color::Rgb(88, 91, 112),
    user_accent: Color::Rgb(137, 180, 250),
    error: Color::Rgb(243, 139, 168),
    code_fg: Color::Rgb(250, 179, 135),
    code_bg: Color::Rgb(24, 24, 37),
    heading: Color::Rgb(137, 180, 250),
};

// ── Catppuccin Latte (light) ──────────────────────────────────────────────────

pub const LATTE: Theme = Theme {
    name: "latte",
    bg: Color::Rgb(239, 241, 245),
    surface: Color::Rgb(230, 233, 239),
    border: Color::Rgb(204, 208, 218),
    border_active: Color::Rgb(172, 176, 190),
    sep: Color::Rgb(220, 224, 232),
    text: Color::Rgb(76, 79, 105),
    title: Color::Rgb(92, 95, 119),
    dim: Color::Rgb(156, 160, 176),
    status_fg: Color::Rgb(172, 176, 190),
    user_accent: Color::Rgb(30, 102, 245),
    error: Color::Rgb(210, 15, 57),
    code_fg: Color::Rgb(254, 100, 11),
    code_bg: Color::Rgb(220, 224, 232),
    heading: Color::Rgb(30, 102, 245),
};

// ── Tokyo Night ───────────────────────────────────────────────────────────────

pub const TOKYO_NIGHT: Theme = Theme {
    name: "tokyo-night",
    bg: Color::Rgb(26, 27, 38),
    surface: Color::Rgb(22, 22, 30),
    border: Color::Rgb(41, 46, 66),
    border_active: Color::Rgb(86, 95, 137),
    sep: Color::Rgb(41, 46, 66),
    text: Color::Rgb(192, 202, 245),
    title: Color::Rgb(169, 177, 214),
    dim: Color::Rgb(86, 95, 137),
    status_fg: Color::Rgb(65, 72, 104),
    user_accent: Color::Rgb(122, 162, 247),
    error: Color::Rgb(247, 118, 142),
    code_fg: Color::Rgb(255, 158, 100),
    code_bg: Color::Rgb(22, 22, 30),
    heading: Color::Rgb(187, 154, 247),
};

// ── Dracula ───────────────────────────────────────────────────────────────────

pub const DRACULA: Theme = Theme {
    name: "dracula",
    bg: Color::Rgb(40, 42, 54),
    surface: Color::Rgb(33, 34, 44),
    border: Color::Rgb(68, 71, 90),
    border_active: Color::Rgb(98, 114, 164),
    sep: Color::Rgb(68, 71, 90),
    text: Color::Rgb(248, 248, 242),
    title: Color::Rgb(248, 248, 242),
    dim: Color::Rgb(98, 114, 164),
    status_fg: Color::Rgb(98, 114, 164),
    user_accent: Color::Rgb(139, 233, 253),
    error: Color::Rgb(255, 85, 85),
    code_fg: Color::Rgb(255, 184, 108),
    code_bg: Color::Rgb(33, 34, 44),
    heading: Color::Rgb(189, 147, 249),
};
