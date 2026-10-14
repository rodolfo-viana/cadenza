//! Markdown import.
//!
//! Accepts the conventions of reveal.js' Markdown plugin: slides separated by
//! a line containing `---` (horizontal) or `--` (vertical), and speaker notes
//! introduced by a line starting with `Note:`. Each slide's first heading
//! becomes its title; lists and paragraphs become a text box; fenced code
//! becomes a code block and images become image elements.

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
    let mut in_fence = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if !in_fence && (t == "---" || t == "***") {
            chunks.push((false, String::new()));
            continue;
        }
        if !in_fence && t == "--" {
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
    for line in chunk.lines() {
        let t = line.trim_start();
        if !in_notes && (t.starts_with("Note:") || t.starts_with("Notes:")) {
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

    let parser = Parser::new_ext(&body, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES);
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
    // Drop paragraphs left empty by extracted images.
    slide.body_html = cleaned.replace("<p></p>", "").trim().to_string();
    slide.title = title_text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    slide.code = code.map(|(l, c)| (l, c.trim_end().to_string()));
    slide
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
        let image = m.images.first().and_then(|(alt, src)| resolve_image(src).map(|r| (alt.clone(), r)));
        let is_mermaid = m.code.as_ref().is_some_and(|(lang, _)| lang == "mermaid");
        let layout = if is_mermaid {
            if m.title.is_some() { "title-only" } else { "blank" }
        } else if m.code.is_some() {
            "code"
        } else if image.is_some() && (has_body || m.title.is_some()) {
            "image-text"
        } else if image.is_some() {
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
                    if let Some((alt, (src, w, h))) = &image {
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
    fn front_matter_is_skipped() {
        let s = parse("---\ntitle: x\n---\n# Hello\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].title.as_deref(), Some("Hello"));
    }
}
