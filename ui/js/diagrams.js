// Mermaid diagrams.
//
// The editor renders diagram source to SVG with Mermaid (light and dark
// variants) and stores the result in the document. Presentations and exports
// then show the stored SVG, so they never need the (large) Mermaid library.

import { state, apply } from './store.js';
import { t, N_ } from './i18n.js';

/** Light/dark Mermaid themes for each Cadenza diagram style. */
export const DIAGRAM_STYLES = {
  classic: ['default', 'dark'],
  neutral: ['neutral', 'dark'],
  forest: ['forest', 'dark'],
  neo: ['neo', 'neo-dark'],
  redux: ['redux', 'redux-dark'],
  'redux-color': ['redux-color', 'redux-dark-color'],
};

// Template names are shown in a menu (translate with t() there); the Mermaid source stays as is.
export const DIAGRAM_TEMPLATES = [
  [N_('Flowchart'), 'flowchart LR\n  A[Idea] --> B{Worth it?}\n  B -- Yes --> C[Build it]\n  B -- No --> D[Rethink]\n  D --> A'],
  [N_('Sequence'), 'sequenceDiagram\n  participant U as User\n  participant A as App\n  participant S as Server\n  U->>A: Click "Save"\n  A->>S: POST /deck\n  S-->>A: 200 OK\n  A-->>U: Saved ✓'],
  [N_('Class'), 'classDiagram\n  class Deck {\n    +String title\n    +Slide[] slides\n    +save()\n  }\n  class Slide {\n    +Element[] elements\n  }\n  Deck "1" --> "*" Slide'],
  [N_('State'), 'stateDiagram-v2\n  [*] --> Draft\n  Draft --> Review: submit\n  Review --> Draft: changes requested\n  Review --> Published: approve\n  Published --> [*]'],
  [N_('Entity Relationship'), 'erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER ||--|{ LINE_ITEM : contains\n  PRODUCT ||--o{ LINE_ITEM : "ordered in"'],
  [N_('Gantt'), 'gantt\n  title Project plan\n  dateFormat YYYY-MM-DD\n  section Design\n  Research      :a1, 2026-01-05, 10d\n  Prototype     :after a1, 12d\n  section Build\n  Development   :2026-02-01, 30d\n  Launch        :milestone, 2026-03-05, 0d'],
  [N_('Pie'), 'pie title Time spent\n  "Writing" : 45\n  "Designing" : 30\n  "Rehearsing" : 25'],
  [N_('Mindmap'), 'mindmap\n  root((Cadenza))\n    Editing\n      Text\n      Shapes\n      Charts\n    Presenting\n      Presenter view\n      Magic Move\n    Sharing\n      HTML\n      PDF'],
  [N_('Timeline'), 'timeline\n  title Product history\n  2024 : First prototype\n  2025 : Public beta : 10k users\n  2026 : Version 1.0'],
  [N_('Git graph'), 'gitGraph\n  commit\n  branch feature\n  checkout feature\n  commit\n  commit\n  checkout main\n  merge feature\n  commit'],
  [N_('User journey'), 'journey\n  title Giving a talk\n  section Prepare\n    Write slides: 3: Me\n    Rehearse: 4: Me\n  section Present\n    Deliver: 5: Me, Audience\n    Q&A: 4: Audience'],
  [N_('Quadrant'), 'quadrantChart\n  title Priorities\n  x-axis Low effort --> High effort\n  y-axis Low impact --> High impact\n  quadrant-1 Plan\n  quadrant-2 Do now\n  quadrant-3 Skip\n  quadrant-4 Delegate\n  Feature A: [0.3, 0.8]\n  Feature B: [0.7, 0.6]\n  Feature C: [0.2, 0.2]'],
];

const FONT = "Inter, 'Segoe UI', Cantarell, Ubuntu, Roboto, 'Helvetica Neue', Arial, sans-serif";

let loading = null;
function loadMermaid() {
  if (!loading) {
    loading = new Promise((resolve, reject) => {
      if (window.mermaid) return resolve(window.mermaid);
      const s = document.createElement('script');
      s.src = 'vendor/mermaid/mermaid.min.js';
      s.onload = () => resolve(window.mermaid);
      s.onerror = () => {
        loading = null;
        reject(new Error(t('Could not load the diagram engine')));
      };
      document.head.appendChild(s);
    });
  }
  return loading;
}

