//! Editing operations.
//!
//! The UI never mutates the deck directly: it sends an [`Op`], Rust applies
//! it (validating and sanitizing along the way), records undo history and
//! returns the new state.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::layouts;
use crate::model::*;
use crate::sanitize;
pub use crate::sanitize::element as sanitize_element;
use crate::util::new_id;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    /// Adds a slide built from a layout after `after` (or at the end).
    AddSlide {
        after: Option<String>,
        layout: String,
    },
    DuplicateSlides {
        ids: Vec<String>,
    },
    DeleteSlides {
        ids: Vec<String>,
    },
    /// Moves slides so they end up before the slide currently at index `to`
    /// (`to == len` moves them to the end).
    MoveSlides {
        ids: Vec<String>,
        to: usize,
    },
    /// Inserts copies of slides (e.g. from the clipboard) after `after`.
    InsertSlides {
        after: Option<String>,
        slides: Vec<Slide>,
    },
    /// JSON merge patch applied to slide properties (not their elements).
    UpdateSlides {
        ids: Vec<String>,
        patch: Value,
    },
    AddElement {
        slide: String,
        kind: String,
        #[serde(default)]
        variant: String,
        #[serde(default)]
        props: Option<Value>,
    },
    /// Inserts copies of elements (paste), offset by `offset` pixels.
    InsertElements {
        slide: String,
        elements: Vec<Element>,
        #[serde(default)]
        offset: f64,
    },
    UpdateElements {
        slide: String,
        updates: Vec<ElementPatch>,
    },
    DeleteElements {
        slide: String,
        ids: Vec<String>,
    },
    /// "front", "back", "forward" or "backward".
    ArrangeElements {
        slide: String,
        ids: Vec<String>,
        action: String,
    },
    /// "left", "hcenter", "right", "top", "vcenter", "bottom",
    /// "distribute-h" or "distribute-v".
    AlignElements {
        slide: String,
        ids: Vec<String>,
        mode: String,
        #[serde(default)]
        to_slide: bool,
    },
    /// Sets build-in order following the given element order.
    SetBuildOrder {
        slide: String,
        ids: Vec<String>,
    },
    /// JSON merge patch applied to deck properties (not its slides).
    UpdateDeck {
        patch: Value,
    },
    /// Changes the slide size, scaling content to fit.
    ResizeDeck {
        width: u32,
        height: u32,
    },
    /// Replaces the whole deck (imports, pastes).
    ReplaceDeck {
        deck: Box<Deck>,
        /// Undo label (default "Replace Presentation").
        #[serde(default)]
        label: Option<String>,
    },
}

#[derive(Deserialize, Debug, Clone)]
pub struct ElementPatch {
    pub id: String,
    pub patch: Value,
}

/// Outcome of an operation, for the UI.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpOutcome {
    pub label: String,
    /// Slide the UI should show after the change.
    pub slide: Option<String>,
    /// Ids of slides or elements created by the operation.
    pub created: Vec<String>,
}

/// Marks an undo label that is not written straight into `outcome(…)`, so
/// tools/i18n.py finds it for translation (like `N_` in JavaScript).
const fn n_(label: &'static str) -> &'static str {
    label
}

/// The undo label for inserting an element of a kind.
fn insert_label(kind: &str) -> &'static str {
    match kind {
        "text" => n_("Insert Text"),
        "image" => n_("Insert Image"),
        "video" => n_("Insert Video"),
        "shape" => n_("Insert Shape"),
        "line" => n_("Insert Line"),
        "chart" => n_("Insert Chart"),
        "table" => n_("Insert Table"),
        "code" => n_("Insert Code"),
        "math" => n_("Insert Equation"),
        "diagram" => n_("Insert Diagram"),
        "embed" => n_("Insert Web Page"),
        "camera" => n_("Insert Live Camera"),
        _ => n_("Insert"),
    }
}

fn outcome(label: &str, slide: Option<String>, created: Vec<String>) -> OpOutcome {
    OpOutcome {
        label: label.into(),
        slide,
        created,
    }
}

