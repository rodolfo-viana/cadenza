// Theme Builder: create and share custom themes.

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { invoke, dialog } from './api.js';
import { state, set, closeModal, toast, ingestView, openMenu, deck, flushEdits } from './store.js';
import { Modal, Button } from './ui.js';
import { Row, Section, Hint, NumberField, Slider, Select, Segmented, Toggle, TextField, ColorField, FontField } from './controls.js';
import { escapeHtml } from './util.js';
import { t, N_ } from './i18n.js';

/** CSS background of a theme definition (mirrors the Rust generator). */
export const themeBackground = (d) =>
  d.background2 ? `linear-gradient(${d.gradientAngle}deg, ${d.background} 0%, ${d.background2} 100%)` : d.background;

/** Props for a theme gallery card from a custom theme. */
export function cardFromDef(d) {
  const sans = "Inter, 'Segoe UI', Cantarell, Ubuntu, sans-serif";
  return {
    id: d.id,
    name: d.name,
    dark: d.dark,
    custom: true,
    previewBg: themeBackground(d),
    headingColor: d.headingColor,
    textColor: d.textColor,
    headingFont: d.headingFont || sans,
    bodyFont: d.bodyFont || sans,
    accent: d.accent,
    palette: d.palette,
    uppercase: d.headingUppercase,
    def: d,
  };
}

export async function refreshUserThemes() {
  try {
    const userThemes = await invoke('themes_user');
    set({ info: { ...state.info, userThemes } });
  } catch {
    /* ignore */
  }
}

/** Opens the builder, starting from a custom theme or a built-in one. */
export async function openThemeBuilder(start = null) {
  let def = start;
  if (!def) {
    const d = deck();
    if (d?.customTheme && d.customTheme.id === d.theme) def = d.customTheme;
    else {
      def = await invoke('theme_from_builtin', { id: d?.theme || 'cadenza-midnight' });
      def.name = t('{name} (custom)', { name: def.name });
    }
  }
  set({ modal: { type: 'themeBuilder', props: { def: JSON.parse(JSON.stringify(def)) } }, menu: null });
}

export async function applyCustomTheme(def, resetParagraphs = false) {
  try {
    ingestView(await invoke('theme_apply', { def, resetParagraphs }), { navigate: false });
    return true;
  } catch (e) {
    toast(String(e), 'error');
    return false;
  }
}

