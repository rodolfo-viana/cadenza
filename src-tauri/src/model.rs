//! The Cadenza document model.
//!
//! A [`Deck`] is a list of [`Slide`]s, each holding absolutely positioned
//! [`Element`]s on a fixed-size canvas (like Keynote or slides.com). The model
//! is serialized as JSON inside `.cadenza` files and sent as-is to the UI.
//!
//! Every struct uses `#[serde(default)]` so that older files and partial
//! updates (JSON merge patches coming from the UI) always deserialize.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::util::new_id;

/// Version of the on-disk format. Bump when a migration becomes necessary.
pub const FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Deck
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Deck {
    pub format_version: u32,
    pub title: String,
    pub author: String,
    /// Theme id, see [`crate::themes`].
    pub theme: String,
    /// Logical slide size in pixels. reveal.js scales it to fit the screen.
    pub width: u32,
    pub height: u32,
    pub settings: DeckSettings,
    /// Extra CSS appended after the theme, for power users.
    pub custom_css: String,
    /// Deck-wide font overrides (empty = theme fonts).
    pub heading_font: String,
    pub body_font: String,
    /// Fonts embedded in the presentation (e.g. from Google Fonts).
    pub web_fonts: Vec<crate::webfonts::WebFont>,
    /// Alignment guides (slide coordinates).
    pub guides: Guides,
    /// A theme made with the Theme Builder, carried inside the presentation
    /// so it renders anywhere. Active when `theme` equals its id.
    pub custom_theme: Option<crate::custom_theme::ThemeDef>,
    pub slides: Vec<Slide>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Guides {
    /// Vertical guides (x positions).
    pub x: Vec<f64>,
    /// Horizontal guides (y positions).
    pub y: Vec<f64>,
}

impl Default for Deck {
    fn default() -> Self {
        Deck {
            format_version: FORMAT_VERSION,
            title: "Untitled Presentation".into(),
            author: String::new(),
            theme: "cadenza-midnight".into(),
            width: 1280,
            height: 720,
            settings: DeckSettings::default(),
            custom_css: String::new(),
            heading_font: String::new(),
            body_font: String::new(),
            web_fonts: Vec::new(),
            guides: Guides::default(),
            custom_theme: None,
            slides: Vec::new(),
        }
    }
}

impl Deck {
    /// The custom theme, when it is the active one.
    pub fn active_custom_theme(&self) -> Option<&crate::custom_theme::ThemeDef> {
        self.custom_theme.as_ref().filter(|t| t.id == self.theme)
    }

    /// Whether the active theme is dark.
    pub fn theme_is_dark(&self) -> bool {
        match self.active_custom_theme() {
            Some(t) => t.dark,
            None => crate::themes::find(&self.theme).dark,
        }
    }

    /// Stylesheet of a built-in theme (None for custom themes, whose CSS is
    /// generated into the deck CSS).
    pub fn theme_href(&self) -> Option<&'static str> {
        if self.active_custom_theme().is_some() {
            None
        } else {
            Some(crate::themes::find(&self.theme).css)
        }
    }

    pub fn slide(&self, id: &str) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id)
    }

    pub fn slide_mut(&mut self, id: &str) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|s| s.id == id)
    }

    pub fn slide_index(&self, id: &str) -> Option<usize> {
        self.slides.iter().position(|s| s.id == id)
    }

    /// Repairs invariants that the UI or an old file might have broken:
    /// unique ids, a non-nested first slide, sane sizes and geometry.
    pub fn normalize(&mut self) {
        self.format_version = FORMAT_VERSION;
        self.width = self.width.clamp(320, 7680);
        self.height = self.height.clamp(240, 4320);
        let mut seen = BTreeSet::new();
        for slide in &mut self.slides {
            if slide.id.is_empty() || !seen.insert(slide.id.clone()) {
                slide.id = new_id("s");
                seen.insert(slide.id.clone());
            }
            let mut el_seen = BTreeSet::new();
            for el in &mut slide.elements {
                if el.id.is_empty() || !el_seen.insert(el.id.clone()) {
                    el.id = new_id("e");
                    el_seen.insert(el.id.clone());
                }
                if el.link.is_empty() {
                    el.link = el.id.clone();
                }
                el.w = sanitize_len(el.w, 4.0);
                el.h = sanitize_len(el.h, 4.0);
                el.x = sanitize_coord(el.x);
                el.y = sanitize_coord(el.y);
                el.rotation = if el.rotation.is_finite() { el.rotation % 360.0 } else { 0.0 };
                el.opacity = if el.opacity.is_finite() { el.opacity.clamp(0.0, 1.0) } else { 1.0 };
            }
        }
        if let Some(first) = self.slides.first_mut() {
            first.nested = false;
        }
        let (w, h) = (self.width as f64, self.height as f64);
        let tidy = |v: &mut Vec<f64>, max: f64| {
            v.retain(|g| g.is_finite() && *g >= -max && *g <= max * 2.0);
            v.iter_mut().for_each(|g| *g = (*g * 2.0).round() / 2.0);
            v.sort_by(|a, b| a.total_cmp(b));
            v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
            v.truncate(200);
        };
        tidy(&mut self.guides.x, w);
        tidy(&mut self.guides.y, h);
        let mut seen = BTreeSet::new();
        self.web_fonts
            .retain(|f| crate::webfonts::valid_family(&f.family) && seen.insert(f.family.clone()));
        if let Some(t) = &mut self.custom_theme {
            // Fonts of an applied theme live in `web_fonts` as assets.
            t.fonts.clear();
            let id = t.id.clone();
            t.normalize();
            if t.id != id && self.theme == id {
                self.theme = t.id.clone();
            }
        }
    }

    /// Names of every bundled asset referenced anywhere in the deck.
    pub fn referenced_assets(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut add = |s: &str| {
            if !s.is_empty() && !crate::util::is_remote(s) {
                out.insert(s.to_string());
            }
        };
        for font in &self.web_fonts {
            for name in crate::webfonts::asset_names(&font.css) {
                add(&name);
            }
        }
        for slide in &self.slides {
            add(&slide.background.image);
            add(&slide.background.video);
            for el in &slide.elements {
                match &el.content {
                    Content::Image(i) => add(&i.src),
                    Content::Video(v) => {
                        add(&v.src);
                        add(&v.poster);
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// Whether any slide uses a given element kind (used to decide which
    /// libraries an exported presentation needs).
    pub fn uses(&self, pred: impl Fn(&Content) -> bool) -> bool {
        self.slides.iter().any(|s| s.elements.iter().any(|e| pred(&e.content)))
    }
}

fn sanitize_len(v: f64, min: f64) -> f64 {
    if v.is_finite() { v.clamp(min, 20000.0) } else { 100.0 }
}

fn sanitize_coord(v: f64) -> f64 {
    if v.is_finite() { v.clamp(-20000.0, 20000.0) } else { 0.0 }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DeckSettings {
    pub transition: String,
    pub transition_speed: String,
    pub background_transition: String,
    pub controls: bool,
    pub progress: bool,
    pub slide_number: bool,
    #[serde(rename = "loop")]
    pub loop_: bool,
    /// Auto-advance interval in seconds (0 = off).
    pub auto_slide: f64,
    pub keyboard: bool,
    pub overview: bool,
    pub touch: bool,
    /// "default", "linear" or "grid".
    pub navigation_mode: String,
    /// "" (normal) or "scroll" (reveal.js scroll view).
    pub view: String,
    /// highlight.js theme used for code blocks: "monokai" or "zenburn".
    pub code_theme: String,
    pub auto_animate_duration: f64,
    pub auto_animate_easing: String,
    /// Show a "fragment" step for every build even when the slide is revisited.
    pub fragment_in_url: bool,
}

impl Default for DeckSettings {
    fn default() -> Self {
        DeckSettings {
            transition: "slide".into(),
            transition_speed: "default".into(),
            background_transition: "fade".into(),
            controls: true,
            progress: true,
            slide_number: false,
            loop_: false,
            auto_slide: 0.0,
            keyboard: true,
            overview: true,
            touch: true,
            navigation_mode: "default".into(),
            view: String::new(),
            code_theme: "monokai".into(),
            auto_animate_duration: 1.0,
            auto_animate_easing: "ease".into(),
            fragment_in_url: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Slides
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Slide {
    pub id: String,
    /// A vertical sub-slide stacked below the previous top-level slide.
    pub nested: bool,
    /// Skipped during the presentation (`data-visibility="hidden"`).
    pub hidden: bool,
    /// Layout the slide was created from (informational).
    pub layout: String,
    pub background: Background,
    /// Empty string means "use the deck default".
    pub transition: String,
    pub transition_speed: String,
    /// Magic Move: animate matching elements from the previous slide.
    pub auto_animate: bool,
    /// Per-slide auto-advance override in seconds (0 = deck default).
    pub auto_slide: f64,
    /// Speaker notes (plain text, newlines preserved).
    pub notes: String,
    pub elements: Vec<Element>,
}

impl Default for Slide {
    fn default() -> Self {
        Slide {
            id: new_id("s"),
            nested: false,
            hidden: false,
            layout: "blank".into(),
            background: Background::default(),
            transition: String::new(),
            transition_speed: String::new(),
            auto_animate: false,
            auto_slide: 0.0,
            notes: String::new(),
            elements: Vec::new(),
        }
    }
}

impl Slide {
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    /// Deep copy with fresh ids. Element `link`s are preserved so that
    /// Magic Move (reveal.js auto-animate) can match them across slides.
    pub fn duplicate(&self) -> Slide {
        let mut copy = self.clone();
        copy.id = new_id("s");
        for el in &mut copy.elements {
            el.id = new_id("e");
        }
        copy
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Background {
    /// "theme", "color", "gradient", "image" or "video".
    pub kind: String,
    pub color: String,
    pub gradient: Gradient,
    /// Asset name or URL.
    pub image: String,
    /// "cover", "contain" or a CSS size.
    pub size: String,
    pub position: String,
    pub repeat: bool,
    pub opacity: f64,
    /// Asset name or URL.
    pub video: String,
    pub video_loop: bool,
    pub video_muted: bool,
}

impl Default for Background {
    fn default() -> Self {
        Background {
            kind: "theme".into(),
            color: "#1e293b".into(),
            gradient: Gradient::default(),
            image: String::new(),
            size: "cover".into(),
            position: "center".into(),
            repeat: false,
            opacity: 1.0,
            video: String::new(),
            video_loop: true,
            video_muted: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Gradient {
    /// "linear" or "radial".
    pub kind: String,
    pub from: String,
    pub to: String,
    pub angle: f64,
}

impl Default for Gradient {
    fn default() -> Self {
        Gradient {
            kind: "linear".into(),
            from: "#4f46e5".into(),
            to: "#0ea5e9".into(),
            angle: 135.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Elements
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Element {
    pub id: String,
    /// Identity used to match elements between slides for Magic Move
    /// (rendered as reveal.js `data-id`).
    pub link: String,
    /// Optional user-facing name shown in the build order list.
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Degrees, clockwise.
    pub rotation: f64,
    pub opacity: f64,
    pub locked: bool,
    pub style: BoxStyle,
    pub build: Build,
    /// Click action during the presentation.
    pub action: Action,
    pub content: Content,
}

impl Default for Element {
    fn default() -> Self {
        let id = new_id("e");
        Element {
            link: id.clone(),
            id,
            name: String::new(),
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 100.0,
            rotation: 0.0,
            opacity: 1.0,
            locked: false,
            style: BoxStyle::default(),
            build: Build::default(),
            action: Action::default(),
            content: Content::Text(TextContent::default()),
        }
    }
}

impl Element {
    pub fn kind(&self) -> &'static str {
        self.content.kind()
    }

    /// Gives the element (and its Magic Move link) a fresh identity.
    pub fn reidentify(&mut self) {
        self.id = new_id("e");
        self.link = self.id.clone();
    }
}

/// Box decoration shared by every element kind. For shapes, `fill` and the
/// border describe the shape itself rather than its bounding box.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BoxStyle {
    pub fill: String,
    pub border_color: String,
    pub border_width: f64,
    /// "solid", "dashed" or "dotted".
    pub border_style: String,
    pub radius: f64,
    /// "none", "soft", "medium", "hard" or "glow".
    pub shadow: String,
    pub shadow_color: String,
    pub padding: f64,
}

impl Default for BoxStyle {
    fn default() -> Self {
        BoxStyle {
            fill: String::new(),
            border_color: "#94a3b8".into(),
            border_width: 0.0,
            border_style: "solid".into(),
            radius: 0.0,
            shadow: "none".into(),
            shadow_color: "rgba(0,0,0,0.35)".into(),
            padding: 0.0,
        }
    }
}

/// Build-in / build-out animations, mapped to reveal.js fragments.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct Build {
    /// reveal.js fragment style for the build-in ("" = none).
    pub effect: String,
    /// Step at which the build-in happens (1-based, `data-fragment-index`).
    pub order: u32,
    /// Fragment style for the build-out ("" = none), e.g. "fade-out".
    pub out_effect: String,
    pub out_order: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    /// "none", "url" or "slide".
    pub kind: String,
    pub url: String,
    pub slide: String,
}

impl Default for Action {
    fn default() -> Self {
        Action {
            kind: "none".into(),
            url: String::new(),
            slide: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Content {
    Text(TextContent),
    Shape(ShapeContent),
    Image(ImageContent),
    Video(VideoContent),
    Code(CodeContent),
    Math(MathContent),
    Embed(EmbedContent),
    Table(TableContent),
    Chart(ChartContent),
    Diagram(DiagramContent),
}

impl Default for Content {
    fn default() -> Self {
        Content::Text(TextContent::default())
    }
}

impl Content {
    pub fn kind(&self) -> &'static str {
        match self {
            Content::Text(_) => "text",
            Content::Shape(_) => "shape",
            Content::Image(_) => "image",
            Content::Video(_) => "video",
            Content::Code(_) => "code",
            Content::Math(_) => "math",
            Content::Embed(_) => "embed",
            Content::Table(_) => "table",
            Content::Chart(_) => "chart",
            Content::Diagram(_) => "diagram",
        }
    }

    /// The rich text carried by this element, if any.
    pub fn text(&self) -> Option<&RichText> {
        match self {
            Content::Text(t) => Some(&t.text),
            Content::Shape(s) => Some(&s.text),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut RichText> {
        match self {
            Content::Text(t) => Some(&mut t.text),
            Content::Shape(s) => Some(&mut s.text),
            _ => None,
        }
    }
}

/// Sanitized rich text plus its paragraph-level style.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct RichText {
    pub html: String,
    /// Shown in the editor while `html` is empty; never presented.
    pub placeholder: String,
    pub style: TextStyle,
}

impl RichText {
    pub fn is_empty(&self) -> bool {
        let stripped = self.html.replace("<br>", "").replace("<p>", "").replace("</p>", "").replace("&nbsp;", "");
        stripped.trim().is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextStyle {
    /// "title", "subtitle", "heading", "body", "caption" or "quote". Roles
    /// pull fonts and colors from the theme.
    pub role: String,
    /// Empty = from the theme.
    pub font_family: String,
    pub font_size: f64,
    /// 0 = from the role/theme.
    pub font_weight: u16,
    pub italic: bool,
    /// Empty = from the theme.
    pub color: String,
    /// "left", "center", "right" or "justify".
    pub align: String,
    /// "top", "middle" or "bottom".
    pub valign: String,
    /// 0 = from the role.
    pub line_height: f64,
    /// In em.
    pub letter_spacing: f64,
    /// "", "none", "uppercase", "lowercase" or "capitalize".
    pub transform: String,
    /// Space after paragraphs, in em.
    pub paragraph_spacing: f64,
    /// Grow the box vertically to fit the text.
    pub auto_height: bool,
    /// "none", "soft", "hard" or "glow".
    pub shadow: String,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            role: "body".into(),
            font_family: String::new(),
            font_size: 32.0,
            font_weight: 0,
            italic: false,
            color: String::new(),
            align: "left".into(),
            valign: "top".into(),
            line_height: 0.0,
            letter_spacing: 0.0,
            transform: String::new(),
            paragraph_spacing: 0.4,
            auto_height: true,
            shadow: "none".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TextContent {
    #[serde(flatten)]
    pub text: RichText,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeContent {
    /// See [`crate::shapes::SHAPES`].
    pub shape: String,
    /// Arrowheads for lines and arrows: "none", "arrow", "dot".
    pub start_cap: String,
    pub end_cap: String,
    pub text: RichText,
}

impl Default for ShapeContent {
    fn default() -> Self {
        let mut text = RichText::default();
        text.style.align = "center".into();
        text.style.valign = "middle".into();
        text.style.auto_height = false;
        text.style.font_size = 28.0;
        ShapeContent {
            shape: "rect".into(),
            start_cap: "none".into(),
            end_cap: "none".into(),
            text,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageContent {
    /// Asset name or URL. Empty = placeholder.
    pub src: String,
    pub alt: String,
    /// "cover", "contain" or "fill".
    pub fit: String,
    /// Focal point for "cover" in percent.
    pub focus_x: f64,
    pub focus_y: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    pub grayscale: f64,
    pub blur: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub saturate: f64,
    /// Intrinsic size, when known.
    pub natural_width: u32,
    pub natural_height: u32,
}

impl Default for ImageContent {
    fn default() -> Self {
        ImageContent {
            src: String::new(),
            alt: String::new(),
            fit: "cover".into(),
            focus_x: 50.0,
            focus_y: 50.0,
            flip_x: false,
            flip_y: false,
            grayscale: 0.0,
            blur: 0.0,
            brightness: 100.0,
            contrast: 100.0,
            saturate: 100.0,
            natural_width: 0,
            natural_height: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoContent {
    /// Asset name, direct URL, or a YouTube/Vimeo page URL.
    pub src: String,
    pub poster: String,
    pub autoplay: bool,
    pub looped: bool,
    pub muted: bool,
    pub controls: bool,
    /// "cover" or "contain".
    pub fit: String,
}

impl Default for VideoContent {
    fn default() -> Self {
        VideoContent {
            src: String::new(),
            poster: String::new(),
            autoplay: true,
            looped: false,
            muted: false,
            controls: true,
            fit: "contain".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeContent {
    pub code: String,
    pub language: String,
    pub line_numbers: bool,
    /// reveal.js step-by-step highlights, e.g. "1|2-3|5".
    pub highlight_steps: String,
    pub font_size: f64,
    pub title: String,
}

impl Default for CodeContent {
    fn default() -> Self {
        CodeContent {
            code: String::new(),
            language: "javascript".into(),
            line_numbers: true,
            highlight_steps: String::new(),
            font_size: 22.0,
            title: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MathContent {
    pub latex: String,
    pub font_size: f64,
    /// Empty = theme text color.
    pub color: String,
    pub align: String,
}

impl Default for MathContent {
    fn default() -> Self {
        MathContent {
            latex: r"e^{i\pi} + 1 = 0".into(),
            font_size: 48.0,
            color: String::new(),
            align: "center".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct EmbedContent {
    pub url: String,
    /// Keep the page interactive during the presentation.
    pub interactive: bool,
}

impl Default for EmbedContent {
    fn default() -> Self {
        EmbedContent {
            url: String::new(),
            interactive: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TableContent {
    /// Cells as sanitized inline HTML, row-major.
    pub rows: Vec<Vec<String>>,
    pub header_row: bool,
    pub header_col: bool,
    pub striped: bool,
    pub font_size: f64,
    /// Empty = theme text color.
    pub color: String,
    pub header_fill: String,
    pub header_color: String,
    pub stripe_fill: String,
    pub grid_color: String,
    /// "left", "center" or "right".
    pub align: String,
}

impl Default for TableContent {
    fn default() -> Self {
        TableContent {
            rows: vec![
                vec!["Feature".into(), "Plan A".into(), "Plan B".into()],
                vec!["Speed".into(), "Fast".into(), "Faster".into()],
                vec!["Price".into(), "$10".into(), "$20".into()],
            ],
            header_row: true,
            header_col: false,
            striped: true,
            font_size: 26.0,
            color: String::new(),
            header_fill: "rgba(99,102,241,0.85)".into(),
            header_color: "#ffffff".into(),
            stripe_fill: "rgba(148,163,184,0.12)".into(),
            grid_color: "rgba(148,163,184,0.35)".into(),
            align: "left".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartContent {
    /// "bar", "column", "line", "area", "pie" or "donut".
    pub chart: String,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    pub show_legend: bool,
    pub show_values: bool,
    pub show_grid: bool,
    pub font_size: f64,
    /// Empty = theme text color.
    pub color: String,
    pub stacked: bool,
}

impl Default for ChartContent {
    fn default() -> Self {
        ChartContent {
            chart: "column".into(),
            labels: vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
            series: vec![
                Series {
                    name: "2025".into(),
                    values: vec![12.0, 19.0, 15.0, 24.0],
                    color: String::new(),
                },
                Series {
                    name: "2026".into(),
                    values: vec![16.0, 22.0, 21.0, 31.0],
                    color: String::new(),
                },
            ],
            show_legend: true,
            show_values: false,
            show_grid: true,
            font_size: 20.0,
            color: String::new(),
            stacked: false,
        }
    }
}

/// A Mermaid diagram. The editor renders the source to SVG (light and dark
/// variants) and stores it, so presentations need no Mermaid library.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiagramContent {
    pub source: String,
    /// "classic", "neutral", "forest", "neo", "redux" or "redux-color".
    pub style: String,
    /// "classic", "handDrawn" or "neo".
    pub look: String,
    /// "auto" (follow the theme), "light" or "dark".
    pub mode: String,
    pub svg_light: String,
    pub svg_dark: String,
}

impl Default for DiagramContent {
    fn default() -> Self {
        DiagramContent {
            source: "flowchart LR\n  A[Idea] --> B{Worth it?}\n  B -- Yes --> C[Build it]\n  B -- No --> D[Rethink]\n  D --> A".into(),
            style: "classic".into(),
            look: "classic".into(),
            mode: "auto".into(),
            svg_light: String::new(),
            svg_dark: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    /// Empty = from the palette.
    pub color: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_round_trips_through_json() {
        let el = Element {
            content: Content::Shape(ShapeContent::default()),
            ..Default::default()
        };
        let json = serde_json::to_value(&el).unwrap();
        assert_eq!(json["content"]["kind"], "shape");
        let back: Element = serde_json::from_value(json).unwrap();
        assert_eq!(back, el);
    }

    #[test]
    fn text_content_is_flat() {
        let el = Element::default();
        let json = serde_json::to_value(&el).unwrap();
        assert_eq!(json["content"]["kind"], "text");
        assert!(json["content"]["style"].is_object());
        assert!(json["content"]["html"].is_string());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let deck: Deck = serde_json::from_str(r#"{"title":"x","slides":[{"elements":[{"content":{"kind":"image"}}]}]}"#).unwrap();
        assert_eq!(deck.width, 1280);
        assert_eq!(deck.slides.len(), 1);
        assert!(matches!(deck.slides[0].elements[0].content, Content::Image(_)));
    }

    #[test]
    fn normalize_fixes_ids_and_first_slide() {
        let mut deck = Deck::default();
        let mut s = Slide {
            nested: true,
            ..Default::default()
        };
        let e = Element::default();
        s.elements.push(e.clone());
        s.elements.push(e);
        deck.slides.push(s.clone());
        deck.slides.push(s);
        deck.normalize();
        assert!(!deck.slides[0].nested);
        assert_ne!(deck.slides[0].id, deck.slides[1].id);
        assert_ne!(deck.slides[0].elements[0].id, deck.slides[0].elements[1].id);
    }

    #[test]
    fn duplicate_keeps_links() {
        let mut s = Slide::default();
        s.elements.push(Element::default());
        let d = s.duplicate();
        assert_ne!(d.id, s.id);
        assert_ne!(d.elements[0].id, s.elements[0].id);
        assert_eq!(d.elements[0].link, s.elements[0].link);
    }

    #[test]
    fn rich_text_emptiness() {
        let mut t = RichText::default();
        assert!(t.is_empty());
        t.html = "<p><br></p>".into();
        assert!(t.is_empty());
        t.html = "<p>Hi</p>".into();
        assert!(!t.is_empty());
    }
}
