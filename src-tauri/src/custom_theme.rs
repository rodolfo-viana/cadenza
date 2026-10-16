//! User-made themes: the Theme Builder's data model, CSS generation, the
//! personal theme library and shareable `.cadenza-theme` files.
//!
//! A theme file is JSON. Its fonts are embedded as data URIs so the file is
//! self-contained; when a theme is applied to a presentation those fonts are
//! converted into presentation assets.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::util::{css_value, new_id};
use crate::webfonts::{ASSET_PREFIX, WebFont};

pub const EXTENSION: &str = "cadenza-theme";
const FORMAT: u32 = 1;

/// The reveal.js rule set every theme shares (from reveal.js' "simple").
const TEMPLATE: &str = include_str!("../../ui/vendor/reveal/theme/simple.css");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeDef {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    pub dark: bool,
    pub background: String,
    /// Second gradient color ("" = solid background).
    pub background2: String,
    pub gradient_angle: f64,
    pub text_color: String,
    pub heading_color: String,
    pub accent: String,
    /// Text drawn on top of the accent color (shape labels).
    pub on_accent: String,
    /// "" = accent.
    pub link_color: String,
    pub palette: Vec<String>,
    /// CSS font families ("" = system sans-serif).
    pub heading_font: String,
    pub body_font: String,
    pub heading_weight: u16,
    pub heading_uppercase: bool,
    /// em
    pub heading_letter_spacing: f64,
    /// Base text size in px (reveal.js main font size).
    pub base_size: f64,
    /// Fonts used by the theme. In theme files their CSS holds data URIs; in
    /// presentations they are moved to `Deck::web_fonts`.
    pub fonts: Vec<WebFont>,
}

impl Default for ThemeDef {
    fn default() -> Self {
        ThemeDef {
            format: FORMAT,
            id: String::new(),
            name: "My Theme".into(),
            author: String::new(),
            dark: true,
            background: "#101828".into(),
            background2: "#1e1b4b".into(),
            gradient_angle: 160.0,
            text_color: "#d0d5dd".into(),
            heading_color: "#ffffff".into(),
            accent: "#7c6cff".into(),
            on_accent: "#ffffff".into(),
            link_color: String::new(),
            palette: vec![
                "#7c6cff".into(),
                "#22d3ee".into(),
                "#f472b6".into(),
                "#facc15".into(),
                "#34d399".into(),
                "#fb923c".into(),
            ],
            heading_font: String::new(),
            body_font: String::new(),
            heading_weight: 700,
            heading_uppercase: false,
            heading_letter_spacing: -0.02,
            base_size: 34.0,
            fonts: Vec::new(),
        }
    }
}

const SANS: &str = "Inter, 'Segoe UI', Cantarell, Ubuntu, Roboto, 'Helvetica Neue', Arial, sans-serif";
const MONO: &str = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'DejaVu Sans Mono', Menlo, Consolas, monospace";

fn color<'a>(v: &'a str, fallback: &'a str) -> &'a str {
    match css_value(v) {
        "" => fallback,
        c => c,
    }
}

fn family(f: &str) -> String {
    let f = css_value(f);
    if f.is_empty() { SANS.to_string() } else { format!("{f}, {SANS}") }
}

impl ThemeDef {
    pub fn is_custom_id(id: &str) -> bool {
        id.starts_with("custom-")
    }

    /// Repairs values from untrusted files.
    pub fn normalize(&mut self) {
        if !Self::is_custom_id(&self.id) || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || self.id.len() > 64 {
            self.id = new_id("custom");
        }
        self.name = self.name.trim().chars().take(60).collect();
        if self.name.is_empty() {
            self.name = "Untitled Theme".into();
        }
        self.author = self.author.trim().chars().take(80).collect();
        self.palette.truncate(6);
        while self.palette.len() < 6 {
            self.palette.push(self.accent.clone());
        }
        self.heading_weight = self.heading_weight.clamp(100, 900);
        self.base_size = if self.base_size.is_finite() { self.base_size.clamp(12.0, 96.0) } else { 34.0 };
        self.heading_letter_spacing = if self.heading_letter_spacing.is_finite() {
            self.heading_letter_spacing.clamp(-0.2, 0.5)
        } else {
            0.0
        };
        self.gradient_angle = if self.gradient_angle.is_finite() {
            self.gradient_angle % 360.0
        } else {
            160.0
        };
        self.fonts.retain(|f| crate::webfonts::valid_family(&f.family));
        self.format = FORMAT;
    }

