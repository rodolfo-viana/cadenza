//! Theme registry.
//!
//! Cadenza ships every reveal.js theme plus a set of its own modern themes.
//! Each theme is a reveal.js-compatible stylesheet; on top of that Cadenza
//! injects a few CSS variables (accent color, chart palette, ...) that its
//! own elements use for theme-aware defaults.

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub dark: bool,
    /// Stylesheet path relative to the UI root.
    pub css: &'static str,
    /// CSS `background` used for previews in the theme gallery.
    pub preview_bg: &'static str,
    pub heading_color: &'static str,
    pub text_color: &'static str,
    pub heading_font: &'static str,
    pub body_font: &'static str,
    pub accent: &'static str,
    /// Text color drawn on top of the accent (shape labels).
    pub on_accent: &'static str,
    /// Subtle surface for code blocks and cards.
    pub surface: &'static str,
    pub palette: [&'static str; 6],
}

const SANS: &str = "Inter, 'Segoe UI', Cantarell, Ubuntu, Roboto, 'Helvetica Neue', Arial, sans-serif";
const SERIF: &str = "'Source Serif 4', 'Source Serif Pro', Georgia, 'DejaVu Serif', 'Times New Roman', serif";

macro_rules! theme {
    ($id:expr, $name:expr, $group:expr, $dark:expr, $css:expr, $bg:expr, $head:expr, $text:expr, $hf:expr, $bf:expr, $acc:expr, $on:expr, $surf:expr, [$($p:expr),*]) => {
        ThemeInfo {
            id: $id, name: $name, group: $group, dark: $dark, css: $css, preview_bg: $bg,
            heading_color: $head, text_color: $text, heading_font: $hf, body_font: $bf,
            accent: $acc, on_accent: $on, surface: $surf, palette: [$($p),*],
        }
    };
}