/// RFC 7396 JSON merge patch.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    match patch {
        Value::Object(p) => {
            if !target.is_object() {
                *target = Value::Object(Map::new());
            }
            let t = target.as_object_mut().expect("object");
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge_patch(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        }
        _ => *target = patch.clone(),
    }
}

fn strip(patch: &Value, keys: &[&str]) -> Value {
    let mut p = patch.clone();
    if let Value::Object(m) = &mut p {
        for k in keys {
            m.remove(*k);
        }
    }
    p
}

fn patched<T: serde::Serialize + serde::de::DeserializeOwned>(item: &T, patch: &Value) -> anyhow::Result<T> {
    let mut v = serde_json::to_value(item)?;
    merge_patch(&mut v, patch);
    Ok(serde_json::from_value(v)?)
}

fn find_slide<'a>(deck: &'a mut Deck, id: &str) -> anyhow::Result<&'a mut Slide> {
    deck.slide_mut(id).ok_or_else(|| anyhow::anyhow!("slide not found"))
}

/// Applies an operation. On error the deck is left unchanged.
pub fn apply(deck: &mut Deck, op: Op) -> anyhow::Result<OpOutcome> {
    let mut work = deck.clone();
    let out = apply_inner(&mut work, op)?;
    work.normalize();
    *deck = work;
    Ok(out)
}

