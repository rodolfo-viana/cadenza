// Visual controls edit the same Mermaid source as the source tab.
import { html, useEffect, useState } from './lib.js';
import { Row, Select, TextField, Hint } from './controls.js';
import { Button } from './ui.js';
import { t, N_ } from './i18n.js';
import { addVisualNode, removeVisualNode, layoutVisualDiagram, visualTemplates } from './diagram-visual.js';

const TEMPLATE_NAMES = { process: N_('Process'), cycle: N_('Cycle'), hierarchy: N_('Hierarchy'), decision: N_('Decision') };
const SHAPES = [
  { value: 'rectangle', label: N_('Rectangle') }, { value: 'rounded', label: N_('Rounded rectangle') },
  { value: 'decision', label: N_('Decision') }, { value: 'circle', label: N_('Circle') },
];
let markers = 0;

function connectionPath(from, to, self) {
  const ax = from.x + from.w / 2, ay = from.y + from.h / 2;
  const bx = to.x + to.w / 2, by = to.y + to.h / 2;
  if (self) return `M ${ax} ${from.y} C ${ax + 70} ${from.y - 28}, ${ax - 70} ${from.y - 28}, ${ax - 12} ${from.y}`;
  if (Math.abs(bx - ax) >= Math.abs(by - ay)) {
    const sign = bx > ax ? 1 : -1;
    const sx = ax + sign * (from.shape === 'circle' ? 28 : from.w / 2), tx = bx - sign * (to.shape === 'circle' ? 28 : to.w / 2);
    // A return edge uses the space above the blocks rather than cutting through them.
    if (sign < 0 && Math.abs(by - ay) < 20) return `M ${ax} ${from.y} C ${ax} 4, ${bx} 4, ${bx} ${to.y}`;
    const mid = (sx + tx) / 2;
    return `M ${sx} ${ay} C ${mid} ${ay}, ${mid} ${by}, ${tx} ${by}`;
  }
  const sign = by > ay ? 1 : -1;
  const sy = ay + sign * from.h / 2, ty = by - sign * to.h / 2;
  const mid = (sy + ty) / 2;
  return `M ${ax} ${sy} C ${ax} ${mid}, ${bx} ${mid}, ${bx} ${ty}`;
}

/** The selection preview uses graph geometry, not static template artwork. */
export function VisualDiagramPreview({ graph, selected, onSelect, small = false }) {
  const [marker] = useState(() => `cz-diagram-arrow-${++markers}`);
  const layout = layoutVisualDiagram(graph);
  const byId = new Map(layout.nodes.map((n) => [n.id, n]));
  return html`<svg class=${small ? 'diagram-thumbnail' : 'diagram-visual-canvas'}
    viewBox=${`0 0 ${layout.width} ${layout.height}`} role=${onSelect ? 'group' : 'img'}
    aria-label=${t('Diagram preview')} aria-hidden=${small ? 'true' : undefined}>
    <defs><marker id=${marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
      <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
    </marker></defs>
    ${layout.edges.map((edge) => {
      const from = byId.get(edge.from), to = byId.get(edge.to);
      return html`<g class="diagram-visual-edge">
        <path d=${connectionPath(from, to, edge.from === edge.to)} fill="none" stroke="currentColor" stroke-width="2" marker-end=${`url(#${marker})`} />
        ${edge.label && html`<text x=${(from.x + to.x + from.w) / 2} y=${(from.y + to.y + from.h) / 2 - 9} text-anchor="middle">${edge.label.slice(0, 14)}</text>`}
      </g>`;
    })}
    ${layout.nodes.map((node) => html`<g class=${`diagram-visual-node${selected === node.id ? ' selected' : ''}`}
      transform=${`translate(${node.x},${node.y})`} role=${onSelect ? 'button' : undefined}
      tabindex=${onSelect ? 0 : undefined} aria-label=${node.label || t('Untitled block')}
      aria-pressed=${onSelect ? selected === node.id : undefined}
      onClick=${() => onSelect?.(node.id)} onKeyDown=${(e) => {
        if (onSelect && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onSelect(node.id); }
      }}>
      <title>${node.label}</title>
      ${node.shape === 'decision'
        ? html`<polygon points="60,0 120,28 60,56 0,28" />`
        : node.shape === 'circle'
          ? html`<circle cx="60" cy="28" r="28" />`
          : html`<rect width="120" height="56" rx=${node.shape === 'rounded' ? 12 : 2} />`}
      <text x="60" y="32" text-anchor="middle">${node.label.length > 17 ? node.label.slice(0, 16) + '…' : node.label}</text>
    </g>`)}
  </svg>`;
}

