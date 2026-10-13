// Theme Builder: create and share custom themes.

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { invoke, dialog } from './api.js';
import { state, set, closeModal, toast, ingestView, openMenu, deck } from './store.js';
import { Modal, Button } from './ui.js';
import { Row, Section, Hint, Slider, Select, Segmented, Toggle, TextField, ColorField, FontField } from './controls.js';
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
    def = d?.customTheme && d.customTheme.id === d.theme ? d.customTheme : await invoke('theme_from_builtin', { id: d?.theme || 'cadenza-midnight' });
  }
  set({ modal: { type: 'themeBuilder', props: { def: JSON.parse(JSON.stringify(def)) } }, menu: null });
}

export async function applyCustomTheme(def) {
  try {
    ingestView(await invoke('theme_apply', { def }), { navigate: false });
  } catch (e) {
    toast(String(e), 'error');
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

function Preview({ def }) {
  const [p, setP] = useState(null);
  const ref = useRef();
  const key = JSON.stringify(def);
  useEffect(() => {
    const timer = setTimeout(() => invoke('theme_preview', { def }).then(setP).catch(() => {}), 120);
    return () => clearTimeout(timer);
  }, [key]);
  const width = 470;
  const doc = useMemo(() => (p ? previewDoc(p, width) : ''), [p]);
  const height = p ? Math.ceil((p.height * width) / p.width) * p.slides.length + 14 * (p.slides.length - 1) + 8 : 600;
  return html`<iframe ref=${ref} class="tb-preview" title=${t('Theme preview')} srcdoc=${doc} style=${{ width: width + 6 + 'px', height: height + 'px' }} />`;
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

export function ThemeBuilder({ def: initial }) {
  const [def, setDef] = useState(initial);
  const upd = (patch) => setDef((d) => ({ ...d, ...patch }));
  const setPal = (i, c) => upd({ palette: def.palette.map((x, j) => (j === i ? c : x)) });
  const hasDoc = !!state.doc;
  const weights = WEIGHTS.map((w) => ({ ...w, label: t(w.label) }));

  const save = async () => {
    try {
      const saved = await invoke('theme_save', { def });
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
    onClose=${closeModal}
    class="tb-modal"
    footer=${html`
      <${Button} icon="download" label=${t('Export')} onClick=${(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        openMenu(r.left, r.top - 90, [
          { label: t('Theme File (.cadenza-theme)…'), icon: 'download', run: () => exportTheme(def) },
          { label: t('reveal.js CSS…'), icon: 'file-code-2', run: () => exportTheme(def, true) },
        ]);
      }} />
      <span class="spacer" />
      <${Button} label=${t('Cancel')} onClick=${closeModal} />
      <${Button} icon="save" label=${t('Save to My Themes')} onClick=${save} />
      ${hasDoc &&
      html`<${Button}
        variant="primary"
        icon="check"
        label=${t('Apply to Presentation')}
        onClick=${async () => {
          const saved = await save();
          if (saved) {
            await applyCustomTheme(saved);
            closeModal();
          }
        }}
      />`}
    `}
  >
    <div class="tb">
      <div class="tb-form">
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
        <${Section} title=${t('Typography')} id="tb-type">
          <${Row} label=${t('Headings')}><${FontField} value=${def.headingFont} placeholder=${t('System sans-serif')} onChange=${(v) => upd({ headingFont: v })} /><//>
          <${Row} label=${t('Body text')}><${FontField} value=${def.bodyFont} placeholder=${t('System sans-serif')} onChange=${(v) => upd({ bodyFont: v })} /><//>
          <${Row} label=${t('Weight')}><${Select} value=${String(def.headingWeight)} options=${weights} onChange=${(v) => upd({ headingWeight: Number(v) })} /><//>
          <${Row} label=${t('Tracking')}><${Slider} value=${def.headingLetterSpacing} min=${-0.1} max=${0.3} step=${0.005} format=${3} unit="em" onChange=${(v) => upd({ headingLetterSpacing: v })} /><//>
          <${Row}><${Toggle} value=${def.headingUppercase} label=${t('Uppercase headings')} onChange=${(v) => upd({ headingUppercase: v })} /><//>
          <${Row} label=${t('Base size')}><${Slider} value=${def.baseSize} min=${16} max=${64} unit="px" onChange=${(v) => upd({ baseSize: v })} /><//>
          ${!hasDoc && html`<${Hint}>${t('Open a presentation to pick Google Fonts here.')}</${Hint}>`}
        <//>
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
      </div>
      <div class="tb-preview-wrap">
        <div class="tb-preview-label">${t('Live preview')}</div>
        <${Preview} def=${def} />
      </div>
    </div>
  <//>`;
}