fn apply_inner(deck: &mut Deck, op: Op) -> anyhow::Result<OpOutcome> {
    match op {
        Op::AddSlide { after, layout } => {
            let slide = layouts::new_slide(&layout, deck.width, deck.height);
            let id = slide.id.clone();
            let idx = after.and_then(|a| deck.slide_index(&a)).map(|i| i + 1).unwrap_or(deck.slides.len());
            // A slide inserted inside a vertical stack joins the stack.
            let mut slide = slide;
            slide.nested = idx > 0 && idx < deck.slides.len() && deck.slides[idx].nested;
            deck.slides.insert(idx, slide);
            Ok(outcome("New Slide", Some(id.clone()), vec![id]))
        }
        Op::DuplicateSlides { ids } => {
            let mut created = Vec::new();
            let mut last = None;
            // Insert each copy right after the last selected slide, in order.
            let positions: Vec<usize> = ids.iter().filter_map(|id| deck.slide_index(id)).collect();
            let Some(&insert_after) = positions.iter().max() else {
                anyhow::bail!("nothing to duplicate")
            };
            let mut sorted = positions.clone();
            sorted.sort_unstable();
            let copies: Vec<Slide> = sorted.iter().map(|&i| deck.slides[i].duplicate()).collect();
            for (k, copy) in copies.into_iter().enumerate() {
                created.push(copy.id.clone());
                last = Some(copy.id.clone());
                deck.slides.insert(insert_after + 1 + k, copy);
            }
            Ok(outcome("Duplicate Slide", last, created))
        }
        Op::DeleteSlides { ids } => {
            let first = ids.iter().filter_map(|id| deck.slide_index(id)).min();
            deck.slides.retain(|s| !ids.contains(&s.id));
            if deck.slides.is_empty() {
                deck.slides.push(layouts::new_slide("blank", deck.width, deck.height));
            }
            let next = first.map(|i| i.min(deck.slides.len() - 1)).map(|i| deck.slides[i].id.clone());
            Ok(outcome(if ids.len() > 1 { n_("Delete Slides") } else { n_("Delete Slide") }, next, vec![]))
        }
        Op::MoveSlides { ids, to } => {
            let to = to.min(deck.slides.len());
            let before = deck.slides[..to].iter().filter(|s| ids.contains(&s.id)).count();
            let moving: Vec<Slide> = deck.slides.iter().filter(|s| ids.contains(&s.id)).cloned().collect();
            if moving.is_empty() {
                anyhow::bail!("nothing to move");
            }
            deck.slides.retain(|s| !ids.contains(&s.id));
            let at = (to - before).min(deck.slides.len());
            let hint = moving.first().map(|s| s.id.clone());
            for (k, s) in moving.into_iter().enumerate() {
                deck.slides.insert(at + k, s);
            }
            Ok(outcome("Move Slides", hint, vec![]))
        }
        Op::InsertSlides { after, slides } => {
            let start = after.and_then(|a| deck.slide_index(&a)).map(|i| i + 1).unwrap_or(deck.slides.len());
            let mut created = Vec::new();
            for (offset, mut s) in slides.into_iter().enumerate() {
                s.id = new_id("s");
                for el in &mut s.elements {
                    el.id = new_id("e");
                    sanitize_element(el);
                }
                created.push(s.id.clone());
                deck.slides.insert(start + offset, s);
            }
            Ok(outcome("Paste Slides", created.last().cloned(), created))
        }
        Op::UpdateSlides { ids, patch } => {
            let patch = strip(&patch, &["id", "elements"]);
            for id in &ids {
                let slide = find_slide(deck, id)?;
                let elements = std::mem::take(&mut slide.elements);
                let mut updated: Slide = patched(slide, &patch)?;
                updated.id = id.clone();
                updated.elements = elements;
                *slide = updated;
            }
            Ok(outcome("Change Slide", ids.first().cloned(), vec![]))
        }
        Op::AddElement { slide, kind, variant, props } => {
            let mut el = layouts::new_element(&kind, &variant, deck.width, deck.height);
            if let Some(p) = props {
                el = patched(&el, &strip(&p, &["id", "link"]))?;
            }
            sanitize_element(&mut el);
            let id = el.id.clone();
            let s = find_slide(deck, &slide)?;
            s.elements.push(el);
            Ok(outcome(insert_label(&kind), Some(slide), vec![id]))
        }
        Op::InsertElements { slide, elements, offset } => {
            let s = find_slide(deck, &slide)?;
            let existing_links: Vec<String> = s.elements.iter().map(|e| e.link.clone()).collect();
            let mut created = Vec::new();
            for mut el in elements {
                let keep_link = !existing_links.contains(&el.link) && !el.link.is_empty();
                let link = el.link.clone();
                el.reidentify();
                if keep_link {
                    // Pasting onto another slide keeps the Magic Move link.
                    el.link = link;
                }
                el.x += offset;
                el.y += offset;
                sanitize_element(&mut el);
                created.push(el.id.clone());
                s.elements.push(el);
            }
            Ok(outcome("Paste", Some(slide), created))
        }
        Op::UpdateElements { slide, updates } => {
            let s = find_slide(deck, &slide)?;
            let mut label = n_("Change");
            for u in &updates {
                let el = s.element_mut(&u.id).ok_or_else(|| anyhow::anyhow!("element not found"))?;
                let mut patch = strip(&u.patch, &["id"]);
                if let Some(Value::Object(c)) = patch.get_mut("content") {
                    c.remove("kind");
                }
                if let Value::Object(m) = &patch {
                    if m.keys().all(|k| matches!(k.as_str(), "x" | "y")) {
                        label = n_("Move");
                    } else if m.keys().all(|k| matches!(k.as_str(), "x" | "y" | "w" | "h")) {
                        label = n_("Resize");
                    } else if m.contains_key("content") {
                        label = n_("Edit");
                    }
                }
                let mut updated = patched(el, &patch)?;
                updated.id = u.id.clone();
                sanitize_element(&mut updated);
                *el = updated;
            }
            Ok(outcome(label, Some(slide), vec![]))
        }
        Op::DeleteElements { slide, ids } => {
            let s = find_slide(deck, &slide)?;
            s.elements.retain(|e| !ids.contains(&e.id));
            Ok(outcome("Delete", Some(slide), vec![]))
        }
        Op::ArrangeElements { slide, ids, action } => {
            let s = find_slide(deck, &slide)?;
            arrange(&mut s.elements, &ids, &action);
            Ok(outcome("Arrange", Some(slide), vec![]))
        }
        Op::AlignElements { slide, ids, mode, to_slide } => {
            let (w, h) = (deck.width as f64, deck.height as f64);
            let s = find_slide(deck, &slide)?;
            align(&mut s.elements, &ids, &mode, to_slide, w, h);
            Ok(outcome("Align", Some(slide), vec![]))
        }
        Op::SetBuildOrder { slide, ids } => {
            // Build events in play order: "ID" is an element's build-in,
            // "ID#path" its motion path. A chart built by series takes one
            // more step per series.
            let s = find_slide(deck, &slide)?;
            let mut step = 1;
            for token in &ids {
                let (id, is_path) = match token.strip_suffix("#path") {
                    Some(id) => (id, true),
                    None => (token.as_str(), false),
                };
                if let Some(el) = s.element_mut(id) {
                    if is_path {
                        el.build.path_order = step;
                        step += 1;
                    } else {
                        el.build.order = step;
                        step += 1 + el.series_steps();
                    }
                }
            }
            Ok(outcome("Reorder Builds", Some(slide), vec![]))
        }
        Op::UpdateDeck { patch } => {
            let patch = strip(&patch, &["slides", "formatVersion", "width", "height"]);
            let slides = std::mem::take(&mut deck.slides);
            let mut updated: Deck = patched(deck, &patch)?;
            updated.slides = slides;
            let label = if patch.get("theme").is_some() {
                n_("Change Theme")
            } else {
                n_("Change Document")
            };
            *deck = updated;
            Ok(outcome(label, None, vec![]))
        }
        Op::ResizeDeck { width, height } => {
            let (ow, oh) = (deck.width as f64, deck.height as f64);
            let (nw, nh) = (width.clamp(320, 7680) as f64, height.clamp(240, 4320) as f64);
            let (sx, sy) = (nw / ow, nh / oh);
            let s = sx.min(sy);
            for slide in &mut deck.slides {
                for el in &mut slide.elements {
                    let (cx, cy) = ((el.x + el.w / 2.0) * sx, (el.y + el.h / 2.0) * sy);
                    el.w *= s;
                    el.h *= s;
                    el.x = cx - el.w / 2.0;
                    el.y = cy - el.h / 2.0;
                    // Motion paths lead to the same places on the slide.
                    for p in &mut el.build.path {
                        p[0] *= sx;
                        p[1] *= sy;
                    }
                    scale_fonts(&mut el.content, s);
                }
            }
            deck.width = nw as u32;
            deck.height = nh as u32;
            Ok(outcome("Change Slide Size", None, vec![]))
        }
        Op::ReplaceDeck { deck: new, label } => {
            *deck = *new;
            for slide in &mut deck.slides {
                for el in &mut slide.elements {
                    sanitize_element(el);
                }
            }
            // Imports start at the first slide; labelled replacements
            // (pastes) leave the view where it is, also when undone.
            let slide = if label.is_some() { None } else { deck.slides.first().map(|s| s.id.clone()) };
            Ok(outcome(label.as_deref().unwrap_or(n_("Replace Presentation")), slide, vec![]))
        }
    }
}

