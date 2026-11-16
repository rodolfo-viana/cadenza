# MathLive 0.111.4

Pinned browser ES module from the official npm package. No package scripts or
runtime CDN dependencies are used. The MIT license is in `LICENSE.txt`.

- Package: https://registry.npmjs.org/mathlive/0.111.4
- Tarball: https://registry.npmjs.org/mathlive/-/mathlive-0.111.4.tgz
- Verified npm integrity: `sha512-1E5EOQnUHWn+KfEXaDTmuyxRWZrZw4WlQFyZLzJmjKEPdaX4WAnFqVfZb36mhulykcIl7/W7gKkTmutP2/9wjQ==`
- Vendored files: `package/mathlive.min.mjs`, `package/LICENSE.txt`.

All 20 WOFF2 files in the package's `fonts/` directory were compared byte for
byte with the existing files in `../katex/fonts/`; every file is identical.
The editor sets `MathfieldElement.fontsDirectory` to that local directory and
sets `soundsDirectory` to `null`. The existing KaTeX license applies to those
shared fonts. Check all 20 files again before changing the pinned version.

Primary integration references:

- https://mathlive.io/mathfield/guides/integration/
- https://mathlive.io/mathfield/guides/interacting/
- https://mathlive.io/mathfield/guides/virtual-keyboard/

Only `content.latex` is saved. MathLive supplies the editor; the existing KaTeX
renderer continues to render slides and exports. Opening the visual editor
uses silent `setValue()`. User input is exported with
`getValue('latex-without-placeholders')`, so empty editable slots remain valid
LaTeX rather than introducing MathLive-only placeholder commands.
