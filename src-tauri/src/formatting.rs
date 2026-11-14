//! The format painter cannot modify geometry, source media, links or builds.
use crate::model::{Content, Element};
use serde_json::Value;

fn keys(value: &Value, allowed: &[&str]) -> anyhow::Result<()> {
    let map = value.as_object().ok_or_else(|| anyhow::anyhow!("Invalid formatting"))?;
    anyhow::ensure!(map.keys().all(|k| allowed.contains(&k.as_str())), "Not a formatting property");
    Ok(())
}

pub fn validate(el: &Element, patch: &Value) -> anyhow::Result<()> {
    keys(patch, &["style", "content"])?;
    if let Some(style) = patch.get("style") {
        keys(
            style,
            &[
                "fill",
                "borderColor",
                "borderWidth",
                "borderStyle",
                "radius",
                "shadow",
                "shadowColor",
                "padding",
            ],
        )?;
    }
    if let Some(content) = patch.get("content") {
        let text = match &el.content {
            Content::Text(_) => Some(content),
            Content::Shape(_) => {
                keys(content, &["text"])?;
                content.get("text")
            }
            Content::Image(_) => {
                keys(content, &["grayscale", "blur", "brightness", "contrast", "saturate"])?;
                None
            }
            _ => {
                anyhow::bail!("This object only supports box formatting")
            }
        };
        if let Some(text) = text {
            keys(text, &["html", "style"])?;
            if let Some(style) = text.get("style") {
                keys(
                    style,
                    &[
                        "role",
                        "fontFamily",
                        "fontSize",
                        "fontWeight",
                        "italic",
                        "upright",
                        "color",
                        "align",
                        "valign",
                        "lineHeight",
                        "letterSpacing",
                        "transform",
                        "paragraphSpacing",
                        "shadow",
                    ],
                )?;
            }
        }
    }
    Ok(())
}
