// AI assistant (opt-in): settings, "Create with AI" and the Assistant panel.

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { invoke, events } from './api.js';
import { state, set, useStore, settings, updateSettings, toast, closeModal, ingestView, openView, flushEdits, undo } from './store.js';
import { Button, Modal } from './ui.js';
import { Row, Segmented, Select, TextField, Toggle } from './controls.js';
import { KeyPrompt } from './media.js';
import { cls } from './util.js';
import { t, tn, N_, LANGUAGES as UI_LANGUAGES } from './i18n.js';

export const aiEnabled = () => !!settings().aiEnabled;

const PROVIDERS = [
  { value: 'anthropic', label: 'Claude' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'compatible', label: N_('Other') },
];

/** Languages a new presentation can be written in (the model gets the English name). */
const LANGUAGES = UI_LANGUAGES.map((l) => ({ value: l.english, label: l.name }));

// ---------------------------------------------------------------------------
// Running requests
// ---------------------------------------------------------------------------

let current = null;

events.listen('cz-ai-progress', (e) => {
  if (state.ai) set({ ai: { ...state.ai, ...e.payload } });
});

/** Runs an assistant request; resolves null when it was stopped. */
export async function runAi(req) {
  if (state.ai) return null;
  const token = {};
  current = token;
  set({ ai: { phase: 'connecting', slides: 0, title: '' } });
  try {
    const res = await invoke('ai_run', { req });
    return current === token ? res : null;
  } catch (e) {
    if (current !== token || String(e) === 'Cancelled') return null;
    throw e;
  } finally {
    if (current === token) {
      current = null;
      set({ ai: null });
    }
  }
}

export function cancelAi() {
  invoke('ai_cancel').catch(() => {});
  current = null;
  set({ ai: null });
}

function progressText(p) {
  if (p.phase === 'writing' && p.slides > 0) return p.title ? t('Writing slide {n}: “{title}”', { n: p.slides, title: p.title }) : t('Writing slide {n}…', { n: p.slides });
  if (p.phase === 'writing') return t('Writing…');
  if (p.phase === 'images') return tn('Finding a photo…', 'Finding {n} photos…', p.count);
  if (p.phase === 'thinking') return t('Thinking…');
  return t('Connecting…');
}

