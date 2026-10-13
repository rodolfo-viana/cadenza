//! Slide layouts and default element factories.
//!
//! Layouts are expressed in fractions of the slide size so they work for any
//! aspect ratio.

use serde::Serialize;

use crate::model::*;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LayoutInfo {
    pub id: &'static str,
    pub name: &'static str,
}

pub const LAYOUTS: &[LayoutInfo] = &[
    LayoutInfo { id: "title", name: "Title" },
    LayoutInfo { id: "title-content", name: "Title & Content" },
    LayoutInfo { id: "section", name: "Section Header" },
    LayoutInfo { id: "two-column", name: "Two Columns" },
    LayoutInfo { id: "image-text", name: "Image & Text" },
    LayoutInfo { id: "big-image", name: "Big Image" },
    LayoutInfo { id: "statement", name: "Statement" },
    LayoutInfo { id: "quote", name: "Quote" },
    LayoutInfo { id: "big-number", name: "Big Number" },
    LayoutInfo { id: "code", name: "Code" },
    LayoutInfo { id: "title-only", name: "Title Only" },
    LayoutInfo { id: "blank", name: "Blank" },
];

/// Geometry helper: fractions of the slide.
struct Frame {
    w: f64,
    h: f64,
}

impl Frame {
    fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        ((self.w * x).round(), (self.h * y).round(), (self.w * w).round(), (self.h * h).round())
    }
    /// Font size scaled relative to a 1280×720 reference slide.
    fn font(&self, px: f64) -> f64 {
        (px * (self.w / 1280.0).min(self.h / 720.0)).round()
    }
}

fn placed(mut el: Element, (x, y, w, h): (f64, f64, f64, f64)) -> Element {
    el.x = x;
    el.y = y;
    el.w = w;
    el.h = h;
    el
}

/// A text box with a role and placeholder.
pub fn text(role: &str, placeholder: &str, size: f64, align: &str) -> Element {
    let mut t = TextContent::default();
    t.text.placeholder = placeholder.into();
    t.text.style.role = role.into();
    t.text.style.font_size = size;
    t.text.style.align = align.into();
    if role == "title" || role == "heading" {
        t.text.style.line_height = 1.1;
    }
    Element { name: role_name(role).into(), content: Content::Text(t), ..Default::default() }
}

fn role_name(role: &str) -> &'static str {
    match role {
        "title" => "Title",
        "subtitle" => "Subtitle",
        "heading" => "Heading",
        "caption" => "Caption",
        "quote" => "Quote",
        _ => "Text",
    }
}

