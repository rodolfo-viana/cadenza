// A diagram edit is one operation: source, rendering inputs and both SVGs.
// Rendering may outlive a selected inspector panel, so capture the target and
// check it again inside the document's operation queue.
import { state, apply, trackEdit, registerFlush } from './store.js';
import { renderDiagram, diagramInputs, sameDiagramInputs } from './diagrams.js';
import { t } from './i18n.js';

const sessions = new Map();

export function createDiagramEditor(target, onChange) {
  const key = JSON.stringify([target.docId, target.slide, target.id]);
  if (sessions.has(key)) return sessions.get(key).connect(onChange);
  const read = () => state.doc?.docId === target.docId
    ? state.doc.deck.slides.find((s) => s.id === target.slide)?.elements.find((e) => e.id === target.id)?.content
    : null;
  let expected = diagramInputs(read());
  let draft = { ...expected };
  let writing = null;
  let queue = Promise.resolve();
  let version = 0, pending = 0;
  let owner = null, observer = null;
  let status = { busy: false, error: null };
  let retry = false;
  const notify = (value) => { status = value; observer?.({ draft, ...status }); };
  const release = () => {
    if (!owner && !pending && (!retry || read()?.kind !== 'diagram')) {
      unregister();
      sessions.delete(key);
    }
  };

  function sync(content) {
    const inputs = diagramInputs(content);
    if (sameDiagramInputs(inputs, expected)) return;
    if (writing && sameDiagramInputs(inputs, writing)) {
      expected = inputs;
      return;
    }
    // An external edit/history change wins over a drawing still in progress.
    version++;
    expected = inputs;
    draft = { ...inputs };
    retry = false;
    notify({ busy: false, error: null });
  }

  function update(patch, coalesce = null) {
    if (read()?.kind !== 'diagram') return Promise.resolve(false);
    sync(read());
    draft = { ...draft, ...patch };
    const next = { ...draft };
    const token = ++version;
    const latest = () => version === token;
    notify({ busy: true, error: null });
    const run = async () => {
      if (!latest()) return false;
      if (read()?.kind !== 'diagram' || !sameDiagramInputs(diagramInputs(read()), expected)) {
        sync(read());
        return false;
      }
      let svgs = null;
      let error = null;
      try {
        svgs = await renderDiagram(next.source, next.style, next.look);
      } catch (e) {
        error = String(e?.message || e?.str || e);
      }
      if (!latest()) return false;
      // Invalid input is still a real edit. Leave the last good SVG and its
      // renderedFor stamp intact, so the inspector can identify the fallback.
      const content = svgs ? { ...next, ...svgs, renderedFor: next } : next;
      writing = next;
      let attempted = false;
      const outcome = await apply(() => {
        const now = read();
        if (!latest() || now?.kind !== 'diagram' || !sameDiagramInputs(diagramInputs(now), expected)) return null;
        attempted = true;
        return { op: 'updateElements', slide: target.slide, updates: [{ id: target.id, patch: { content } }] };
      }, { docId: target.docId, coalesce, navigate: false });
      writing = null;
      const applied = outcome !== null;
      if (applied && read()?.kind === 'diagram') expected = diagramInputs(read());
      if (latest()) {
        retry = attempted && !applied && read()?.kind === 'diagram' && sameDiagramInputs(diagramInputs(read()), expected);
        if (retry) error = t('Could not save the diagram. Your changes are still here; try again.');
        else {
          expected = diagramInputs(read());
          draft = { ...expected };
        }
        notify({ busy: false, error });
      }
      return applied;
    };
    pending++;
    queue = queue.then(run, run).finally(() => { pending--; release(); });
    return trackEdit(queue);
  }

  const unregister = registerFlush(async () => {
    await queue;
    if (read()?.kind !== 'diagram') { retry = false; release(); return true; }
    return retry ? update({}) : true;
  });
  const session = {
    connect(callback) {
      const token = Symbol();
      owner = token;
      observer = callback;
      sync(read());
      queueMicrotask(() => { if (owner === token) observer({ draft, ...status }); });
      return {
        update: (patch, coalesce) => owner === token ? update(patch, coalesce) : Promise.resolve(false),
        sync: (content) => { if (owner === token) sync(content); },
        dispose() {
          if (owner !== token) return;
          owner = null;
          observer = null;
          release();
        },
      };
    },
  };
  sessions.set(key, session);
  return session.connect(onChange);
}