export function AiProgress({ compact }) {
  const st = useStore();
  if (!st.ai) return null;
  return html`<div class=${cls('ai-progress', compact && 'compact')}>
    <span class="ai-spinner"><${Icon} name="loader-circle" size=${compact ? 15 : 18} /></span>
    <span class="ai-progress-text">${progressText(st.ai)}</span>
    <${Button} size="sm" variant="ghost" icon="circle-stop" label=${t('Stop')} onClick=${cancelAi} />
  </div>`;
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

export function AiSettings() {
  const s = settings();
  const provider = s.aiProvider || 'anthropic';
  const model = (s.aiModels || {})[provider] || '';
  const [keys, setKeys] = useState(null);
  const [models, setModels] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const loadKeys = () => invoke('secret_status').then(setKeys).catch(() => setKeys({}));
  useEffect(() => {
    loadKeys();
  }, []);
  const hasKey = !!keys?.[provider];
  const ready = hasKey || provider === 'compatible';

  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke('ai_models', { provider, baseUrl: s.aiBaseUrl });
      setModels(list);
      if (!model && list.length) updateSettings({ aiModels: { [provider]: list[0].id } });
    } catch (e) {
      setModels(null);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    setModels(null);
    setError(null);
    if (s.aiEnabled && keys && ready) refresh();
  }, [provider, s.aiEnabled, hasKey, s.aiBaseUrl, !!keys]);

  const options = (models || []).map((m) => ({ value: m.id, label: m.name === m.id ? m.id : `${m.name} (${m.id})` }));
  if (model && !options.some((o) => o.value === model)) options.unshift({ value: model, label: model });

  return html`
    <h4>${t('AI Assistant')}</h4>
    <${Row}><${Toggle} value=${!!s.aiEnabled} label=${t('Enable the AI assistant')} onChange=${(v) => updateSettings({ aiEnabled: v })} /><//>
    <p class="ihint">
      ${t('Optional and off by default. When it’s on, your request and the text of the presentation are sent to the provider you choose, only when you ask the assistant something.')}
    </p>
    ${s.aiEnabled &&
    html`
      <${Row} label=${t('Provider')}>
        <${Segmented} value=${provider} options=${PROVIDERS.map((p) => ({ ...p, label: t(p.label) }))} onChange=${(v) => updateSettings({ aiProvider: v })} />
      <//>
      ${provider === 'compatible' &&
      html`<${Row} label=${t('Server')}>
          <${TextField} value=${s.aiBaseUrl} placeholder="http://localhost:11434/v1" onChange=${(v) => updateSettings({ aiBaseUrl: v.trim() })} />
        <//>
        <p class="ihint">${t('Any OpenAI-compatible server: Ollama, LM Studio, llama.cpp, vLLM, OpenRouter…')}</p>`}
      ${keys &&
      (hasKey
        ? html`<${Row} label=${t('API key')}>
            <div class="row-inline">
              <span class="int-ok"><${Icon} name="check" size=${13} /> ${t('Saved on this computer')}</span>
              <${Button} size="sm" variant="ghost" label=${t('Remove')} onClick=${async () => { await invoke('secret_delete', { name: provider }); loadKeys(); }} />
            </div>
          <//>`
        : html`<${KeyPrompt} name=${provider} compact onSaved=${loadKeys} />`)}
      ${ready &&
      html`<${Row} label=${t('Model')}>
        <div class="row-inline ai-model-row">
          ${options.length
            ? html`<${Select} value=${model} options=${options} onChange=${(v) => updateSettings({ aiModels: { [provider]: v } })} />`
            : html`<${TextField} value=${model} placeholder=${provider === 'compatible' ? t('e.g. llama3.3') : t('Model id')} onChange=${(v) => updateSettings({ aiModels: { [provider]: v.trim() } })} />`}
          <${Button} size="sm" variant="ghost" icon="refresh-cw" title=${t('Refresh the model list')} disabled=${loading} onClick=${refresh} />
        </div>
      <//>`}
      ${error && html`<p class="ihint error">${error}</p>`}
      <${Row}><${Toggle} value=${s.aiImages !== false} label=${t('Add stock photos to image slides (Openverse, or Unsplash when connected)')} onChange=${(v) => updateSettings({ aiImages: v })} /><//>
      <p class="ihint">
        ${t('ChatGPT Plus and Claude Pro/Max subscriptions can’t be used by other apps; the providers offer API keys for that (billed by use, usually a few cents per presentation). For free, offline use, run a local model with Ollama and choose “Other”.')}
      </p>
    `}
  `;
}

// ---------------------------------------------------------------------------
// Create a presentation
// ---------------------------------------------------------------------------

