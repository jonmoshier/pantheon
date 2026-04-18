/**
 * theme.js – Light / Dark theme switcher
 *
 * Strategy
 * ────────
 * • The <html> element carries a `data-theme` attribute ("light" | "dark").
 * • CSS custom properties keyed on that attribute drive every colour.
 * • The chosen theme is persisted in localStorage so it survives page reloads.
 * • On first visit we respect the OS-level preference via
 *   `prefers-color-scheme` media query.
 */

(function () {
  "use strict";

  const STORAGE_KEY = "user-theme";
  const THEMES = { LIGHT: "light", DARK: "dark" };

  /* ── Helpers ──────────────────────────────────────────────────────────── */

  /**
   * Returns the theme that should be active on page load:
   *   1. Saved preference (localStorage)  – highest priority
   *   2. OS / browser preference          – fallback
   *   3. "light"                          – default
   */
  function getInitialTheme() {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === THEMES.LIGHT || saved === THEMES.DARK) return saved;

    if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
      return THEMES.DARK;
    }

    return THEMES.LIGHT;
  }

  /** Apply a theme to the root <html> element. */
  function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
  }

  /** Persist the user's choice. */
  function saveTheme(theme) {
    localStorage.setItem(STORAGE_KEY, theme);
  }

  /** Return the opposite theme. */
  function opposite(theme) {
    return theme === THEMES.DARK ? THEMES.LIGHT : THEMES.DARK;
  }

  /* ── Boot – set theme before first paint ─────────────────────────────── */
  const initialTheme = getInitialTheme();
  applyTheme(initialTheme);

  /* ── Wire up the toggle button after the DOM is ready ────────────────── */
  document.addEventListener("DOMContentLoaded", () => {
    const btn = document.getElementById("themeToggle");

    if (!btn) {
      console.warn("Theme toggle button (#themeToggle) not found.");
      return;
    }

    btn.addEventListener("click", () => {
      const current = document.documentElement.getAttribute("data-theme") || THEMES.LIGHT;
      const next = opposite(current);

      applyTheme(next);
      saveTheme(next);

      /* ── Spin animation ─────────────────────────────────── */
      btn.classList.remove("spin");          // reset any previous run
      void btn.offsetWidth;                  // force reflow so animation retriggers
      btn.classList.add("spin");
      btn.addEventListener("animationend", () => btn.classList.remove("spin"), { once: true });

      /* ── Announce change to screen readers ──────────────── */
      btn.setAttribute(
        "aria-label",
        next === THEMES.DARK ? "Switch to light theme" : "Switch to dark theme"
      );
    });

    /* Keep aria-label in sync with the initial state */
    btn.setAttribute(
      "aria-label",
      initialTheme === THEMES.DARK ? "Switch to light theme" : "Switch to dark theme"
    );

    /* ── Sync when another tab changes the theme ────────────────────────── */
    window.addEventListener("storage", (e) => {
      if (e.key === STORAGE_KEY && (e.newValue === THEMES.LIGHT || e.newValue === THEMES.DARK)) {
        applyTheme(e.newValue);
      }
    });
  });
})();
