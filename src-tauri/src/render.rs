//! Turns a [`Deck`] into reveal.js markup.
//!
//! This is the single source of truth for how slides look: the editor canvas,
//! the slide thumbnails, the live presentation and every export all display
//! HTML produced here, which keeps the editor faithful to the final result.

use serde::Serialize;
use serde_json::{Value, json};
use std::fmt::Write as _;

use crate::chart::chart_svg;
use crate::mathtext::{has_math, wrap_math};
use crate::model::*;
use crate::shapes::shape_svg;
use crate::themes;
use crate::util::{brightness, css_value, esc, is_remote, num, parse_color};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Editor canvas and thumbnails: no fragments, placeholders visible,
    /// heavy embeds replaced by previews.
    Editor,
    /// Live presentation and exports.
    Show,
}

/// Rendering context: the mode plus a way to turn asset names into URLs.
pub struct Ctx<'a> {
    pub mode: Mode,
    pub asset_url: &'a dyn Fn(&str) -> String,
}

impl Ctx<'_> {
    fn url(&self, src: &str) -> String {
        if src.is_empty() || is_remote(src) { src.to_string() } else { (self.asset_url)(src) }
    }

    fn editor(&self) -> bool {
        self.mode == Mode::Editor
    }
}

/// What the editor needs to display one slide.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlideRender {
    /// Background layer plus elements.
    pub html: String,
    /// Extra classes for the `<section>` (dark/light background hints).
    pub classes: String,
}

// ---------------------------------------------------------------------------
// Slides
// ---------------------------------------------------------------------------

/// Renders a slide for the editor canvas / thumbnails.
pub fn slide_for_editor(deck: &Deck, slide: &Slide, ctx: &Ctx) -> SlideRender {
    let mut html = String::with_capacity(2048);
    html.push_str(&editor_background(slide, ctx));
    for el in &slide.elements {
        html.push_str(&element(deck, el, ctx));
    }
    SlideRender { html, classes: background_class(slide).to_string() }
}

/// reveal.js marks slides with a solid background color as dark or light so
/// themes can adapt text colors; the editor mirrors that.
pub fn background_class(slide: &Slide) -> &'static str {
    if slide.background.kind != "color" {
        return "";
    }
    match parse_color(&slide.background.color) {
        Some(c) if c.a > 0.0 => {
            if brightness(c) < 128.0 { "has-dark-background" } else { "has-light-background" }
        }
        _ => "",
    }
}

fn gradient_css(g: &Gradient) -> String {
    let from = css_value(&g.from);
    let to = css_value(&g.to);
    if g.kind == "radial" {
        format!("radial-gradient(circle at center, {from}, {to})")
    } else {
        format!("linear-gradient({}deg, {from}, {to})", num(g.angle))
    }
}