pub static THEMES: &[ThemeInfo] = &[
    theme!("cadenza-midnight", "Midnight", "Cadenza", true, "themes/cadenza-midnight.css",
        "linear-gradient(160deg, #111827 0%, #0b1120 100%)", "#ffffff", "#cbd5e1", SANS, SANS,
        "#818cf8", "#ffffff", "rgba(148,163,184,0.12)",
        ["#818cf8", "#22d3ee", "#f472b6", "#fbbf24", "#34d399", "#a78bfa"]),
    theme!("cadenza-aurora", "Aurora", "Cadenza", true, "themes/cadenza-aurora.css",
        "radial-gradient(circle at 15% 20%, #3b1d70 0%, rgba(59,29,112,0) 45%), radial-gradient(circle at 85% 80%, #0e5e6f 0%, rgba(14,94,111,0) 50%), #0a0f1f",
        "#ffffff", "#dbe4f3", SANS, SANS,
        "#22d3ee", "#04121a", "rgba(255,255,255,0.08)",
        ["#22d3ee", "#c084fc", "#f472b6", "#facc15", "#4ade80", "#60a5fa"]),
    theme!("cadenza-paper", "Paper", "Cadenza", false, "themes/cadenza-paper.css",
        "#faf7f2", "#1c1917", "#44403c", SERIF, SANS,
        "#c2410c", "#ffffff", "rgba(120,113,108,0.10)",
        ["#c2410c", "#0f766e", "#1d4ed8", "#a16207", "#be123c", "#4d7c0f"]),
    theme!("cadenza-slate", "Slate", "Cadenza", false, "themes/cadenza-slate.css",
        "#ffffff", "#0f172a", "#334155", SANS, SANS,
        "#2563eb", "#ffffff", "rgba(100,116,139,0.10)",
        ["#2563eb", "#0891b2", "#7c3aed", "#db2777", "#ea580c", "#16a34a"]),
    theme!("cadenza-ember", "Ember", "Cadenza", true, "themes/cadenza-ember.css",
        "linear-gradient(160deg, #1c1917 0%, #0c0a09 100%)", "#fafaf9", "#d6d3d1", SANS, SANS,
        "#f97316", "#1c1917", "rgba(250,250,249,0.08)",
        ["#f97316", "#facc15", "#ef4444", "#a3e635", "#38bdf8", "#e879f9"]),
    theme!("cadenza-forest", "Forest", "Cadenza", true, "themes/cadenza-forest.css",
        "linear-gradient(160deg, #0f2a1f 0%, #071510 100%)", "#ecfdf5", "#c7e3d4", SERIF, SANS,
        "#4ade80", "#052e16", "rgba(236,253,245,0.08)",
        ["#4ade80", "#fbbf24", "#38bdf8", "#f472b6", "#a78bfa", "#fb923c"]),
    theme!("cadenza-mono", "Mono", "Cadenza", false, "themes/cadenza-mono.css",
        "#f5f5f4", "#0a0a0a", "#262626", "'JetBrains Mono', 'Fira Code', 'DejaVu Sans Mono', Menlo, Consolas, monospace", SANS,
        "#0a0a0a", "#ffffff", "rgba(10,10,10,0.06)",
        ["#0a0a0a", "#525252", "#a3a3a3", "#dc2626", "#2563eb", "#16a34a"]),
    theme!("cadenza-sunrise", "Sunrise", "Cadenza", false, "themes/cadenza-sunrise.css",
        "linear-gradient(135deg, #fff1eb 0%, #ace0f9 100%)", "#1e1b4b", "#312e81", SANS, SANS,
        "#e11d48", "#ffffff", "rgba(255,255,255,0.55)",
        ["#e11d48", "#7c3aed", "#0284c7", "#ea580c", "#059669", "#ca8a04"]),
    // reveal.js built-in themes
    theme!("black", "Black", "reveal.js", true, "vendor/reveal/theme/black.css",
        "#191919", "#ffffff", "#ffffff", "'Source Sans Pro', Helvetica, sans-serif", "'Source Sans Pro', Helvetica, sans-serif",
        "#42affa", "#ffffff", "rgba(255,255,255,0.08)",
        ["#42affa", "#f6c343", "#e05d7e", "#5fd38d", "#b48ef0", "#ff9f5a"]),
    theme!("white", "White", "reveal.js", false, "vendor/reveal/theme/white.css",
        "#ffffff", "#222222", "#222222", "'Source Sans Pro', Helvetica, sans-serif", "'Source Sans Pro', Helvetica, sans-serif",
        "#2a76dd", "#ffffff", "rgba(0,0,0,0.05)",
        ["#2a76dd", "#e8a317", "#d6336c", "#2f9e44", "#7048e8", "#f76707"]),
    theme!("league", "League", "reveal.js", true, "vendor/reveal/theme/league.css",
        "radial-gradient(#555a5f, #1c1e20)", "#eeeeee", "#eeeeee", "'League Gothic', Impact, sans-serif", "Lato, sans-serif",
        "#13daec", "#1c1e20", "rgba(255,255,255,0.08)",
        ["#13daec", "#f6c343", "#e05d7e", "#5fd38d", "#b48ef0", "#ff9f5a"]),
    theme!("beige", "Beige", "reveal.js", false, "vendor/reveal/theme/beige.css",
        "radial-gradient(#ffffff, #f7f2d3)", "#333333", "#333333", "'League Gothic', Impact, sans-serif", "Lato, sans-serif",
        "#8b743d", "#ffffff", "rgba(0,0,0,0.05)",
        ["#8b743d", "#4d7c0f", "#b45309", "#0e7490", "#9f1239", "#6d28d9"]),
    theme!("sky", "Sky", "reveal.js", false, "vendor/reveal/theme/sky.css",
        "radial-gradient(#f7fbfc, #add9e4)", "#333333", "#333333", "Quicksand, sans-serif", "'Open Sans', sans-serif",
        "#3b759e", "#ffffff", "rgba(255,255,255,0.5)",
        ["#3b759e", "#e8a317", "#d6336c", "#2f9e44", "#7048e8", "#f76707"]),
    theme!("night", "Night", "reveal.js", true, "vendor/reveal/theme/night.css",
        "#111111", "#eeeeee", "#eeeeee", "Montserrat, Impact, sans-serif", "'Open Sans', sans-serif",
        "#e7ad52", "#111111", "rgba(255,255,255,0.08)",
        ["#e7ad52", "#42affa", "#e05d7e", "#5fd38d", "#b48ef0", "#ff9f5a"]),
    theme!("serif", "Serif", "reveal.js", false, "vendor/reveal/theme/serif.css",
        "#f0f1eb", "#383d3d", "#000000", "'Palatino Linotype', 'Book Antiqua', Palatino, serif", "'Palatino Linotype', 'Book Antiqua', Palatino, serif",
        "#51483d", "#ffffff", "rgba(0,0,0,0.05)",
        ["#51483d", "#8c6d3f", "#4b6a88", "#7a4b5f", "#4f7a4b", "#9a5b2e"]),
    theme!("simple", "Simple", "reveal.js", false, "vendor/reveal/theme/simple.css",
        "#ffffff", "#000000", "#000000", "'News Cycle', Impact, sans-serif", "Lato, sans-serif",
        "#00008b", "#ffffff", "rgba(0,0,0,0.05)",
        ["#00008b", "#c92a2a", "#2b8a3e", "#e67700", "#5f3dc4", "#0b7285"]),
    theme!("solarized", "Solarized", "reveal.js", false, "vendor/reveal/theme/solarized.css",
        "#fdf6e3", "#586e75", "#657b83", "'League Gothic', Impact, sans-serif", "Lato, sans-serif",
        "#268bd2", "#fdf6e3", "rgba(0,0,0,0.04)",
        ["#268bd2", "#2aa198", "#859900", "#b58900", "#cb4b16", "#d33682"]),
    theme!("moon", "Moon", "reveal.js", true, "vendor/reveal/theme/moon.css",
        "#002b36", "#eee8d5", "#93a1a1", "'League Gothic', Impact, sans-serif", "Lato, sans-serif",
        "#268bd2", "#fdf6e3", "rgba(255,255,255,0.06)",
        ["#268bd2", "#2aa198", "#859900", "#b58900", "#cb4b16", "#d33682"]),
    theme!("dracula", "Dracula", "reveal.js", true, "vendor/reveal/theme/dracula.css",
        "#282a36", "#bd93f9", "#f8f8f2", "'Fira Sans', sans-serif", "'Fira Sans', sans-serif",
        "#ff79c6", "#282a36", "rgba(255,255,255,0.06)",
        ["#bd93f9", "#ff79c6", "#8be9fd", "#50fa7b", "#ffb86c", "#f1fa8c"]),
    theme!("blood", "Blood", "reveal.js", true, "vendor/reveal/theme/blood.css",
        "#222222", "#eeeeee", "#eeeeee", "Ubuntu, sans-serif", "Ubuntu, sans-serif",
        "#aa2233", "#ffffff", "rgba(255,255,255,0.06)",
        ["#aa2233", "#e7ad52", "#42affa", "#5fd38d", "#b48ef0", "#ff9f5a"]),
    theme!("black-contrast", "Black (High Contrast)", "reveal.js", true, "vendor/reveal/theme/black-contrast.css",
        "#000000", "#ffffff", "#ffffff", "'Source Sans Pro', Helvetica, sans-serif", "'Source Sans Pro', Helvetica, sans-serif",
        "#42affa", "#000000", "rgba(255,255,255,0.1)",
        ["#42affa", "#f6c343", "#e05d7e", "#5fd38d", "#b48ef0", "#ff9f5a"]),
    theme!("white-contrast", "White (High Contrast)", "reveal.js", false, "vendor/reveal/theme/white-contrast.css",
        "#ffffff", "#000000", "#000000", "'Source Sans Pro', Helvetica, sans-serif", "'Source Sans Pro', Helvetica, sans-serif",
        "#2a76dd", "#ffffff", "rgba(0,0,0,0.06)",
        ["#2a76dd", "#e8a317", "#d6336c", "#2f9e44", "#7048e8", "#f76707"]),
];

