// Presentation-wide literal text search. HTML is indexed as text nodes so
// matches can cross formatting spans without searching tags or attributes.

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import * as S from './store.js';
import { endAllEditing } from './textedit.js';
import { Button } from './ui.js';
import { t } from './i18n.js';

const BLOCKS = new Set(['P', 'DIV', 'LI', 'BLOCKQUOTE', 'UL', 'OL']);

/** Text offsets use UTF-16, like DOM Range and RegExp match indices. */
export function indexHtml(source) {
  const root = document.createElement('div');
  root.innerHTML = source;
  let text = '';
  const nodes = [];
  const boundary = () => {
    if (text && !text.endsWith('\n')) text += '\n';
  };
  const walk = (node) => {
    if (node.nodeType === 3) {
      nodes.push({ node, start: text.length, end: text.length + node.data.length });
      text += node.data;
      return;
    }
    if (node.nodeType !== 1) return;
    if (node.tagName === 'BR') {
      text += '\n';
      return;
    }
    if (BLOCKS.has(node.tagName)) boundary();
    for (const child of node.childNodes) walk(child);
    if (BLOCKS.has(node.tagName)) boundary();
  };
  for (const child of root.childNodes) walk(child);
  return { root, text, nodes };
}

export function literalMatches(text, query, matchCase = false) {
  if (!query) return [];
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return [...text.matchAll(new RegExp(escaped, matchCase ? 'gu' : 'giu'))].map((m) => ({ start: m.index, end: m.index + m[0].length }));
}

/** Replace backwards, retaining every unaffected node, attribute and link. */
export function replaceHtml(source, matches, replacement) {
  const { root, nodes } = indexHtml(source);
  for (const match of [...matches].sort((a, b) => b.start - a.start)) {
    const touched = nodes.filter((n) => n.start < match.end && n.end > match.start);
    if (!touched.length) continue;
    touched.forEach((entry, i) => {
      const from = Math.max(0, match.start - entry.start);
      const to = Math.min(entry.end - entry.start, match.end - entry.start);
      entry.node.data = entry.node.data.slice(0, from) + (i ? '' : replacement) + entry.node.data.slice(to);
    });
  }
  return root.innerHTML;
}

/** Each surface is one independently validated field in the document. */
export function textSurfaces(deck, { slideId = null, notes = false } = {}) {
  const surfaces = [];
  for (const [index, slide] of deck.slides.entries()) {
    if (slideId && slide.id !== slideId) continue;
    const add = (field, source, extra = {}) => surfaces.push({ slide: slide.id, number: index + 1, field, source, ...extra });
    for (const element of slide.elements) {
      const content = element.content;
      const rich = content.kind === 'text' ? content : content.kind === 'shape' ? content.text : null;
      if (rich) add('richText', rich.html, { element: element.id });
      if (content.kind === 'table') {
        content.rows.forEach((row, r) => row.forEach((cell, column) => add('tableCell', cell, { element: element.id, row: r, column })));
      }
    }
    if (notes) add('notes', slide.notes);
  }
  return surfaces;
}

export function searchDeck(deck, query, options = {}) {
  if (!deck || !query) return [];
  return textSurfaces(deck, options).flatMap((surface) => {
    const text = surface.field === 'notes' ? surface.source : indexHtml(surface.source).text;
    return literalMatches(text, query, options.matchCase).map((range) => ({ ...surface, ...range, text }));
  });
}

export function replacementChanges(matches, replacement) {
  const fields = new Map();
  for (const match of matches) {
    const key = JSON.stringify([match.slide, match.element, match.field, match.row, match.column]);
    if (!fields.has(key)) fields.set(key, []);
    fields.get(key).push(match);
  }
  return [...fields.values()].map((ranges) => {
    const first = ranges[0];
    let after;
    if (first.field === 'notes') {
      after = first.source;
      for (const r of [...ranges].sort((a, b) => b.start - a.start)) after = after.slice(0, r.start) + replacement + after.slice(r.end);
    } else after = replaceHtml(first.source, ranges, replacement);
    return { slide: first.slide, element: first.element || null, field: first.field, row: first.row, column: first.column, before: first.source, after };
  });
}

export async function openFindReplace() {
  const docId = S.state.doc?.docId;
  if (!docId || !(await endAllEditing()) || S.state.doc?.docId !== docId) return;
  S.set({ findReplace: true, menu: null });
  window.dispatchEvent(new CustomEvent('cz-find-focus'));
}

export function closeFindReplace() {
  S.set({ findReplace: false });
}

