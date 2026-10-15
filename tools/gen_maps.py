#!/usr/bin/env python3
"""Builds the map data for map charts (src-tauri/maps/*.bin) from Natural
Earth (https://www.naturalearthdata.com, public domain).

    curl -LO https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_admin_0_countries.geojson
    curl -LO https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_10m_admin_1_states_provinces.geojson
    python3 tools/gen_maps.py ne_50m_admin_0_countries.geojson ne_10m_admin_1_states_provinces.geojson

Shapes are simplified with mapshaper (run through npx), which keeps shared
borders identical so neighbors never gap. States and provinces are
simplified country by country, in proportion to the country's size, so
that small and large countries both look right when they fill a slide;
islands too small to see at that size are left out (never a region's
main shape).

File format (read by src-tauri/src/maps.rs), integers as LEB128 varints:
    b"CZMAP1", count, then per region:
    id, parent, name, alternative names (count + strings),
    names in other languages (bit mask over LANGS + strings),
    polygons: count, per polygon rings: count, per ring points: count,
    then zigzag deltas of longitude and latitude in 1/10000 degree.
"""

import json
import math
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
MAPSHAPER = ["npx", "--yes", "mapshaper@0.6.121"]
# Languages with names of their own, in the order of maps.rs (LANGS).
LANGS = ["ar", "bn", "de", "es", "fr", "hi", "it", "ja", "nl", "pl", "pt", "ru", "tr", "zh"]
CONTINENTS = {
    "Africa": "africa",
    "Asia": "asia",
    "Europe": "europe",
    "North America": "north-america",
    "South America": "south-america",
    "Oceania": "oceania",
    "Antarctica": "antarctica",
}


