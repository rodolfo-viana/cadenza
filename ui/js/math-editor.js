// One target-bound draft shared by the visual and LaTeX equation controls.
// Rust and the existing operation queue remain the document/history owners.
import { state, apply, trackEdit, registerFlush } from './store.js';
import { t } from './i18n.js';

// Retain an unsaved draft when selection changes. Reopening the same equation
// reconnects to it, and Save can retry a failed write even without its panel.
const sessions = new Map();

export function createMathEditor(target, onChange) {
  const key = JSON.stringify([target.docId, target.slide, target.id]);
  if (sessions.has(key)) return sessions.get(key).connect(onChange);
  const read = () => state.doc?.docId === target.docId
    ? state.doc.deck.slides.find((s) => s.id === target.slide)?.elements.find((e) => e.id === target.id)?.content
    : null;
  let saved = read()?.latex || '', draft = saved;
  let latest = 0, active = 0;
  let pending = Promise.resolve(true);
  let owner = null, observer = null, error = null;
  let composing = false, retry = false;
  let coalesce = null;
  const notify = (message = null) => { error = message; observer?.({ draft, busy: active > 0, error }); };
  const release = () => {
    if (!owner && !active && (read()?.kind !== 'math' || (!retry && draft === read().latex))) {
      unregister();
      sessions.delete(key);
    }
  };

  function commit(value) {
    if (read()?.kind !== 'math') return Promise.resolve(false);
    const token = ++latest;
    active++;
    notify();
    let attempted = false;
    pending = trackEdit(apply(() => {
      const now = read();
      if (now?.kind !== 'math' || now.latex === value) return null;
      attempted = true;
      return { op: 'updateElements', slide: target.slide, updates: [{ id: target.id, patch: { content: { latex: value } } }] };
    }, { docId: target.docId, coalesce, navigate: false }).then((outcome) => {
      active--;
      const now = read();
      const ok = outcome !== null || (!attempted && now?.kind === 'math' && now.latex === value);
      if (token === latest) {
        retry = !ok && now?.kind === 'math';
        if (ok) saved = now.latex;
        notify(retry ? t('Could not save the equation. Your changes are still here; try again.') : null);
      }
      release();
      return ok;
    }));
    return pending;
  }

  function setDraft(value, key = null) {
    draft = value;
    coalesce = key;
    notify();
    return composing ? Promise.resolve(true) : commit(value);
  }

  function sync(value) {
    // Acknowledgements of earlier keystrokes cannot replace a newer draft.
    // History changes run after flush and become the authoritative value.
    if (active || (retry && value === saved) || value === draft) return;
    saved = value;
    draft = value;
    retry = false;
    notify();
  }

  async function flush() {
    await pending;
    if (composing) return false;
    if (read()?.kind !== 'math') { retry = false; release(); return true; }
    return retry || draft !== read().latex ? commit(draft) : true;
  }
  const unregister = registerFlush(flush);
  const session = {
    connect(callback) {
      const token = Symbol();
      owner = token;
      observer = callback;
      const now = read();
      if (now?.kind === 'math') sync(now.latex);
      queueMicrotask(() => { if (owner === token) observer({ draft, busy: active > 0, error }); });
      return {
        setDraft: (value, key) => owner === token ? setDraft(value, key) : Promise.resolve(false),
        sync: (value) => { if (owner === token) sync(value); },
        flush,
        beginComposition() { if (owner === token) composing = true; },
        endComposition(value, key) {
          if (owner !== token) return Promise.resolve(false);
          composing = false;
          return setDraft(value, key);
        },
        dispose() {
          if (owner !== token) return;
          owner = null;
          observer = null;
          composing = false;
          release();
        },
      };
    },
  };
  sessions.set(key, session);
  return session.connect(onChange);
}