/** Gives the SVG explicit dimensions so it scales correctly inside <img>. */
function normalizeSvg(svg) {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');
  const el = doc.documentElement;
  if (!el || el.nodeName !== 'svg') return svg;
  const vb = (el.getAttribute('viewBox') || '').split(/[\s,]+/).map(Number);
  if (vb.length === 4 && vb[2] > 0 && vb[3] > 0) {
    el.setAttribute('width', String(Math.ceil(vb[2])));
    el.setAttribute('height', String(Math.ceil(vb[3])));
  }
  el.style.removeProperty('max-width');
  if (!el.getAttribute('xmlns')) el.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  // Mermaid's SVG text renderer leaves escaped labels as visible HTML
  // entities (e.g. &#34;). Decode text once, never SVG markup: a literal
  // <script> label must remain text, and an escaped #34; must remain #34;.
  for (const text of el.querySelectorAll('text')) {
    const walker = doc.createTreeWalker(text, 4); // NodeFilter.SHOW_TEXT
    const nodes = [];
    let source = '';
    while (walker.nextNode()) {
      const node = walker.currentNode, start = source.length;
      source += node.nodeValue;
      nodes.push({ node, start, end: source.length });
    }
    // A wrapped entity can span multiple tspans. Walk matches backwards to
    // retain both the original offsets and Mermaid's line/style geometry.
    let index = nodes.length - 1;
    const matches = [...source.matchAll(/&#(\d+);|&(quot|apos|amp|lt|gt);/g)];
    for (const match of matches.reverse()) {
      const [, code, name] = match, n = Number(code);
      if (!name && !(n > 0 && n <= 0x10ffff && !(n >= 0xd800 && n <= 0xdfff))) continue;
      const decoded = name ? { quot: '"', apos: "'", amp: '&', lt: '<', gt: '>' }[name] : String.fromCodePoint(n);
      const start = match.index, end = start + match[0].length;
      while (index >= 0 && nodes[index].start >= end) index--;
      let i = index;
      for (; i >= 0 && nodes[i].end > start; i--) {
        const part = nodes[i], value = part.node.nodeValue;
        part.node.nodeValue = value.slice(0, Math.max(0, start - part.start)) +
          (start >= part.start ? decoded : '') + value.slice(Math.min(part.end, end) - part.start);
      }
      index = i + 1;
    }
  }
  return new XMLSerializer().serializeToString(el);
}

let counter = 0;
let queue = Promise.resolve();

/**
 * Renders Mermaid source into { svgLight, svgDark }. Throws with Mermaid's
 * message when the source is invalid. Renders are serialized because
 * Mermaid's configuration is global.
 */
export function renderDiagram(source, style = 'classic', look = 'classic') {
  const run = async () => {
    const mermaid = await loadMermaid();
    const [light, dark] = DIAGRAM_STYLES[style] || DIAGRAM_STYLES.classic;
    const out = {};
    for (const [key, theme] of [
      ['svgLight', light],
      ['svgDark', dark],
    ]) {
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: 'strict',
        theme,
        look,
        handDrawnSeed: 7,
        fontFamily: FONT,
        htmlLabels: false,
        flowchart: { htmlLabels: false },
      });
      await mermaid.parse(source);
      const { svg } = await mermaid.render(`cz-mmd-${++counter}`, source);
      out[key] = normalizeSvg(svg);
    }
    return out;
  };
  const p = queue.then(run, run);
  queue = p.catch(() => {});
  return p;
}

/** Aspect ratio (w/h) of a rendered diagram. */
export function svgAspect(svg) {
  const m = /viewBox="[\d.\-]+[\s,]+[\d.\-]+[\s,]+([\d.]+)[\s,]+([\d.]+)"/.exec(svg || '');
  return m ? Number(m[1]) / Number(m[2]) : null;
}

/** The inputs a cached SVG represents, independent of its light/dark display mode. */
export function diagramInputs(content) {
  return { source: content?.source || '', style: content?.style || 'classic', look: content?.look || 'classic' };
}

export function sameDiagramInputs(a, b) {
  return !!a && !!b && a.source === b.source && a.style === b.style && a.look === b.look;
}

export function diagramPreviewCurrent(content) {
  return !!content?.svgLight && !!content?.svgDark && sameDiagramInputs(content.renderedFor, diagramInputs(content));
}

const pending = new Set();
const failed = new Map(); // document/element → inputs that failed to render

/**
 * Renders diagrams that have no stored SVG yet (new diagrams, imported
 * Markdown, files from other tools). These updates are cached data, not
 * edits, so they do not create undo steps. Passes run one after another,
 * so the promise of one also covers the renders that were under way when it
 * was asked for (shows and exports wait for it).
 */
let passes = Promise.resolve();
export function ensureDiagrams() {
  passes = passes.then(renderMissing, renderMissing);
  return passes;
}

async function renderMissing() {
  const deck = state.doc?.deck;
  if (!deck) return;
  const docId = state.doc.docId;
  for (const slide of deck.slides) {
    for (const el of slide.elements) {
      const c = el.content;
      const key = JSON.stringify([docId, el.id]);
      const inputs = diagramInputs(c);
      if (c.kind !== 'diagram' || !c.source.trim() || diagramPreviewCurrent(c) || pending.has(key)) continue;
      if (sameDiagramInputs(failed.get(key), inputs)) continue;
      pending.add(key);
      try {
        const svgs = await renderDiagram(c.source, c.style, c.look);
        // Stored only if the diagram still has what was drawn: it may have
        // been edited, deleted or replaced by another presentation meanwhile.
        await apply(
          () => {
            const now = state.doc?.deck.slides.find((s) => s.id === slide.id)?.elements.find((e) => e.id === el.id)?.content;
            const same = now?.kind === 'diagram' && sameDiagramInputs(diagramInputs(now), inputs);
            return same ? { op: 'updateElements', slide: slide.id, updates: [{ id: el.id, patch: { content: { ...svgs, renderedFor: inputs } } }] } : null;
          },
          { docId, track: false, navigate: false },
        );
      } catch (e) {
        failed.set(key, inputs);
        console.warn('diagram render failed', e);
      } finally {
        pending.delete(key);
      }
    }
  }
}
