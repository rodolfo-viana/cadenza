# PptxGenJS 4.0.1

Pinned browser bundle from the official npm package, including JSZip 3.10.1.
No package scripts, CDN requests or browser download actions are used.

- Package: https://registry.npmjs.org/pptxgenjs/4.0.1
- Tarball: https://registry.npmjs.org/pptxgenjs/-/pptxgenjs-4.0.1.tgz
- Verified npm integrity: `sha512-TeJISr8wouAuXw4C1F/mC33xbZs/FuEG6nH9FG1Zj+nuPcGMP5YRHl6X+j3HSUnS1f3at6k75ZZXPMZlA5Lj9A==`
- Bundle SHA-256: `4fb9eac5cfefb213e2d8743c2b7151025f31bfb3f834c73c12062916daa0f3f8`

`LICENSE.txt` covers PptxGenJS (MIT). The additional license files cover
JSZip and its bundled dependencies. JSZip is used under its MIT option.
The installed package and official tarball bundle were compared byte for byte.

Primary API references: https://gitbrent.github.io/PptxGenJS/docs/ .
The conversion adapter lives in `ui/js/pptx-export.js`; special content is
captured through Cadenza's native renderer rather than another HTML renderer.

The unmodified 4.0.1 bundle has compatibility cases handled by the adapter:
category-chart workbooks use `value || ''`, so zero is supplied as the numeric
lexical form `"0"` (the generated cell remains numeric); rich-text paragraph
properties are deduplicated by `pptx-ooxml.js`; text hyperlinks are assigned
to runs before relationship generation. Regression tests check both chart
caches and embedded workbooks, paragraph structure, and every hyperlink
relationship. Re-evaluate these workarounds when upgrading the library.
