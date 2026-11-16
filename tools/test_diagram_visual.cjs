// Safe visual editing of Mermaid flowcharts; no browser or dependencies needed.
// Run: node --experimental-vm-modules tools/test_diagram_visual.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');

const loaded = (async () => {
  const file = path.join(__dirname, '..', 'ui', 'js', 'diagram-visual.js');
  const mod = new vm.SourceTextModule(fs.readFileSync(file, 'utf8'), { identifier: file });
  await mod.link(() => { throw new Error('The visual diagram model must stay dependency-free'); });
  await mod.evaluate();
  return mod.namespace;
})();
const plain = (v) => JSON.parse(JSON.stringify(v));

test('the existing Cadenza flowchart stays visually editable', async () => {
  const { parseVisualDiagram } = await loaded;
  const graph = parseVisualDiagram('flowchart LR\n  A[Idea] --> B{Worth it?}\n  B -- Yes --> C[Build it]\n  B -- No --> D[Rethink]\n  D --> A');
  assert.equal(graph.direction, 'LR');
  assert.deepEqual(plain(graph.nodes), [
    { id: 'A', label: 'Idea', shape: 'rectangle' },
    { id: 'B', label: 'Worth it?', shape: 'decision' },
    { id: 'C', label: 'Build it', shape: 'rectangle' },
    { id: 'D', label: 'Rethink', shape: 'rectangle' },
  ]);
  assert.equal(graph.edges[1].label, 'Yes');
  assert.equal(graph.edges[3].to, 'A');
});

test('all visual templates round-trip without a second stored representation', async () => {
  const { visualTemplates, parseVisualDiagram, visualDiagramSource } = await loaded;
  assert.equal(visualTemplates().length, 4);
  for (const template of visualTemplates()) {
    assert.deepEqual(plain(parseVisualDiagram(visualDiagramSource(template.graph))), plain(template.graph), template.id);
  }
});

test('literal labels cannot create Mermaid nodes, directives or HTML', async () => {
  const { parseVisualDiagram, visualDiagramSource, visualTemplates } = await loaded;
  const graph = visualTemplates()[0].graph;
  const label = 'A "quoted" [label] {x} & <script> #34; | ; ` text';
  graph.nodes[0].label = label;
  graph.edges[0].label = label;
  const source = visualDiagramSource(graph);
  assert.ok(!source.includes('<script>'));
  assert.ok(!source.includes('"quoted"'));
  assert.equal(parseVisualDiagram(source).nodes[0].label, label);
  assert.equal(parseVisualDiagram(source).edges[0].label, label);
});

test('unsupported Mermaid is rejected intact, never partially interpreted', async () => {
  const { parseVisualDiagram } = await loaded;
  for (const source of [
    'sequenceDiagram\nA->>B: Hello',
    'flowchart LR\nA --> B\nstyle A fill:red',
    'flowchart LR\nA --> B\n%% Keep this comment',
    'flowchart LR\nA --> B --> C',
    'flowchart LR\nsubgraph group\nA --> B\nend',
    'flowchart LR\nA["`Markdown **bold**`"] --> B',
    'flowchart LR\nA[<b>HTML</b>] --> B',
    'flowchart LR\nA[First]\nA[Second]',
    'flowchart LR\nA -->|label| B\nclick A "https://example.com"',
    '%%{init: {}}%%\nflowchart LR\nA --> B',
    'flowchart LR\nA -.-> B',
  ]) assert.equal(parseVisualDiagram(source), null, source);
});

test('bare nodes, quoted edge labels and every supported shape survive editing', async () => {
  const { parseVisualDiagram, visualDiagramSource } = await loaded;
  const graph = parseVisualDiagram('graph TD\nA -->|"next"| B("Rounded")\nB --> C(("Circle"))\nC --> D{"Decision"}');
  assert.equal(graph.nodes[0].label, 'A');
  assert.deepEqual(graph.nodes.map((n) => n.shape), ['rectangle', 'rounded', 'circle', 'decision']);
  assert.deepEqual(plain(parseVisualDiagram(visualDiagramSource(graph))), plain(graph));
});

test('removing a block also removes its incident connections', async () => {
  const { visualTemplates, removeVisualNode, addVisualNode } = await loaded;
  const graph = visualTemplates()[0].graph;
  const changed = removeVisualNode(graph, 'B');
  assert.equal(changed.nodes.length, 2);
  assert.equal(changed.edges.length, 0);
  assert.equal(graph.nodes.length, 3, 'input remains unchanged');
  const added = addVisualNode(changed, 'New block');
  assert.equal(added.nodes.length, 3);
  assert.equal(new Set(added.nodes.map((n) => n.id)).size, 3);
  assert.equal(added.edges.at(-1).from, 'C');
  assert.equal(added.edges.at(-1).to, added.nodes.at(-1).id);
});

test('invalid graph edits fail before generating Mermaid', async () => {
  const { visualTemplates, visualDiagramSource, removeVisualNode } = await loaded;
  const graph = visualTemplates()[0].graph;
  assert.throws(() => visualDiagramSource({ ...graph, direction: 'invalid' }));
  assert.throws(() => visualDiagramSource({ ...graph, nodes: [...graph.nodes, graph.nodes[0]] }));
  assert.throws(() => visualDiagramSource({ ...graph, edges: [{ from: 'A', to: 'Missing', label: '' }] }));
  assert.equal(removeVisualNode({ direction: 'LR', nodes: [graph.nodes[0]], edges: [] }, 'A').nodes.length, 1);
});

test('layout is deterministic, finite, and honors the chosen direction', async () => {
  const { visualTemplates, layoutVisualDiagram } = await loaded;
  const graph = visualTemplates()[0].graph;
  const lr = layoutVisualDiagram(graph);
  const rl = layoutVisualDiagram({ ...graph, direction: 'RL' });
  const tb = layoutVisualDiagram({ ...graph, direction: 'TB' });
  assert.ok(lr.nodes[0].x < lr.nodes[2].x);
  assert.ok(rl.nodes[0].x > rl.nodes[2].x);
  assert.ok(tb.nodes[0].y < tb.nodes[2].y);
  assert.deepEqual(plain(layoutVisualDiagram(graph)), plain(lr));
  for (const template of visualTemplates()) {
    const layout = layoutVisualDiagram(template.graph);
    assert.ok(Number.isFinite(layout.width) && Number.isFinite(layout.height));
    for (const node of layout.nodes) assert.ok(node.x >= 0 && node.y >= 0 && node.x + node.w <= layout.width && node.y + node.h <= layout.height);
  }
});
