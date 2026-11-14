//! Flat groups preserve the slide's exact render and animation order. The
//! oriented frame provides a stable pivot across repeated gestures and saves.
use crate::model::{Element, Slide};
use crate::util::new_id;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GroupFrame {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
}

impl GroupFrame {
    pub fn valid(&self) -> bool {
        !self.id.is_empty() && [self.x, self.y, self.w, self.h, self.rotation].iter().all(|n| n.is_finite()) && self.w > 0.0 && self.h > 0.0
    }
}

pub fn expand(slide: &Slide, ids: &[String]) -> Vec<String> {
    let groups: HashSet<&str> = slide
        .elements
        .iter()
        .filter(|e| ids.contains(&e.id))
        .filter_map(|e| e.group.as_ref().map(|g| g.id.as_str()))
        .collect();
    slide
        .elements
        .iter()
        .filter(|e| ids.contains(&e.id) || e.group.as_ref().is_some_and(|g| groups.contains(g.id.as_str())))
        .map(|e| e.id.clone())
        .collect()
}

pub fn frame(elements: &[&Element], id: String) -> GroupFrame {
    let (mut left, mut top, mut right, mut bottom) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for e in elements {
        let a = e.rotation.to_radians();
        let w = e.w * a.cos().abs() + e.h * a.sin().abs();
        let h = e.w * a.sin().abs() + e.h * a.cos().abs();
        left = left.min(e.x + (e.w - w) / 2.0);
        top = top.min(e.y + (e.h - h) / 2.0);
        right = right.max(e.x + (e.w + w) / 2.0);
        bottom = bottom.max(e.y + (e.h + h) / 2.0);
    }
    GroupFrame {
        id,
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
        rotation: 0.0,
    }
}

pub fn group(slide: &mut Slide, ids: &[String]) -> anyhow::Result<Vec<String>> {
    let ids = expand(slide, ids);
    let members: Vec<_> = slide.elements.iter().filter(|e| ids.contains(&e.id)).collect();
    anyhow::ensure!(members.len() >= 2, "Select at least two objects");
    anyhow::ensure!(members.iter().all(|e| !e.locked), "Unlock objects before grouping");
    let group = frame(&members, new_id("g"));
    for e in &mut slide.elements {
        if ids.contains(&e.id) {
            e.group = Some(group.clone());
        }
    }
    Ok(ids)
}

pub fn ungroup(slide: &mut Slide, ids: &[String]) {
    let ids = expand(slide, ids);
    for e in &mut slide.elements {
        if ids.contains(&e.id) {
            e.group = None;
        }
    }
}

pub fn transform(slide: &mut Slide, before: &GroupFrame, after: &GroupFrame) -> anyhow::Result<()> {
    anyhow::ensure!(before.valid() && after.valid() && before.id == after.id, "Invalid group frame");
    let k = after.w / before.w;
    anyhow::ensure!((after.h / before.h - k).abs() < 1e-8, "Groups resize proportionally");
    let members: Vec<_> = slide.elements.iter().filter(|e| e.group.as_ref().is_some_and(|g| g.id == before.id)).collect();
    anyhow::ensure!(
        members.len() >= 2 && members.iter().all(|e| e.group.as_ref() == Some(before)),
        "The group changed. Try again."
    );
    anyhow::ensure!(members.iter().all(|e| !e.locked), "The group contains locked objects");
    anyhow::ensure!(
        members
            .iter()
            .all(|e| e.content_scale * k >= 0.001 && e.content_scale * k <= 1000.0 && e.w * k >= 4.0 && e.h * k >= 4.0),
        "Group size is out of range"
    );
    let a = (after.rotation - before.rotation).to_radians();
    let (sin, cos) = a.sin_cos();
    for e in &mut slide.elements {
        if e.group.as_ref().is_none_or(|g| g.id != before.id) {
            continue;
        }
        let dx = (e.x + e.w / 2.0 - before.x - before.w / 2.0) * k;
        let dy = (e.y + e.h / 2.0 - before.y - before.h / 2.0) * k;
        e.w *= k;
        e.h *= k;
        e.content_scale *= k;
        e.x = after.x + after.w / 2.0 + dx * cos - dy * sin - e.w / 2.0;
        e.y = after.y + after.h / 2.0 + dx * sin + dy * cos - e.h / 2.0;
        e.rotation += after.rotation - before.rotation;
        validate_geometry(e)?;
        for p in &mut e.build.path {
            *p = [k * (p[0] * cos - p[1] * sin), k * (p[0] * sin + p[1] * cos)];
        }
        e.group = Some(after.clone());
    }
    Ok(())
}

/// A group transform must never be silently clamped by document normalization.
pub fn validate_geometry(e: &Element) -> anyhow::Result<()> {
    anyhow::ensure!(
        [e.x, e.y, e.w, e.h, e.rotation, e.content_scale].iter().all(|v| v.is_finite())
            && e.x.abs() <= 20000.0
            && e.y.abs() <= 20000.0
            && (4.0..=20000.0).contains(&e.w)
            && (4.0..=20000.0).contains(&e.h),
        "Group size is out of range"
    );
    Ok(())
}

/// Keep the pivot when an ordinary move translates every member. Individual
/// geometry edits (Alt-click) establish a new enclosing frame.
pub fn reconcile(slide: &mut Slide, before: &[Element]) {
    let groups: HashSet<String> = slide.elements.iter().filter_map(|e| e.group.as_ref().map(|g| g.id.clone())).collect();
    for id in groups {
        let members: Vec<_> = slide.elements.iter().filter(|e| e.group.as_ref().is_some_and(|g| g.id == id)).collect();
        let Some(first) = members.first() else {
            continue;
        };
        let Some(old) = before.iter().find(|e| e.id == first.id) else {
            continue;
        };
        let (dx, dy) = (first.x - old.x, first.y - old.y);
        let changed = members.iter().any(|e| {
            before
                .iter()
                .find(|o| o.id == e.id)
                .is_some_and(|o| (e.x, e.y, e.w, e.h, e.rotation) != (o.x, o.y, o.w, o.h, o.rotation))
        });
        if !changed {
            continue;
        }
        let translation = members.iter().all(|e| {
            before
                .iter()
                .find(|o| o.id == e.id)
                .is_some_and(|o| (e.x - o.x - dx).abs() < 1e-8 && (e.y - o.y - dy).abs() < 1e-8 && (e.w, e.h, e.rotation) == (o.w, o.h, o.rotation))
        });
        let g = if translation {
            let mut g = first.group.clone().unwrap();
            g.x += dx;
            g.y += dy;
            g
        } else {
            frame(&members, id.clone())
        };
        for e in &mut slide.elements {
            if e.group.as_ref().is_some_and(|g| g.id == id) {
                e.group = Some(g.clone());
            }
        }
    }
}

pub fn normalize(slide: &mut Slide) {
    let mut groups: HashMap<String, Vec<GroupFrame>> = HashMap::new();
    for e in &slide.elements {
        if let Some(g) = &e.group {
            groups.entry(g.id.clone()).or_default().push(g.clone());
        }
    }
    for e in &mut slide.elements {
        if e.group.as_ref().is_some_and(|g| {
            let members = &groups[&g.id];
            !g.valid() || members.len() < 2 || members.iter().any(|m| m != g)
        }) {
            e.group = None;
        }
    }
}

pub fn reidentify(elements: &mut [Element]) {
    let mut ids = HashMap::new();
    for e in elements {
        if let Some(g) = &mut e.group {
            g.id = ids.entry(g.id.clone()).or_insert_with(|| new_id("g")).clone();
        }
    }
}