def varint(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


def zigzag(n: int) -> bytes:
    return varint((n << 1) ^ (n >> 63))


def text(s: str) -> bytes:
    b = s.encode("utf-8")
    return varint(len(b)) + b


def simplify(src: pathlib.Path, out: pathlib.Path, interval: float, where: str | None = None) -> None:
    cmd = [*MAPSHAPER, "-i", str(src)]
    if where:
        cmd += ["-filter", where]
    cmd += ["-simplify", f"interval={interval}", "keep-shapes", "-o", "format=geojson", "precision=0.0001", str(out)]
    subprocess.run(cmd, check=True, capture_output=True)


def polygons(geom) -> list:
    if not geom:
        return []
    if geom["type"] == "Polygon":
        return [geom["coordinates"]]
    if geom["type"] == "MultiPolygon":
        return geom["coordinates"]
    return []


def encode_shape(polys: list) -> bytes:
    out = bytearray(varint(len(polys)))
    for poly in polys:
        out += varint(len(poly))
        for ring in poly:
            pts = [(round(x * 1e4), round(y * 1e4)) for x, y in ring]
            # The closing point repeats the first one: leave it out.
            if len(pts) > 1 and pts[0] == pts[-1]:
                pts.pop()
            out += varint(len(pts))
            px = py = 0
            for x, y in pts:
                out += zigzag(x - px) + zigzag(y - py)
                px, py = x, y
    return bytes(out)


def clean(s) -> str:
    return " ".join(str(s or "").split())


def area_km2(poly: list) -> float:
    """Area of a polygon's outer ring, roughly (equirectangular)."""
    ring = poly[0] if poly else []
    if len(ring) < 3:
        return 0.0
    lat = math.radians(sum(y for _, y in ring) / len(ring))
    a = sum(x0 * y1 - x1 * y0 for (x0, y0), (x1, y1) in zip(ring, ring[1:] + ring[:1])) / 2
    return abs(a) * 111.32 * 110.57 * math.cos(lat)


def drop_islets(polys: list, min_km2: float) -> list:
    """Leaves out islands too small to see, but never a region's main shape."""
    if len(polys) < 2:
        return polys
    largest = max(polys, key=area_km2)
    return [p for p in polys if p is largest or area_km2(p) >= min_km2]


def encode_region(rid: str, parent: str, name: str, alts: list, langs: dict, polys: list) -> bytes:
    out = bytearray(text(rid) + text(parent) + text(name))
    alts = [a for a in dict.fromkeys(clean(a) for a in alts) if a and a != name]
    out += varint(len(alts))
    for a in alts:
        out += text(a)
    mask, names = 0, []
    for i, lang in enumerate(LANGS):
        n = clean(langs.get(lang))
        if n and n != name:
            mask |= 1 << i
            names.append(n)
    out += varint(mask)
    for n in names:
        out += text(n)
    return bytes(out) + encode_shape(polys)


def write(path: pathlib.Path, regions: list) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = b"CZMAP1" + varint(len(regions)) + b"".join(regions)
    path.write_bytes(data)
    print(f"{path.relative_to(ROOT)}: {len(regions)} regions, {len(data) / 1024:.0f} KiB")


def extent_km(polys: list) -> float:
    pts = [pt for p in polys for r in p for pt in r]
    if not pts:
        return 0.0
    # Longitudes around their circular mean, so that a country across the
    # date line is measured whole.
    mid = math.degrees(math.atan2(sum(math.sin(math.radians(x)) for x, _ in pts), sum(math.cos(math.radians(x)) for x, _ in pts)))
    xs = [mid + (x - mid + 540) % 360 - 180 for x, _ in pts]
    ys = [y for _, y in pts]
    lat = math.radians((max(ys) + min(ys)) / 2)
    return max((max(xs) - min(xs)) * 111.32 * math.cos(lat), (max(ys) - min(ys)) * 110.57)


def main_extent_km(polys: list) -> float:
    """Size of a country's main group of shapes, without far islands and
    overseas parts (shapes less than 2.5° apart belong together, as in
    maps.rs), so that those parts keep their detail in insets."""
    polys = [p for p in polys if p and p[0]]
    if not polys:
        return 0.0
    big = max(area_km2(p) for p in polys)
    polys = [p for p in polys if area_km2(p) >= big * 0.01]
    mid = math.degrees(math.atan2(sum(math.sin(math.radians(x)) for p in polys for x, _ in p[0]), sum(math.cos(math.radians(x)) for p in polys for x, _ in p[0])))
    boxes = []
    for p in polys:
        xs = [mid + (x - mid + 540) % 360 - 180 for x, _ in p[0]]
        ys = [y for _, y in p[0]]
        boxes.append((min(xs), max(xs), min(ys), max(ys)))
    group = list(range(len(polys)))

    def root(i: int) -> int:
        while group[i] != i:
            group[i] = group[group[i]]
            i = group[i]
        return i

    for i, (aw, ae, as_, an) in enumerate(boxes):
        for j in range(i + 1, len(boxes)):
            bw, be, bs, bn = boxes[j]
            cos = math.cos(math.radians((as_ + an + bs + bn) / 4))
            dx = max(aw - be, bw - ae, 0) * cos
            dy = max(as_ - bn, bs - an, 0)
            if math.hypot(dx, dy) < 2.5:
                group[root(i)] = root(j)
    size: dict[int, float] = {}
    for i, p in enumerate(polys):
        size[root(i)] = size.get(root(i), 0) + area_km2(p)
    main = max(size, key=size.get)
    return extent_km([p for i, p in enumerate(polys) if root(i) == main])


def main() -> None:
    countries_src, states_src = map(pathlib.Path, sys.argv[1:3])
    tmp = pathlib.Path(tempfile.mkdtemp())

    # Countries: the world and continent maps.
    simplify(countries_src, tmp / "countries.json", 4000)
    regions = []
    for f in json.loads((tmp / "countries.json").read_text())["features"]:
        p = f["properties"]
        rid = p["ADM0_A3"]
        alts = [p.get("NAME"), p.get("NAME_LONG"), p.get("NAME_ALT"), p.get("NAME_CIAWF"), p.get("NAME_SORT"), p.get("ISO_A3")]
        alts += [p.get(k) for k in ("ISO_A2", "ISO_A2_EH", "ISO_A3_EH") if p.get(k) not in (None, "-99")]
        langs = {lang: p.get(f"NAME_{lang.upper()}") for lang in LANGS}
        continent = CONTINENTS.get(p.get("CONTINENT"), "")
        # A pixel of a continent map is about 8 km.
        shape = drop_islets(polygons(f["geometry"]), 150)
        regions.append(encode_region(rid, continent, clean(p.get("NAME_EN") or p["NAME"]), alts, langs, shape))
    write(ROOT / "src-tauri/maps/countries.bin", regions)

    # States and provinces: country maps, simplified per size of country.
    raw = json.loads(states_src.read_text())["features"]
    by_country: dict[str, list] = {}
    for f in raw:
        by_country.setdefault(f["properties"]["adm0_a3"], []).append(polygons(f["geometry"]))
    buckets: dict[int, list] = {}
    sizes = {}
    for country, shapes in by_country.items():
        sizes[country] = extent_km([p for s in shapes for p in s])
        km = main_extent_km([p for s in shapes for p in s])
        # About 1/1500 of the country's size, in a few steps (meters).
        interval = next((m for m in (60, 200, 600, 2000, 6000) if km / 1500 * 1000 <= m * 1.8), 6000)
        buckets.setdefault(interval, []).append(country)
    records = []
    for interval, countries in sorted(buckets.items()):
        out = tmp / f"states-{interval}.json"
        codes = ",".join(json.dumps(c) for c in countries)
        simplify(states_src, out, interval, f"[{codes}].indexOf(adm0_a3) >= 0")
        for f in json.loads(out.read_text())["features"]:
            p = f["properties"]
            rid = p.get("iso_3166_2") or p.get("adm1_code")
            if not rid or rid.endswith("~") or rid == "-99":
                rid = p.get("adm1_code")
            name = clean(p.get("name_en") or p.get("name") or p.get("gn_name"))
            # Some English names are a word of the local one ("Federal" for
            # "Distrito Federal"): the local name is clearer.
            local = clean(p.get("name"))
            if local and name != local and name in local:
                name = local
            alts = [p.get("name"), p.get("name_local"), p.get("gn_name"), p.get("woe_name"), p.get("postal"), p.get("iso_3166_2")]
            alts += (p.get("name_alt") or "").split("|")
            langs = {lang: p.get(f"name_{lang}") for lang in LANGS}
            # Islands smaller than about 1/150 of the country squared, or
            # 1/40 of the region itself (shown alone in an inset).
            shape = polygons(f["geometry"])
            shape = drop_islets(shape, min((sizes[p["adm0_a3"]] / 150) ** 2, (extent_km(shape) / 40) ** 2))
            records.append({"country": p["adm0_a3"], "id": rid, "adm1": p.get("adm1_code"), "name": name, "alts": alts, "langs": langs, "shape": shape})
        print(f"  {interval} m: {len(countries)} countries")
    write(ROOT / "src-tauri/maps/states.bin", [encode_region(r["id"], r["country"], r["name"], r["alts"], r["langs"], r["shape"]) for r in unique_codes(records)])


def unique_codes(records: list) -> list:
    """Natural Earth gives some codes to more than one feature. Pieces of one
    region (same code and name: Cebu, Mbarara) become one region. Otherwise
    the largest keeps the code and the others take their own Natural Earth
    code (a city that shares its province's code), so that every region can
    be named by its code."""
    groups: dict[tuple, list] = {}
    for r in records:
        groups.setdefault((r["country"], r["id"]), []).append(r)
    out, merged = [], set()
    for r in records:
        key = (r["country"], r["id"])
        group = groups[key]
        if len(group) > 1 and len({g["name"] for g in group}) == 1:
            if key in merged:
                continue
            merged.add(key)
            r = {**r, "shape": [poly for g in group for poly in g["shape"]]}
        elif len(group) > 1:
            keeper = max(group, key=lambda g: sum(area_km2(poly) for poly in g["shape"]))
            if r is not keeper:
                r = {**r, "id": r["adm1"], "alts": [a for a in r["alts"] if clean(a) != r["id"]]}
        out.append(r)
    return out


if __name__ == "__main__":
    main()
