//! Markdown import.
//!
//! Accepts the conventions of reveal.js' Markdown plugin: slides separated by
//! a line containing `---` (horizontal) or `--` (vertical), and speaker notes
//! introduced by a line starting with `Note:` (inside code blocks, both are
//! code). Each slide's first heading becomes its title; lists and paragraphs
//! become a text box; fenced code becomes a code block and images become
//! image elements.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

use crate::layouts;
use crate::model::*;
use crate::sanitize;

#[derive(Debug, Default, PartialEq)]
pub struct MdSlide {
    pub nested: bool,
    pub title: Option<String>,
    pub title_level: u8,
    pub body_html: String,
    pub code: Option<(String, String)>,
    pub images: Vec<(String, String)>,
    pub notes: String,
}

/// Splits Markdown into slides.
pub fn parse(md: &str) -> Vec<MdSlide> {
    let md = strip_front_matter(md);
    let mut chunks: Vec<(bool, String)> = vec![(false, String::new())];
    for (line, in_code) in lines_in_code(md) {
        let t = line.trim();
        if !in_code && (t == "---" || t == "***") {
            chunks.push((false, String::new()));
            continue;
        }
        if !in_code && t == "--" {
            chunks.push((true, String::new()));
            continue;
        }
        let cur = chunks.last_mut().expect("chunk");
        cur.1.push_str(line);
        cur.1.push('\n');
    }
    chunks
        .into_iter()
        .filter(|(_, c)| !c.trim().is_empty())
        .map(|(nested, chunk)| parse_slide(nested, &chunk))
        .collect()
}

const OPTIONS: Options = Options::ENABLE_STRIKETHROUGH.union(Options::ENABLE_TABLES);

/// The lines of `md`, each with whether it lies in a code block. The parser
/// knows exactly where those are (fence characters and lengths, indented
/// code, code inside lists and quotes); separators and notes markers there
/// are part of the code.
fn lines_in_code(md: &str) -> Vec<(&str, bool)> {
    let code: Vec<_> = Parser::new_ext(md, OPTIONS)
        .into_offset_iter()
        .filter_map(|(ev, range)| matches!(ev, Event::Start(Tag::CodeBlock(_))).then_some(range))
        .collect();
    let mut start = 0;
    md.split_inclusive('\n')
        .map(|raw| {
            let line = raw.strip_suffix('\n').unwrap_or(raw);
            let line = line.strip_suffix('\r').unwrap_or(line);
            // A block's range starts at its opening fence or first code character.
            let first = start + line.len() - line.trim_start().len();
            start += raw.len();
            (line, code.iter().any(|r| r.contains(&first)))
        })
        .collect()
}

fn strip_front_matter(md: &str) -> &str {
    if let Some(rest) = md.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---\n")
    {
        let head = &rest[..end];
        if head.lines().all(|l| l.contains(':') || l.trim().is_empty()) {
            return &rest[end + 5..];
        }
    }
    md
}

