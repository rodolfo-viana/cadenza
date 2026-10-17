// Syntax-checks every UI module and verifies that imported names are exported.
//
//   node tools/check_js.cjs            # checks ui/js
//   node tools/check_js.cjs <dir>
const fs = require('fs'), path = require('path'), cp = require('child_process'), os = require('os');
const dir = process.argv[2] || path.join(__dirname, '..', 'ui', 'js');
const files = fs.readdirSync(dir).filter((f) => f.endsWith('.js'));
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'czjs-'));
let bad = 0;
for (const f of files) {
  const out = path.join(tmp, f.replace(/\.js$/, '.mjs'));
  fs.copyFileSync(path.join(dir, f), out);
  try { cp.execFileSync(process.execPath, ['--check', out], { stdio: 'pipe' }); } catch (e) {
    // No exit status: node never ran (or was killed), which says nothing about the file.
    if (e.status == null) { console.error(`cannot run node --check on ${f}: ${e.code || e.signal || e.message}`); fs.rmSync(tmp, { recursive: true, force: true }); process.exit(2); }
    console.log(`SYNTAX ${f}: ${String(e.stderr).split('\n').slice(0, 5).join('\n')}`); bad++;
  }
}
const exportsOf = {};
for (const f of files) {
  const src = fs.readFileSync(path.join(dir, f), 'utf8'); const names = new Set();
  for (const m of src.matchAll(/export\s+(?:async\s+)?(?:function\*?|const|let|class)\s+([A-Za-z_$][\w$]*)/g)) names.add(m[1]);
  for (const m of src.matchAll(/export\s*\{([^}]*)\}/g)) for (const part of m[1].split(',')) { const p = part.trim(); if (p) { const as = p.split(/\s+as\s+/); names.add((as[1] || as[0]).trim()); } }
  exportsOf[f] = names;
}
exportsOf['__vendor'] = new Set(['h', 'html', 'render', 'Component', 'createContext', 'useState', 'useReducer', 'useEffect', 'useLayoutEffect', 'useRef', 'useImperativeHandle', 'useMemo', 'useCallback', 'useContext']);
for (const f of files) {
  const src = fs.readFileSync(path.join(dir, f), 'utf8');
  for (const m of src.matchAll(/import\s*\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]/g)) {
    const target = m[2].startsWith('../vendor') ? '__vendor' : path.basename(m[2]);
    const ex = exportsOf[target];
    if (!ex) { console.log(`${f}: unknown module ${m[2]}`); bad++; continue; }
    for (const part of m[1].split(',')) { const n = part.trim().split(/\s+as\s+/)[0].trim(); if (n && !ex.has(n)) { console.log(`${f}: '${n}' not exported by ${m[2]}`); bad++; } }
  }
  for (const m of src.matchAll(/import\('\.\/([\w-]+\.js)'\)\.then\((?:async )?\(m\) => (?:\{[^}]*?)?m\.(\w+)/g)) if (!exportsOf[m[1]]?.has(m[2])) { console.log(`${f}: dynamic ${m[1]}.${m[2]} missing`); bad++; }
  for (const [ns, mod] of [['S', 'store.js'], ['A', 'actions.js']]) {
    if (!new RegExp(`import \\* as ${ns} from`).test(src)) continue;
    for (const m of src.matchAll(new RegExp(`\\b${ns}\\.([A-Za-z_$][\\w$]*)`, 'g'))) if (!exportsOf[mod].has(m[1])) { console.log(`${f}: ${ns}.${m[1]} missing in ${mod}`); bad++; }
  }
}
fs.rmSync(tmp, { recursive: true, force: true });
console.log(bad ? `${bad} problems` : `JS OK (${files.length} modules)`);
process.exit(bad ? 1 : 0);
