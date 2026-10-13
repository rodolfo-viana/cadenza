//! Inline LaTeX inside rich text.
//!
//! Text boxes, shape labels and table cells may contain TeX math written the
//! usual way:
//!
//! * `$…$` or `\(…\)` — inline math
//! * `$$…$$` or `\[…\]` — display math
//! * `\$` — a literal dollar sign
//!
//! The stored HTML keeps the plain source; at render time the math is wrapped
//! in `<span class="cz-math-inline|cz-math-display" data-latex="…">` and
//! typeset by KaTeX (in the editor, the presentation and exports alike).
//!
//! To avoid false positives with prices ("$5 and $10"), `$…$` follows
//! Pandoc's rules: the opening `$` must be followed by a non-space, the
//! closing `$` preceded by a non-space and not followed by a digit.

use std::borrow::Cow;

use crate::util::esc;

/// Wraps TeX math found in the text of (sanitized) HTML.
pub fn wrap_math(html: &str) -> Cow<'_, str> {
    if !may_contain_math(html) {
        return Cow::Borrowed(html);
    }
    let mut out = String::with_capacity(html.len() + 64);
    let mut code_depth = 0usize;
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').map(|i| i + 1).unwrap_or(rest.len());
            let tag = &rest[..end];
            let lower = tag.to_ascii_lowercase();
            if lower.starts_with("<code") || lower.starts_with("<pre") {
                code_depth += 1;
            } else if (lower.starts_with("</code") || lower.starts_with("</pre")) && code_depth > 0 {
                code_depth -= 1;
            }
            out.push_str(tag);
            rest = &rest[end..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let text = &rest[..end];
            if code_depth == 0 {
                wrap_text(text, &mut out);
            } else {
                out.push_str(text);
            }
            rest = &rest[end..];
        }
    }
    Cow::Owned(out)
}

/// Whether the HTML contains at least one math expression.
pub fn has_math(html: &str) -> bool {
    may_contain_math(html) && wrap_math(html).contains("data-latex=")
}

fn may_contain_math(html: &str) -> bool {
    html.contains('$') || html.contains("\\(") || html.contains("\\[")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace('\u{a0}', " ")
        .replace("&amp;", "&")
}

fn emit(out: &mut String, source: &str, latex: &str, display: bool) {
    let latex = unescape(latex.trim());
    out.push_str(&format!(
        r#"<span class="{}" data-latex="{}">{}</span>"#,
        if display { "cz-math-display" } else { "cz-math-inline" },
        esc(&latex),
        source
    ));
}

/// Processes one text node (still HTML-escaped).
fn wrap_text(text: &str, out: &mut String) {
    let mut i = 0;
    while i < text.len() {
        let r = &text[i..];
        // Escaped dollar → literal "$".
        if r.starts_with("\\$") {
            out.push('$');
            i += 2;
            continue;
        }
        if let Some((len, latex, display)) = match_at(r) {
            emit(out, &r[..len], latex, display);
            i += len;
            continue;
        }
        let ch = r.chars().next().expect("non-empty");
        out.push(ch);
        i += ch.len_utf8();
    }
}

/// A math expression starting at `r`: (consumed length, latex, display).
fn match_at(r: &str) -> Option<(usize, &str, bool)> {
    for (open, close, display) in [("$$", "$$", true), ("\\[", "\\]", true), ("\\(", "\\)", false)] {
        if let Some(inner) = r.strip_prefix(open) {
            let end = inner.find(close)?;
            if inner[..end].trim().is_empty() {
                return None;
            }
            return Some((open.len() + end + close.len(), &inner[..end], display));
        }
    }
    if r.starts_with('$') {
        let len = inline_dollar(r)?;
        return Some((len, &r[1..len - 1], false));
    }
    None
}

/// Length of a `$…$` expression at the start of `s`, if any.
fn inline_dollar(s: &str) -> Option<usize> {
    let body = &s[1..];
    let first = body.chars().next()?;
    if first.is_whitespace() || first == '$' {
        return None;
    }
    let mut prev = first;
    let mut idx = first.len_utf8();
    let mut chars = body[idx..].chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && prev != '\\' {
            if prev.is_whitespace() {
                return None;
            }
            if chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                return None;
            }
            return Some(1 + idx + 1);
        }
        prev = c;
        idx += c.len_utf8();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latex_of(html: &str) -> Vec<(String, bool)> {
        let out = wrap_math(html).to_string();
        let mut v = Vec::new();
        let mut rest = out.as_str();
        while let Some(i) = rest.find("<span class=\"cz-math-") {
            rest = &rest[i + 21..];
            let display = rest.starts_with("display");
            let s = rest.find("data-latex=\"").unwrap() + 12;
            let e = rest[s..].find('"').unwrap() + s;
            v.push((rest[s..e].to_string(), display));
        }
        v
    }

    #[test]
    fn finds_inline_and_display_math() {
        assert_eq!(latex_of("<p>Energy $E=mc^2$ and $$\\int_0^1 x\\,dx$$</p>"), vec![
            ("E=mc^2".to_string(), false),
            ("\\int_0^1 x\\,dx".to_string(), true),
        ]);
        assert_eq!(latex_of("<p>\\(a+b\\) then \\[c\\]</p>"), vec![("a+b".into(), false), ("c".into(), true)]);
    }

    #[test]
    fn ignores_prices_and_escapes() {
        assert!(latex_of("<p>It costs $5 and $10 today</p>").is_empty());
        assert!(latex_of("<p>Price: $ 5 $</p>").is_empty());
        let out = wrap_math("<p>Literal \\$x\\$ dollars</p>");
        assert_eq!(out, "<p>Literal $x$ dollars</p>");
    }

    #[test]
    fn unescapes_entities_into_latex() {
        // The attribute holds the unescaped LaTeX, escaped once for HTML.
        assert_eq!(latex_of("<p>$a &lt; b &amp; c$</p>"), vec![("a &lt; b &amp; c".to_string(), false)]);
    }

    #[test]
    fn skips_code_and_attributes() {
        assert!(latex_of("<p><code>$x$</code></p>").is_empty());
        assert!(latex_of(r#"<p><a href="https://x.y/$a$">link</a></p>"#).is_empty());
        assert_eq!(latex_of(r#"<p><a href="https://x.y/">$a$</a></p>"#).len(), 1);
    }

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(wrap_math("<p>No math here</p>"), Cow::Borrowed(_)));
        assert!(!has_math("<p>$5 only</p>"));
        assert!(has_math("<p>$x$</p>"));
    }

    #[test]
    fn handles_unicode() {
        assert_eq!(latex_of("<p>ação $α+β$ fim</p>"), vec![("α+β".into(), false)]);
    }
}
