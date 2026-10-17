//! Map charts: regions colored by value on a world, continent or country
//! map. Boundaries come from Natural Earth (public domain), simplified by
//! tools/gen_maps.py into maps/*.bin and decoded once, on first use.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::OnceLock;

use serde::Serialize;

use crate::util::{esc, num};

type Point = (f64, f64);
/// A box on the chart: x, y, width, height.
type Frame = (f64, f64, f64, f64);
/// Rings of a polygon (the first is the outline, the others holes), as
/// longitude and latitude in degrees.
type Polygon = Vec<Vec<Point>>;

/// Languages with region names of their own, in the order of
/// tools/gen_maps.py (LANGS).
const LANGS: [&str; 14] = ["ar", "bn", "de", "es", "fr", "hi", "it", "ja", "nl", "pl", "pt", "ru", "tr", "zh"];

/// The continent maps (named in ui/js/charts.js): id and the part of the
/// globe they show (west, east, south, north).
pub const CONTINENTS: &[(&str, [f64; 4])] = &[
    ("africa", [-20.0, 55.0, -36.0, 38.0]),
    ("asia", [25.0, 150.0, -11.0, 78.0]),
    ("europe", [-25.0, 45.0, 34.0, 72.0]),
    ("north-america", [-170.0, -50.0, 7.0, 84.0]),
    ("oceania", [110.0, 180.0, -50.0, 0.0]),
    ("south-america", [-82.0, -34.0, -56.0, 13.0]),
];

pub struct Region {
    /// Countries: ISO-like three-letter code; states: ISO 3166-2 code.
    pub id: String,
    /// Countries: continent id; states: their country's id.
    pub parent: String,
    /// The English name.
    pub name: String,
    alts: Vec<String>,
    langs: Vec<(usize, String)>,
    polys: Vec<Polygon>,
}

impl Region {
    /// The region's name in `language` (a UI language code like "pt-BR").
    pub fn name_in(&self, language: &str) -> &str {
        let lang = language.split('-').next().unwrap_or("");
        LANGS
            .iter()
            .position(|l| *l == lang)
            .and_then(|i| self.langs.iter().find(|(k, _)| *k == i))
            .map_or(self.name.as_str(), |(_, n)| n.as_str())
    }

    /// The names a row can use for the region, by rank: its code, its
    /// English name, other English names and codes, names in other
    /// languages.
    fn names(&self, rank: usize) -> Vec<&str> {
        match rank {
            0 => vec![self.id.as_str()],
            1 => vec![self.name.as_str()],
            2 => self.alts.iter().map(String::as_str).collect(),
            _ => self.langs.iter().map(|(_, n)| n.as_str()).collect(),
        }
    }
}

struct Atlas {
    countries: Vec<Region>,
    states: Vec<Region>,
}

fn atlas() -> &'static Atlas {
    static ATLAS: OnceLock<Atlas> = OnceLock::new();
    ATLAS.get_or_init(|| Atlas {
        countries: decode(include_bytes!("../maps/countries.bin")),
        states: decode(include_bytes!("../maps/states.bin")),
    })
}

