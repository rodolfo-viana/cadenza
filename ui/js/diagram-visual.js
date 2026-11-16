// A deliberately bounded, lossless editing model for simple Mermaid flowcharts.
// Mermaid source remains the saved representation. A source we cannot fully
// understand stays in the Mermaid editor rather than losing unsupported syntax.

const DIRECTIONS = new Set(['LR', 'RL', 'TB', 'BT']);
const SHAPES = { rectangle: ['[', ']'], rounded: ['(', ')'], decision: ['{', '}'], circle: ['((', '))'] };
const ID = /^[A-Za-z_][A-Za-z0-9_]*$/;
const MAX_NODES = 100;
const MAX_EDGES = 200;

function quoteLabel(label) {
  // Encode the delimiters as Mermaid entities, including # itself. Decode only
  // once, so a literal "#34;" remains literal through repeated visual edits.
  return '"' + String(label).replace(/["#&<>`\\\r\n]/g, (c) => `#${c.charCodeAt(0)};`) + '"';
}

function labelText(value) {
  const quoted = value.startsWith('"') && value.endsWith('"');
  if (quoted) value = value.slice(1, -1);
  // HTML, Markdown strings, escaped quotes and unknown entities need Mermaid's
  // own parser; accepting them here would silently change their meaning.
  if (/["<>`\\\r\n]/.test(value) || /&(?!amp;|quot;|lt;|gt;)/.test(value)) return null;
  if (!quoted && /[\[\]{}()|;]/.test(value)) return null;
  if (/#(?!\d+;)/.test(value)) return null;
  let valid = true;
  const result = value.replace(/#(\d+);|&(amp|quot|lt|gt);/g, (_, code, name) => {
    if (name) return { amp: '&', quot: '"', lt: '<', gt: '>' }[name];
    const n = Number(code);
    if (n <= 0 || n > 0x10ffff || (n >= 0xd800 && n <= 0xdfff)) { valid = false; return ''; }
    return String.fromCodePoint(n);
  });
  return valid ? result : null;
}

function readNode(input) {
  const id = /^[A-Za-z_][A-Za-z0-9_]*/.exec(input)?.[0];
  if (!id) return null;
  let rest = input.slice(id.length).trimStart();
  const shape = ['circle', 'rectangle', 'rounded', 'decision'].find((name) => rest.startsWith(SHAPES[name][0]));
  if (!shape) return { node: { id, label: id, shape: 'rectangle' }, defined: false, rest };
  const [open, close] = SHAPES[shape];
  let quoted = false;
  let end = -1;
  for (let i = open.length; i < rest.length; i++) {
    if (rest[i] === '"') quoted = !quoted;
    if (!quoted && rest.startsWith(close, i)) { end = i; break; }
  }
  if (end < 0) return null;
  const label = labelText(rest.slice(open.length, end));
  if (label === null) return null;
  rest = rest.slice(end + close.length).trimStart();
  return { node: { id, label, shape }, defined: true, rest };
}

/** Returns the entire editable graph, or null if any syntax would be lost. */
export function parseVisualDiagram(source) {
  if (typeof source !== 'string' || source.length > 100000) return null;
  const lines = source.trim().split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
  const header = /^(?:flowchart|graph)\s+(LR|RL|TB|TD|BT)\s*;?$/.exec(lines.shift() || '');
  if (!header) return null;
  const graph = { direction: header[1] === 'TD' ? 'TB' : header[1], nodes: [], edges: [] };
  const definitions = new Set();
  function add(parsed) {
    const previous = graph.nodes.find((n) => n.id === parsed.node.id);
    if (!previous) graph.nodes.push(parsed.node);
    else if (parsed.defined) {
      if (definitions.has(parsed.node.id) && JSON.stringify(previous) !== JSON.stringify(parsed.node)) return false;
      Object.assign(previous, parsed.node);
    }
    if (parsed.defined) definitions.add(parsed.node.id);
    return true;
  }
  for (let line of lines) {
    // A final statement separator is harmless; multiple statements are not
    // accepted, so directives and chained edges cannot be swallowed.
    if (line.endsWith(';')) line = line.slice(0, -1).trimEnd();
    const first = readNode(line);
    if (!first || !add(first)) return null;
    if (!first.rest) continue;
    let edgeLabel = '';
    let rest = first.rest;
    if (rest.startsWith('-->')) {
      rest = rest.slice(3).trimStart();
      if (rest.startsWith('|')) {
        let quoted = false;
        let end = -1;
        for (let i = 1; i < rest.length; i++) {
          if (rest[i] === '"') quoted = !quoted;
          if (!quoted && rest[i] === '|') { end = i; break; }
        }
        if (end < 0) return null;
        edgeLabel = labelText(rest.slice(1, end).trim());
        rest = rest.slice(end + 1).trimStart();
      }
    } else {
      const arrow = /^--\s+(.+?)\s+-->\s*/.exec(rest);
      if (!arrow) return null;
      edgeLabel = labelText(arrow[1]);
      rest = rest.slice(arrow[0].length);
    }
    if (edgeLabel === null) return null;
    const last = readNode(rest);
    if (!last || last.rest || !add(last)) return null;
    graph.edges.push({ from: first.node.id, to: last.node.id, label: edgeLabel });
  }
  return graph.nodes.length && graph.nodes.length <= MAX_NODES && graph.edges.length <= MAX_EDGES ? graph : null;
}

/** Serialize only the supported graph, escaping all user-entered labels. */
export function visualDiagramSource(graph) {
  if (!DIRECTIONS.has(graph?.direction) || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges) ||
      !graph.nodes.length || graph.nodes.length > MAX_NODES || graph.edges.length > MAX_EDGES) throw new Error('Invalid visual diagram');
  const ids = new Set();
  const lines = [`flowchart ${graph.direction}`];
  for (const node of graph.nodes) {
    if (!ID.test(node.id) || ids.has(node.id) || !SHAPES[node.shape] || typeof node.label !== 'string') throw new Error('Invalid visual diagram block');
    ids.add(node.id);
    const [open, close] = SHAPES[node.shape];
    lines.push(`  ${node.id}${open}${quoteLabel(node.label)}${close}`);
  }
  for (const edge of graph.edges) {
    if (!ids.has(edge.from) || !ids.has(edge.to) || typeof edge.label !== 'string') throw new Error('Invalid visual diagram connection');
    lines.push(`  ${edge.from} -->${edge.label ? `|${quoteLabel(edge.label)}|` : ''} ${edge.to}`);
  }
  return lines.join('\n');
}

/** Fresh models; callers can edit a template without changing future inserts. */
export function visualTemplates() {
  const node = (id, label, shape = 'rounded') => ({ id, label, shape });
  const edge = (from, to, label = '') => ({ from, to, label });
  return [
    { id: 'process', graph: { direction: 'LR', nodes: [node('A', 'Plan'), node('B', 'Build'), node('C', 'Review')], edges: [edge('A', 'B'), edge('B', 'C')] } },
    { id: 'cycle', graph: { direction: 'LR', nodes: [node('A', 'Observe'), node('B', 'Learn'), node('C', 'Improve')], edges: [edge('A', 'B'), edge('B', 'C'), edge('C', 'A')] } },
    { id: 'hierarchy', graph: { direction: 'TB', nodes: [node('A', 'Team'), node('B', 'Design'), node('C', 'Engineering')], edges: [edge('A', 'B'), edge('A', 'C')] } },
    { id: 'decision', graph: { direction: 'TB', nodes: [node('A', 'Ready?', 'decision'), node('B', 'Continue'), node('C', 'Revise')], edges: [edge('A', 'B', 'Yes'), edge('A', 'C', 'No')] } },
  ];
}

export function addVisualNode(graph, label) {
  if (graph.nodes.length >= MAX_NODES) return graph;
  const used = new Set(graph.nodes.map((n) => n.id));
  let n = 1;
  while (used.has(`N${n}`)) n++;
  const node = { id: `N${n}`, label, shape: 'rounded' };
  const from = graph.nodes.at(-1)?.id;
  return { ...graph, nodes: [...graph.nodes, node], edges: from && graph.edges.length < MAX_EDGES ? [...graph.edges, { from, to: node.id, label: '' }] : graph.edges };
}

export function removeVisualNode(graph, id) {
  if (graph.nodes.length <= 1) return graph;
  return { ...graph, nodes: graph.nodes.filter((n) => n.id !== id), edges: graph.edges.filter((e) => e.from !== id && e.to !== id) };
}

/** Small deterministic graph layout for the interactive visual editor. */
export function layoutVisualDiagram(graph) {
  const level = new Map();
  const incoming = new Set(graph.edges.map((e) => e.to));
  const roots = graph.nodes.filter((n) => !incoming.has(n.id));
  const visit = (id, depth) => {
    if (level.has(id)) return;
    level.set(id, depth);
    for (const edge of graph.edges.filter((e) => e.from === id)) visit(edge.to, depth + 1);
  };
  for (const node of [...roots, ...graph.nodes]) visit(node.id, 0);
  const layers = [];
  for (const node of graph.nodes) (layers[level.get(node.id)] ||= []).push(node);
  const horizontal = graph.direction === 'LR' || graph.direction === 'RL';
  const width = horizontal ? layers.length * 156 + 32 : Math.max(...layers.map((l) => l.length)) * 156 + 32;
  const height = horizontal ? Math.max(...layers.map((l) => l.length)) * 96 + 32 : layers.length * 96 + 32;
  const nodes = graph.nodes.map((node) => {
    const depth = level.get(node.id);
    const layer = layers[depth];
    const lane = layer.indexOf(node);
    let x = horizontal ? 32 + depth * 156 : (width - layer.length * 156) / 2 + 18 + lane * 156;
    let y = horizontal ? (height - layer.length * 96) / 2 + 20 + lane * 96 : 32 + depth * 96;
    if (graph.direction === 'RL') x = width - x - 120;
    if (graph.direction === 'BT') y = height - y - 56;
    return { ...node, x, y, w: 120, h: 56 };
  });
  return { nodes, edges: graph.edges, width, height };
}