export function GenerateDialog({ theme, aspect }) {
  const st = useStore();
  const [prompt, setPrompt] = useState('');
  const [source, setSource] = useState('');
  const [showSource, setShowSource] = useState(false);
  const [count, setCount] = useState(0);
  const [audience, setAudience] = useState('');
  const [language, setLanguage] = useState('');
  const running = !!st.ai;
  const ref = useRef();
  useEffect(() => ref.current?.focus(), []);

  const go = async () => {
    if (!prompt.trim() && !source.trim()) return;
    try {
      const res = await runAi({ mode: 'new', prompt: prompt.trim() || 'A presentation based on the source material.', slides: count, audience, language, source, theme, aspect });
      if (!res?.view) return;
      closeModal();
      openView(res.view);
      toast(res.reply || t('Your presentation is ready'), 'success', { duration: 6000 });
    } catch (e) {
      toast(String(e), 'error', { duration: 9000 });
    }
  };

  return html`<${Modal}
    title=${html`<span class="tb-title"><${Icon} name="wand-sparkles" size=${18} /> ${t('Create with AI')}</span>`}
    width=${620}
    onClose=${running ? null : closeModal}
    footer=${running
      ? html`<${AiProgress} />`
      : html`<span class="muted small ai-model-note">${modelLabel()}</span>
          <span class="spacer" />
          <${Button} label=${t('Cancel')} onClick=${closeModal} />
          <${Button} variant="primary" icon="wand-sparkles" label=${t('Create Presentation')} disabled=${!prompt.trim() && !source.trim()} onClick=${go} />`}
  >
    <div class=${cls('ai-generate', running && 'running')}>
      <label class="ai-label">${t('What is your presentation about?')}</label>
      <textarea
        ref=${ref}
        class="textfield ai-prompt"
        rows="4"
        placeholder=${t('e.g. A 10-minute talk introducing photosynthesis to high-school students, with a diagram of the light reactions')}
        value=${prompt}
        disabled=${running}
        onInput=${(e) => setPrompt(e.target.value)}
        onKeyDown=${(e) => e.key === 'Enter' && (e.ctrlKey || e.metaKey) && go()}
      />
      ${showSource
        ? html`<label class="ai-label">${t('Notes or source text (optional)')}</label>
            <textarea class="textfield" rows="5" placeholder=${t('Paste an outline, an article, meeting notes…')} value=${source} disabled=${running} onInput=${(e) => setSource(e.target.value)} />`
        : html`<button type="button" class="link-btn ai-add-source" onClick=${() => setShowSource(true)}>${t('+ Add notes or source text')}</button>`}
      <div class="ai-grid">
        <${Row} label=${t('Slides')}>
          <${Segmented}
            value=${count}
            options=${[0, 5, 8, 12, 16, 20].map((n) => ({ value: n, label: n ? String(n) : t('Auto') }))}
            onChange=${setCount}
          />
        <//>
        <${Row} label=${t('Audience')}>
          <${TextField} value=${audience} placeholder=${t('e.g. investors, students, a conference')} onChange=${setAudience} live />
        <//>
        <${Row} label=${t('Language')}>
          <${Select} value=${language} options=${[{ value: '', label: t('Same as my description') }, ...LANGUAGES]} onChange=${setLanguage} />
        <//>
      </div>
    </div>
  <//>`;
}

function modelLabel() {
  const s = settings();
  const provider = s.aiProvider || 'anthropic';
  const model = (s.aiModels || {})[provider] || '';
  const name = { anthropic: 'Claude', openai: 'OpenAI', compatible: t('Local / other') }[provider];
  return model ? `${name} · ${model}` : name;
}

// ---------------------------------------------------------------------------
// Assistant panel (inspector tab)
// ---------------------------------------------------------------------------

/** Conversations per document (kept while the app runs). */
const chats = new Map();

function pushMessage(docId, msg) {
  chats.set(docId, [...(chats.get(docId) || []), msg].slice(-40));
  set({ aiChat: (state.aiChat || 0) + 1 });
}

// Sent in the interface language, so the assistant answers in it too.
const SUGGESTIONS = [
  [N_('Write speaker notes for every slide'), 'message-square-text'],
  [N_('Make this slide more concise'), 'wand'],
  [N_('Add a summary slide at the end'), 'list-plus'],
  [N_('Fix spelling and grammar everywhere'), 'check'],
  [N_('Turn this slide into a diagram'), 'workflow'],
];

function summaryChips(s) {
  const out = [];
  if (s.inserted) out.push(tn('+{n} slide', '+{n} slides', s.inserted));
  if (s.replaced) out.push(tn('{n} rebuilt', '{n} rebuilt', s.replaced));
  if (s.edited) out.push(tn('{n} text edited', '{n} texts edited', s.edited));
  if (s.notes) out.push(tn('notes on {n} slide', 'notes on {n} slides', s.notes));
  if (s.deleted) out.push(tn('{n} deleted', '{n} deleted', s.deleted));
  if (s.theme) out.push(t('new theme'));
  if (s.title) out.push(t('renamed'));
  return out;
}

