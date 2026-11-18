use super::n_;
use anyhow::{Context, bail, ensure};
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::Path;

pub const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_PART: u64 = 32 * 1024 * 1024;
const MAX_XML: u64 = 16 * 1024 * 1024;

pub struct Archive {
    pub parts: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct Rel {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub external: bool,
}

/// Resolve OPC targets, including ordinary ../media paths, without escaping
/// the package. Nothing here accesses the host filesystem or the network.
pub fn resolve(part: &str, target: &str) -> anyhow::Result<String> {
    ensure!(!target.contains(['\\', '\0', '?', '#']), n_("Invalid PPTX relationship target"));
    let mut decoded = String::new();
    let mut chars = target.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let a = chars.next().and_then(|v| v.to_digit(16)).context(n_("Invalid encoded PPTX target"))?;
            let b = chars.next().and_then(|v| v.to_digit(16)).context(n_("Invalid encoded PPTX target"))?;
            let byte = (a * 16 + b) as u8;
            ensure!(byte.is_ascii() && byte != b'\\' && byte != 0, n_("Invalid encoded PPTX target"));
            decoded.push(byte as char);
        } else {
            decoded.push(c);
        }
    }
    ensure!(!decoded.contains(':'), n_("External PPTX target was not marked external"));
    let mut segments: Vec<&str> = if decoded.starts_with('/') {
        vec![]
    } else {
        part.rsplit_once('/').map(|(p, _)| p.split('/').collect()).unwrap_or_default()
    };
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                ensure!(segments.pop().is_some(), n_("PPTX relationship escapes the package"));
            }
            x => segments.push(x),
        }
    }
    ensure!(!segments.is_empty(), n_("Empty PPTX relationship target"));
    Ok(segments.join("/"))
}

impl Archive {
    fn relationships_part(part: &str) -> String {
        match part.rsplit_once('/') {
            Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
            None => format!("_rels/{part}.rels"),
        }
    }

    pub fn relationships_bytes(&self, part: &str) -> usize {
        self.parts.get(&Self::relationships_part(part)).map_or(0, Vec::len)
    }
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path).context(n_("Cannot open PowerPoint presentation"))?;
        ensure!(file.metadata()?.len() <= MAX_TOTAL, n_("PowerPoint file is too large (maximum 256 MB)"));
        let mut zip = zip::ZipArchive::new(file).context(n_("This is not an unencrypted PPTX presentation"))?;
        ensure!(zip.len() <= MAX_ENTRIES, n_("PowerPoint archive has too many parts"));
        let mut parts = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            ensure!(!entry.encrypted(), n_("Encrypted PPTX files are not supported"));
            if !entry.is_file() {
                continue;
            }
            let raw = entry.name()?.into_owned();
            let name = resolve("", &raw)?;
            ensure!(raw == name, n_("Noncanonical PPTX part path"));
            ensure!(!parts.contains_key(&name), n_("Duplicate PPTX part"));
            let limit = if name.ends_with(".xml") || name.ends_with(".rels") {
                MAX_XML
            } else {
                MAX_PART
            };
            ensure!(entry.size() <= limit, n_("PowerPoint part is too large"));
            let mut bytes = Vec::new();
            entry.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= limit, n_("Expanded PowerPoint part is too large"));
            total = total.checked_add(bytes.len() as u64).context(n_("PowerPoint size overflow"))?;
            ensure!(total <= MAX_TOTAL, n_("Expanded PowerPoint archive exceeds 256 MB"));
            if name.ends_with(".xml") || name.ends_with(".rels") {
                parse_xml(&bytes)?;
            }
            parts.insert(name, bytes);
        }
        ensure!(parts.contains_key("[Content_Types].xml"), n_("PowerPoint content types are missing"));
        Ok(Self { parts })
    }

    pub fn xml(&self, part: &str) -> anyhow::Result<Document<'_>> {
        parse_xml(self.parts.get(part).with_context(|| format!("Missing PowerPoint part: {part}"))?)
    }

    pub fn rels(&self, part: &str) -> anyhow::Result<Vec<Rel>> {
        let path = Self::relationships_part(part);
        let Some(bytes) = self.parts.get(&path) else { return Ok(vec![]) };
        let doc = parse_xml(bytes)?;
        let ns = Some("http://schemas.openxmlformats.org/package/2006/relationships");
        ensure!(
            doc.root_element().tag_name().name() == "Relationships" && doc.root_element().tag_name().namespace() == ns,
            n_("Invalid PowerPoint relationships")
        );
        let mut seen = HashSet::new();
        let mut rels = Vec::new();
        for n in doc.root_element().children().filter(Node::is_element) {
            ensure!(
                n.tag_name().name() == "Relationship" && n.tag_name().namespace() == ns,
                n_("Invalid PowerPoint relationship")
            );
            let id = attr(n, "Id").context(n_("PowerPoint relationship has no id"))?.to_owned();
            ensure!(seen.insert(id.clone()), n_("Duplicate PowerPoint relationship id"));
            let raw = attr(n, "Target").context(n_("PowerPoint relationship has no target"))?;
            let external = attr(n, "TargetMode") == Some("External");
            let relationship_type = attr(n, "Type").unwrap_or_default();
            let standard = relationship_type.starts_with("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
                || relationship_type.starts_with("http://purl.oclc.org/ooxml/officeDocument/relationships/");
            rels.push(Rel {
                id,
                kind: if standard {
                    relationship_type.rsplit('/').next().unwrap_or_default().into()
                } else {
                    String::new()
                },
                target: if external { raw.into() } else { resolve(part, raw)? },
                external,
            });
        }
        Ok(rels)
    }

    pub fn related(&self, part: &str, kind: &str) -> anyhow::Result<Option<String>> {
        Ok(self.rels(part)?.into_iter().find(|r| r.kind == kind && !r.external).map(|r| r.target))
    }
}

pub fn parse_xml(bytes: &[u8]) -> anyhow::Result<Document<'_>> {
    let text = std::str::from_utf8(bytes).context(n_("PPTX XML must be UTF-8"))?;
    let doc = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 200_000,
            entity_resolver: None,
        },
    )
    .context(n_("Malformed or unsafe PowerPoint XML"))?;
    if doc.descendants().any(|n| n.ancestors().take(129).count() > 128) {
        bail!(n_("PowerPoint XML is nested too deeply"));
    }
    Ok(doc)
}

pub fn named(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace().is_some_and(|ns| {
            ns == P
                || ns == A
                || ns == "http://schemas.openxmlformats.org/drawingml/2006/chart"
                || ns == "http://schemas.openxmlformats.org/markup-compatibility/2006"
                || matches!(
                    ns,
                    "http://purl.oclc.org/ooxml/presentationml/main"
                        | "http://purl.oclc.org/ooxml/drawingml/main"
                        | "http://purl.oclc.org/ooxml/drawingml/chart"
                )
        })
}
pub fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|n| named(*n, name))
}
pub fn desc<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.descendants().find(|n| named(*n, name))
}
/// OOXML attributes without a prefix must not match extension namespaces.
/// roxmltree 0.21's string lookup matches only the local name.
pub fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.namespace().is_none() && a.name() == name).map(|a| a.value())
}
pub fn rel_attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute((R, name))
        .or_else(|| n.attribute(("http://purl.oclc.org/ooxml/officeDocument/relationships", name)))
}
pub fn num(n: Node<'_, '_>, key: &str, default: f64) -> f64 {
    attr(n, key).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}