pub const DEFAULT_THEME: &str = "cadenza-midnight";

pub fn find(id: &str) -> &'static ThemeInfo {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}

/// CSS custom properties Cadenza elements rely on, for the given theme.
pub fn vars_css(id: &str) -> String {
    let t = find(id);
    let mut css = format!(
        ":root{{--cz-accent:{};--cz-on-accent:{};--cz-surface:{};--cz-text:{};--cz-heading:{};",
        t.accent, t.on_accent, t.surface, t.text_color, t.heading_color
    );
    for (i, c) in t.palette.iter().enumerate() {
        css.push_str(&format!("--cz-chart-{}:{};", i + 1, c));
    }
    css.push('}');
    // reveal.js tags slides whose background color contrasts with the theme;
    // make Cadenza's text follow, like the theme does for headings.
    if t.dark {
        css.push_str(".reveal section.has-light-background .cz-text-box .cz-text,.reveal section.has-light-background .cz-chart-box,.reveal section.has-light-background .cz-math-box,.reveal section.has-light-background .cz-table{color:#1f2937}");
    } else {
        css.push_str(".reveal section.has-dark-background .cz-text-box .cz-text,.reveal section.has-dark-background .cz-chart-box,.reveal section.has-dark-background .cz-math-box,.reveal section.has-dark-background .cz-table{color:#ffffff}");
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn theme_ids_are_unique() {
        let ids: HashSet<_> = THEMES.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), THEMES.len());
    }

    #[test]
    fn unknown_theme_falls_back() {
        assert_eq!(find("nope").id, DEFAULT_THEME);
        assert!(vars_css("black").contains("--cz-accent:#42affa"));
    }
}