/// Reads tools/gen_maps.py's format; stops quietly at anything malformed.
fn decode(data: &[u8]) -> Vec<Region> {
    struct Reader<'a> {
        data: &'a [u8],
        pos: usize,
    }
    impl Reader<'_> {
        fn varint(&mut self) -> Option<u64> {
            let (mut n, mut shift) = (0u64, 0);
            loop {
                let b = *self.data.get(self.pos)?;
                self.pos += 1;
                n |= ((b & 0x7f) as u64) << shift;
                if b < 0x80 {
                    return Some(n);
                }
                shift += 7;
                if shift > 63 {
                    return None;
                }
            }
        }
        fn zigzag(&mut self) -> Option<i64> {
            let n = self.varint()?;
            Some((n >> 1) as i64 ^ -((n & 1) as i64))
        }
        fn text(&mut self) -> Option<String> {
            let n = self.varint()? as usize;
            let s = self.data.get(self.pos..self.pos + n)?;
            self.pos += n;
            Some(String::from_utf8_lossy(s).into_owned())
        }
        fn region(&mut self) -> Option<Region> {
            let (id, parent, name) = (self.text()?, self.text()?, self.text()?);
            let mut alts = (0..self.varint()?).map(|_| self.text()).collect::<Option<Vec<_>>>()?;
            // Natural Earth writes "-99" for a missing code; kept as a name,
            // rows labeled "-99" or "99" would find whichever country has it.
            alts.retain(|a| fold(a) != "99");
            let mask = self.varint()?;
            let langs = (0..LANGS.len())
                .filter(|i| mask >> i & 1 == 1)
                .map(|i| Some((i, self.text()?)))
                .collect::<Option<Vec<_>>>()?;
            let mut polys = Vec::new();
            for _ in 0..self.varint()? {
                let mut rings = Vec::new();
                for _ in 0..self.varint()? {
                    let (mut x, mut y) = (0i64, 0i64);
                    let mut ring = Vec::new();
                    for _ in 0..self.varint()? {
                        x += self.zigzag()?;
                        y += self.zigzag()?;
                        ring.push((x as f64 / 1e4, y as f64 / 1e4));
                    }
                    rings.push(ring);
                }
                polys.push(rings);
            }
            Some(Region {
                id,
                parent,
                name,
                alts,
                langs,
                polys,
            })
        }
    }
    let mut r = Reader { data, pos: 0 };
    if data.get(..6) != Some(b"CZMAP1") {
        return Vec::new();
    }
    r.pos = 6;
    let n = r.varint().unwrap_or(0);
    (0..n).map_while(|_| r.region()).collect()
}

/// A name as matching sees it: lower case, without accents, spaces or
/// punctuation ("São Paulo", "SAO-PAULO" and "Sao Paulo" are alike).
pub fn fold(s: &str) -> String {
    const LETTERS: &[(&str, &str)] = &[
        ("àáâãäåāăąǎạảấầẩẫậắằẳẵặ", "a"),
        ("çćĉċč", "c"),
        ("ďđð", "d"),
        ("èéêëēĕėęěẹẻẽếềểễệ", "e"),
        ("ĝğġģ", "g"),
        ("ĥħ", "h"),
        ("ìíîïĩīĭįıǐịỉ", "i"),
        ("ĵ", "j"),
        ("ķ", "k"),
        ("ĺļľŀł", "l"),
        ("ñńņňŉ", "n"),
        ("òóôõöøōŏőǒọỏốồổỗộớờởỡợơ", "o"),
        ("ŕŗř", "r"),
        ("śŝşšș", "s"),
        ("ţťŧț", "t"),
        ("ùúûüũūŭůűųǔụủứừửữựư", "u"),
        ("ŵ", "w"),
        ("ýÿŷỳỵỷỹ", "y"),
        ("źżž", "z"),
        ("ß", "ss"),
        ("æ", "ae"),
        ("œ", "oe"),
        ("þ", "th"),
    ];
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        if let Some((_, plain)) = LETTERS.iter().find(|(set, _)| set.contains(c)) {
            out.push_str(plain);
        } else if c.is_alphanumeric() {
            out.push(c);
        }
    }
    out
}

/// What a map shows: the regions that take values, the regions drawn
/// around them for context and how the globe is projected.
struct View<'a> {
    regions: Vec<&'a Region>,
    context: Vec<&'a Region>,
    projection: Projection,
    /// Longitude/latitude window that fills the chart (None: all regions).
    window: Option<[f64; 4]>,
    /// Parts of a country far from the rest, drawn in boxes of their own.
    insets: Vec<&'a Region>,
}

#[derive(Clone, Copy)]
enum Projection {
    /// Equal Earth, for the world.
    EqualEarth,
    /// Lambert azimuthal equal-area, centered on (longitude, latitude).
    Azimuthal(f64, f64),
}

impl Projection {
    /// Projects a point; y grows downward, as in SVG.
    fn project(self, (lon, lat): Point) -> Option<Point> {
        let (l, p) = (lon.to_radians(), lat.to_radians());
        match self {
            Projection::EqualEarth => {
                const A: [f64; 4] = [1.340264, -0.081106, 0.000893, 0.003796];
                let m = 3f64.sqrt() / 2.0;
                let t = (m * p.sin()).asin();
                let (t2, t6) = (t * t, t.powi(6));
                let x = l * t.cos() / (m * (A[0] + 3.0 * A[1] * t2 + t6 * (7.0 * A[2] + 9.0 * A[3] * t2)));
                let y = t * (A[0] + A[1] * t2 + t6 * (A[2] + A[3] * t2));
                Some((x, -y))
            }
            Projection::Azimuthal(lon0, lat0) => {
                let dl = wrap(lon - lon0).to_radians();
                let p0 = lat0.to_radians();
                let cosc = p0.sin() * p.sin() + p0.cos() * p.cos() * dl.cos();
                if 1.0 + cosc < 1e-6 {
                    return None;
                }
                let k = (2.0 / (1.0 + cosc)).sqrt();
                Some((k * p.cos() * dl.sin(), -k * (p0.cos() * p.sin() - p0.sin() * p.cos() * dl.cos())))
            }
        }
    }
}