/// Builds the elements of a layout.
pub fn layout_elements(layout: &str, width: u32, height: u32) -> Vec<Element> {
    let f = Frame { w: width as f64, h: height as f64 };
    match layout {
        "title" => vec![
            placed(text("title", "Presentation title", f.font(84.0), "center"), f.rect(0.08, 0.30, 0.84, 0.2)),
            placed(text("subtitle", "Subtitle", f.font(36.0), "center"), f.rect(0.15, 0.56, 0.70, 0.1)),
        ],
        "title-content" => vec![
            placed(text("title", "Slide title", f.font(56.0), "left"), f.rect(0.07, 0.08, 0.86, 0.14)),
            placed(text("body", "Click to add text", f.font(32.0), "left"), f.rect(0.07, 0.28, 0.86, 0.6)),
        ],
        "section" => vec![
            placed(text("caption", "SECTION 01", f.font(24.0), "left"), f.rect(0.08, 0.30, 0.6, 0.06)),
            placed(text("title", "Section title", f.font(72.0), "left"), f.rect(0.08, 0.38, 0.84, 0.2)),
        ],
        "two-column" => vec![
            placed(text("title", "Slide title", f.font(56.0), "left"), f.rect(0.07, 0.08, 0.86, 0.14)),
            placed(text("body", "Left column", f.font(30.0), "left"), f.rect(0.07, 0.28, 0.41, 0.6)),
            placed(text("body", "Right column", f.font(30.0), "left"), f.rect(0.52, 0.28, 0.41, 0.6)),
        ],
        "image-text" => vec![
            placed(image_placeholder(), f.rect(0.0, 0.0, 0.5, 1.0)),
            placed(text("title", "Slide title", f.font(52.0), "left"), f.rect(0.55, 0.18, 0.39, 0.2)),
            placed(text("body", "Describe the image", f.font(28.0), "left"), f.rect(0.55, 0.42, 0.39, 0.4)),
        ],
        "big-image" => vec![
            placed(image_placeholder(), f.rect(0.0, 0.0, 1.0, 1.0)),
            placed(text("caption", "Caption", f.font(24.0), "left"), f.rect(0.05, 0.86, 0.6, 0.08)),
        ],
        "statement" => vec![placed(
            text("title", "A bold statement.", f.font(72.0), "center"),
            f.rect(0.1, 0.35, 0.8, 0.3),
        )],
        "quote" => vec![
            placed(text("quote", "“A memorable quote goes here.”", f.font(52.0), "center"), f.rect(0.12, 0.3, 0.76, 0.3)),
            placed(text("caption", "— Attribution", f.font(26.0), "center"), f.rect(0.25, 0.66, 0.5, 0.08)),
        ],
        "big-number" => vec![
            placed(text("title", "100%", f.font(180.0), "center"), f.rect(0.1, 0.22, 0.8, 0.36)),
            placed(text("subtitle", "What this number means", f.font(34.0), "center"), f.rect(0.15, 0.62, 0.7, 0.1)),
        ],
        "code" => {
            let mut code = Element {
                name: "Code".into(),
                content: Content::Code(CodeContent {
                    code: "function greet(name) {\n  return `Hello, ${name}!`;\n}\n\nconsole.log(greet('Cadenza'));".into(),
                    font_size: f.font(26.0),
                    ..Default::default()
                }),
                ..Default::default()
            };
            code.style.radius = 12.0;
            vec![
                placed(text("title", "Slide title", f.font(52.0), "left"), f.rect(0.07, 0.08, 0.86, 0.14)),
                placed(code, f.rect(0.07, 0.27, 0.86, 0.62)),
            ]
        }
        "title-only" => vec![placed(text("title", "Slide title", f.font(56.0), "left"), f.rect(0.07, 0.08, 0.86, 0.14))],
        _ => Vec::new(),
    }
}

fn image_placeholder() -> Element {
    Element { name: "Image".into(), content: Content::Image(ImageContent::default()), ..Default::default() }
}

/// A new slide built from a layout.
pub fn new_slide(layout: &str, width: u32, height: u32) -> Slide {
    let layout = if LAYOUTS.iter().any(|l| l.id == layout) { layout } else { "blank" };
    Slide { layout: layout.into(), elements: layout_elements(layout, width, height), ..Default::default() }
}

/// The deck every new document starts with: a single title slide.
pub fn starter_deck(theme: &str, width: u32, height: u32) -> Deck {
    let mut deck = Deck { theme: theme.into(), width, height, ..Default::default() };
    deck.slides.push(new_slide("title", width, height));
    deck
}