fn parse_slide(nested: bool, chunk: &str) -> MdSlide {
    let mut slide = MdSlide { nested, ..Default::default() };
    // Speaker notes.
    let mut body = String::new();
    let mut in_notes = false;
    for (line, in_code) in lines_in_code(chunk) {
        let t = line.trim_start();
        if !in_notes && !in_code && (t.starts_with("Note:") || t.starts_with("Notes:")) {
            in_notes = true;
            let rest = t.split_once(':').map(|x| x.1).unwrap_or("").trim();
            if !rest.is_empty() {
                slide.notes.push_str(rest);
                slide.notes.push('\n');
            }
            continue;
        }
        if in_notes {
            slide.notes.push_str(line);
            slide.notes.push('\n');
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    slide.notes = slide.notes.trim().to_string();

    let parser = Parser::new_ext(&body, OPTIONS);
    let mut kept: Vec<Event> = Vec::new();
    let mut title_text: Option<String> = None;
    let mut capturing_title = false;
    let mut code: Option<(String, String)> = None;
    let mut capturing_code = false;
    let mut image: Option<(String, String)> = None;
    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) if title_text.is_none() && !capturing_title => {
                capturing_title = true;
                slide.title_level = heading_num(level);
                title_text = Some(String::new());
            }
            Event::End(TagEnd::Heading(_)) if capturing_title => capturing_title = false,
            Event::Start(Tag::Heading { .. }) => {
                kept.push(Event::Start(Tag::Paragraph));
                kept.push(Event::Start(Tag::Strong));
            }
            Event::End(TagEnd::Heading(_)) => {
                kept.push(Event::End(TagEnd::Strong));
                kept.push(Event::End(TagEnd::Paragraph));
            }
            Event::Start(Tag::CodeBlock(kind)) if code.is_none() => {
                capturing_code = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) if capturing_code => capturing_code = false,
            Event::Start(Tag::Image { dest_url, .. }) => image = Some((String::new(), dest_url.to_string())),
            Event::End(TagEnd::Image) => {
                if let Some(img) = image.take() {
                    slide.images.push(img);
                }
            }
            Event::Text(t) | Event::Code(t) if capturing_title => {
                if let Some(s) = title_text.as_mut() {
                    s.push_str(&t);
                }
            }
            Event::Text(t) if capturing_code => {
                if let Some((_, c)) = code.as_mut() {
                    c.push_str(&t);
                }
            }
            Event::Text(t) if image.is_some() => {
                if let Some((alt, _)) = image.as_mut() {
                    alt.push_str(&t);
                }
            }
            other => {
                if !capturing_title && !capturing_code && image.is_none() {
                    kept.push(other);
                }
            }
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, kept.into_iter());
    let cleaned = sanitize::rich_text(&out);
    slide.body_html = drop_empty_paragraphs(&cleaned).trim().to_string();
    slide.title = title_text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    slide.code = code.map(|(l, c)| (l, c.trim_end().to_string()));
    slide
}

/// Drops paragraphs left empty by extracted images, or holding only the line
/// breaks between them.
fn drop_empty_paragraphs(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find("<p>") {
        out.push_str(&rest[..i]);
        let inner = &rest[i + 3..];
        match inner.find("</p>") {
            Some(end) if inner[..end].replace("<br />", "").replace("<br>", "").trim().is_empty() => rest = &inner[end + 4..],
            _ => {
                out.push_str("<p>");
                rest = inner;
            }
        }
    }
    out.push_str(rest);
    out
}

fn heading_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn set_text(el: &mut Element, html: &str) {
    if let Some(t) = el.content.text_mut() {
        t.html = sanitize::rich_text(html);
    }
}

/// An image found for a Markdown reference: asset name or URL, natural
/// width and height.
pub type ResolvedImage = (String, u32, u32);