    pub fn background_css(&self) -> String {
        let bg = color(&self.background, "#111111");
        match css_value(&self.background2) {
            "" => bg.to_string(),
            b2 => format!("linear-gradient({}deg, {bg} 0%, {b2} 100%)", crate::util::num(self.gradient_angle)),
        }
    }

    /// A complete reveal.js theme stylesheet (fonts not included).
    pub fn css(&self) -> String {
        let bg = color(&self.background, "#111111");
        let text = color(&self.text_color, if self.dark { "#eeeeee" } else { "#222222" });
        let heading = color(&self.heading_color, text);
        let accent = color(&self.accent, "#7c6cff");
        let link = color(&self.link_color, accent);
        let root = format!(
            ":root{{--r-background:{bg};--r-background-color:{bg};--r-main-font:{body};--r-main-font-size:{size}px;--r-main-color:{text};\
--r-block-margin:20px;--r-heading-margin:0 0 20px 0;--r-heading-font:{hfont};--r-heading-color:{heading};--r-heading-line-height:1.12;\
--r-heading-letter-spacing:{ls}em;--r-heading-text-transform:{transform};--r-heading-text-shadow:none;--r-heading-font-weight:{weight};\
--r-heading1-text-shadow:none;--r-heading1-size:2.5em;--r-heading2-size:1.6em;--r-heading3-size:1.3em;--r-heading4-size:1em;\
--r-code-font:{MONO};--r-link-color:{link};--r-link-color-dark:{link};--r-link-color-hover:{link};\
--r-selection-background-color:{accent};--r-selection-color:#fff;--r-overlay-element-bg-color:240, 240, 240;--r-overlay-element-fg-color:0, 0, 0}}",
            body = family(&self.body_font),
            hfont = family(&self.heading_font),
            size = crate::util::num(self.base_size),
            ls = crate::util::num(self.heading_letter_spacing),
            transform = if self.heading_uppercase { "uppercase" } else { "none" },
            weight = self.heading_weight,
        );
        let mut css = strip_imports(TEMPLATE);
        css = replace_block(&css, ":root{", &root);
        css = replace_block(
            &css,
            ".reveal-viewport{",
            &format!(".reveal-viewport{{background:{};background-color:{bg}}}", self.background_css()),
        );
        // Light themes fix text on dark slide backgrounds and vice versa.
        if let Some(i) = css.rfind("section.has-") {
            css.truncate(i);
        }
        if self.dark {
            css.push_str(
                "section.has-light-background,section.has-light-background h1,section.has-light-background h2,section.has-light-background h3{color:#1f2937}",
            );
        } else {
            css.push_str(
                "section.has-dark-background,section.has-dark-background h1,section.has-dark-background h2,section.has-dark-background h3{color:#fff}",
            );
        }
        css
    }

    /// Cadenza's theme variables (accent, palette...) for this theme.
    pub fn vars_css(&self) -> String {
        let accent = color(&self.accent, "#7c6cff");
        let text = color(&self.text_color, "#cccccc");
        let surface = if self.dark { "rgba(255,255,255,0.08)" } else { "rgba(0,0,0,0.05)" };
        let mut css = format!(
            ":root{{--cz-accent:{accent};--cz-on-accent:{};--cz-surface:{surface};--cz-text:{text};--cz-heading:{};",
            color(&self.on_accent, "#ffffff"),
            color(&self.heading_color, text)
        );
        for (i, c) in self.palette.iter().enumerate().take(6) {
            css.push_str(&format!("--cz-chart-{}:{};", i + 1, color(c, accent)));
        }
        css.push('}');
        if self.dark {
            css.push_str(".reveal section.has-light-background .cz-text-box .cz-text,.reveal section.has-light-background .cz-chart-box,.reveal section.has-light-background .cz-math-box,.reveal section.has-light-background .cz-table{color:#1f2937}");
        } else {
            css.push_str(".reveal section.has-dark-background .cz-text-box .cz-text,.reveal section.has-dark-background .cz-chart-box,.reveal section.has-dark-background .cz-math-box,.reveal section.has-dark-background .cz-table{color:#ffffff}");
        }
        css
    }
}

