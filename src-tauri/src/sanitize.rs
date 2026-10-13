//! Sanitization of rich text produced by the editor's `contenteditable`
//! surfaces, so that stored decks never carry scripts or arbitrary markup.

use std::collections::HashSet;
use std::sync::OnceLock;

use ammonia::{Builder, UrlRelative};

fn builder() -> &'static Builder<'static> {
    static B: OnceLock<Builder<'static>> = OnceLock::new();
    B.get_or_init(|| {
        let tags: HashSet<&str> = [
            "p",
            "br",
            "div",
            "span",
            "b",
            "strong",
            "i",
            "em",
            "u",
            "s",
            "strike",
            "del",
            "sub",
            "sup",
            "a",
            "ul",
            "ol",
            "li",
            "code",
            "mark",
            "small",
            "blockquote",
        ]
        .into_iter()
        .collect();
        let styles: HashSet<&str> = [
            "color",
            "background-color",
            "font-size",
            "font-weight",
            "font-style",
            "font-family",
            "text-decoration",
            "text-decoration-line",
            "text-align",
            "letter-spacing",
            "vertical-align",
        ]
        .into_iter()
        .collect();
        let mut b = Builder::default();
        b.tags(tags)
            .clean_content_tags(["script", "style"].into_iter().collect())
            .generic_attributes(["style"].into_iter().collect())
            .add_tag_attributes("a", ["href", "target"])
            .add_tag_attributes("ol", ["start", "type"])
            .filter_style_properties(styles)
            .url_schemes(["http", "https", "mailto"].into_iter().collect())
            .url_relative(UrlRelative::PassThrough)
            .link_rel(Some("noopener noreferrer"));
        b
    })
}

/// Cleans rich text HTML for storage and rendering.
pub fn rich_text(html: &str) -> String {
    let cleaned = builder().clean(html).to_string();
    // Normalize the empty states editors leave behind.
    let trimmed = cleaned.trim();
    if trimmed == "<br>" || trimmed == "<p><br></p>" || trimmed == "<div><br></div>" {
        return String::new();
    }
    cleaned
}

/// Converts plain text into paragraphs of escaped HTML.
pub fn plain_to_html(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                "<p><br></p>".to_string()
            } else {
                format!("<p>{}</p>", crate::util::esc(line))
            }
        })
        .collect()
}

/// Makes user-provided CSS safe to embed inside a `<style>` element.
pub fn style_block(css: &str) -> String {
    css.replace("</", "<\\/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_and_handlers() {
        let out = rich_text(r#"<p onclick="x()">Hi<script>alert(1)</script><img src=x onerror=y></p>"#);
        assert_eq!(out, "<p>Hi</p>");
    }

    #[test]
    fn keeps_formatting() {
        let html = r#"<p><b>Bold</b> <span style="color: red; position: fixed">red</span></p><ul><li>a</li></ul>"#;
        let out = rich_text(html);
        assert!(out.contains("<b>Bold</b>"));
        assert!(out.contains(r#"style="color:red""#) || out.contains(r#"style="color: red""#), "{out}");
        assert!(!out.contains("position"));
        assert!(out.contains("<li>a</li>"));
    }

    #[test]
    fn keeps_internal_links() {
        let out = rich_text(r##"<a href="#/s-abc">go</a> <a href="javascript:alert(1)">x</a>"##);
        assert!(out.contains(r##"href="#/s-abc""##));
        assert!(!out.contains("javascript"));
    }

    #[test]
    fn plain_text_becomes_paragraphs() {
        assert_eq!(plain_to_html("a<b\n\nc"), "<p>a&lt;b</p><p><br></p><p>c</p>");
    }

    #[test]
    fn empty_states_collapse() {
        assert_eq!(rich_text("<p><br></p>"), "");
    }
}