/// Converts parsed Markdown into slides. `resolve_image` maps an image
/// reference to its [`ResolvedImage`].
pub fn to_slides(md: &[MdSlide], width: u32, height: u32, resolve_image: &mut dyn FnMut(&str) -> Option<ResolvedImage>) -> Vec<Slide> {
    let mut out = Vec::new();
    for (i, m) in md.iter().enumerate() {
        let has_body = !m.body_html.is_empty();
        // A heading plus one short line reads as a title + subtitle.
        let short_body =
            has_body && !m.body_html.contains("<ul") && !m.body_html.contains("<ol") && m.body_html.matches("<p>").count() <= 1 && m.body_html.len() < 240;
        let images: Vec<(String, ResolvedImage)> = m.images.iter().filter_map(|(alt, src)| resolve_image(src).map(|r| (alt.clone(), r))).collect();
        let is_mermaid = m.code.as_ref().is_some_and(|(lang, _)| lang == "mermaid");
        let layout = if is_mermaid {
            if m.title.is_some() { "title-only" } else { "blank" }
        } else if m.code.is_some() {
            "code"
        } else if !images.is_empty() && (has_body || m.title.is_some()) {
            "image-text"
        } else if !images.is_empty() {
            "big-image"
        } else if i == 0 && m.title.is_some() && (!has_body || short_body) {
            "title"
        } else if !has_body && m.title.is_some() {
            "section"
        } else if m.title.is_none() {
            "statement"
        } else {
            "title-content"
        };
        let mut slide = layouts::new_slide(layout, width, height);
        slide.nested = m.nested && i > 0;
        slide.notes = m.notes.clone();
        let title = m.title.as_deref().map(crate::util::esc).unwrap_or_default();
        for el in slide.elements.iter_mut() {
            let role = el.content.text().map(|t| t.style.role.clone()).unwrap_or_default();
            match (&mut el.content, layout, role.as_str()) {
                (Content::Code(c), _, _) => {
                    if let Some((lang, code)) = &m.code {
                        c.code = code.clone();
                        c.language = if lang.is_empty() { "plaintext".into() } else { lang.clone() };
                    }
                }
                (Content::Image(img), _, _) => {
                    if let Some((alt, (src, w, h))) = images.first() {
                        img.src = src.clone();
                        img.alt = alt.clone();
                        img.natural_width = *w;
                        img.natural_height = *h;
                    }
                }
                (_, "statement", "title") => {
                    let html = if m.body_html.is_empty() {
                        format!("<p>{title}</p>")
                    } else {
                        m.body_html.clone()
                    };
                    set_text(el, &html);
                }
                (_, _, "title") => set_text(el, &format!("<p>{title}</p>")),
                (_, "title", "subtitle") | (_, "big-number", "subtitle") => set_text(el, &m.body_html),
                (_, "section", "caption") => set_text(el, ""),
                (_, _, "body") | (_, _, "caption") => set_text(el, &m.body_html),
                _ => {}
            }
        }
        if is_mermaid && let Some((_, source)) = &m.code {
            let (w, h) = (width as f64, height as f64);
            let top = if m.title.is_some() { 0.26 } else { 0.08 };
            slide.elements.push(Element {
                name: "Diagram".into(),
                x: (w * 0.07).round(),
                y: (h * top).round(),
                w: (w * 0.86).round(),
                h: (h * (0.92 - top)).round(),
                content: Content::Diagram(DiagramContent {
                    source: source.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        // Code and diagrams come without a text box: text that goes with
        // them sits between the title and them, as tall as its lines.
        if m.code.is_some() && has_body {
            let (w, h) = (width as f64, height as f64);
            let font = (32.0 * (w / 1280.0).min(h / 720.0)).round();
            let top = if m.title.is_some() { (h * 0.25).round() } else { (h * 0.08).round() };
            let lines = m.body_html.matches("<li").count() + m.body_html.matches("<p").count();
            let body_h = (lines.max(1) as f64 * font * 1.5 + font * 0.5).min(h * 0.4).round();
            let mut body = layouts::text("body", "", font, "left");
            (body.x, body.y, body.w, body.h) = ((w * 0.07).round(), top, (w * 0.86).round(), body_h);
            set_text(&mut body, &m.body_html);
            for e in slide.elements.iter_mut() {
                if matches!(e.content, Content::Code(_) | Content::Diagram(_)) {
                    let bottom = e.y + e.h;
                    e.y = top + body_h + (h * 0.03).round();
                    e.h = (bottom - e.y).max((h * 0.2).round());
                }
            }
            slide.elements.push(body);
        }
        let (w, h) = (width as f64, height as f64);
        let gap = (w * 0.02).round();
        if m.code.is_some() {
            // Images go in a column beside the code or diagram.
            let main = slide.elements.iter_mut().find(|e| matches!(e.content, Content::Code(_) | Content::Diagram(_)));
            if let Some(e) = main.filter(|_| !images.is_empty()) {
                e.w = (w * 0.55).round();
                let area = ((w * 0.65).round(), e.y, (w * 0.28).round(), e.h);
                slide.elements.extend(image_grid(&images, area, gap));
            }
        } else if images.len() > 1
            && let Some(at) = slide.elements.iter().position(|e| matches!(e.content, Content::Image(_)))
        {
            // Several images share the placeholder's part of the slide.
            let area = if layout == "image-text" {
                ((w * 0.05).round(), (h * 0.08).round(), (w * 0.45).round(), (h * 0.84).round())
            } else {
                ((w * 0.05).round(), (h * 0.08).round(), (w * 0.9).round(), (h * 0.84).round())
            };
            slide.elements.splice(at..=at, image_grid(&images, area, gap));
        }
        // Remove empty placeholders that the Markdown did not fill, except
        // the main ones a user would expect to type into.
        slide.elements.retain(|e| match &e.content {
            Content::Text(t) => !t.text.is_empty() || t.text.style.role == "title",
            _ => true,
        });
        out.push(slide);
    }
    out
}

/// Image elements for `images` in reading order, in a grid over `area`
/// (x, y, w, h): a row when the area is wide, a column when it is tall, two
/// of them from four images on. Each image keeps its proportions.
fn image_grid(images: &[(String, ResolvedImage)], (x, y, w, h): (f64, f64, f64, f64), gap: f64) -> Vec<Element> {
    let n = images.len();
    let lines = if n >= 4 { 2 } else { 1 };
    let (cols, rows) = if w >= h { (n.div_ceil(lines), lines) } else { (lines, n.div_ceil(lines)) };
    let cell_w = (w - gap * (cols - 1) as f64) / cols as f64;
    let cell_h = (h - gap * (rows - 1) as f64) / rows as f64;
    images
        .iter()
        .enumerate()
        .map(|(i, (alt, (src, nw, nh)))| {
            let (row, col) = (i / cols, i % cols);
            // A short last row is centered.
            let shift = (cols - cols.min(n - row * cols)) as f64 * (cell_w + gap) / 2.0;
            let (cx, cy) = (x + shift + col as f64 * (cell_w + gap), y + row as f64 * (cell_h + gap));
            let (bw, bh) = if *nw > 0 && *nh > 0 {
                let scale = (cell_w / *nw as f64).min(cell_h / *nh as f64);
                (*nw as f64 * scale, *nh as f64 * scale)
            } else {
                (cell_w, cell_h)
            };
            Element {
                name: "Image".into(),
                x: (cx + (cell_w - bw) / 2.0).round(),
                y: (cy + (cell_h - bh) / 2.0).round(),
                w: bw.round(),
                h: bh.round(),
                content: Content::Image(ImageContent {
                    src: src.clone(),
                    alt: alt.clone(),
                    fit: "contain".into(),
                    natural_width: *nw,
                    natural_height: *nh,
                    ..Default::default()
                }),
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Welcome\n\nA subtitle\n\n---\n\n## Agenda\n\n- One\n- **Two**\n\nNote: say hi\nsecond line\n\n--\n\n## Code\n\n```rust\nfn main() {}\n```\n\n---\n\n![Cat](cat.png)\n";

    #[test]
    fn splits_slides_and_notes() {
        let s = parse(SAMPLE);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].title.as_deref(), Some("Welcome"));
        assert_eq!(s[1].title.as_deref(), Some("Agenda"));
        assert!(s[1].body_html.contains("<li><strong>Two</strong></li>"));
        assert_eq!(s[1].notes, "say hi\nsecond line");
        assert!(s[2].nested);
        assert_eq!(s[2].code, Some(("rust".into(), "fn main() {}".into())));
        assert_eq!(s[3].images, vec![("Cat".into(), "cat.png".into())]);
    }

    #[test]
    fn separators_inside_code_are_ignored() {
        let s = parse("```\n---\n```\n");
        assert_eq!(s.len(), 1);
        assert!(s[0].code.as_ref().unwrap().1.contains("---"));
    }

    #[test]
    fn builds_slides_with_layouts() {
        let md = parse(SAMPLE);
        let mut resolver = |src: &str| Some((format!("x-{src}"), 100, 50));
        let slides = to_slides(&md, 1280, 720, &mut resolver);
        assert_eq!(slides[0].layout, "title");
        assert_eq!(slides[1].layout, "title-content");
        assert_eq!(slides[2].layout, "code");
        assert_eq!(slides[3].layout, "big-image");
        assert!(slides[2].nested);
        let img = slides[3].elements.iter().find_map(|e| match &e.content {
            Content::Image(i) => Some(i.src.clone()),
            _ => None,
        });
        assert_eq!(img.as_deref(), Some("x-cat.png"));
    }

    #[test]
    fn text_with_code_or_a_diagram_is_kept() {
        let md =
            parse("## Code\n\n- One\n- Two\n\n```python\nprint(1)\n```\n\n---\n\n## Flow\n\nWhy it matters.\n\n```mermaid\nflowchart LR\n  A --> B\n```\n");
        let slides = to_slides(&md, 1280, 720, &mut |_| None);
        for (slide, text) in slides.iter().zip(["One", "Why it matters."]) {
            let body = slide
                .elements
                .iter()
                .find(|e| e.content.text().is_some_and(|t| t.style.role == "body"))
                .expect("a text box");
            assert!(body.content.text().unwrap().html.contains(text));
            let other = slide
                .elements
                .iter()
                .find(|e| matches!(e.content, Content::Code(_) | Content::Diagram(_)))
                .unwrap();
            assert!(other.y >= body.y + body.h, "{} overlaps the text", other.name);
            assert!(other.y + other.h <= 720.0);
        }
    }

    #[test]
    fn mermaid_fences_become_diagrams() {
        let md = parse("## Flow\n\n```mermaid\ngraph TD; A-->B\n```\n");
        let slides = to_slides(&md, 1280, 720, &mut |_| None);
        assert_eq!(slides[0].layout, "title-only");
        assert!(
            slides[0]
                .elements
                .iter()
                .any(|e| matches!(&e.content, Content::Diagram(d) if d.source.contains("A-->B")))
        );
    }

    #[test]
    fn fences_keep_separators_and_notes_markers() {
        let code = |md: &str| {
            let s = parse(md);
            assert_eq!(s.len(), 1, "{md:?}");
            assert_eq!(s[0].notes, "", "{md:?}");
            s[0].code.clone().unwrap().1
        };
        assert_eq!(code("```\nbefore\nNote: literal code\nafter\n```\n"), "before\nNote: literal code\nafter");
        assert_eq!(code("~~~\nNotes: x\n---\n--\n~~~\n"), "Notes: x\n---\n--");
        // A closing fence uses the opening character, at least as many times,
        // without an info string.
        assert_eq!(code("````\n```\n---\n````\n"), "```\n---");
        assert_eq!(code("~~~\n```\n---\n```\n~~~\n"), "```\n---\n```");
        assert_eq!(code("```\n``` rust\n---\n```\n"), "``` rust\n---");
        assert_eq!(code("```\n~~~\n***\n```\n"), "~~~\n***");
        // Up to three spaces of indentation; inside lists too.
        assert_eq!(code("   ```js\n---\n   ```\n"), "---");
        assert_eq!(code("- Item\n\n  ```\n  Note: x\n  ```\n"), "Note: x");
        // An unclosed fence runs to the end.
        assert_eq!(code("```\na\n---\nb\n"), "a\n---\nb");
        // Four spaces is not a fence.
        assert_eq!(parse("Text\n    ```\n---\nMore\n    ```\n").len(), 2);
    }

    #[test]
    fn notes_and_separators_after_a_closed_block() {
        let s = parse("## A\n\n~~~~\ncode\n~~~~\n\nNote: real notes\n```\nkept raw\n```\n---\n\n## B\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].code, Some((String::new(), "code".into())));
        assert_eq!(s[0].notes, "real notes\n```\nkept raw\n```");
        assert_eq!(s[1].title.as_deref(), Some("B"));
    }

    fn images_of(slide: &Slide) -> Vec<&Element> {
        slide.elements.iter().filter(|e| matches!(e.content, Content::Image(_))).collect()
    }

    fn src(e: &Element) -> &str {
        match &e.content {
            Content::Image(i) => &i.src,
            _ => "",
        }
    }

    fn assert_apart(slide: &Slide) {
        for (i, a) in slide.elements.iter().enumerate() {
            assert!(
                a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= 1281.0 && a.y + a.h <= 721.0,
                "{} leaves the slide",
                a.name
            );
            for b in &slide.elements[i + 1..] {
                let overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "{} overlaps {}", a.name, b.name);
            }
        }
    }

    fn import(md: &str) -> Vec<Slide> {
        to_slides(&parse(md), 1280, 720, &mut |src: &str| Some((format!("x-{src}"), 1600, 900)))
    }

    #[test]
    fn one_image_keeps_its_layouts() {
        let slides = import("## Cat\n\n- Soft\n- Loud\n\n![Cat](cat.png)\n\n---\n\n![Dog](dog.png)\n");
        assert_eq!(slides[0].layout, "image-text");
        let img = images_of(&slides[0]);
        assert_eq!((img.len(), src(img[0])), (1, "x-cat.png"));
        assert_eq!((img[0].x, img[0].y, img[0].w, img[0].h), (0.0, 0.0, 640.0, 720.0));
        assert!(slides[0].elements.iter().any(|e| e.content.text().is_some_and(|t| t.html.contains("Loud"))));
        assert_eq!(slides[1].layout, "big-image");
        let img = images_of(&slides[1]);
        assert_eq!((img.len(), img[0].w, img[0].h), (1, 1280.0, 720.0));
    }

    #[test]
    fn every_image_is_kept_in_order() {
        let slides = import(
            "## Pets\n\nTwo of them.\n\n![A](a.png) ![B](b.png)\n\n---\n\n![A](a.png)\n![B](b.png)  \n![C](c.png)\n\n---\n\n![](1.png)![](2.png)![](3.png)![](4.png)![](5.png)\n",
        );
        assert_eq!(slides[0].layout, "image-text");
        let img = images_of(&slides[0]);
        assert_eq!(img.iter().map(|e| src(e)).collect::<Vec<_>>(), ["x-a.png", "x-b.png"]);
        // Stacked in the left half, beside the text.
        assert!(img[0].y + img[0].h <= img[1].y);
        assert!(img.iter().all(|e| e.x + e.w <= 640.0));
        assert!(
            slides[0]
                .elements
                .iter()
                .any(|e| e.content.text().is_some_and(|t| t.html.contains("Two of them.")))
        );

        assert_eq!(slides[1].layout, "big-image");
        let img = images_of(&slides[1]);
        assert_eq!(img.iter().map(|e| src(e)).collect::<Vec<_>>(), ["x-a.png", "x-b.png", "x-c.png"]);
        // Side by side, in their proportions.
        assert!(img[0].x + img[0].w <= img[1].x && img[1].x + img[1].w <= img[2].x);
        assert!(img.iter().all(|e| (e.w / e.h - 16.0 / 9.0).abs() < 0.02));

        let img = images_of(&slides[2]);
        assert_eq!(img.len(), 5);
        assert!(img[2].y + img[2].h <= img[3].y, "two rows");
        for slide in &slides {
            assert_apart(slide);
        }
    }

    #[test]
    fn images_sit_beside_code_and_diagrams() {
        let slides = import(
            "## Code\n\n```rust\nfn main() {}\n```\n\n![Shot](shot.png)\n\n---\n\n## Flow\n\n- Why\n\n![A](a.png)\n\n```mermaid\ngraph TD; A-->B\n```\n\n![B](b.png)\n",
        );
        assert_eq!(slides[0].layout, "code");
        assert_eq!(images_of(&slides[0]).iter().map(|e| src(e)).collect::<Vec<_>>(), ["x-shot.png"]);
        assert_eq!(images_of(&slides[1]).iter().map(|e| src(e)).collect::<Vec<_>>(), ["x-a.png", "x-b.png"]);
        for slide in &slides {
            let main = slide
                .elements
                .iter()
                .find(|e| matches!(e.content, Content::Code(_) | Content::Diagram(_)))
                .unwrap();
            assert!(images_of(slide).iter().all(|e| e.x >= main.x + main.w && e.y >= main.y));
            assert_apart(slide);
        }
        assert!(slides[1].elements.iter().any(|e| e.content.text().is_some_and(|t| t.html.contains("Why"))));
    }

    #[test]
    fn front_matter_is_skipped() {
        let s = parse("---\ntitle: x\n---\n# Hello\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].title.as_deref(), Some("Hello"));
    }
}