/// A longitude difference brought into -180..180.
fn wrap(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

/// Bounds (west, east, south, north) of polygons, longitudes taken
/// around `lon0` so that shapes across the date line stay whole.
fn bounds<'a>(polys: impl Iterator<Item = &'a Polygon>, lon0: f64) -> Option<[f64; 4]> {
    let mut b: Option<[f64; 4]> = None;
    for &(x, y) in polys.filter_map(|p| p.first()).flatten() {
        let x = lon0 + wrap(x - lon0);
        b = Some(match b {
            None => [x, x, y, y],
            Some([w, e, s, n]) => [w.min(x), e.max(x), s.min(y), n.max(y)],
        });
    }
    b
}

/// The middle longitude of a set of polygons (a circular mean, so that a
/// country across the date line is not centered on the other side).
fn mid_longitude<'a>(polys: impl Iterator<Item = &'a Polygon>) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for &(x, _) in polys.filter_map(|p| p.first()).flatten() {
        s += x.to_radians().sin();
        c += x.to_radians().cos();
    }
    s.atan2(c).to_degrees()
}

/// Rough area in square degrees, shrunk toward the poles.
fn area(poly: &Polygon) -> f64 {
    let Some(ring) = poly.first().filter(|r| r.len() > 2) else { return 0.0 };
    let lat = ring.iter().map(|p| p.1).sum::<f64>() / ring.len() as f64;
    let twice: f64 = ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
    (twice / 2.0).abs() * lat.to_radians().cos()
}