export function FindReplace() {
  const state = S.useStore();
  const [query, setQuery] = useState('');
  const [replacement, setReplacement] = useState('');
  const [matchCase, setMatchCase] = useState(false);
  const [notes, setNotes] = useState(false);
  const [scope, setScope] = useState(null);
  const [index, setIndex] = useState(0);
  const [working, setWorking] = useState(false);
  const [message, setMessage] = useState('');
  const input = useRef();
  const docId = state.doc.docId;
  const options = { slideId: scope, matchCase, notes };
  const matches = useMemo(() => searchDeck(state.doc.deck, query, options), [state.doc.version, query, scope, matchCase, notes]);
  const at = Math.min(index, Math.max(0, matches.length - 1));

  useEffect(() => {
    const focus = () => {
      input.current?.focus();
      input.current?.select();
    };
    focus();
    window.addEventListener('cz-find-focus', focus);
    return () => window.removeEventListener('cz-find-focus', focus);
  }, []);
  useEffect(() => {
    setIndex(0);
    setMessage('');
  }, [query, scope, matchCase, notes]);

  async function show(n) {
    const match = matches[n];
    if (!match || !(await endAllEditing()) || S.state.doc?.docId !== docId) return;
    S.goToSlide(match.slide);
    if (match.element) S.select([match.element]);
    else if (!S.settings().showNotes) S.updateSettings({ showNotes: true });
    setIndex(n);
  }

  async function replace(all) {
    if (working || !matches.length) return;
    const chosen = matches[at];
    setWorking(true);
    try {
      if (!(await endAllEditing()) || S.state.doc?.docId !== docId) return;
      let count = 0;
      const out = await S.apply(
        () => {
          const fresh = searchDeck(S.deck(), query, options);
          const selected = all
            ? fresh
            : fresh.filter(
                (m) =>
                  m.slide === chosen.slide &&
                  m.element === chosen.element &&
                  m.field === chosen.field &&
                  m.row === chosen.row &&
                  m.column === chosen.column &&
                  m.start === chosen.start &&
                  m.source === chosen.source,
              );
          count = selected.length;
          return count ? { op: 'replaceText', all, changes: replacementChanges(selected, replacement) } : null;
        },
        { docId, navigate: false, select: false },
      );
      if (S.state.doc?.docId !== docId) return;
      setMessage(out ? t('Replaced {n} occurrence(s).', { n: count }) : t('The text changed. Search again before replacing it.'));
    } finally {
      setWorking(false);
    }
  }

  const excerpt = (m) =>
    html`${m.text.slice(Math.max(0, m.start - 25), m.start)}<mark>${m.text.slice(m.start, m.end)}</mark>${m.text.slice(m.end, m.end + 45)}`;
  return html`<aside
    class="find-replace"
    aria-label=${t('Find and Replace')}
    onKeyDown=${(e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        closeFindReplace();
      }
    }}
  >
    <div class="find-heading"><strong>${t('Find and Replace')}</strong><${Button} icon="x" title=${t('Close')} onClick=${closeFindReplace} /></div>
    <label class="field-label" for="find-query">${t('Find')}</label>
    <input
      ref=${input}
      id="find-query"
      class="textfield"
      value=${query}
      onInput=${(e) => setQuery(e.target.value)}
      onKeyDown=${(e) => {
        if (e.key === 'Enter' && matches.length) {
          e.preventDefault();
          show((at + (e.shiftKey ? matches.length - 1 : 1)) % matches.length);
        }
      }}
    />
    <label class="field-label" for="find-replacement">${t('Replace with')}</label>
    <input id="find-replacement" class="textfield" value=${replacement} onInput=${(e) => setReplacement(e.target.value)} />
    <div class="find-options">
      <label><input type="checkbox" checked=${matchCase} onChange=${(e) => setMatchCase(e.target.checked)} />${t('Match case')}</label>
      <label><input type="checkbox" checked=${notes} onChange=${(e) => setNotes(e.target.checked)} />${t('Include speaker notes')}</label>
      <label
        >${t('Search in')}<select value=${scope ? 'slide' : 'deck'} onChange=${(e) => setScope(e.target.value === 'slide' ? S.state.slideId : null)}>
          <option value="deck">${t('All slides')}</option>
          <option value="slide">${t('Current slide')}</option>
        </select></label
      >
    </div>
    <p class="hint">${t('Text boxes, shapes and table cells.')}</p>
    <div class="find-status" role="status">${message || t('{n} occurrence(s)', { n: matches.length })}</div>
    <div class="find-results" aria-label=${t('Search results')}>
      ${matches.slice(0, 500).map(
        (m, i) =>
          html`<button type="button" class=${i === at ? 'active' : ''} onClick=${() => show(i)}>
            <span class="muted">${m.field === 'notes' ? t('Slide {n} — notes', { n: m.number }) : t('Slide {n}', { n: m.number })}</span>
            <span>${excerpt(m)}</span>
          </button>`,
      )}
      ${matches.length > 500 && html`<p class="hint">${t('Showing the first 500 results. All occurrences are included in Replace All.')}</p>`}
    </div>
    <div class="find-actions">
      <${Button}
        icon="chevron-up"
        title=${t('Previous match')}
        disabled=${!matches.length || working}
        onClick=${() => show((at + matches.length - 1) % matches.length)}
      />
      <${Button} icon="chevron-down" title=${t('Next match')} disabled=${!matches.length || working} onClick=${() => show((at + 1) % matches.length)} />
      <span class="spacer" />
      <${Button} label=${t('Replace')} disabled=${!matches.length || working} onClick=${() => replace(false)} />
      <${Button} label=${t('Replace All')} disabled=${!matches.length || working} onClick=${() => replace(true)} />
    </div>
  </aside>`;
}