/// Default element of a given kind, centered on the slide. `variant` picks a
/// shape id, chart type or text role.
pub fn new_element(kind: &str, variant: &str, width: u32, height: u32) -> Element {
    let f = Frame { w: width as f64, h: height as f64 };
    let (w, h) = match kind {
        "text" => (f.w * 0.5, f.font(32.0) * 1.6),
        "shape" if variant == "line" => (f.w * 0.3, 24.0),
        "shape" => (f.h * 0.3, f.h * 0.3),
        "image" | "video" | "embed" => (f.w * 0.5, f.w * 0.5 * 9.0 / 16.0),
        "code" => (f.w * 0.6, f.h * 0.45),
        "math" => (f.w * 0.5, f.h * 0.2),
        "table" => (f.w * 0.6, f.h * 0.4),
        "chart" => (f.w * 0.6, f.h * 0.55),
        "diagram" => (f.w * 0.6, f.h * 0.55),
        _ => (f.w * 0.4, f.h * 0.2),
    };
    let content = match kind {
        "shape" => {
            let mut s = ShapeContent { shape: if variant.is_empty() { "rect".into() } else { variant.into() }, ..Default::default() };
            s.text.style.font_size = f.font(28.0);
            if variant == "line" {
                s.end_cap = "arrow".into();
            }
            Content::Shape(s)
        }
        "image" => Content::Image(ImageContent::default()),
        "video" => Content::Video(VideoContent::default()),
        "code" => Content::Code(CodeContent {
            code: "// Your code here\nconst answer = 42;".into(),
            font_size: f.font(24.0),
            ..Default::default()
        }),
        "math" => Content::Math(MathContent { font_size: f.font(48.0), ..Default::default() }),
        "embed" => Content::Embed(EmbedContent::default()),
        "diagram" => Content::Diagram(DiagramContent::default()),
        "table" => Content::Table(TableContent { font_size: f.font(26.0), ..Default::default() }),
        "chart" => Content::Chart(ChartContent {
            chart: if variant.is_empty() { "column".into() } else { variant.into() },
            font_size: f.font(20.0),
            ..Default::default()
        }),
        _ => {
            let role = if variant.is_empty() { "body" } else { variant };
            let mut el = text(role, "", f.font(match role {
                "title" => 56.0,
                "subtitle" => 36.0,
                "heading" => 44.0,
                "caption" => 22.0,
                _ => 32.0,
            }), "left");
            el.w = w;
            el.h = h;
            el.x = ((f.w - w) / 2.0).round();
            el.y = ((f.h - h) / 2.0).round();
            return el;
        }
    };
    let mut el = Element {
        name: match kind {
            "shape" => crate::shapes::SHAPES.iter().find(|s| s.0 == variant).map(|s| s.1).unwrap_or("Shape").into(),
            k => {
                let mut c = k.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        },
        content,
        ..Default::default()
    };
    if kind == "code" || kind == "embed" {
        el.style.radius = 12.0;
    }
    el.w = w.round();
    el.h = h.round();
    el.x = ((f.w - el.w) / 2.0).round();
    el.y = ((f.h - el.h) / 2.0).round();
    el
}

/// Sample slides used to preview themes.
pub fn sample_slides(width: u32, height: u32) -> Vec<Slide> {
    let html = |el: &mut Element, h: &str| {
        if let Some(t) = el.content.text_mut() {
            t.html = h.into();
        }
    };
    let mut title = new_slide("title", width, height);
    html(&mut title.elements[0], "<p>Theme Preview</p>");
    html(&mut title.elements[1], "<p>Headings, text, accents and charts</p>");

    let f = Frame { w: width as f64, h: height as f64 };
    let mut content = new_slide("two-column", width, height);
    html(&mut content.elements[0], "<p>What’s inside</p>");
    html(&mut content.elements[1], "<ul><li>Clear, readable text</li><li>A <a href=\"#\">link</a> and <b>bold</b> words</li><li>Theme accent colors</li></ul>");
    content.elements.truncate(2);
    content.elements[1].w = (f.w * 0.42).round();
    let mut chart = new_element("chart", "column", width, height);
    let (x, y, w, h) = f.rect(0.52, 0.28, 0.41, 0.6);
    chart.x = x;
    chart.y = y;
    chart.w = w;
    chart.h = h;
    content.elements.push(chart);

    let mut quote = new_slide("quote", width, height);
    html(&mut quote.elements[0], "<p>“Simplicity is the ultimate sophistication.”</p>");
    html(&mut quote.elements[1], "<p>— Leonardo da Vinci</p>");
    let mut shape = new_element("shape", "rounded", width, height);
    let (x, y, w, h) = f.rect(0.42, 0.8, 0.16, 0.09);
    shape.x = x;
    shape.y = y;
    shape.w = w;
    shape.h = h;
    if let Some(t) = shape.content.text_mut() {
        t.html = "<p>Button</p>".into();
        t.style.font_size = f.font(22.0);
    }
    quote.elements.push(shape);
    vec![title, content, quote]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_fits_the_slide() {
        for l in LAYOUTS {
            for (w, h) in [(1280, 720), (1024, 768)] {
                let s = new_slide(l.id, w, h);
                for e in &s.elements {
                    assert!(e.x >= 0.0 && e.y >= 0.0, "{}", l.id);
                    assert!(e.x + e.w <= w as f64 + 1.0 && e.y + e.h <= h as f64 + 1.0, "{}", l.id);
                }
            }
        }
    }

    #[test]
    fn factories_center_elements() {
        for kind in ["text", "shape", "image", "video", "code", "math", "embed", "table", "chart", "diagram"] {
            let e = new_element(kind, "", 1280, 720);
            assert_eq!(e.kind(), kind);
            assert!(((e.x + e.w / 2.0) - 640.0).abs() <= 1.0, "{kind}");
        }
    }
}