fn view(map: &str) -> View<'static> {
    let a = atlas();
    if let Some((_, window)) = CONTINENTS.iter().find(|c| c.0 == map) {
        let [w, e, s, n] = *window;
        let (lon0, lat0) = ((w + e) / 2.0, (s + n) / 2.0);
        let inside = |r: &Region| bounds(r.polys.iter(), lon0).is_some_and(|[rw, re, rs, rn]| rw <= e && re >= w && rs <= n && rn >= s);
        return View {
            regions: a.countries.iter().filter(|r| r.parent == map).collect(),
            context: a.countries.iter().filter(|r| r.parent != map && inside(r)).collect(),
            projection: Projection::Azimuthal(lon0, lat0),
            window: Some(*window),
            insets: Vec::new(),
        };
    }
    let states: Vec<&Region> = a.states.iter().filter(|r| r.parent == map).collect();
    if states.is_empty() {
        return View {
            regions: a.countries.iter().filter(|r| r.parent != "antarctica").collect(),
            context: Vec::new(),
            projection: Projection::EqualEarth,
            window: None,
            insets: Vec::new(),
        };
    }
    // A country: the main group of shapes fills the chart; regions far
    // from it (Alaska, overseas departments, the Canaries) get insets.
    let lon0 = mid_longitude(states.iter().flat_map(|r| r.polys.iter()));
    let polys: Vec<(usize, &Polygon, [f64; 4])> = states
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.polys.iter().filter_map(move |p| Some((i, p, bounds(std::iter::once(p), lon0)?))))
        .collect();
    let mut group: Vec<usize> = (0..polys.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..polys.len() {
        for j in i + 1..polys.len() {
            let ([aw, ae, as_, an], [bw, be, bs, bn]) = (polys[i].2, polys[j].2);
            let cos = ((as_ + an + bs + bn) / 4.0).to_radians().cos();
            let dx = (aw - be).max(bw - ae).max(0.0) * cos;
            let dy = (as_ - bn).max(bs - an).max(0.0);
            // Shapes less than 2.5° apart belong together.
            if dx.hypot(dy) < 2.5 {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut size: HashMap<usize, f64> = HashMap::new();
    for (i, (_, poly, _)) in polys.iter().enumerate() {
        *size.entry(root(&mut group, i)).or_default() += area(poly);
    }
    let main = size.iter().max_by(|a, b| a.1.total_cmp(b.1)).map(|(k, _)| *k);
    let window = bounds((0..polys.len()).filter(|&i| Some(root(&mut group, i)) == main).map(|i| polys[i].1), lon0);
    let near = |r: &Region| {
        let Some([w, e, s, n]) = window else { return true };
        r.polys
            .iter()
            .filter_map(|p| bounds(std::iter::once(p), lon0))
            .any(|[rw, re, rs, rn]| rw <= e + 1.0 && re >= w - 1.0 && rs <= n + 1.0 && rn >= s - 1.0)
    };
    let insets: Vec<&Region> = states.iter().copied().filter(|r| !near(r)).take(6).collect();
    let lat0 = window.map_or(0.0, |[_, _, s, n]| (s + n) / 2.0);
    let lon_mid = window.map_or(lon0, |[w, e, _, _]| (w + e) / 2.0);
    View {
        regions: states,
        context: Vec::new(),
        projection: Projection::Azimuthal(lon_mid, lat0),
        window,
        insets,
    }
}

/// Which region each name or code (folded) finds. A name goes to the
/// region with the higher-ranked name: Saint Martin's Dutch name
/// "Sint-Maarten" never takes Sint Maarten's English name.
fn name_index(regions: &[&Region]) -> HashMap<String, usize> {
    let mut index: HashMap<String, usize> = HashMap::new();
    for rank in 0..4 {
        for (i, r) in regions.iter().enumerate() {
            for n in r.names(rank) {
                index.entry(fold(n)).or_insert(i);
            }
        }
    }
    index
}

/// The region each row (category label) names, by any of its names or
/// codes, if any.
fn row_regions(regions: &[&Region], labels: &[String]) -> Vec<Option<usize>> {
    rows_in(&name_index(regions), labels)
}

fn rows_in(index: &HashMap<String, usize>, labels: &[String]) -> Vec<Option<usize>> {
    labels.iter().map(|l| index.get(&fold(l)).copied().filter(|_| !l.trim().is_empty())).collect()
}

/// Which row gives each region its value: the first that names it.
fn match_rows(regions: &[&Region], labels: &[String]) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    for (row, region) in row_regions(regions, labels).into_iter().enumerate() {
        if let Some(i) = region {
            out.entry(i).or_insert(row);
        }
    }
    out
}

/// The maps of countries with states or provinces, as (id, name in
/// `language`), sorted by name.
pub fn countries(language: &str) -> Vec<(String, String)> {
    let a = atlas();
    let mut out: Vec<(String, String)> = a
        .countries
        .iter()
        .filter(|c| a.states.iter().any(|s| s.parent == c.id))
        .map(|c| (c.id.clone(), c.name_in(language).to_string()))
        .collect();
    out.sort_by_cached_key(|(_, n)| fold(n));
    out
}

/// A region of a map, for the chart editor.
#[derive(Serialize, Debug)]
pub struct MapRegion {
    pub id: String,
    /// The name in the editor's language.
    pub name: String,
    /// Whether a row names the region.
    pub named: bool,
    /// What a new row for the region says: its name, or its code where the
    /// name finds another region first ("Sanaa" the city and the province).
    pub label: String,
}

/// What the chart editor shows about a map's rows.
#[derive(Serialize, Debug)]
pub struct MapRows {
    /// The map's regions, sorted by name. Areas without a name (a few
    /// islands) are drawn but have no row.
    pub regions: Vec<MapRegion>,
    /// Labels that name no region.
    pub unmatched: Vec<String>,
    /// Labels naming a region that an earlier row names already ("BR-AC"
    /// and then "Acre"): their values are left out.
    pub repeated: Vec<String>,
}

/// The regions of `map` and how the rows (labels) name them.
pub fn regions(map: &str, language: &str, labels: &[String]) -> MapRows {
    let v = view(map);
    let index = name_index(&v.regions);
    let rows = rows_in(&index, labels);
    let named: HashSet<usize> = rows.iter().flatten().copied().collect();
    let mut regions: Vec<MapRegion> = v
        .regions
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.name.trim().is_empty())
        .map(|(i, r)| {
            let name = r.name_in(language);
            let own = index.get(&fold(name)) == Some(&i);
            MapRegion {
                id: r.id.clone(),
                name: name.to_string(),
                named: named.contains(&i),
                label: if own { name } else { &r.id }.to_string(),
            }
        })
        .collect();
    regions.sort_by_cached_key(|r| fold(&r.name));
    let (mut unmatched, mut repeated, mut seen) = (Vec::new(), Vec::new(), HashSet::new());
    for (label, region) in labels.iter().zip(&rows) {
        match region {
            None if !label.trim().is_empty() => unmatched.push(label.clone()),
            Some(i) if !seen.insert(*i) => repeated.push(label.clone()),
            _ => {}
        }
    }
    MapRows { regions, unmatched, repeated }
}

/// Draws polygons as one SVG path (holes cut out with even-odd). Points
/// closer than half a pixel to the previous one are left out, and so are
/// rings too small to see.
fn path_data(polys: &[Polygon], to_px: &dyn Fn(Point) -> Option<Point>) -> String {
    let mut d = String::new();
    for ring in polys.iter().flatten() {
        let mut pts: Vec<Point> = Vec::with_capacity(ring.len());
        for p in ring.iter().filter_map(|&p| to_px(p)) {
            if pts.last().is_none_or(|q: &Point| (p.0 - q.0).abs() + (p.1 - q.1).abs() >= 0.5) {
                pts.push(p);
            }
        }
        let pts = simplify(&pts, 0.35);
        if pts.len() < 3 {
            continue;
        }
        for (k, (x, y)) in pts.iter().enumerate() {
            let _ = write!(d, "{}{:.1} {:.1}", if k == 0 { "M" } else { "L" }, x, y);
        }
        d.push('Z');
    }
    d
}

/// Douglas–Peucker: the points of a ring that keep its shape within
/// `tolerance` pixels.
fn simplify(pts: &[Point], tolerance: f64) -> Vec<Point> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    let (first, last) = (0, pts.len() - 1);
    keep[first] = true;
    keep[last] = true;
    let mut stack = vec![(first, last)];
    while let Some((a, b)) = stack.pop() {
        let (p, q) = (pts[a], pts[b]);
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let len = dx.hypot(dy);
        let mut far = (0.0, 0);
        for (i, r) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = if len > 1e-9 {
                ((r.0 - p.0) * dy - (r.1 - p.1) * dx).abs() / len
            } else {
                (r.0 - p.0).hypot(r.1 - p.1)
            };
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > tolerance {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Where a region's value label goes: the middle of its largest shape,
/// with the room that shape has (width, height).
fn label_spot(r: &Region, to_px: &dyn Fn(Point) -> Option<Point>) -> Option<(Point, f64, f64)> {
    let poly = r.polys.iter().max_by(|a, b| area(a).total_cmp(&area(b)))?;
    let pts: Vec<Point> = poly.first()?.iter().filter_map(|&p| to_px(p)).collect();
    let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    // Area-weighted center of the outline.
    let (mut cx, mut cy, mut a) = (0.0, 0.0, 0.0);
    for (p, q) in pts.iter().zip(pts.iter().cycle().skip(1)) {
        let c = p.0 * q.1 - q.0 * p.1;
        a += c;
        cx += (p.0 + q.0) * c;
        cy += (p.1 + q.1) * c;
    }
    if a.abs() < 1e-9 {
        return None;
    }
    Some(((cx / (3.0 * a), cy / (3.0 * a)), x1 - x0, y1 - y0))
}

/// Fits projected bounds into a box: scale and offset.
fn fit(b: [f64; 4], (bx, by, bw, bh): Frame) -> impl Fn(Point) -> Point {
    let [x0, x1, y0, y1] = b;
    let s = (bw / (x1 - x0).max(1e-9)).min(bh / (y1 - y0).max(1e-9));
    let (ox, oy) = (bx + (bw - (x1 - x0) * s) / 2.0, by + (bh - (y1 - y0) * s) / 2.0);
    move |(x, y)| (ox + (x - x0) * s, oy + (y - y0) * s)
}

/// Projected bounds of a longitude/latitude window (its edges sampled).
fn window_bounds(p: Projection, [w, e, s, n]: [f64; 4]) -> Option<[f64; 4]> {
    let steps = 24;
    let edge = (0..=steps).flat_map(|k| {
        let t = k as f64 / steps as f64;
        [(w + (e - w) * t, s), (w + (e - w) * t, n), (w, s + (n - s) * t), (e, s + (n - s) * t)]
    });
    projected_bounds(edge.filter_map(|q| p.project(q)))
}

fn projected_bounds(points: impl Iterator<Item = Point>) -> Option<[f64; 4]> {
    points.fold(None, |b, (x, y)| {
        Some(match b {
            None => [x, x, y, y],
            Some([x0, x1, y0, y1]) => [x0.min(x), x1.max(x), y0.min(y), y1.max(y)],
        })
    })
}

/// A map chart: regions colored by the first series (rows matched by
/// name or code), the others gray. `color` is the color of the highest
/// value; returns the value range, for the legend.
pub fn map_chart(
    svg: &mut String,
    map: &str,
    labels: &[String],
    values: &[f64],
    color: &str,
    show_values: bool,
    (w, h, f): (f64, f64, f64),
) -> Option<(f64, f64)> {
    let v = view(if map.is_empty() { "world" } else { map });
    let rows = match_rows(&v.regions, labels);
    let value = |i: usize| rows.get(&i).and_then(|&row| values.get(row)).copied().filter(|x| x.is_finite());
    let range = (0..v.regions.len())
        .filter_map(value)
        .fold(None, |r: Option<(f64, f64)>, x| Some(r.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x)))));
    let pad = f * 0.3;
    let plot = (pad, pad, (w - pad * 2.0).max(10.0), (h - pad * 2.0).max(10.0));
    let p = v.projection;
    let extent = match v.window {
        Some(win) => window_bounds(p, win),
        None => projected_bounds(v.regions.iter().flat_map(|r| r.polys.iter().flatten().flatten()).filter_map(|&q| p.project(q))),
    };
    let Some(b) = extent else { return range };
    // Insets go in a column on the left or in a row along the bottom,
    // whichever leaves the main map larger.
    let n = v.insets.len();
    let gap = f * 0.4;
    let most = (plot.2 * 0.18).min(plot.3 * 0.3);
    let scale = |bw: f64, bh: f64| (bw / (b[1] - b[0]).max(1e-9)).min(bh / (b[3] - b[2]).max(1e-9));
    let (main, boxes): (Frame, Vec<(f64, f64, f64)>) = if n == 0 {
        (plot, Vec::new())
    } else {
        let column = most.min((plot.3 - gap * (n - 1) as f64) / n as f64);
        let row = most.min((plot.2 - gap * (n - 1) as f64) / n as f64);
        let beside = (plot.0 + column + gap, plot.1, plot.2 - column - gap, plot.3);
        let above = (plot.0, plot.1, plot.2, plot.3 - row - gap);
        if scale(beside.2, beside.3) >= scale(above.2, above.3) {
            (beside, (0..n).map(|k| (plot.0, plot.1 + k as f64 * (column + gap), column)).collect())
        } else {
            (above, (0..n).map(|k| (plot.0 + k as f64 * (row + gap), plot.1 + plot.3 - row, row)).collect())
        }
    };
    let place = fit(b, main);
    let to_px = |q: Point| p.project(q).map(&place);
    // Everything outside the main map's box is cut off (the rest of a
    // continent, far islands of a region drawn on the main map).
    let _ = write!(
        svg,
        r#"<svg x="{}" y="{}" width="{}" height="{}" overflow="hidden">"#,
        num(main.0),
        num(main.1),
        num(main.2),
        num(main.3)
    );
    let shifted = |q: Point| to_px(q).map(|(x, y)| (x - main.0, y - main.1));
    let border = r#" stroke="currentColor" stroke-opacity="0.35" stroke-width="0.6" stroke-linejoin="round" fill-rule="evenodd""#;
    for r in &v.context {
        let d = path_data(&r.polys, &shifted);
        if !d.is_empty() {
            let _ = write!(svg, r#"<path d="{d}" fill="currentColor" fill-opacity="0.05"{border}/>"#);
        }
    }
    let fill = |i: usize| match (value(i), range) {
        (Some(x), Some(rg)) => (format!("fill:{}", esc(color)), super::chart::heat(x, rg)),
        _ => ("fill:currentColor".to_string(), 0.12),
    };
    let draw = |svg: &mut String, i: usize, r: &Region, to: &dyn Fn(Point) -> Option<Point>| {
        let d = path_data(&r.polys, to);
        if d.is_empty() {
            return;
        }
        let (style, o) = fill(i);
        let _ = write!(svg, r#"<path d="{d}" style="{style}" fill-opacity="{}"{border}/>"#, num(o));
    };
    for (i, r) in v.regions.iter().enumerate() {
        if !v.insets.iter().any(|x| std::ptr::eq(*x, *r)) {
            draw(svg, i, r, &shifted);
        }
    }
    if show_values {
        for (i, r) in v.regions.iter().enumerate() {
            let (Some(x), Some(rg)) = (value(i), range) else { continue };
            if v.insets.iter().any(|x| std::ptr::eq(*x, *r)) {
                continue;
            }
            let text = super::chart::fmt_value(x);
            let Some(((cx, cy), rw, rh)) = label_spot(r, &shifted) else { continue };
            if rw < text.len() as f64 * f * 0.45 + f * 0.3 || rh < f * 0.9 {
                continue;
            }
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}"{}>{}</text>"#,
                num(cx),
                num(cy),
                num(f * 0.7),
                if super::chart::heat(x, rg) > 0.55 { r#" style="fill:#fff""# } else { "" },
                text
            );
        }
    }
    svg.push_str("</svg>");
    // Each inset is projected around its own middle.
    for (r, &(bx, by, side)) in v.insets.iter().zip(&boxes) {
        let Some(i) = v.regions.iter().position(|x| std::ptr::eq(*x, *r)) else {
            continue;
        };
        let lon0 = mid_longitude(r.polys.iter());
        let Some([rw, re, rs, rn]) = bounds(r.polys.iter(), lon0) else { continue };
        let ip = Projection::Azimuthal((rw + re) / 2.0, (rs + rn) / 2.0);
        let Some(ib) = projected_bounds(r.polys.iter().flatten().flatten().filter_map(|&q| ip.project(q))) else {
            continue;
        };
        let inner = (bx + side * 0.08, by + side * 0.08, side * 0.84, side * 0.84);
        let place = fit(ib, inner);
        let _ = write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="none" stroke="currentColor" stroke-opacity="0.25"/>"#,
            num(bx),
            num(by),
            num(side),
            num(side),
            num(f * 0.2)
        );
        draw(svg, i, r, &|q: Point| ip.project(q).map(&place));
    }
    range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_data_decodes() {
        let a = atlas();
        assert!(a.countries.len() > 200, "{}", a.countries.len());
        assert!(a.states.len() > 4000, "{}", a.states.len());
        let bra = a.countries.iter().find(|c| c.id == "BRA").expect("Brazil");
        assert_eq!(bra.parent, "south-america");
        assert_eq!(bra.name_in("pt-BR"), "Brasil");
        assert_eq!(bra.name_in("en"), "Brazil");
        assert_eq!(a.states.iter().filter(|s| s.parent == "BRA").count(), 27);
    }

    #[test]
    fn rows_match_names_codes_and_other_languages() {
        let v = view("BRA");
        let labels: Vec<String> = ["SAO PAULO", "BR-RJ", "minas gerais", "Atlantis", ""].iter().map(|s| s.to_string()).collect();
        let rows = match_rows(&v.regions, &labels);
        assert_eq!(rows.len(), 3);
        let found = regions("BRA", "pt-BR", &labels);
        assert_eq!(found.unmatched, ["Atlantis"]);
        assert!(found.repeated.is_empty());
        assert_eq!(found.regions.iter().filter(|r| r.named).count(), 3);
        let world = view("world");
        let rows = match_rows(&world.regions, &["Allemagne".into(), "DEU".into(), "Deutschland".into(), "日本".into()]);
        assert_eq!(rows.len(), 2, "French and German names find Germany, Japanese finds Japan");
        assert_eq!(fold("São Paulo"), fold("sao-paulo"));
    }

    #[test]
    fn a_region_named_twice_is_found_once() {
        // A code, then the name of the same state: Acre has a row already.
        let labels: Vec<String> = ["BR-AC", "Acre", "Bahia"].iter().map(|s| s.to_string()).collect();
        let found = regions("BRA", "en", &labels);
        assert_eq!(found.repeated, ["Acre"]);
        assert!(found.unmatched.is_empty());
        let named: Vec<&str> = found.regions.iter().filter(|r| r.named).map(|r| r.id.as_str()).collect();
        assert_eq!(named, ["BR-AC", "BR-BA"]);
        assert_eq!(found.regions.len(), 27);
        // Sint Maarten's English name finds it, not Saint Martin's Dutch name.
        let world = view("world");
        let rows = row_regions(&world.regions, &["Sint Maarten".into(), "Saint Martin".into()]);
        let ids: Vec<&str> = rows.iter().map(|r| world.regions[r.unwrap()].id.as_str()).collect();
        assert_eq!(ids, ["SXM", "MAF"]);
    }

    #[test]
    fn missing_codes_name_no_region() {
        let a = atlas();
        let mut maps: Vec<String> = vec!["world".into()];
        maps.extend(CONTINENTS.iter().map(|c| c.0.to_string()));
        maps.extend(countries("en").into_iter().map(|c| c.0));
        let labels: Vec<String> = vec!["-99".into(), "99".into()];
        for map in &maps {
            assert_eq!(regions(map, "en", &labels).unmatched, labels, "{map}");
        }
        // The countries that had "-99" as a code keep their real names and codes.
        let world = view("world");
        for id in ["SOL", "NOR", "KOS", "FRA", "CYN", "IOA", "ATC", "KAS"] {
            let r = a.countries.iter().find(|c| c.id == id).expect(id);
            let rows = row_regions(&world.regions, &[r.id.clone(), r.name.clone()]);
            let found: Vec<Option<&str>> = rows.iter().map(|i| i.map(|i| world.regions[i].id.as_str())).collect();
            assert_eq!(found, [Some(id), Some(id)], "{}", r.name);
        }
        let nor = a.countries.iter().find(|c| c.id == "NOR").unwrap();
        assert!(nor.alts.iter().any(|n| n == "NO"), "{:?}", nor.alts);
    }

    #[test]
    fn every_region_has_a_code_of_its_own() {
        let a = atlas();
        let mut seen = HashSet::new();
        for r in a.countries.iter().chain(&a.states) {
            assert!(seen.insert((r.parent.as_str(), r.id.as_str())), "{} {} twice", r.parent, r.id);
        }
        // Pieces of one region are one region; a city keeps a code of its own.
        let cebu = a.states.iter().filter(|r| r.parent == "PHL" && r.name == "Cebu").count();
        assert_eq!(cebu, 1);
    }

    #[test]
    fn adding_all_regions_twice_adds_nothing() {
        // On every map and in every language, the rows Add All Regions
        // writes name their own regions: a second click finds none missing.
        let a = atlas();
        let mut maps: Vec<String> = vec!["world".into()];
        maps.extend(CONTINENTS.iter().map(|c| c.0.to_string()));
        maps.extend(a.countries.iter().filter(|c| a.states.iter().any(|s| s.parent == c.id)).map(|c| c.id.clone()));
        for map in &maps {
            for lang in ["en", "ar", "bn", "de", "es", "fr", "hi", "it", "ja", "nl", "pl", "pt-BR", "ru", "tr", "zh-CN"] {
                let labels: Vec<String> = regions(map, lang, &[]).regions.into_iter().map(|r| r.label).collect();
                let found = regions(map, lang, &labels);
                let missing: Vec<&str> = found.regions.iter().filter(|r| !r.named).map(|r| r.id.as_str()).collect();
                assert!(
                    missing.is_empty() && found.repeated.is_empty() && found.unmatched.is_empty(),
                    "{map} {lang}: missing {missing:?}, repeated {:?}, unmatched {:?}",
                    found.repeated,
                    found.unmatched
                );
            }
        }
    }

    #[test]
    fn far_away_parts_get_insets() {
        let ids = |m: &str| view(m).insets.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        let usa = ids("USA");
        assert!(usa.contains(&"US-AK".to_string()) && usa.contains(&"US-HI".to_string()), "{usa:?}");
        assert!(ids("BRA").is_empty());
        assert!(ids("DEU").is_empty());
    }

    #[test]
    fn every_map_draws() {
        let mut maps: Vec<String> = ["world", ""].iter().map(|s| s.to_string()).collect();
        maps.extend(CONTINENTS.iter().map(|c| c.0.to_string()));
        maps.extend(countries("en").into_iter().map(|c| c.0));
        for m in maps {
            let mut svg = String::new();
            map_chart(&mut svg, &m, &["X".into()], &[1.0], "red", true, (800.0, 450.0, 20.0));
            assert!(svg.contains("<path"), "{m}");
            assert!(!svg.contains("NaN") && !svg.contains("inf"), "{m}");
        }
    }
}
