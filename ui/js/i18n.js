// Interface translations. English text is the key (gettext style); each
// language has a catalog in ../locales/<code>.json mapping English strings to
// translations. Missing entries fall back to English.
//
//   t('Save')                          → "Salvar"
//   t('Slide {n}', { n: 3 })           → "Slide 3"
//   tn('{n} slide', '{n} slides', n)   → plural forms of the language
//   N_('Format')                       → marks a string for extraction; translate it with t() where shown
//   t('line height::Line')             → "context::" disambiguates identical English words (shown: "Line")
//
// Catalog values are strings, or for plurals objects keyed by CLDR category
// ({ "one": "…", "few": "…", "many": "…", "other": "…" }).

export const LANGUAGES = [
  { code: 'ar', name: 'العربية', english: 'Arabic', rtl: true },
  { code: 'bn', name: 'বাংলা', english: 'Bengali' },
  { code: 'zh-CN', name: '简体中文', english: 'Chinese (Simplified)' },
  { code: 'nl', name: 'Nederlands', english: 'Dutch' },
  { code: 'en', name: 'English', english: 'English' },
  { code: 'fr', name: 'Français', english: 'French' },
  { code: 'de', name: 'Deutsch', english: 'German' },
  { code: 'hi', name: 'हिन्दी', english: 'Hindi' },
  { code: 'it', name: 'Italiano', english: 'Italian' },
  { code: 'ja', name: '日本語', english: 'Japanese' },
  { code: 'pl', name: 'Polski', english: 'Polish' },
  { code: 'pt-PT', name: 'Português', english: 'Portuguese' },
  { code: 'pt-BR', name: 'Português (Brasil)', english: 'Portuguese (Brazil)' },
  { code: 'ru', name: 'Русский', english: 'Russian' },
  { code: 'es', name: 'Español', english: 'Spanish' },
  { code: 'tr', name: 'Türkçe', english: 'Turkish' },
];

let lang = 'en';
let catalog = {};
let plurals = new Intl.PluralRules('en');

export const language = () => lang;
export const languageInfo = (code = lang) => LANGUAGES.find((l) => l.code === code) || LANGUAGES.find((l) => l.code === 'en');
export const isRtl = () => !!languageInfo().rtl;

/** Loads a language (falls back to English) and sets the page direction. */
export async function setLanguage(code) {
  const info = languageInfo(code);
  let cat = {};
  if (info.code !== 'en') {
    try {
      const base = new URL('../locales/', import.meta.url);
      const res = await fetch(new URL(`${info.code}.json`, base));
      if (res.ok) cat = await res.json();
    } catch {
      cat = {};
    }
  }
  lang = info.code;
  catalog = cat;
  plurals = new Intl.PluralRules(info.code);
  document.documentElement.lang = info.code;
  document.documentElement.dir = info.rtl ? 'rtl' : 'ltr';
}

/** English text of a key (without its "context::" prefix). */
const english = (key) => /^[a-z][a-z ]*::/.exec(key) ? key.slice(key.indexOf('::') + 2) : key;

function fill(s, vars) {
  return vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s;
}

/** Translates `key` (an English string), filling `{name}` placeholders. */
export function t(key, vars) {
  const v = catalog[key];
  if (typeof v === 'string' && v) return fill(v, vars);
  if (v && typeof v === 'object') {
    const s = v[plurals.select(Number(vars?.n ?? 1))] || v.other;
    if (s) return fill(s, vars);
  }
  return fill(english(key), vars);
}

/** Translates a count-dependent string; `{n}` is the count. */
export function tn(one, other, n, vars = {}) {
  const v = catalog[other];
  const all = { ...vars, n };
  if (v && typeof v === 'object') {
    const s = v[plurals.select(n)] || v.other;
    if (s) return fill(s, all);
  }
  if (typeof v === 'string' && v) return fill(v, all);
  return fill(english(n === 1 ? one : other), all);
}

/** Marks a string for translation without translating it (constants). */
export const N_ = (s) => s;

/** Number formatting in the interface language. */
export const formatNumber = (n, opts) => new Intl.NumberFormat(lang, opts).format(n);
