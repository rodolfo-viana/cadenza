//! Atomic rich-text replacements prepared by the editor's DOM text index.
//!
//! The browser maps visible characters back to text nodes without touching
//! markup. Rust checks the original value and sanitizes the result at the
//! same document boundary as other edits. A stale result never overwrites
//! text that changed while the search panel was open.

use crate::model::{Content, Deck};
use crate::sanitize;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "field", rename_all = "camelCase")]
pub enum TextField {
    RichText,
    TableCell { row: usize, column: usize },
    Notes,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub slide: String,
    pub element: Option<String>,
    #[serde(flatten)]
    pub target: TextField,
    pub before: String,
    pub after: String,
}

pub fn apply(deck: &mut Deck, changes: Vec<TextChange>) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for change in changes {
        anyhow::ensure!(
            seen.insert((change.slide.clone(), change.element.clone(), change.target)),
            "duplicate text replacement"
        );
        let slide = deck.slide_mut(&change.slide).ok_or_else(|| anyhow::anyhow!("slide not found"))?;
        let value = match change.target {
            TextField::Notes => {
                anyhow::ensure!(change.element.is_none(), "invalid text replacement target");
                &mut slide.notes
            }
            TextField::RichText => {
                let element = change.element.as_deref().and_then(|id| slide.element_mut(id));
                &mut element
                    .and_then(|el| el.content.text_mut())
                    .ok_or_else(|| anyhow::anyhow!("text not found"))?
                    .html
            }
            TextField::TableCell { row, column } => {
                let element = change.element.as_deref().and_then(|id| slide.element_mut(id));
                let Some(Content::Table(table)) = element.map(|el| &mut el.content) else {
                    anyhow::bail!("table not found")
                };
                table
                    .rows
                    .get_mut(row)
                    .and_then(|r| r.get_mut(column))
                    .ok_or_else(|| anyhow::anyhow!("table cell not found"))?
            }
        };
        anyhow::ensure!(*value == change.before, "The text changed. Search again before replacing it.");
        *value = if change.target == TextField::Notes {
            change.after
        } else {
            sanitize::rich_text(&change.after)
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{layouts, model::*, ops, ops::Op};
    use serde_json::json;

    fn operation(changes: serde_json::Value) -> Op {
        serde_json::from_value(json!({"op": "replaceText", "all": true, "changes": changes})).unwrap()
    }

    #[test]
    fn replacing_across_slides_preserves_formatting_and_is_one_history_step() {
        let mut deck = Deck::default();
        for _ in 0..2 {
            let mut slide = layouts::new_slide("blank", deck.width, deck.height);
            let mut el = Element::default();
            el.content.text_mut().unwrap().html = "<p><b>Old</b> <i>kept</i></p>".into();
            slide.elements.push(el);
            deck.slides.push(slide);
        }
        let changes: Vec<_> = deck
            .slides
            .iter()
            .map(|s| {
                json!({
                    "slide": s.id, "element": s.elements[0].id, "field": "richText",
                    "before": "<p><b>Old</b> <i>kept</i></p>", "after": "<p><b>New</b> <i>kept</i></p>"
                })
            })
            .collect();
        let before = deck.clone();
        let out = ops::apply(&mut deck, operation(json!(changes))).unwrap();
        let mut history = ops::History::default();
        history.record(before.clone(), &deck, &out, None);
        for s in &deck.slides {
            assert_eq!(s.elements[0].content.text().unwrap().html, "<p><b>New</b> <i>kept</i></p>");
        }
        let after = deck.clone();
        history.undo(&mut deck).unwrap();
        assert_eq!(deck, before);
        assert!(history.undo_label().is_none());
        history.redo(&mut deck).unwrap();
        assert_eq!(deck, after);
    }

    #[test]
    fn stale_or_invalid_later_changes_leave_the_entire_deck_untouched() {
        let mut deck = Deck::default();
        let mut slide = Slide {
            notes: "Original".into(),
            ..Slide::default()
        };
        let mut el = Element::default();
        el.content.text_mut().unwrap().html = "<p>Current</p>".into();
        slide.elements.push(el);
        deck.slides.push(slide);
        let before = deck.clone();
        let sid = deck.slides[0].id.clone();
        let eid = deck.slides[0].elements[0].id.clone();
        for field in ["richText", "notes"] {
            let result = ops::apply(
                &mut deck,
                operation(json!([
                    {"slide": sid, "field": "notes", "before": "Original", "after": "Changed"},
                    {"slide": sid, "element": eid, "field": field, "before": "Stale", "after": "Lost edit"}
                ])),
            );
            assert!(result.is_err());
            assert_eq!(deck, before);
        }
    }

    #[test]
    fn table_cells_are_sanitized_but_notes_remain_plain_text() {
        let mut deck = Deck::default();
        let mut slide = Slide::default();
        let table = Element {
            content: Content::Table(TableContent {
                rows: vec![vec!["Old".into()]],
                ..TableContent::default()
            }),
            ..Element::default()
        };
        let sid = slide.id.clone();
        let tid = table.id.clone();
        slide.elements.push(table);
        deck.slides.push(slide);
        ops::apply(
            &mut deck,
            operation(json!([
                {"slide": sid, "element": tid, "field": "tableCell", "row": 0, "column": 0,
                 "before": "Old", "after": "<b>New</b><script>bad()</script>"},
                {"slide": sid, "field": "notes", "before": "", "after": "<plain> & notes"}
            ])),
        )
        .unwrap();
        assert_eq!(deck.slides[0].notes, "<plain> & notes");
        let Content::Table(table) = &deck.slides[0].elements[0].content else {
            panic!()
        };
        assert_eq!(table.rows[0][0], "<b>New</b>");
    }
}