fn editor_background(slide: &Slide, ctx: &Ctx) -> String {
    let bg = &slide.background;
    match bg.kind.as_str() {
        "color" if !css_value(&bg.color).is_empty() => {
            format!(r#"<div class="cz-bg" style="background:{}"></div>"#, esc(css_value(&bg.color)))
        }
        "gradient" => format!(r#"<div class="cz-bg" style="background:{}"></div>"#, esc(&gradient_css(&bg.gradient))),
        "image" if !bg.image.is_empty() => format!(
            r#"<div class="cz-bg" style="background-image:url(&quot;{}&quot;);background-size:{};background-position:{};background-repeat:{};opacity:{}"></div>"#,
            esc(&ctx.url(&bg.image)),
            esc(css_value(&bg.size)),
            esc(css_value(&bg.position)),
            if bg.repeat { "repeat" } else { "no-repeat" },
            num(bg.opacity.clamp(0.0, 1.0))
        ),
        "video" if !bg.video.is_empty() => format!(
            r#"<div class="cz-bg" style="opacity:{}"><video class="cz-bg-video" src="{}#t=0.1" preload="metadata" muted playsinline></video></div>"#,
            num(bg.opacity.clamp(0.0, 1.0)),
            esc(&ctx.url(&bg.video))
        ),
        _ => String::new(),
    }
}

/// Attributes of the `<section>` element of a slide in show mode.
fn section_attrs(slide: &Slide, ctx: &Ctx) -> String {
    let mut a = format!(r#" id="{}""#, esc(&slide.id));
    if !slide.transition.is_empty() {
        let _ = write!(a, r#" data-transition="{}""#, esc(&slide.transition));
    }
    if !slide.transition_speed.is_empty() && slide.transition_speed != "default" {
        let _ = write!(a, r#" data-transition-speed="{}""#, esc(&slide.transition_speed));
    }
    if slide.auto_animate {
        a.push_str(" data-auto-animate");
    }
    if slide.auto_slide > 0.0 {
        let _ = write!(a, r#" data-autoslide="{}""#, (slide.auto_slide * 1000.0).round() as i64);
    }
    let bg = &slide.background;
    match bg.kind.as_str() {
        "color" if !css_value(&bg.color).is_empty() => {
            let _ = write!(a, r#" data-background-color="{}""#, esc(css_value(&bg.color)));
        }
        "gradient" => {
            let _ = write!(a, r#" data-background-gradient="{}""#, esc(&gradient_css(&bg.gradient)));
        }
        "image" if !bg.image.is_empty() => {
            let _ = write!(
                a,
                r#" data-background-image="{}" data-background-size="{}" data-background-position="{}" data-background-repeat="{}" data-background-opacity="{}""#,
                esc(&ctx.url(&bg.image)),
                esc(css_value(&bg.size)),
                esc(css_value(&bg.position)),
                if bg.repeat { "repeat" } else { "no-repeat" },
                num(bg.opacity.clamp(0.0, 1.0))
            );
        }
        "video" if !bg.video.is_empty() => {
            let _ = write!(a, r#" data-background-video="{}" data-background-opacity="{}""#, esc(&ctx.url(&bg.video)), num(bg.opacity.clamp(0.0, 1.0)));
            if bg.video_loop {
                a.push_str(" data-background-video-loop");
            }
            if bg.video_muted {
                a.push_str(" data-background-video-muted");
            }
        }
        _ => {}
    }
    a
}

/// One `<section>` for a slide (show mode), including speaker notes.
pub fn section(deck: &Deck, slide: &Slide, ctx: &Ctx) -> String {
    let mut s = String::with_capacity(2048);
    let _ = write!(s, "<section{}>", section_attrs(slide, ctx));
    let _ = write!(s, r#"<div class="cz-slide" style="width:{}px;height:{}px">"#, deck.width, deck.height);
    for el in &slide.elements {
        s.push_str(&element(deck, el, ctx));
    }
    s.push_str("</div>");
    if !slide.notes.trim().is_empty() {
        let notes: Vec<String> = slide.notes.lines().map(esc).collect();
        let _ = write!(s, r#"<aside class="notes">{}</aside>"#, notes.join("<br>"));
    }
    s.push_str("</section>");
    s
}

/// Visible slides grouped into horizontal positions: each group is a
/// top-level slide followed by its nested (vertical) slides. Hidden
/// (skipped) slides are left out, and a nested slide whose head is hidden
/// takes its place.
pub fn visible_groups(deck: &Deck) -> Vec<Vec<&Slide>> {
    let mut groups: Vec<Vec<&Slide>> = Vec::new();
    for slide in deck.slides.iter() {
        if slide.hidden {
            continue;
        }
        match groups.last_mut() {
            Some(group) if slide.nested => group.push(slide),
            _ => groups.push(vec![slide]),
        }
    }
    groups
}

/// All visible slides as reveal.js sections, grouping nested slides into
/// vertical stacks.
pub fn sections(deck: &Deck, ctx: &Ctx) -> String {
    let mut out = String::with_capacity(deck.slides.len() * 2048);
    for group in visible_groups(deck) {
        if group.len() > 1 {
            out.push_str("<section>");
            for slide in group {
                out.push_str(&section(deck, slide, ctx));
            }
            out.push_str("</section>");
        } else {
            out.push_str(&section(deck, group[0], ctx));
        }
    }
    out
}

/// Maps a slide id to its reveal.js (h, v) indices in the presentation. A
/// hidden slide maps to the next visible one (or the last slide).
pub fn slide_indices(deck: &Deck, slide_id: &str) -> Option<(usize, usize)> {
    let start = deck.slide_index(slide_id)?;
    let target = deck.slides[start..]
        .iter()
        .find(|s| !s.hidden)
        .or_else(|| deck.slides.iter().rev().find(|s| !s.hidden))?;
    for (h, group) in visible_groups(deck).iter().enumerate() {
        if let Some(v) = group.iter().position(|s| s.id == target.id) {
            return Some((h, v));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Elements
// ---------------------------------------------------------------------------

fn shadow_css(preset: &str, color: &str) -> Option<String> {
    let color = match css_value(color) {
        "" => "rgba(0,0,0,0.35)",
        c => c,
    };
    Some(match preset {
        "soft" => format!("0 8px 24px {color}"),
        "medium" => format!("0 14px 36px {color}"),
        "hard" => format!("8px 8px 0 {color}"),
        "glow" => format!("0 0 28px {color}"),
        _ => return None,
    })
}

fn box_css(style: &BoxStyle, with_fill: bool) -> String {
    let mut css = String::new();
    if with_fill {
        let fill = css_value(&style.fill);
        if !fill.is_empty() {
            let _ = write!(css, "background:{fill};");
        }
        if style.border_width > 0.0 {
            let _ = write!(
                css,
                "border:{}px {} {};",
                num(style.border_width),
                match style.border_style.as_str() {
                    "dashed" => "dashed",
                    "dotted" => "dotted",
                    _ => "solid",
                },
                css_value(&style.border_color)
            );
        }
        if let Some(sh) = shadow_css(&style.shadow, &style.shadow_color) {
            let _ = write!(css, "box-shadow:{sh};");
        }
    }
    if style.radius > 0.0 {
        let _ = write!(css, "border-radius:{}px;", num(style.radius));
    }
    if style.padding > 0.0 {
        let _ = write!(css, "padding:{}px;", num(style.padding));
    }
    css
}

/// Renders one element (wrapper + content).
pub fn element(deck: &Deck, el: &Element, ctx: &Ctx) -> String {
    let inner = match &el.content {
        Content::Text(t) => text_element(&t.text, &el.style, ctx),
        Content::Shape(s) => shape_element(s, el),
        Content::Image(i) => image_element(i, el, ctx),
        Content::Video(v) => video_element(v, el, ctx),
        Content::Code(c) => code_element(c, el, deck),
        Content::Math(m) => math_element(m, el),
        Content::Embed(e) => embed_element(e, el, ctx),
        Content::Table(t) => table_element(t, el),
        Content::Chart(c) => chart_element(c, el),
        Content::Diagram(d) => diagram_element(d, el, deck, ctx),
    };
    let Some(inner) = inner else { return String::new() };

    // Auto-height text boxes grow with their content: the stored height is
    // only a minimum, so the editor and every export lay them out alike.
    let auto = matches!(&el.content, Content::Text(t) if t.text.style.auto_height);
    let mut style = format!(
        "left:{}px;top:{}px;width:{}px;{}:{}px;",
        num(el.x),
        num(el.y),
        num(el.w),
        if auto { "min-height" } else { "height" },
        num(el.h)
    );
    if el.opacity < 1.0 {
        let _ = write!(style, "opacity:{};", num(el.opacity.clamp(0.0, 1.0)));
    }
    let mut out = String::with_capacity(inner.len() + 256);
    let _ = write!(out, r#"<div class="cz-el cz-{}{}""#, el.kind(), if auto { " cz-auto" } else { "" });
    if ctx.editor() {
        let _ = write!(out, r#" data-cz-id="{}""#, esc(&el.id));
    } else {
        let _ = write!(out, r#" data-id="{}""#, esc(&el.link));
    }
    let _ = write!(out, r#" style="{}">"#, style);

    // Rotation lives on an inner node so that auto-animate (which drives the
    // wrapper's transform) and rotation do not fight.
    if el.rotation.abs() > 0.001 {
        let _ = write!(out, r#"<div class="cz-rot" style="transform:rotate({}deg)">"#, num(el.rotation));
    } else {
        out.push_str(r#"<div class="cz-rot">"#);
    }

    let mut closers = 0;
    if !ctx.editor() {
        if !el.build.out_effect.is_empty() {
            let _ = write!(
                out,
                r#"<div class="cz-frag fragment {}"{}>"#,
                esc(&el.build.out_effect),
                fragment_index(el.build.out_order)
            );
            closers += 1;
        }
        if !el.build.effect.is_empty() {
            let _ = write!(
                out,
                r#"<div class="cz-frag fragment {}"{}>"#,
                esc(&el.build.effect),
                fragment_index(el.build.order)
            );
            closers += 1;
        }
        if let Some(href) = action_href(&el.action) {
            let target = if el.action.kind == "url" { r#" target="_blank" rel="noopener""# } else { "" };
            let _ = write!(out, r#"<a class="cz-link" href="{}"{}>"#, esc(&href), target);
            out.push_str(&inner);
            out.push_str("</a>");
        } else {
            out.push_str(&inner);
        }
    } else {
        out.push_str(&inner);
    }
    for _ in 0..closers {
        out.push_str("</div>");
    }
    out.push_str("</div></div>");
    out
}

fn fragment_index(order: u32) -> String {
    if order > 0 { format!(r#" data-fragment-index="{}""#, order) } else { String::new() }
}

fn action_href(action: &Action) -> Option<String> {
    match action.kind.as_str() {
        "url" => {
            let u = action.url.trim();
            let l = u.to_ascii_lowercase();
            if l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:") {
                Some(u.to_string())
            } else if !u.is_empty() && !l.contains(':') {
                Some(format!("https://{u}"))
            } else {
                None
            }
        }
        "slide" if !action.slide.is_empty() => Some(format!("#/{}", action.slide)),
        "next" => Some("#/next".into()),
        _ => None,
    }
}

fn text_css(t: &TextStyle) -> String {
    let mut css = format!("font-size:{}px;", num(t.font_size.clamp(4.0, 800.0)));
    let family = css_value(&t.font_family);
    if !family.is_empty() {
        let _ = write!(css, "font-family:{family};");
    }
    let color = css_value(&t.color);
    if !color.is_empty() {
        let _ = write!(css, "color:{color};");
    }
    if t.font_weight > 0 {
        let _ = write!(css, "font-weight:{};", t.font_weight.clamp(100, 900));
    }
    if t.italic {
        css.push_str("font-style:italic;");
    }
    match t.align.as_str() {
        "center" | "right" | "justify" => {
            let _ = write!(css, "text-align:{};", t.align);
        }
        _ => css.push_str("text-align:left;"),
    }
    if t.line_height > 0.0 {
        let _ = write!(css, "line-height:{};", num(t.line_height));
    }
    if t.letter_spacing != 0.0 {
        let _ = write!(css, "letter-spacing:{}em;", num(t.letter_spacing));
    }
    if matches!(t.transform.as_str(), "none" | "uppercase" | "lowercase" | "capitalize") {
        let _ = write!(css, "text-transform:{};", t.transform);
    }
    let _ = write!(css, "--cz-ps:{}em;", num(t.paragraph_spacing.clamp(0.0, 5.0)));
    match t.shadow.as_str() {
        "soft" => css.push_str("text-shadow:0 2px 10px rgba(0,0,0,0.45);"),
        "hard" => css.push_str("text-shadow:3px 3px 0 rgba(0,0,0,0.55);"),
        "glow" => css.push_str("text-shadow:0 0 18px var(--cz-accent);"),
        _ => {}
    }
    css
}

fn valign_class(v: &str) -> &'static str {
    match v {
        "middle" => "cz-v-middle",
        "bottom" => "cz-v-bottom",
        _ => "cz-v-top",
    }
}

fn role_class(role: &str) -> &'static str {
    match role {
        "title" => "cz-role-title",
        "subtitle" => "cz-role-subtitle",
        "heading" => "cz-role-heading",
        "caption" => "cz-role-caption",
        "quote" => "cz-role-quote",
        _ => "cz-role-body",
    }
}

/// The text body shared by text boxes and shapes.
fn rich_text_body(text: &RichText, ctx: &Ctx, extra_class: &str) -> Option<String> {
    let empty = text.is_empty();
    if empty && !ctx.editor() {
        return None;
    }
    let body = if empty {
        format!(r#"<p class="cz-placeholder">{}</p>"#, esc(&text.placeholder))
    } else {
        wrap_math(&text.html).into_owned()
    };
    Some(format!(
        // dir="auto": right-to-left text (Arabic, Hebrew…) flows correctly.
        r#"<div class="cz-text {}{}{}" dir="auto" style="{}">{}</div>"#,
        role_class(&text.style.role),
        if empty { " cz-empty" } else { "" },
        extra_class,
        esc(&text_css(&text.style)),
        body
    ))
}

fn text_element(text: &RichText, style: &BoxStyle, ctx: &Ctx) -> Option<String> {
    if text.is_empty() && !ctx.editor() {
        return None;
    }
    let body = rich_text_body(text, ctx, "")?;
    Some(format!(
        r#"<div class="cz-box cz-text-box {}" style="{}">{}</div>"#,
        valign_class(&text.style.valign),
        esc(&box_css(style, true)),
        body
    ))
}

fn shape_element(s: &ShapeContent, el: &Element) -> Option<String> {
    let svg = shape_svg(&s.shape, el.w, el.h, &el.style, &s.start_cap, &s.end_cap);
    let filter = shadow_css(&el.style.shadow, &el.style.shadow_color)
        .map(|_| {
            let color = match css_value(&el.style.shadow_color) {
                "" => "rgba(0,0,0,0.35)",
                c => c,
            };
            match el.style.shadow.as_str() {
                "hard" => format!("filter:drop-shadow(6px 6px 0 {color});"),
                "glow" => format!("filter:drop-shadow(0 0 16px {color});"),
                _ => format!("filter:drop-shadow(0 8px 16px {color});"),
            }
        })
        .unwrap_or_default();
    let mut out = format!(r#"<div class="cz-box cz-shape-box" style="{}">{}"#, esc(&filter), svg);
    if s.shape != "line" {
        // Shapes always carry a (possibly empty) text layer so that the
        // editor can start typing into them.
        let empty = s.text.is_empty();
        let pad = if el.style.padding > 0.0 { el.style.padding } else { 12.0 };
        let body = if empty {
            format!(
                r#"<div class="cz-text {} cz-empty" dir="auto" style="{}"></div>"#,
                role_class(&s.text.style.role),
                esc(&text_css(&s.text.style))
            )
        } else {
            format!(
                r#"<div class="cz-text {}" dir="auto" style="{}">{}</div>"#,
                role_class(&s.text.style.role),
                esc(&text_css(&s.text.style)),
                wrap_math(&s.text.html)
            )
        };
        let _ = write!(
            out,
            r#"<div class="cz-shape-text {}" style="padding:{}px">{}</div>"#,
            valign_class(&s.text.style.valign),
            num(pad),
            body
        );
    }
    out.push_str("</div>");
    Some(out)
}

fn image_element(i: &ImageContent, el: &Element, ctx: &Ctx) -> Option<String> {
    let box_style = box_css(&el.style, true);
    if i.src.is_empty() {
        if !ctx.editor() {
            return None;
        }
        return Some(format!(
            r#"<div class="cz-box cz-image-box cz-image-empty" style="{}"><div class="cz-placeholder-media"><span class="cz-ph-icon cz-ph-image"></span><span>Double-click to choose an image</span></div></div>"#,
            esc(&box_style)
        ));
    }
    let mut img_style = format!(
        "object-fit:{};object-position:{}% {}%;",
        match i.fit.as_str() {
            "contain" => "contain",
            "fill" => "fill",
            _ => "cover",
        },
        num(i.focus_x.clamp(0.0, 100.0)),
        num(i.focus_y.clamp(0.0, 100.0))
    );
    let mut filters = Vec::new();
    if i.grayscale > 0.0 {
        filters.push(format!("grayscale({}%)", num(i.grayscale.clamp(0.0, 100.0))));
    }
    if i.blur > 0.0 {
        filters.push(format!("blur({}px)", num(i.blur.clamp(0.0, 100.0))));
    }
    for (name, v) in [("brightness", i.brightness), ("contrast", i.contrast), ("saturate", i.saturate)] {
        if (v - 100.0).abs() > 0.01 {
            filters.push(format!("{name}({}%)", num(v.clamp(0.0, 400.0))));
        }
    }
    if !filters.is_empty() {
        let _ = write!(img_style, "filter:{};", filters.join(" "));
    }
    if i.flip_x || i.flip_y {
        let _ = write!(img_style, "transform:scale({},{});", if i.flip_x { -1 } else { 1 }, if i.flip_y { -1 } else { 1 });
    }
    Some(format!(
        r#"<div class="cz-box cz-image-box" style="{}"><img src="{}" alt="{}" draggable="false" style="{}"></div>"#,
        esc(&box_style),
        esc(&ctx.url(&i.src)),
        esc(&i.alt),
        esc(&img_style)
    ))
}

/// Recognizes YouTube and Vimeo page URLs.
pub fn video_embed(src: &str) -> Option<(&'static str, String)> {
    let s = src.trim();
    let after = |marker: &str| -> Option<String> {
        let idx = s.find(marker)? + marker.len();
        let id: String = s[idx..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        if id.is_empty() { None } else { Some(id) }
    };
    if s.contains("youtube.com") || s.contains("youtu.be") {
        let id = after("v=").or_else(|| after("youtu.be/")).or_else(|| after("/embed/")).or_else(|| after("/shorts/"))?;
        return Some(("youtube", id));
    }
    if s.contains("vimeo.com") {
        let id = after("vimeo.com/video/").or_else(|| after("vimeo.com/"))?;
        if id.chars().all(|c| c.is_ascii_digit()) {
            return Some(("vimeo", id));
        }
    }
    None
}

fn video_element(v: &VideoContent, el: &Element, ctx: &Ctx) -> Option<String> {
    let box_style = box_css(&el.style, true);
    if v.src.trim().is_empty() {
        if !ctx.editor() {
            return None;
        }
        return Some(format!(
            r#"<div class="cz-box cz-video-box cz-image-empty" style="{}"><div class="cz-placeholder-media"><span class="cz-ph-icon cz-ph-video"></span><span>Double-click to choose a video</span></div></div>"#,
            esc(&box_style)
        ));
    }
    if let Some((provider, id)) = video_embed(&v.src) {
        if ctx.editor() {
            let thumb = if provider == "youtube" {
                format!(r#"<img src="https://img.youtube.com/vi/{}/hqdefault.jpg" alt="" draggable="false" onerror="this.remove()">"#, esc(&id))
            } else {
                String::new()
            };
            return Some(format!(
                r#"<div class="cz-box cz-video-box cz-embed-preview" style="{}">{}<div class="cz-placeholder-media"><span class="cz-ph-icon cz-ph-play"></span><span>{}</span></div></div>"#,
                esc(&box_style),
                thumb,
                if provider == "youtube" { "YouTube video" } else { "Vimeo video" }
            ));
        }
        let url = if provider == "youtube" {
            let mut u = format!("https://www.youtube.com/embed/{id}?rel=0&enablejsapi=1");
            if v.muted {
                u.push_str("&mute=1");
            }
            if v.looped {
                let _ = write!(u, "&loop=1&playlist={id}");
            }
            if !v.controls {
                u.push_str("&controls=0");
            }
            u
        } else {
            let mut u = format!("https://player.vimeo.com/video/{id}?api=1");
            if v.muted {
                u.push_str("&muted=1");
            }
            if v.looped {
                u.push_str("&loop=1");
            }
            u
        };
        return Some(format!(
            r#"<div class="cz-box cz-video-box" style="{}"><iframe data-src="{}"{} allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen></iframe></div>"#,
            esc(&box_style),
            esc(&url),
            if v.autoplay { " data-autoplay" } else { "" }
        ));
    }
    let fit = if v.fit == "cover" { "cover" } else { "contain" };
    let url = ctx.url(&v.src);
    if ctx.editor() {
        let poster = if v.poster.is_empty() { String::new() } else { format!(r#" poster="{}""#, esc(&ctx.url(&v.poster))) };
        return Some(format!(
            r##"<div class="cz-box cz-video-box" style="{}"><video src="{}#t=0.1" preload="metadata" muted playsinline{} style="object-fit:{}"></video><span class="cz-video-badge"></span></div>"##,
            esc(&box_style),
            esc(&url),
            poster,
            fit
        ));
    }
    let mut attrs = String::new();
    if v.autoplay {
        attrs.push_str(" data-autoplay");
    }
    if v.looped {
        attrs.push_str(" loop");
    }
    if v.muted {
        attrs.push_str(" muted");
    }
    if v.controls {
        attrs.push_str(" controls");
    }
    if !v.poster.is_empty() {
        let _ = write!(attrs, r#" poster="{}""#, esc(&ctx.url(&v.poster)));
    }
    Some(format!(
        r#"<div class="cz-box cz-video-box" style="{}"><video data-src="{}" playsinline preload="metadata"{} style="object-fit:{}"></video></div>"#,
        esc(&box_style),
        esc(&url),
        attrs,
        fit
    ))
}

fn code_element(c: &CodeContent, el: &Element, _deck: &Deck) -> Option<String> {
    let mut box_style = box_css(&el.style, true);
    let _ = write!(box_style, "font-size:{}px;", num(c.font_size.clamp(6.0, 200.0)));
    let lang = c.language.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '+' || *ch == '#').collect::<String>();
    let steps: String = c.highlight_steps.chars().filter(|ch| ch.is_ascii_digit() || matches!(ch, '|' | ',' | '-' | ' ')).collect();
    let line_attr = if c.line_numbers || !steps.trim().is_empty() {
        format!(r#" data-line-numbers="{}""#, esc(steps.trim()))
    } else {
        String::new()
    };
    let title = if c.title.trim().is_empty() {
        String::new()
    } else {
        format!(r#"<div class="cz-code-title">{}</div>"#, esc(&c.title))
    };
    Some(format!(
        r#"<div class="cz-box cz-code-box{}" style="{}">{}<pre class="cz-code"><code class="language-{}"{} data-trim>{}</code></pre></div>"#,
        if c.line_numbers { "" } else { " cz-no-ln" },
        esc(&box_style),
        title,
        if lang.is_empty() { "plaintext".to_string() } else { lang },
        line_attr,
        esc(&c.code)
    ))
}

fn math_element(m: &MathContent, el: &Element) -> Option<String> {
    let mut style = box_css(&el.style, true);
    let _ = write!(style, "font-size:{}px;", num(m.font_size.clamp(6.0, 400.0)));
    let color = css_value(&m.color);
    if !color.is_empty() {
        let _ = write!(style, "color:{color};");
    }
    let align = match m.align.as_str() {
        "left" => "flex-start",
        "right" => "flex-end",
        _ => "center",
    };
    let _ = write!(style, "align-items:{align};");
    Some(format!(
        r#"<div class="cz-box cz-math-box" style="{}"><div class="cz-math" data-latex="{}"></div></div>"#,
        esc(&style),
        esc(&m.latex)
    ))
}

fn embed_element(e: &EmbedContent, el: &Element, ctx: &Ctx) -> Option<String> {
    let url = e.url.trim();
    let lower = url.to_ascii_lowercase();
    let valid = lower.starts_with("https://") || lower.starts_with("http://");
    let box_style = box_css(&el.style, true);
    if ctx.editor() || !valid {
        if !ctx.editor() {
            return None;
        }
        return Some(format!(
            r#"<div class="cz-box cz-embed-box cz-embed-preview" style="{}"><div class="cz-placeholder-media"><span class="cz-ph-icon cz-ph-web"></span><span>{}</span></div></div>"#,
            esc(&box_style),
            if valid { esc(url) } else { "Web page — set its address in the inspector".into() }
        ));
    }
    Some(format!(
        r#"<div class="cz-box cz-embed-box" style="{}"><iframe data-src="{}" data-preload allow="fullscreen; autoplay; clipboard-read; clipboard-write"{}></iframe></div>"#,
        esc(&box_style),
        esc(url),
        if e.interactive { "" } else { r#" style="pointer-events:none""# }
    ))
}

fn table_element(t: &TableContent, el: &Element) -> Option<String> {
    let mut vars = format!(
        "font-size:{}px;--cz-grid:{};--cz-head-bg:{};--cz-head-fg:{};--cz-stripe:{};text-align:{};",
        num(t.font_size.clamp(4.0, 200.0)),
        css_value(&t.grid_color),
        css_value(&t.header_fill),
        css_value(&t.header_color),
        css_value(&t.stripe_fill),
        match t.align.as_str() {
            "center" => "center",
            "right" => "right",
            _ => "left",
        }
    );
    let color = css_value(&t.color);
    if !color.is_empty() {
        let _ = write!(vars, "color:{color};");
    }
    let mut html = format!(
        r#"<div class="cz-box cz-table-box" style="{}"><table class="cz-table{}{}{}" style="{}"><tbody>"#,
        esc(&box_css(&el.style, true)),
        if t.striped { " cz-striped" } else { "" },
        if t.header_row { " cz-head-row" } else { "" },
        if t.header_col { " cz-head-col" } else { "" },
        esc(&vars)
    );
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    for (r, row) in t.rows.iter().enumerate() {
        html.push_str("<tr>");
        for c in 0..cols {
            let cell = wrap_math(row.get(c).map(String::as_str).unwrap_or(""));
            let tag = if (t.header_row && r == 0) || (t.header_col && c == 0) { "th" } else { "td" };
            let _ = write!(html, r#"<{tag} data-r="{r}" data-c="{c}">{cell}</{tag}>"#);
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table></div>");
    Some(html)
}

fn chart_element(c: &ChartContent, el: &Element) -> Option<String> {
    let mut style = box_css(&el.style, true);
    let color = css_value(&c.color);
    if !color.is_empty() {
        let _ = write!(style, "color:{color};");
    }
    let pad = el.style.padding.max(0.0) * 2.0;
    Some(format!(
        r#"<div class="cz-box cz-chart-box" style="{}">{}</div>"#,
        esc(&style),
        chart_svg(c, (el.w - pad).max(10.0), (el.h - pad).max(10.0))
    ))
}

/// Diagrams are shown as images: their SVG (produced by Mermaid in the
/// editor) is isolated from the page, so it can never run scripts.
fn diagram_element(d: &DiagramContent, el: &Element, deck: &Deck, ctx: &Ctx) -> Option<String> {
    let dark = match d.mode.as_str() {
        "dark" => true,
        "light" => false,
        _ => deck.theme_is_dark(),
    };
    let svg = if dark && !d.svg_dark.is_empty() { &d.svg_dark } else if !d.svg_light.is_empty() { &d.svg_light } else { &d.svg_dark };
    let box_style = box_css(&el.style, true);
    if svg.is_empty() || svg.len() > 8 * 1024 * 1024 {
        if !ctx.editor() {
            return None;
        }
        return Some(format!(
            r#"<div class="cz-box cz-diagram-box cz-image-empty" style="{}"><div class="cz-placeholder-media"><span class="cz-ph-icon cz-ph-diagram"></span><span>Rendering diagram…</span></div></div>"#,
            esc(&box_style)
        ));
    }
    use base64::Engine;
    Some(format!(
        r#"<div class="cz-box cz-diagram-box" style="{}"><img src="data:image/svg+xml;base64,{}" alt="Diagram" draggable="false"></div>"#,
        esc(&box_style),
        base64::engine::general_purpose::STANDARD.encode(svg.as_bytes())
    ))
}

// ---------------------------------------------------------------------------
// Whole documents
// ---------------------------------------------------------------------------

/// reveal.js configuration for a deck. `extra` is merged on top.
pub fn reveal_config(deck: &Deck, extra: Value) -> Value {
    let s = &deck.settings;
    let mut cfg = json!({
        "width": deck.width,
        "height": deck.height,
        "margin": 0,
        "minScale": 0.05,
        "maxScale": 8.0,
        "center": false,
        "hash": true,
        "respondToHashChanges": true,
        "controls": s.controls,
        "progress": s.progress,
        "slideNumber": s.slide_number,
        "loop": s.loop_,
        "autoSlide": (s.auto_slide.max(0.0) * 1000.0).round() as i64,
        "keyboard": s.keyboard,
        "overview": s.overview,
        "touch": s.touch,
        "navigationMode": match s.navigation_mode.as_str() { "linear" | "grid" => s.navigation_mode.as_str(), _ => "default" },
        "transition": if s.transition.is_empty() { "slide" } else { s.transition.as_str() },
        "transitionSpeed": if s.transition_speed.is_empty() { "default" } else { s.transition_speed.as_str() },
        "backgroundTransition": if s.background_transition.is_empty() { "fade" } else { s.background_transition.as_str() },
        "autoAnimateDuration": s.auto_animate_duration.clamp(0.05, 10.0),
        "autoAnimateEasing": if s.auto_animate_easing.is_empty() { "ease" } else { s.auto_animate_easing.as_str() },
        "fragmentInURL": s.fragment_in_url,
        "hideInactiveCursor": true,
        "pdfSeparateFragments": false,
        "pdfMaxPagesPerSlide": 1,
    });
    if s.view == "scroll" {
        cfg["view"] = json!("scroll");
    }
    if let (Value::Object(base), Value::Object(extra)) = (&mut cfg, extra) {
        for (k, v) in extra {
            base.insert(k, v);
        }
    }
    cfg
}

/// Libraries a deck needs at presentation time.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Needs {
    pub code: bool,
    pub math: bool,
}

pub fn needs(deck: &Deck) -> Needs {
    Needs {
        code: deck.uses(|c| matches!(c, Content::Code(_))),
        math: deck.uses(|c| match c {
            Content::Math(_) => true,
            Content::Text(_) | Content::Shape(_) => c.text().is_some_and(|t| has_math(&t.html)),
            Content::Table(t) => t.rows.iter().flatten().any(|cell| has_math(cell)),
            _ => false,
        }),
    }
}

/// CSS appended after the theme: embedded fonts, Cadenza variables, deck
/// font overrides and the user's CSS.
pub fn deck_css(deck: &Deck, ctx: &Ctx) -> String {
    let mut css = String::new();
    for font in &deck.web_fonts {
        css.push_str(&crate::webfonts::resolve_css(&font.css, &|name: &str| ctx.url(name)));
    }
    match deck.active_custom_theme() {
        Some(t) => {
            css.push_str(&t.css());
            css.push_str(&t.vars_css());
        }
        None => css.push_str(&themes::vars_css(&deck.theme)),
    }
    let heading = css_value(&deck.heading_font);
    let body = css_value(&deck.body_font);
    if !heading.is_empty() || !body.is_empty() {
        css.push_str(":root{");
        if !heading.is_empty() {
            let _ = write!(css, "--r-heading-font:{heading};");
        }
        if !body.is_empty() {
            let _ = write!(css, "--r-main-font:{body};");
        }
        css.push('}');
    }
    if !deck.custom_css.trim().is_empty() {
        css.push('\n');
        css.push_str(&crate::sanitize::style_block(&deck.custom_css));
    }
    css
}

/// Where the document's library files come from.
pub trait LibLinker {
    /// Markup that loads a stylesheet from the given UI-relative path.
    fn stylesheet(&self, path: &str) -> String;
    /// Markup that loads a script from the given UI-relative path.
    fn script(&self, path: &str) -> String;
}

/// A complete standalone HTML document for the deck.
pub fn document(deck: &Deck, ctx: &Ctx, libs: &dyn LibLinker) -> String {
    let need = needs(deck);
    let mut head = String::new();
    head.push_str(&libs.stylesheet("vendor/reveal/reset.css"));
    head.push_str(&libs.stylesheet("vendor/reveal/reveal.css"));
    if let Some(href) = deck.theme_href() {
        head.push_str(&libs.stylesheet(href));
    }
    if need.code {
        let code_theme = if deck.settings.code_theme == "zenburn" { "zenburn" } else { "monokai" };
        head.push_str(&libs.stylesheet(&format!("vendor/reveal/plugin/highlight/{code_theme}.css")));
    }
    if need.math {
        head.push_str(&libs.stylesheet("vendor/katex/katex.min.css"));
    }
    head.push_str(&libs.stylesheet("css/content.css"));

    let mut scripts = String::new();
    scripts.push_str(&libs.script("vendor/reveal/reveal.js"));
    scripts.push_str(&libs.script("vendor/reveal/plugin/notes.js"));
    if need.code {
        scripts.push_str(&libs.script("vendor/reveal/plugin/highlight.js"));
    }
    if need.math {
        scripts.push_str(&libs.script("vendor/katex/katex.min.js"));
    }
    scripts.push_str(&libs.script("js/runtime.js"));

    let config = reveal_config(deck, json!({}));
    let mut title = deck.title.trim().to_string();
    if title.is_empty() {
        title = "Presentation".into();
    }
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
<meta name="generator" content="Cadenza {version}">
{author}<title>{title}</title>
{head}<style>{css}</style>
</head>
<body>
<div class="reveal"><div class="slides">{sections}</div></div>
{scripts}<script>CadenzaRuntime.start({config});</script>
</body>
</html>
"#,
        version = env!("CARGO_PKG_VERSION"),
        author = if deck.author.trim().is_empty() {
            String::new()
        } else {
            format!("<meta name=\"author\" content=\"{}\">\n", esc(&deck.author))
        },
        title = esc(&title),
        head = head,
        css = deck_css(deck, ctx),
        sections = sections(deck, ctx),
        scripts = scripts,
        config = serde_json::to_string(&config).unwrap_or_else(|_| "{}".into()).replace("</", "<\\/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mode: Mode) -> Ctx<'static> {
        Ctx { mode, asset_url: &|n: &str| format!("assets/{n}") }
    }

    fn deck_with(content: Content) -> Deck {
        let mut deck = Deck::default();
        let mut slide = Slide::default();
        slide.elements.push(Element { content, ..Default::default() });
        deck.slides.push(slide);
        deck
    }

    #[test]
    fn editor_marks_elements_and_hides_fragments() {
        let mut deck = deck_with(Content::Text(TextContent {
            text: RichText { html: "<p>Hello</p>".into(), ..Default::default() },
        }));
        deck.slides[0].elements[0].build.effect = "fade-up".into();
        let r = slide_for_editor(&deck, &deck.slides[0], &ctx(Mode::Editor));
        assert!(r.html.contains("data-cz-id="));
        assert!(!r.html.contains("fragment"));
        let s = sections(&deck, &ctx(Mode::Show));
        assert!(s.contains("fragment fade-up"));
        assert!(s.contains("data-id="));
    }

    #[test]
    fn empty_text_is_placeholder_only_in_editor() {
        let mut t = TextContent::default();
        t.text.placeholder = "Add title".into();
        let deck = deck_with(Content::Text(t));
        assert!(slide_for_editor(&deck, &deck.slides[0], &ctx(Mode::Editor)).html.contains("Add title"));
        assert!(!sections(&deck, &ctx(Mode::Show)).contains("cz-el"));
    }

    #[test]
    fn nested_slides_form_stacks() {
        let mut deck = Deck::default();
        let a = Slide::default();
        let b = Slide { nested: true, ..Default::default() };
        let c = Slide::default();
        let (bid, cid) = (b.id.clone(), c.id.clone());
        deck.slides = vec![a, b, c];
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(html.starts_with("<section><section"));
        assert_eq!(slide_indices(&deck, &bid), Some((0, 1)));
        assert_eq!(slide_indices(&deck, &cid), Some((1, 0)));
    }

    #[test]
    fn hidden_slides_are_skipped() {
        let mut deck = Deck::default();
        let a = Slide { hidden: true, ..Default::default() };
        let b = Slide { nested: true, ..Default::default() };
        let c = Slide { hidden: true, ..Default::default() };
        let d = Slide::default();
        let (aid, bid, cid, did) = (a.id.clone(), b.id.clone(), c.id.clone(), d.id.clone());
        deck.slides = vec![a, b, c, d];
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(!html.contains(&aid) && !html.contains(&cid));
        // `b` is promoted to the head of the first group.
        assert_eq!(slide_indices(&deck, &bid), Some((0, 0)));
        assert_eq!(slide_indices(&deck, &aid), Some((0, 0)));
        assert_eq!(slide_indices(&deck, &cid), Some((1, 0)));
        assert_eq!(slide_indices(&deck, &did), Some((1, 0)));
    }

    #[test]
    fn background_classes() {
        let mut s = Slide::default();
        assert_eq!(background_class(&s), "");
        s.background.kind = "color".into();
        s.background.color = "#ffffff".into();
        assert_eq!(background_class(&s), "has-light-background");
        s.background.color = "#101010".into();
        assert_eq!(background_class(&s), "has-dark-background");
    }

    #[test]
    fn assets_resolve_through_context() {
        let deck = deck_with(Content::Image(ImageContent { src: "abc.png".into(), ..Default::default() }));
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(html.contains(r#"src="assets/abc.png""#));
    }

    #[test]
    fn youtube_urls_are_recognized() {
        assert_eq!(video_embed("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1"), Some(("youtube", "dQw4w9WgXcQ".into())));
        assert_eq!(video_embed("https://youtu.be/dQw4w9WgXcQ"), Some(("youtube", "dQw4w9WgXcQ".into())));
        assert_eq!(video_embed("https://vimeo.com/76979871"), Some(("vimeo", "76979871".into())));
        assert_eq!(video_embed("https://example.com/v.mp4"), None);
    }

    #[test]
    fn inline_math_is_wrapped_and_loads_katex() {
        let deck = deck_with(Content::Text(TextContent {
            text: RichText { html: "<p>Area $\\pi r^2$</p>".into(), ..Default::default() },
        }));
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(html.contains(r#"<span class="cz-math-inline" data-latex="\pi r^2">"#), "{html}");
        assert!(needs(&deck).math);
        let plain = deck_with(Content::Text(TextContent {
            text: RichText { html: "<p>Costs $5 or $10</p>".into(), ..Default::default() },
        }));
        assert!(!needs(&plain).math);
    }

    #[test]
    fn deck_css_embeds_fonts_and_overrides() {
        let mut deck = Deck::default();
        deck.heading_font = "'Lora'".into();
        deck.web_fonts.push(crate::webfonts::WebFont {
            family: "Lora".into(),
            source: "google".into(),
            css: "@font-face{font-family:'Lora';src:url(cz-asset:lora1.woff2)}".into(),
        });
        let css = deck_css(&deck, &ctx(Mode::Show));
        assert!(css.contains("url(assets/lora1.woff2)"));
        assert!(css.contains("--r-heading-font:'Lora';"));
        assert!(deck.referenced_assets().contains("lora1.woff2"));
    }

    #[test]
    fn diagrams_pick_variant_by_theme() {
        let mut deck = deck_with(Content::Diagram(DiagramContent {
            svg_light: "<svg>L</svg>".into(),
            svg_dark: "<svg>D</svg>".into(),
            ..Default::default()
        }));
        use base64::Engine;
        let enc = |s: &str| base64::engine::general_purpose::STANDARD.encode(s);
        deck.theme = "cadenza-midnight".into();
        assert!(sections(&deck, &ctx(Mode::Show)).contains(&enc("<svg>D</svg>")));
        deck.theme = "cadenza-paper".into();
        assert!(sections(&deck, &ctx(Mode::Show)).contains(&enc("<svg>L</svg>")));
        let empty = deck_with(Content::Diagram(DiagramContent::default()));
        assert!(!sections(&empty, &ctx(Mode::Show)).contains("cz-diagram"));
        assert!(slide_for_editor(&empty, &empty.slides[0], &ctx(Mode::Editor)).html.contains("Rendering diagram"));
    }

    #[test]
    fn code_is_escaped() {
        let deck = deck_with(Content::Code(CodeContent { code: "<script>x</script>".into(), ..Default::default() }));
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>x"));
    }

    #[test]
    fn notes_are_escaped() {
        let mut deck = deck_with(Content::default());
        deck.slides[0].notes = "a <b>\nline 2".into();
        let html = sections(&deck, &ctx(Mode::Show));
        assert!(html.contains(r#"<aside class="notes">a &lt;b&gt;<br>line 2</aside>"#));
    }

    #[test]
    fn config_reflects_settings() {
        let mut deck = Deck::default();
        deck.settings.auto_slide = 5.0;
        deck.settings.view = "scroll".into();
        let cfg = reveal_config(&deck, json!({"hash": false}));
        assert_eq!(cfg["autoSlide"], 5000);
        assert_eq!(cfg["view"], "scroll");
        assert_eq!(cfg["hash"], false);
        assert_eq!(cfg["center"], false);
    }

    struct Linked;
    impl LibLinker for Linked {
        fn stylesheet(&self, p: &str) -> String {
            format!("<link rel=\"stylesheet\" href=\"lib/{p}\">\n")
        }
        fn script(&self, p: &str) -> String {
            format!("<script src=\"lib/{p}\"></script>\n")
        }
    }

    #[test]
    fn document_includes_only_needed_libs() {
        let deck = deck_with(Content::Math(MathContent::default()));
        let html = document(&deck, &ctx(Mode::Show), &Linked);
        assert!(html.contains("katex.min.js"));
        assert!(!html.contains("highlight.js"));
        assert!(html.contains("CadenzaRuntime.start("));
    }
}