/// Font sizes follow the slide size, including the sizes of parts of a text.
fn scale_fonts(content: &mut Content, s: f64) {
    if let Some(t) = content.text_mut() {
        t.style.font_size = (t.style.font_size * s).round().max(4.0);
        t.html = sanitize::scale_font_sizes(&t.html, s);
    }
    match content {
        Content::Code(c) => c.font_size = (c.font_size * s).round().max(4.0),
        Content::Math(m) => m.font_size = (m.font_size * s).round().max(4.0),
        Content::Table(t) => {
            t.font_size = (t.font_size * s).round().max(4.0);
            for cell in t.rows.iter_mut().flatten() {
                *cell = sanitize::scale_font_sizes(cell, s);
            }
        }
        Content::Chart(c) => c.font_size = (c.font_size * s).round().max(4.0),
        _ => {}
    }
}

/// Z-order changes. The element list is back-to-front.
pub fn arrange(elements: &mut Vec<Element>, ids: &[String], action: &str) {
    let selected = |e: &Element| ids.contains(&e.id);
    match action {
        "front" => {
            let (mut sel, rest): (Vec<_>, Vec<_>) = elements.drain(..).partition(|e| selected(e));
            elements.extend(rest);
            elements.append(&mut sel);
        }
        "back" => {
            let (sel, mut rest): (Vec<_>, Vec<_>) = elements.drain(..).partition(|e| selected(e));
            elements.extend(sel);
            elements.append(&mut rest);
        }
        "forward" => {
            for i in (0..elements.len().saturating_sub(1)).rev() {
                if selected(&elements[i]) && !selected(&elements[i + 1]) {
                    elements.swap(i, i + 1);
                }
            }
        }
        "backward" => {
            for i in 1..elements.len() {
                if selected(&elements[i]) && !selected(&elements[i - 1]) {
                    elements.swap(i, i - 1);
                }
            }
        }
        _ => {}
    }
}