export function DiagramGallery({ onChoose }) {
  return html`<div class="diagram-gallery" aria-label=${t('Visual diagram templates')}>
    ${visualTemplates().map((template) => html`<button type="button" class="diagram-template" onClick=${() => onChoose(template.graph)}>
      <${VisualDiagramPreview} graph=${template.graph} small />
      <span>${t(TEMPLATE_NAMES[template.id])}</span>
    </button>`)}
  </div>`;
}

export function VisualDiagramEditor({ graph, onChange }) {
  const [selected, setSelected] = useState(graph.nodes[0].id);
  const [edgeIndex, setEdgeIndex] = useState(0);
  const node = graph.nodes.find((n) => n.id === selected) || graph.nodes[0];
  const edge = graph.edges[edgeIndex] || graph.edges[0];
  const actualEdge = edge ? graph.edges.indexOf(edge) : -1;
  useEffect(() => {
    if (!graph.nodes.some((n) => n.id === selected)) setSelected(graph.nodes[0].id);
    if (edgeIndex >= graph.edges.length) setEdgeIndex(0);
  }, [graph, selected, edgeIndex]);
  const nodeOptions = graph.nodes.map((n) => ({ value: n.id, label: n.label || t('Untitled block') }));
  const editNode = (patch, live = false) => onChange({ ...graph, nodes: graph.nodes.map((n) => n.id === node.id ? { ...n, ...patch } : n) }, live);
  const editEdge = (patch, live = false) => onChange({ ...graph, edges: graph.edges.map((e, i) => i === actualEdge ? { ...e, ...patch } : e) }, live);
  return html`<div class="visual-diagram-editor">
    <${VisualDiagramPreview} graph=${graph} selected=${node.id} onSelect=${setSelected} />
    <${Hint}>${t('Select a block to edit it. Mermaid arranges the diagram automatically.')}<//>
    <${Row} label=${t('Direction')}><${Select} value=${graph.direction} options=${[
      { value: 'LR', label: t('Left to right') }, { value: 'RL', label: t('Right to left') },
      { value: 'TB', label: t('Top to bottom') }, { value: 'BT', label: t('Bottom to top') },
    ]} onChange=${(direction) => onChange({ ...graph, direction })} /><//>
    <${Row} label=${t('Block')}><${Select} value=${node.id} options=${nodeOptions} onChange=${setSelected} /><//>
    <${Row} label=${t('Label')}><${TextField} key=${node.id} value=${node.label} onChange=${(label, live) => editNode({ label }, live)} /><//>
    <${Row} label=${t('Shape')}><${Select} value=${node.shape} options=${SHAPES.map((shape) => ({ ...shape, label: t(shape.label) }))}
      onChange=${(shape) => editNode({ shape })} /><//>
    <div class="row-inline">
      <${Button} size="sm" icon="plus" label=${t('Add block')} disabled=${graph.nodes.length >= 100} onClick=${() => {
        const next = addVisualNode(graph, t('New block')); setSelected(next.nodes.at(-1).id); onChange(next);
      }} />
      <${Button} size="sm" icon="trash-2" label=${t('Remove block')} disabled=${graph.nodes.length < 2}
        onClick=${() => onChange(removeVisualNode(graph, node.id))} />
    </div>
    <h4 class="diagram-subheading">${t('Connections')}</h4>
    ${edge && html`<div class="diagram-connection-fields">
      <${Row} label=${t('Connection')}><${Select} value=${String(actualEdge)} options=${graph.edges.map((e, i) => ({
        value: String(i), label: `${nodeOptions.find((n) => n.value === e.from).label} → ${nodeOptions.find((n) => n.value === e.to).label}`,
      }))} onChange=${(index) => setEdgeIndex(Number(index))} /><//>
      <${Row} label=${t('From')}><${Select} value=${edge.from} options=${nodeOptions} onChange=${(from) => editEdge({ from })} /><//>
      <${Row} label=${t('To')}><${Select} value=${edge.to} options=${nodeOptions} onChange=${(to) => editEdge({ to })} /><//>
      <${Row} label=${t('Label')}><${TextField} key=${`edge-${actualEdge}-${edge.from}-${edge.to}`} value=${edge.label}
        placeholder=${t('Optional label')} onChange=${(label, live) => editEdge({ label }, live)} /><//>
    </div>`}
    <div class="row-inline">
      <${Button} size="sm" icon="plus" label=${t('Add connection')} disabled=${graph.edges.length >= 200} onClick=${() => {
        const to = graph.nodes.find((n) => n.id !== node.id)?.id || node.id;
        setEdgeIndex(graph.edges.length);
        onChange({ ...graph, edges: [...graph.edges, { from: node.id, to, label: '' }] });
      }} />
      <${Button} size="sm" icon="trash-2" label=${t('Remove connection')} disabled=${!edge}
        onClick=${() => onChange({ ...graph, edges: graph.edges.filter((_, i) => i !== actualEdge) })} />
    </div>
  </div>`;
}