export async function exportTheme(def, css = false) {
  const base = (def.name || t('Theme')).replace(/[\\/:*?"<>|]+/g, '-');
  const path = await dialog.save({
    title: css ? t('Export reveal.js Theme CSS') : t('Export Theme'),
    defaultPath: css ? `${base}.css` : `${base}.cadenza-theme`,
    filters: css ? [{ name: 'CSS', extensions: ['css'] }] : [{ name: t('Cadenza Theme'), extensions: ['cadenza-theme'] }],
  });
  if (!path) return;
  try {
    const out = await invoke('theme_export', { def, path, css });
    toast(t('Exported {name}', { name: out.split(/[\\/]/).pop() }), 'success');
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function importTheme() {
  const path = await dialog.open({ title: t('Import Theme'), multiple: false, filters: [{ name: t('Cadenza Theme'), extensions: ['cadenza-theme'] }] });
  if (!path) return null;
  try {
    const def = await invoke('theme_import', { path });
    await refreshUserThemes();
    toast(t('Added “{name}” to My Themes', { name: def.name }), 'success');
    return def;
  } catch (e) {
    toast(String(e), 'error');
    return null;
  }
}

export function themeMenu(e, def) {
  e.preventDefault();
  openMenu(e.clientX, e.clientY, [
    { label: t('Edit Theme…'), icon: 'pencil', run: () => openThemeBuilder(def) },
    {
      label: t('Duplicate'),
      icon: 'copy-plus',
      run: async () => {
        await invoke('theme_save', { def: { ...def, id: '', name: t('{name} copy', { name: def.name }) } });
        refreshUserThemes();
      },
    },
    '-',
    { label: t('Export Theme File…'), icon: 'download', run: () => exportTheme(def) },
    { label: t('Export as reveal.js CSS…'), icon: 'file-code-2', run: () => exportTheme(def, true) },
    '-',
    {
      label: t('Delete from My Themes'),
      icon: 'trash-2',
      danger: true,
      run: async () => {
        await invoke('theme_delete', { id: def.id });
        refreshUserThemes();
      },
    },
  ]);
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

function previewDoc(p, width) {
  const s = width / p.width;
  // The frame paints its own background; match the canvas behind it.
  const canvas = getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim() || 'transparent';
  const slides = p.slides
    .map(
      (r) => `<div class="pv" style="width:${p.width * s}px;height:${p.height * s}px">
        <div class="reveal-viewport bg"></div>
        <div class="reveal cz-editor-reveal" style="width:${p.width}px;height:${p.height}px;transform:scale(${s})">
          <div class="slides"><section class="present ${escapeHtml(r.classes)}" style="width:${p.width}px;height:${p.height}px">
            <div class="cz-slide" style="width:${p.width}px;height:${p.height}px">${r.html}</div>
          </section></div>
        </div>
      </div>`,
    )
    .join('');
  return `<!doctype html><html><head><meta charset="utf-8">
    <link rel="stylesheet" href="css/content.css"><link rel="stylesheet" href="css/editor-slide.css">
    <style>${p.css.replace(/<\/style/gi, '<\\/style')}</style>
    <style>html,body{margin:0;background:${canvas};overflow:hidden}
      body{display:flex;flex-direction:column;gap:14px;padding:2px}
      .pv{position:relative;overflow:hidden;border-radius:8px;box-shadow:0 6px 20px rgba(0,0,0,.3)}
      .bg{position:absolute;inset:0}
      .cz-placeholder{display:none}</style></head><body>${slides}</body></html>`;
}

export function Preview({ def, tab = 'theme' }) {
  const [p, setP] = useState(null);
  const [error, setError] = useState('');
  const ref = useRef();
  const key = JSON.stringify(def);
  useEffect(() => {
    let current = true;
    const timer = setTimeout(() => {
      setError('');
      invoke('theme_preview', { def }).then((preview) => {
        if (current) setP(preview);
      }).catch((e) => { if (current) setError(String(e)); });
    }, 120);
    return () => { current = false; clearTimeout(timer); };
  }, [key]);
  const width = 470;
  const doc = useMemo(() => {
    if (!p) return '';
    const first = tab === 'lists' ? 4 : tab === 'paragraphs' ? 3 : 0;
    const slides = p.slides[first] ? [p.slides[first], ...p.slides.filter((_, i) => i !== first)] : p.slides;
    return previewDoc({ ...p, slides }, width);
  }, [p, tab]);
  const height = p ? Math.ceil((p.height * width) / p.width) * p.slides.length + 14 * (p.slides.length - 1) + 8 : 600;
  return html`${error && html`<p role="alert" class="tb-error">${t('Could not update the preview.')} ${error}</p>`}
    <iframe ref=${ref} class="tb-preview" title=${t('Theme preview')} srcdoc=${doc} style=${{ width: width + 6 + 'px', height: height + 'px' }} />`;
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

const WEIGHT_NAMES = {
  300: N_('font weight::Light'),
  400: N_('font weight::Regular'),
  500: N_('font weight::Medium'),
  600: N_('font weight::Semibold'),
  700: N_('font weight::Bold'),
  800: N_('font weight::Extra Bold'),
  900: N_('font weight::Black'),
};
const WEIGHTS = [300, 400, 500, 600, 700, 800, 900].map((w) => ({ value: String(w), label: WEIGHT_NAMES[w] }));

function ParagraphFields({ value, onChange }) {
  return html`<${Section} title=${t('Paragraphs')} id="tb-paragraphs">
    <${Row} label=${t('Alignment')}><${Select} value=${value.align} options=${[
      { value: 'left', label: t('Left') }, { value: 'center', label: t('Center') },
      { value: 'right', label: t('Right') }, { value: 'justify', label: t('Justify') },
    ]} onChange=${(align) => onChange({ align })} /><//>
    <${Row} label=${t('Line spacing')}><${NumberField} value=${value.lineHeight} min=${0.8} max=${4} step=${0.05}
      onChange=${(lineHeight) => onChange({ lineHeight })} /><//>
    ${[
      ['spaceBefore', t('Space before'), 0, 5], ['spaceAfter', t('Space after'), 0, 5],
      ['indentLeft', t('Left indent'), 0, 10], ['indentRight', t('Right indent'), 0, 10], ['firstLine', t('First line indent'), -5, 10],
    ].map(([field, label, min, max]) => html`<${Row} label=${label}><${NumberField} value=${value[field]} min=${min} max=${max} step=${0.05}
      unit="em" onChange=${(v) => onChange({ [field]: v })} /><//>`)}
    <${Hint}>${t('Distances in em scale with the text size. A negative first line indent creates a hanging indent.')}</${Hint}>
  <//>`;
}

function ListFields({ value, level, onLevel, onChange, onPreset }) {
  return html`<${Section} title=${t('Bullets & numbering')} id="tb-lists">
    <${Row} label=${t('Presets')}><div class="row-inline">
      <${Button} size="sm" label=${t('Classic')} onClick=${() => onPreset('classic')} />
      <${Button} size="sm" label=${t('Outline')} onClick=${() => onPreset('outline')} />
      <${Button} size="sm" label=${t('Minimal')} onClick=${() => onPreset('minimal')} />
    </div><//>
    <${Hint}>${t('Presets change the marker styles at every level. Spacing and colors stay.')}</${Hint}>
    <${Row} label=${t('List level')}><${Select} value=${String(level)} options=${Array.from({ length: 9 }, (_, i) => ({
      value: String(i), label: t('Level {number}', { number: i + 1 }),
    }))} onChange=${(v) => onLevel(Number(v))} /><//>
    <div key=${level}>
      <${Row} label=${t('Bullet style')}><${Select} value=${value.bullet} options=${[
        ['disc', `●  ${t('Filled circle')}`], ['circle', `○  ${t('Hollow circle')}`], ['square', `■  ${t('Square')}`],
        ['dash', `–  ${t('Dash')}`], ['arrow', `→  ${t('Arrow')}`], ['check', `✓  ${t('Check mark')}`], ['none', t('None')],
      ].map(([value, label]) => ({ value, label }))} onChange=${(bullet) => onChange({ bullet })} /><//>
      <${Row} label=${t('Numbering style')}><${Select} value=${value.numbering} options=${[
        ['decimal', '1, 2, 3'], ['decimal-leading-zero', '01, 02, 03'], ['lower-alpha', 'a, b, c'],
        ['upper-alpha', 'A, B, C'], ['lower-roman', 'i, ii, iii'], ['upper-roman', 'I, II, III'],
      ].map(([value, label]) => ({ value, label }))} onChange=${(numbering) => onChange({ numbering })} /><//>
      <${Row} label=${t('List indent')}><${NumberField} value=${value.indent} min=${0.5} max=${8} step=${0.05} unit="em"
        onChange=${(indent) => onChange({ indent })} /><//>
      <${Row} label=${t('Item spacing')}><${NumberField} value=${value.spacing} nullable placeholder=${t('Automatic')}
        min=${0} max=${5} step=${0.05} unit="em" onChange=${(spacing) => onChange({ spacing })} /><//>
      <${Row} label=${t('Marker color')}><${ColorField} value=${value.markerColor} noneLabel=${t('Same as text')}
        onChange=${(markerColor) => onChange({ markerColor })} /><//>
      <${Row} label=${t('Marker size')}><${Slider} value=${value.markerSize} min=${0.5} max=${2} step=${0.05} unit="em"
        onChange=${(markerSize) => onChange({ markerSize })} /><//>
    </div>
    <${Hint}>${t('Each level has its own settings. Indent is relative to its parent. Automatic item spacing is half the paragraph spacing.')}</${Hint}>
  <//>`;
}

export function ThemeBuilder({ def: initial }) {
  const [def, setDef] = useState(initial);
  const draft = useRef(def);
  const [tab, setTab] = useState('theme');
  const [level, setLevel] = useState(0);
  const [resetParagraphs, setResetParagraphs] = useState(false);
  const [busy, setBusy] = useState(false);
  const upd = (patch) => { draft.current = { ...draft.current, ...patch }; setDef(draft.current); };
  const paragraph = (patch) => upd({ paragraphs: { ...draft.current.paragraphs, ...patch } });
  const list = (patch) => upd({ lists: draft.current.lists.map((value, i) => i === level ? { ...value, ...patch } : value) });
  const setPal = (i, c) => upd({ palette: draft.current.palette.map((x, j) => (j === i ? c : x)) });
  const hasDoc = !!state.doc;
  const weights = WEIGHTS.map((w) => ({ ...w, label: t(w.label) }));

  const save = async () => {
    if (!(await flushEdits())) return null;
    try {
      const saved = await invoke('theme_save', { def: draft.current });
      draft.current = saved;
      setDef(saved);
      await refreshUserThemes();
      toast(t('Saved “{name}” to My Themes', { name: saved.name }), 'success');
      return saved;
    } catch (e) {
      toast(String(e), 'error');
      return null;
    }
  };

  const flipBase = (dark) => {
    if (dark === def.dark) return;
    upd(
      dark
        ? { dark, background: '#101828', background2: '#1e1b4b', textColor: '#d0d5dd', headingColor: '#ffffff', onAccent: '#ffffff' }
        : { dark, background: '#ffffff', background2: '', textColor: '#334155', headingColor: '#0f172a', onAccent: '#ffffff' },
    );
  };

  return html`<${Modal}
    title=${html`<span class="tb-title"><${Icon} name="swatch-book" size=${18} /> ${t('Theme Builder')}</span>`}
    width=${1060}
    onClose=${() => { if (!busy) closeModal(); }}
    class="tb-modal"
    footer=${html`
      <${Button} icon="download" label=${t('Export')} disabled=${busy} onClick=${(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        openMenu(r.left, r.top - 90, [
          { label: t('Theme File (.cadenza-theme)…'), icon: 'download', run: async () => { if (await flushEdits()) await exportTheme(draft.current); } },
          { label: t('reveal.js CSS…'), icon: 'file-code-2', run: async () => { if (await flushEdits()) await exportTheme(draft.current, true); } },
        ]);
      }} />
      <span class="spacer" />
      <${Button} label=${t('Cancel')} disabled=${busy} onClick=${closeModal} />
      <${Button} icon="save" label=${t('Save to My Themes')} disabled=${busy} onClick=${async () => { setBusy(true); try { await save(); } finally { setBusy(false); } }} />
      ${hasDoc &&
      html`<${Button}
        variant="primary"
        icon="check"
        label=${t('Apply to Presentation')}
        disabled=${busy}
        onClick=${async () => {
          setBusy(true);
          try {
            const saved = await save();
            if (saved && await applyCustomTheme(saved, resetParagraphs)) closeModal();
          } finally { setBusy(false); }
        }}
      />`}
    `}
  >
    <div class="tb">
      <div class="tb-form">
        <nav class="tb-nav" aria-label=${t('Theme settings')}>
          ${[
            ['theme', t('Theme')], ['type', t('Typography')], ['paragraphs', t('Paragraphs')], ['lists', t('Lists')], ['colors', t('Colors')],
          ].map(([value, label]) => html`<button type="button" disabled=${busy} aria-pressed=${tab === value} onClick=${() => setTab(value)}>${label}</button>`)}
        </nav>
        <fieldset class="tb-fields" key=${tab} disabled=${busy}>
        ${tab === 'theme' && html`
        <${Section} title=${t('Theme')} id="tb-basics">
          <${Row} label=${t('Name')}><${TextField} value=${def.name} onChange=${(v) => upd({ name: v })} /><//>
          <${Row} label=${t('Author')}><${TextField} value=${def.author} placeholder=${t('Your name')} onChange=${(v) => upd({ author: v })} /><//>
          <${Row} label=${t('Base')}>
            <${Segmented}
              value=${def.dark ? 'dark' : 'light'}
              options=${[
                { value: 'dark', label: t('Dark') },
                { value: 'light', label: t('Light') },
              ]}
              onChange=${(v) => flipBase(v === 'dark')}
            />
          <//>
        <//>
        <${Section} title=${t('Background')} id="tb-bg">
          <${Row} label=${t('Color')}><${ColorField} value=${def.background} allowNone=${false} onChange=${(v) => upd({ background: v })} /><//>
          <${Row}><${Toggle} value=${!!def.background2} label=${t('Gradient')} onChange=${(v) => upd({ background2: v ? (def.dark ? '#312e81' : '#e0e7ff') : '' })} /><//>
          ${def.background2 &&
          html`<${Row} label=${t('To')}><${ColorField} value=${def.background2} allowNone=${false} onChange=${(v) => upd({ background2: v })} /><//>
            <${Row} label=${t('Angle')}><${Slider} value=${def.gradientAngle} min=${0} max=${360} unit="°" onChange=${(v) => upd({ gradientAngle: v })} /><//>`}
        <//>
        `}
        ${tab === 'type' && html`
        <${Section} title=${t('Typography')} id="tb-type">
          <${Row} label=${t('Headings')}><${FontField} value=${def.headingFont} placeholder=${t('System sans-serif')} onChange=${(v) => upd({ headingFont: v })} /><//>
          <${Row} label=${t('Body text')}><${FontField} value=${def.bodyFont} placeholder=${t('System sans-serif')} onChange=${(v) => upd({ bodyFont: v })} /><//>
          <${Row} label=${t('Heading weight')}><${Select} value=${String(def.headingWeight)} options=${weights} onChange=${(v) => upd({ headingWeight: Number(v) })} /><//>
          <${Row} label=${t('Body weight')}><${Select} value=${String(def.bodyWeight)} options=${weights} onChange=${(v) => upd({ bodyWeight: Number(v) })} /><//>
          <${Row} label=${t('Heading line spacing')}><${Slider} value=${def.headingLineHeight} min=${0.8} max=${4} step=${0.05} onChange=${(v) => upd({ headingLineHeight: v })} /><//>
          <${Row} label=${t('Tracking')}><${Slider} value=${def.headingLetterSpacing} min=${-0.1} max=${0.3} step=${0.005} format=${3} unit="em" onChange=${(v) => upd({ headingLetterSpacing: v })} /><//>
          <${Row}><${Toggle} value=${def.headingUppercase} label=${t('Uppercase headings')} onChange=${(v) => upd({ headingUppercase: v })} /><//>
          <${Row} label=${t('reveal.js base size')}><${Slider} value=${def.baseSize} min=${16} max=${64} unit="px" onChange=${(v) => upd({ baseSize: v })} /><//>
          <${Hint}>${t('Text box sizes are set by the layout or the Format panel.')}</${Hint}>
          ${!hasDoc && html`<${Hint}>${t('Open a presentation to pick Google Fonts here.')}</${Hint}>`}
        <//>
        `}
        ${tab === 'paragraphs' && html`<${ParagraphFields} value=${def.paragraphs} onChange=${paragraph} />`}
        ${tab === 'lists' && html`<${ListFields} value=${def.lists[level]} level=${level} onLevel=${setLevel} onChange=${list}
          onPreset=${(preset) => upd({ lists: draft.current.lists.map((value, i) => ({ ...value,
            bullet: preset === 'minimal' ? 'dash' : ['disc', 'circle', 'square'][i % 3],
            numbering: preset === 'outline' ? ['decimal', 'lower-alpha', 'lower-roman'][i % 3] : 'decimal',
          })) })} />`}
        ${tab === 'colors' && html`
        <${Section} title=${t('Colors')} id="tb-colors">
          <${Row} label=${t('Headings')}><${ColorField} value=${def.headingColor} allowNone=${false} onChange=${(v) => upd({ headingColor: v })} /><//>
          <${Row} label=${t('Text')}><${ColorField} value=${def.textColor} allowNone=${false} onChange=${(v) => upd({ textColor: v })} /><//>
          <${Row} label=${t('Accent')}><${ColorField} value=${def.accent} allowNone=${false} onChange=${(v) => upd({ accent: v })} /><//>
          <${Row} label=${t('On accent')}><${ColorField} value=${def.onAccent} allowNone=${false} onChange=${(v) => upd({ onAccent: v })} /><//>
          <${Row} label=${t('Links')}><${ColorField} value=${def.linkColor} noneLabel=${t('Same as accent')} onChange=${(v) => upd({ linkColor: v })} /><//>
        <//>
        <${Section} title=${t('Chart & Shape Palette')} id="tb-palette">
          <div class="tb-palette">
            ${def.palette.map((c, i) => html`<${ColorField} value=${c} allowNone=${false} label=${t('Color {number}', { number: i + 1 })} onChange=${(v) => setPal(i, v)} />`)}
          </div>
        <//>
        `}
        ${hasDoc && html`<div class="tb-adoption">
          <${Toggle} value=${resetParagraphs} label=${t('Use theme paragraph settings in existing text boxes')} onChange=${setResetParagraphs} />
          <${Hint}>${resetParagraphs
            ? t('Applying resets alignment, line spacing and paragraph spacing in all text boxes. Fonts and inline formatting stay. You can undo this.')
            : t('Existing text boxes keep their own formatting. New text follows the theme.')}</${Hint}>
        </div>`}
        </fieldset>
      </div>
      <div class="tb-preview-wrap">
        <div class="tb-preview-label">${t('Live preview')}</div>
        <${Preview} def=${def} tab=${tab} />
      </div>
    </div>
  <//>`;
}