/// Alignment and distribution of unlocked elements (bounding boxes).
pub fn align(elements: &mut [Element], ids: &[String], mode: &str, to_slide: bool, sw: f64, sh: f64) {
    let mut targets: Vec<&mut Element> = elements.iter_mut().filter(|e| ids.contains(&e.id) && !e.locked).collect();
    if targets.is_empty() {
        return;
    }
    let use_slide = to_slide || targets.len() == 1;
    let (l, t, r, b) = if use_slide {
        (0.0, 0.0, sw, sh)
    } else {
        targets.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(l, t, r, b), e| {
            (l.min(e.x), t.min(e.y), r.max(e.x + e.w), b.max(e.y + e.h))
        })
    };
    match mode {
        "left" => targets.iter_mut().for_each(|e| e.x = l),
        "hcenter" => targets.iter_mut().for_each(|e| e.x = ((l + r) / 2.0 - e.w / 2.0).round()),
        "right" => targets.iter_mut().for_each(|e| e.x = r - e.w),
        "top" => targets.iter_mut().for_each(|e| e.y = t),
        "vcenter" => targets.iter_mut().for_each(|e| e.y = ((t + b) / 2.0 - e.h / 2.0).round()),
        "bottom" => targets.iter_mut().for_each(|e| e.y = b - e.h),
        "distribute-h" if targets.len() > 2 || use_slide => {
            targets.sort_by(|a, b| a.x.total_cmp(&b.x));
            let total: f64 = targets.iter().map(|e| e.w).sum();
            let gap = ((r - l) - total) / (targets.len() as f64 - 1.0).max(1.0);
            let mut x = l;
            if targets.len() == 1 {
                targets[0].x = ((l + r) / 2.0 - targets[0].w / 2.0).round();
            } else {
                for e in targets.iter_mut() {
                    e.x = x.round();
                    x += e.w + gap;
                }
            }
        }
        "distribute-v" if targets.len() > 2 || use_slide => {
            targets.sort_by(|a, b| a.y.total_cmp(&b.y));
            let total: f64 = targets.iter().map(|e| e.h).sum();
            let gap = ((b - t) - total) / (targets.len() as f64 - 1.0).max(1.0);
            let mut y = t;
            if targets.len() == 1 {
                targets[0].y = ((t + b) / 2.0 - targets[0].h / 2.0).round();
            } else {
                for e in targets.iter_mut() {
                    e.y = y.round();
                    y += e.h + gap;
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Undo history
// ---------------------------------------------------------------------------

struct Entry {
    deck: Deck,
    label: String,
    slide: Option<String>,
}

/// Snapshot-based undo/redo. Decks are small, so whole-deck snapshots are
/// simple and robust. Consecutive operations sharing a coalescing key (e.g.
/// typing in one text box, dragging a color slider) collapse into one step.
#[derive(Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    last_key: Option<(String, std::time::Instant)>,
}

const MAX_HISTORY: usize = 300;
const COALESCE_WINDOW: std::time::Duration = std::time::Duration::from_millis(2500);

impl History {
    pub fn record(&mut self, before: Deck, after: &Deck, out: &OpOutcome, coalesce: Option<&str>) {
        let now = std::time::Instant::now();
        let merge = match (coalesce, &self.last_key) {
            (Some(k), Some((last, at))) => k == last && now.duration_since(*at) < COALESCE_WINDOW,
            _ => false,
        };
        self.last_key = coalesce.map(|k| (k.to_string(), now));
        self.redo.clear();
        if merge && !self.undo.is_empty() {
            // A step that ends where it began (a letter typed and deleted)
            // would make an undo that changes nothing.
            if self.undo.last().is_some_and(|e| e.deck == *after) {
                self.undo.pop();
                self.last_key = None;
            }
            return;
        }
        self.undo.push(Entry {
            deck: before,
            label: out.label.clone(),
            slide: out.slide.clone(),
        });
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
    }

    /// Breaks coalescing so the next change starts a new undo step.
    pub fn seal(&mut self) {
        self.last_key = None;
    }

    pub fn undo(&mut self, current: &mut Deck) -> Option<OpOutcome> {
        let e = self.undo.pop()?;
        self.last_key = None;
        let out = OpOutcome {
            label: e.label.clone(),
            slide: e.slide.clone(),
            created: vec![],
        };
        let prev = std::mem::replace(current, e.deck);
        self.redo.push(Entry {
            deck: prev,
            label: e.label,
            slide: e.slide,
        });
        Some(out)
    }

    pub fn redo(&mut self, current: &mut Deck) -> Option<OpOutcome> {
        let e = self.redo.pop()?;
        self.last_key = None;
        let out = OpOutcome {
            label: e.label.clone(),
            slide: e.slide.clone(),
            created: vec![],
        };
        let next = std::mem::replace(current, e.deck);
        self.undo.push(Entry {
            deck: next,
            label: e.label,
            slide: e.slide,
        });
        Some(out)
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deck3() -> Deck {
        let mut d = Deck::default();
        for _ in 0..3 {
            d.slides.push(layouts::new_slide("title-content", d.width, d.height));
        }
        d
    }

    fn ids(d: &Deck) -> Vec<String> {
        d.slides.iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn merge_patch_follows_rfc7396() {
        let mut v = json!({"a": 1, "b": {"c": 2, "d": 3}});
        merge_patch(&mut v, &json!({"a": null, "b": {"c": 9}, "e": [1]}));
        assert_eq!(v, json!({"b": {"c": 9, "d": 3}, "e": [1]}));
    }

    #[test]
    fn add_slide_after() {
        let mut d = deck3();
        let before = ids(&d);
        let out = apply(
            &mut d,
            Op::AddSlide {
                after: Some(before[0].clone()),
                layout: "blank".into(),
            },
        )
        .unwrap();
        assert_eq!(d.slides.len(), 4);
        assert_eq!(d.slides[1].id, out.created[0]);
    }

    #[test]
    fn move_slides_to_end_and_start() {
        let mut d = deck3();
        let s = ids(&d);
        apply(
            &mut d,
            Op::MoveSlides {
                ids: vec![s[0].clone()],
                to: 3,
            },
        )
        .unwrap();
        assert_eq!(ids(&d), vec![s[1].clone(), s[2].clone(), s[0].clone()]);
        apply(
            &mut d,
            Op::MoveSlides {
                ids: vec![s[0].clone()],
                to: 0,
            },
        )
        .unwrap();
        assert_eq!(ids(&d), s);
        // Moving two slides between others.
        apply(
            &mut d,
            Op::MoveSlides {
                ids: vec![s[0].clone(), s[1].clone()],
                to: 3,
            },
        )
        .unwrap();
        assert_eq!(ids(&d), vec![s[2].clone(), s[0].clone(), s[1].clone()]);
    }

    #[test]
    fn delete_all_leaves_one_slide() {
        let mut d = deck3();
        let s = ids(&d);
        apply(&mut d, Op::DeleteSlides { ids: s }).unwrap();
        assert_eq!(d.slides.len(), 1);
    }

    #[test]
    fn update_element_sanitizes_and_keeps_id() {
        let mut d = deck3();
        let sid = d.slides[0].id.clone();
        let eid = d.slides[0].elements[0].id.clone();
        apply(
            &mut d,
            Op::UpdateElements {
                slide: sid.clone(),
                updates: vec![ElementPatch {
                    id: eid.clone(),
                    patch: json!({"id": "hacked", "x": 10, "content": {"kind": "image", "html": "<p>ok<script>x</script></p>"}}),
                }],
            },
        )
        .unwrap();
        let el = &d.slides[0].elements[0];
        assert_eq!(el.id, eid);
        assert_eq!(el.x, 10.0);
        assert_eq!(el.kind(), "text");
        assert_eq!(el.content.text().unwrap().html, "<p>ok</p>");
    }

    #[test]
    fn failed_op_leaves_deck_untouched() {
        let mut d = deck3();
        let snapshot = d.clone();
        assert!(
            apply(
                &mut d,
                Op::DeleteElements {
                    slide: "missing".into(),
                    ids: vec![]
                }
            )
            .is_err()
        );
        assert_eq!(d, snapshot);
    }

    #[test]
    fn build_order_gives_chart_series_and_paths_their_steps() {
        let mut d = deck3();
        let sid = d.slides[0].id.clone();
        let chart = Element {
            content: Content::Chart(ChartContent::default()),
            build: Build {
                effect: "cz-wipe-up".into(),
                delivery: "series".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let series = chart.series_steps();
        assert!(series > 1);
        let mover = Element {
            build: Build {
                effect: "fade-in".into(),
                path: vec![[0.0, 0.0], [100.0, 50.0]],
                ..Default::default()
            },
            ..Default::default()
        };
        let (c, m) = (chart.id.clone(), mover.id.clone());
        d.slides[0].elements.extend([chart, mover]);
        let order = vec![m.clone(), c.clone(), format!("{m}#path")];
        apply(&mut d, Op::SetBuildOrder { slide: sid, ids: order }).unwrap();
        let s = &d.slides[0];
        let (chart, mover) = (s.elements.iter().find(|e| e.id == c).unwrap(), s.elements.iter().find(|e| e.id == m).unwrap());
        assert_eq!(mover.build.order, 1);
        assert_eq!(chart.build.order, 2);
        assert_eq!(mover.build.path_order, 3 + series, "the path comes after every series step");
    }

    #[test]
    fn arrange_orders() {
        let mut els: Vec<Element> = (0..4).map(|_| Element::default()).collect();
        let order = |els: &Vec<Element>| els.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        let orig = order(&els);
        arrange(&mut els, &[orig[1].clone()], "front");
        assert_eq!(order(&els).last(), Some(&orig[1]));
        arrange(&mut els, &[orig[1].clone()], "back");
        assert_eq!(order(&els).first(), Some(&orig[1]));
        arrange(&mut els, &[orig[1].clone()], "forward");
        assert_eq!(order(&els)[1], orig[1]);
    }

    #[test]
    fn align_and_distribute() {
        let mut els: Vec<Element> = (0..3)
            .map(|i| Element {
                x: i as f64 * 100.0 + 7.0,
                y: i as f64 * 10.0,
                w: 50.0,
                h: 50.0,
                ..Default::default()
            })
            .collect();
        let ids: Vec<String> = els.iter().map(|e| e.id.clone()).collect();
        align(&mut els, &ids, "top", false, 1280.0, 720.0);
        assert!(els.iter().all(|e| e.y == 0.0));
        align(&mut els, &ids, "distribute-h", true, 1280.0, 720.0);
        assert_eq!(els[0].x, 0.0);
        assert_eq!(els[2].x + els[2].w, 1280.0);
    }

    #[test]
    fn history_undo_redo_and_coalesce() {
        let mut d = deck3();
        let mut h = History::default();
        let sid = d.slides[0].id.clone();
        let eid = d.slides[0].elements[0].id.clone();
        let original = d.clone();
        for x in [1, 2, 3] {
            let before = d.clone();
            let out = apply(
                &mut d,
                Op::UpdateElements {
                    slide: sid.clone(),
                    updates: vec![ElementPatch {
                        id: eid.clone(),
                        patch: json!({"x": x}),
                    }],
                },
            )
            .unwrap();
            h.record(before.clone(), &d, &out, Some("drag"));
        }
        assert_eq!(d.slides[0].elements[0].x, 3.0);
        h.undo(&mut d).unwrap();
        assert_eq!(d, original);
        assert!(h.undo(&mut d).is_none());
        h.redo(&mut d).unwrap();
        assert_eq!(d.slides[0].elements[0].x, 3.0);
    }

    #[test]
    fn a_merged_step_back_where_it_began_is_dropped() {
        let mut d = deck3();
        let mut h = History::default();
        let sid = d.slides[0].id.clone();
        let eid = d.slides[0].elements[0].id.clone();
        let step = |d: &mut Deck, h: &mut History, x: f64, key: &str| {
            let before = d.clone();
            let patch = ElementPatch {
                id: eid.clone(),
                patch: json!({ "x": x }),
            };
            let out = apply(
                d,
                Op::UpdateElements {
                    slide: sid.clone(),
                    updates: vec![patch],
                },
            )
            .unwrap();
            h.record(before.clone(), d, &out, Some(key));
        };
        step(&mut d, &mut h, 50.0, "move");
        let moved = d.clone();
        step(&mut d, &mut h, 7.0, "text");
        step(&mut d, &mut h, 50.0, "text");
        assert_eq!(h.undo.len(), 1, "the typing step undid itself");
        // Later typing starts a new step instead of joining the move.
        step(&mut d, &mut h, 9.0, "text");
        h.undo(&mut d).unwrap();
        assert_eq!(d, moved);
        h.undo(&mut d).unwrap();
        assert_eq!(d.slides[0].elements[0].x, deck3().slides[0].elements[0].x);
    }

    #[test]
    fn resize_deck_scales_content() {
        let mut d = deck3();
        apply(&mut d, Op::ResizeDeck { width: 1024, height: 768 }).unwrap();
        assert_eq!((d.width, d.height), (1024, 768));
        for s in &d.slides {
            for e in &s.elements {
                assert!(e.x >= -1.0 && e.x + e.w <= 1025.0);
            }
        }
    }

    #[test]
    fn resizing_scales_parts_of_texts_and_motion_paths() {
        let mut d = deck3();
        let mut text = RichText {
            html: r#"<p>Big <span style="color: #f00; font-size: 80px">part</span> and font-size: 80px as words</p>"#.into(),
            ..Default::default()
        };
        text.style.font_size = 40.0;
        let el = &mut d.slides[0].elements[0];
        el.content = Content::Text(TextContent { text });
        el.build.path = vec![[0.0, 0.0], [200.0, -100.0]];
        d.slides[0].elements[1].content = Content::Table(TableContent {
            rows: vec![vec![r#"<span style="font-size:30px">x</span>"#.into()]],
            font_size: 24.0,
            ..Default::default()
        });
        apply(&mut d, Op::ResizeDeck { width: 640, height: 360 }).unwrap();
        let els = &d.slides[0].elements;
        let Content::Text(t) = &els[0].content else { panic!() };
        assert_eq!(t.text.style.font_size, 20.0);
        assert!(t.text.html.contains("font-size:40px\">part"), "{}", t.text.html);
        assert!(t.text.html.contains("and font-size: 80px as words"), "text stays as written: {}", t.text.html);
        assert_eq!(els[0].build.path, vec![[0.0, 0.0], [100.0, -50.0]]);
        let Content::Table(table) = &els[1].content else { panic!() };
        assert_eq!(table.font_size, 12.0);
        assert!(table.rows[0][0].contains("font-size:15px"), "{}", table.rows[0][0]);
    }
}