fn strip_imports(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("@import") {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find(';') {
            Some(e) => &rest[i + e + 1..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn replace_block(css: &str, start: &str, replacement: &str) -> String {
    match css.find(start) {
        Some(i) => match css[i..].find('}') {
            Some(e) => format!("{}{}{}", &css[..i], replacement, &css[i + e + 1..]),
            None => css.to_string(),
        },
        None => format!("{replacement}{css}"),
    }
}

// ---------------------------------------------------------------------------
// Fonts: assets ↔ data URIs
// ---------------------------------------------------------------------------

/// Copies the deck fonts the theme uses into the theme, as data URIs.
pub fn embed_fonts(def: &mut ThemeDef, deck_fonts: &[WebFont], assets_dir: &Path) {
    let wanted: Vec<String> = [&def.heading_font, &def.body_font]
        .iter()
        .map(|f| f.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"').to_string())
        .filter(|f| !f.is_empty())
        .collect();
    def.fonts = deck_fonts
        .iter()
        .filter(|f| wanted.contains(&f.family))
        .map(|f| WebFont {
            family: f.family.clone(),
            source: f.source.clone(),
            css: crate::webfonts::resolve_css(&f.css, &|name: &str| {
                fs::read(assets_dir.join(name))
                    .map(|b| format!("data:font/woff2;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
                    .unwrap_or_default()
            }),
        })
        .collect();
}

/// Moves a theme's embedded fonts into presentation assets (`store` saves
/// bytes and returns the asset name). Returns the fonts as deck web fonts.
pub fn extract_fonts(def: &ThemeDef, store: &dyn Fn(&[u8]) -> anyhow::Result<String>) -> anyhow::Result<Vec<WebFont>> {
    let mut out = Vec::new();
    for f in &def.fonts {
        let mut css = String::new();
        let mut rest = f.css.as_str();
        while let Some(i) = rest.find("data:") {
            css.push_str(&rest[..i]);
            let tail = &rest[i..];
            let end = tail.find([')', '"', '\'']).unwrap_or(tail.len());
            let uri = &tail[..end];
            let name = match uri.split_once(";base64,") {
                Some((_, b64)) => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(b64.trim())
                        .context("Corrupted font in theme")?;
                    store(&bytes)?
                }
                None => bail!("Unsupported font data in theme"),
            };
            css.push_str(ASSET_PREFIX);
            css.push_str(&name);
            rest = &tail[end..];
        }
        css.push_str(rest);
        out.push(WebFont {
            family: f.family.clone(),
            source: f.source.clone(),
            css,
        });
    }
    Ok(out)
}

/// The theme as standalone reveal.js CSS (fonts inlined) for sharing with
/// people who use reveal.js directly.
pub fn reveal_css(def: &ThemeDef) -> String {
    let mut css = format!("/* reveal.js theme “{}” — made with Cadenza */\n", def.name.replace("*/", ""));
    for f in &def.fonts {
        css.push_str(&f.css);
        css.push('\n');
    }
    css.push_str(&def.css());
    css.push('\n');
    css
}

// ---------------------------------------------------------------------------
// Library & files
// ---------------------------------------------------------------------------

pub fn library_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("themes")
}

pub fn read_file(path: &Path) -> anyhow::Result<ThemeDef> {
    let bytes = fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    if bytes.len() > 30 * 1024 * 1024 {
        bail!("Theme file is too large");
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes).context("This is not a Cadenza theme file")?;
    if v.get("format").and_then(|f| f.as_u64()).unwrap_or(1) as u32 > FORMAT {
        bail!("This theme was made with a newer version of Cadenza");
    }
    let mut def: ThemeDef = serde_json::from_value(v).context("This is not a Cadenza theme file")?;
    def.normalize();
    Ok(def)
}

pub fn write_file(path: &Path, def: &ThemeDef) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(def)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn list(config_dir: &Path) -> Vec<ThemeDef> {
    let Ok(entries) = fs::read_dir(library_dir(config_dir)) else { return vec![] };
    let mut out: Vec<ThemeDef> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == EXTENSION))
        .filter_map(|e| read_file(&e.path()).ok())
        .collect();
    out.sort_by_key(|t| t.name.to_lowercase());
    out
}

pub fn save(config_dir: &Path, def: &ThemeDef) -> anyhow::Result<()> {
    write_file(&library_dir(config_dir).join(format!("{}.{EXTENSION}", def.id)), def)
}

pub fn find(config_dir: &Path, id: &str) -> Option<ThemeDef> {
    if !ThemeDef::is_custom_id(id) || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    read_file(&library_dir(config_dir).join(format!("{id}.{EXTENSION}"))).ok()
}

pub fn delete(config_dir: &Path, id: &str) -> anyhow::Result<()> {
    if find(config_dir, id).is_some() {
        fs::remove_file(library_dir(config_dir).join(format!("{id}.{EXTENSION}")))?;
    }
    Ok(())
}

/// Starting point for "customize this theme": a built-in theme as a def,
/// with the theme's name (the editor marks it as custom in the interface
/// language).
pub fn from_builtin(id: &str) -> ThemeDef {
    let t = crate::themes::find(id);
    let mut def = ThemeDef {
        id: String::new(),
        name: t.name.into(),
        dark: t.dark,
        text_color: t.text_color.into(),
        heading_color: t.heading_color.into(),
        accent: t.accent.into(),
        on_accent: t.on_accent.into(),
        palette: t.palette.iter().map(|c| c.to_string()).collect(),
        ..Default::default()
    };
    // Backgrounds: keep solid colors and simple two-stop gradients.
    let bg = t.preview_bg;
    if bg.starts_with('#') {
        def.background = bg.into();
        def.background2 = String::new();
    } else {
        let colors: Vec<&str> = bg.split(['(', ',', ')', ' ']).filter(|p| p.starts_with('#')).collect();
        def.background = colors.first().copied().unwrap_or(if t.dark { "#111111" } else { "#ffffff" }).into();
        def.background2 = colors.get(1).copied().unwrap_or("").into();
    }
    def.normalize();
    def
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_reveal_theme_css() {
        let def = ThemeDef {
            heading_font: "'Lora'".into(),
            heading_uppercase: true,
            ..Default::default()
        };
        let css = def.css();
        assert!(css.contains("--r-heading-font:'Lora', Inter"));
        assert!(css.contains("--r-heading-text-transform:uppercase"));
        assert!(css.contains(".reveal-viewport{background:linear-gradient(160deg"));
        assert!(!css.contains("@import"));
        assert!(css.contains(".reveal h1"));
        assert!(css.ends_with("{color:#1f2937}"));
    }

    #[test]
    fn rejects_css_injection() {
        let def = ThemeDef {
            accent: "red;}</style><script>".into(),
            heading_font: "x;}body{display:none".into(),
            ..Default::default()
        };
        let css = def.css();
        assert!(!css.contains("<script>") && !css.contains("display:none"));
    }

    #[test]
    fn fonts_round_trip_through_data_uris() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("abc.woff2"), b"FONTDATA").unwrap();
        let deck_fonts = vec![WebFont {
            family: "Lora".into(),
            source: "google".into(),
            css: "src:url(cz-asset:abc.woff2)".into(),
        }];
        let mut def = ThemeDef {
            heading_font: "'Lora'".into(),
            ..Default::default()
        };
        embed_fonts(&mut def, &deck_fonts, dir.path());
        assert_eq!(def.fonts.len(), 1);
        assert!(def.fonts[0].css.contains("data:font/woff2;base64,Rk9OVERBVEE="));
        let stored = std::cell::RefCell::new(Vec::new());
        let back = extract_fonts(&def, &|b: &[u8]| {
            stored.borrow_mut().push(b.to_vec());
            Ok("new.woff2".to_string())
        })
        .unwrap();
        assert_eq!(back[0].css, "src:url(cz-asset:new.woff2)");
        assert_eq!(stored.borrow()[0], b"FONTDATA");
    }

    #[test]
    fn library_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut def = ThemeDef::default();
        def.normalize();
        save(dir.path(), &def).unwrap();
        let list = list(dir.path());
        assert_eq!(list.len(), 1);
        assert_eq!(find(dir.path(), &def.id).unwrap(), def);
        delete(dir.path(), &def.id).unwrap();
        assert!(super::list(dir.path()).is_empty());
        assert!(find(dir.path(), "../../etc/passwd").is_none());
    }

    #[test]
    fn builtin_themes_convert() {
        let def = from_builtin("cadenza-midnight");
        assert!(def.dark && def.background.starts_with('#') && !def.background2.is_empty());
        let def = from_builtin("white");
        assert!(!def.dark && def.background2.is_empty());
    }
}