/** Sends a request to the assistant from anywhere (commands, menus). */
export function askAssistant(prompt) {
  if (!state.doc) return;
  if (!settings().showInspector) updateSettings({ showInspector: true });
  set({ tab: 'assistant' });
  if (prompt) setTimeout(() => sendRequest(prompt), 0);
  else setTimeout(() => document.querySelector('.ai-composer textarea')?.focus(), 50);
}

async function sendRequest(prompt) {
  const p = prompt.trim();
  const docId = state.doc?.docId;
  if (!p || !docId || state.ai) return;
  const history = (chats.get(docId) || []).filter((m) => !m.error && !m.muted).map((m) => ({ role: m.role, text: m.text }));
  pushMessage(docId, { role: 'user', text: p });
  await flushEdits();
  try {
    const res = await runAi({ mode: 'edit', prompt: p, history, currentSlide: state.slideId, selected: state.selectedSlides });
    if (!res) {
      pushMessage(docId, { role: 'assistant', text: t('Stopped.'), muted: true });
      return;
    }
    if (res.view && state.doc?.docId === docId) ingestView(res.view, { navigate: true });
    const chips = summaryChips(res.summary);
    pushMessage(docId, { role: 'assistant', text: res.reply || (chips.length ? t('Done.') : t('I didn’t change anything.')), chips, changed: !!res.view });
  } catch (e) {
    pushMessage(docId, { role: 'assistant', text: t(String(e)), error: true });
  }
}

export function AssistantPanel() {
  const st = useStore();
  const docId = st.doc?.docId;
  const chat = chats.get(docId) || [];
  const [text, setText] = useState('');
  const list = useRef();
  const running = !!st.ai;
  useEffect(() => {
    if (list.current) list.current.scrollTop = list.current.scrollHeight;
  }, [chat.length, running]);
  const lastChanged = [...chat].reverse().find((m) => m.role === 'assistant');

  const send = () => {
    if (!text.trim() || running) return;
    const p = text;
    setText('');
    sendRequest(p);
  };

  return html`<div class="ai-panel">
    <div class="ai-chat" ref=${list}>
      ${chat.length === 0 &&
      html`<div class="ai-empty">
        <div class="ai-empty-icon"><${Icon} name="wand-sparkles" size=${26} /></div>
        <b>${t('What should I do?')}</b>
        <p class="muted small">${t('I can add or rewrite slides, write speaker notes, translate, proofread, build charts, tables and diagrams, or change the theme.')}</p>
        <div class="ai-suggestions">
          ${SUGGESTIONS.map(
            ([s, icon]) => html`<button type="button" class="ai-suggestion" disabled=${running} onClick=${() => sendRequest(t(s))}>
              <${Icon} name=${icon} size=${14} />${t(s)}
            </button>`,
          )}
        </div>
      </div>`}
      ${chat.map(
        (m) => html`<div class=${cls('ai-msg', m.role, m.error && 'error', m.muted && 'muted')}>
          <div class="ai-bubble">${m.text}</div>
          ${m.chips?.length > 0 &&
          html`<div class="ai-chips">
            ${m.chips.map((c) => html`<span class="ai-chip">${c}</span>`)}
            ${m === lastChanged && m.changed && st.doc?.undoLabel === 'AI Edit' &&
            html`<button type="button" class="link-btn ai-undo" onClick=${undo}><${Icon} name="undo-2" size=${13} /> ${t('Undo')}</button>`}
          </div>`}
        </div>`,
      )}
      ${running && html`<div class="ai-msg assistant"><${AiProgress} compact /></div>`}
    </div>
    <div class="ai-composer">
      <textarea
        class="textfield"
        rows="3"
        placeholder=${t('Ask for a change… (e.g. “Add three slides about pricing after this one”)')}
        value=${text}
        onInput=${(e) => setText(e.target.value)}
        onKeyDown=${(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
      />
      <div class="ai-composer-foot">
        <span class="muted small">${modelLabel()}</span>
        <${Button} size="sm" variant="primary" icon="send" label=${t('Send')} disabled=${!text.trim() || running} onClick=${send} />
      </div>
    </div>
  </div>`;
}
