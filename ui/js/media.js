// Media Library: photos (Openverse, Wikimedia Commons, Pexels, Pixabay, or a
// link to an image), GIFs (GIPHY, Openverse) and videos (YouTube search,
// YouTube/Vimeo links).

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { invoke, opener } from './api.js';
import { state, set, apply, closeModal, toast, currentSlide, deck } from './store.js';
import { Modal, Button } from './ui.js';
import { Toggle } from './controls.js';
import * as A from './actions.js';
import { cls, escapeHtml } from './util.js';
import { t, N_ } from './i18n.js';

// [where to get a key, service name, what the key is called]
const KEY_LINKS = {
  pexels: ['https://www.pexels.com/api/', 'Pexels', N_('API Key')],
  pixabay: ['https://pixabay.com/api/docs/', 'Pixabay', N_('API Key')],
  giphy: ['https://developers.giphy.com/dashboard/', 'GIPHY', N_('API Key')],
  youtube: ['https://console.cloud.google.com/apis/library/youtube.googleapis.com', 'YouTube', N_('Data API key')],
};

const TABS = [
  {
    id: 'photos',
    label: N_('Photos'),
    icon: 'images',
    providers: [['openverse', 'Openverse (CC)'], ['wikimedia', 'Wikimedia Commons'], ['pexels', 'Pexels'], ['pixabay', 'Pixabay']],
  },
  { id: 'gifs', label: N_('GIFs'), icon: 'sticker', providers: [['giphy', 'GIPHY'], ['openverse-gif', 'Openverse GIFs']] },
  { id: 'videos', label: N_('Videos'), icon: 'square-play', providers: [['youtube', 'YouTube'], ['vimeo', 'Vimeo']] },
];

// Services that can also be browsed on their own website (no key needed).
const SITE = { giphy: 'GIPHY', youtube: 'YouTube', vimeo: 'Vimeo' };

// Services whose search needs the user's own API key.
const KEYED = ['pexels', 'pixabay', 'giphy', 'youtube'];

const WEB = {
  youtube: 'https://www.youtube.com/',
  vimeo: 'https://vimeo.com/watch',
  giphy: 'https://giphy.com/',
};

/**
 * Fallback: open the site in the system browser. When the user comes back
 * with a link copied, Cadenza offers to insert it (see `checkClipboardLink`).
 */
export function openInSystemBrowser(provider) {
  closeModal();
  awaitingLinkUntil = Date.now() + 15 * 60 * 1000;
  opener.openUrl(WEB[provider] || WEB.youtube);
  toast(t('Copy the link of the video you want, then come back to Cadenza.'), 'info', { duration: 7000 });
}

let awaitingLinkUntil = 0;
let lastOffered = '';

/** Offers to insert a YouTube/Vimeo link copied in another app. */
export async function checkClipboardLink() {
  if (!state.doc || Date.now() > awaitingLinkUntil) return;
  let text = '';
  try {
    text = (await window.__TAURI__.clipboardManager.readText()) || '';
  } catch {
    return;
  }
  const link = text.trim();
  if (!link || link === lastOffered || !/^https?:\/\/\S+$/i.test(link) || !/youtube\.com|youtu\.be|vimeo\.com/i.test(link)) return;
  lastOffered = link;
  toast(t('Insert the video link you copied?'), 'info', {
    duration: 12000,
    action: {
      label: t('Insert Video'),
      run: () => {
        awaitingLinkUntil = 0;
        A.insert('video', '', { name: N_('Video'), content: { kind: 'video', src: link, autoplay: false } });
      },
    },
  });
}

/** Opens the provider's own website; the pick arrives as `cz-media-picked`. */
export function browseProvider(provider) {
  closeModal();
  invoke('media_browse', { provider }).catch((e) => toast(String(e), 'error'));
}

/** Shows a translated sentence with the value of its `{key}` placeholder in bold (a link with `url`). */
function withBold(sentence, key, value, url) {
  const [before, after = ''] = sentence.split(`{${key}}`);
  const bold = html`<b>${value}</b>`;
  const shown = url ? html`<a href="#" onClick=${(e) => { e.preventDefault(); opener.openUrl(url); }}>${bold}</a>` : bold;
  return html`${before}${shown}${after}`;
}

function BrowseCard({ provider, onKey }) {
  const name = SITE[provider];
  const searchHere = provider === 'giphy' ? t('Or search GIFs right here with an API key…') : t('Or search videos right here with an API key…');
  const howTo = t('Search {site} just like you normally do — then click {button} in the Cadenza bar at the bottom of the page.', { site: name });
  return html`<div class="browse-card">
    <div class="browse-text">
      <b>${t('Browse {site}', { site: name })}</b>
      <p class="muted small">${withBold(howTo, 'button', t('Insert'))} ${t('No account or key needed.')}</p>
    </div>
    <${Button} variant="primary" icon="external-link" label=${t('Open {site}', { site: name })} onClick=${() => browseProvider(provider)} />
    <button class="link-btn" onClick=${() => openInSystemBrowser(provider)}>${t('Open in my web browser instead')}</button>
    ${onKey && html`<button class="link-btn" onClick=${onKey}>${searchHere}</button>`}
  </div>`;
}

const isLink = (q) => /^https?:\/\/\S+$/i.test(q.trim());

/** Asks for an API key inline. */
export function KeyPrompt({ name, onSaved, compact }) {
  const [value, setValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [url, service, what] = KEY_LINKS[name];
  const keyName = t(what);
  const save = async () => {
    setBusy(true);
    try {
      const where = await invoke('secret_set', { name, value });
      toast(where === 'keychain' ? t('{service} key saved in your system keychain', { service }) : t('{service} key saved', { service }));
      onSaved?.();
    } catch (e) {
      toast(String(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  return html`<div class=${cls('key-prompt', compact && 'compact')}>
    ${!compact && html`<${Icon} name="key-round" size=${22} />`}
    <div class="key-body">
      ${!compact && html`<b>${t('Connect {service}', { service })}</b>`}
      <p class="muted small">
        ${t('Paste your free {service} {keyName}.', { service, keyName })}${' '}
        <a href="#" onClick=${(e) => { e.preventDefault(); opener.openUrl(url); }}>${t('Get one here')}</a>.${' '}
        ${t('It is stored securely on this computer.')}
      </p>
      <div class="row-inline">
        <input class="textfield mono" type="password" placeholder=${keyName} value=${value} onInput=${(e) => setValue(e.target.value)} onKeyDown=${(e) => e.key === 'Enter' && value.trim() && save()} />
        <${Button} label=${t('Save')} variant="primary" disabled=${!value.trim() || busy} onClick=${save} />
      </div>
    </div>
  </div>`;
}

export function MediaLibrary({ tab: initialTab = 'photos', replaceId = null }) {
  const [tab, setTab] = useState(initialTab);
  const [keys, setKeys] = useState(null);
  const [provider, setProvider] = useState(null);
  const [query, setQuery] = useState('');
  const [items, setItems] = useState([]);
  const [next, setNext] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [preview, setPreview] = useState(null);
  const [inserting, setInserting] = useState(null);
  const [credit, setCredit] = useState(() => {
    try {
      return localStorage.getItem('cz-media-credit') === '1';
    } catch {
      return false;
    }
  });
  const sentinel = useRef();
  const searchRef = useRef();
  const seq = useRef(0);

  const loadKeys = () => invoke('secret_status').then(setKeys).catch(() => setKeys({}));
  useEffect(() => {
    loadKeys();
  }, []);

  const [showKey, setShowKey] = useState(false);

  // Default provider per tab: a stock photo service once its key is set,
  // otherwise one that needs no key. Saving a key keeps the current one.
  const keysLoaded = keys !== null;
  useEffect(() => {
    if (!keysLoaded) return;
    const tabInfo = TABS.find((x) => x.id === tab);
    const stock = ['pexels', 'pixabay'].find((p) => keys[p]);
    setProvider(tab === 'photos' && stock ? stock : tabInfo.providers[0][0]);
    setShowKey(false);
    setItems([]);
    setNext(null);
    setError(null);
    setPreview(null);
    setTimeout(() => searchRef.current?.focus(), 30);
  }, [tab, keysLoaded]);

  const needsKey = provider && (provider === 'vimeo' || (KEYED.includes(provider) && keys && !keys[provider]));

  async function search(cursor = null) {
    const q = query.trim();
    if (isLink(q)) {
      // A video link, or a link to an image (or a page that shows one).
      const my = ++seq.current;
      setLoading(true);
      setError(null);
      setItems([]);
      setNext(null);
      try {
        const item = await invoke(tab === 'videos' ? 'media_oembed' : 'media_link', { url: q });
        if (my === seq.current) setPreview(item);
      } catch (e) {
        if (my !== seq.current) return;
        setPreview(null);
        setError(String(e));
      } finally {
        if (my === seq.current) setLoading(false);
      }
      return;
    }
    setPreview(null);
    if (!provider || needsKey) return;
    if (!q && !['pexels', 'pixabay', 'giphy'].includes(provider)) {
      setItems([]);
      setNext(null);
      return;
    }
    const my = ++seq.current;
    setLoading(true);
    setError(null);
    try {
      const page = await invoke('media_search', { provider, query: q, page: cursor });
      if (my !== seq.current) return;
      setItems(cursor ? (prev) => [...prev, ...page.items] : page.items);
      setNext(page.next);
    } catch (e) {
      if (my !== seq.current) return;
      setError(String(e) === 'NEEDS_KEY' ? null : String(e));
      if (!cursor) setItems([]);
    } finally {
      if (my === seq.current) setLoading(false);
    }
  }

  // Search as you type (debounced); trending/popular when empty.
  useEffect(() => {
    const timer = setTimeout(() => search(null), query.trim() ? 450 : 0);
    return () => clearTimeout(timer);
  }, [query, provider, needsKey]);

  // Infinite scroll.
  useEffect(() => {
    if (!sentinel.current || !next) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting) && !loading) search(next);
    });
    io.observe(sentinel.current);
    return () => io.disconnect();
  }, [next, loading, items.length]);

  async function insertItem(item) {
    if (inserting) return;
    if (item.kind === 'video') {
      closeModal();
      await A.insert('video', '', { name: N_('Video'), content: { kind: 'video', src: item.url, autoplay: false } });
      return;
    }
    setInserting(item.id);
    try {
      let asset = await invoke('media_import', { url: item.url });
      // SVG files: the size their link said.
      if (!asset.width && item.width) asset = { ...asset, width: item.width, height: item.height };
      const alt = attribution(item);
      if (replaceId) {
        await apply({
          op: 'updateElements',
          slide: state.slideId,
          updates: [{ id: replaceId, patch: { content: { src: asset.name, naturalWidth: asset.width, naturalHeight: asset.height, alt } } }],
        });
      } else {
        const id = await A.insertAsset(asset, null, 0, { alt });
        if (credit && id) await addCredit(id, item);
      }
      closeModal();
    } catch (e) {
      toast(String(e), 'error');
    } finally {
      setInserting(null);
    }
  }

  const current = TABS.find((x) => x.id === tab);
  return html`<${Modal} title=${t('Media Library')} width=${880} onClose=${closeModal} class="media-modal">
    <div class="media-top">
      <div class="media-tabs">
        ${TABS.map(
          (x) => html`<button type="button" class=${cls('media-tab', tab === x.id && 'active')} onClick=${() => setTab(x.id)}>
            <${Icon} name=${x.icon} size=${16} />${t(x.label)}
          </button>`,
        )}
      </div>
      <div class="media-search">
        <${Icon} name="search" size=${16} />
        <input
          ref=${searchRef}
          placeholder=${tab === 'videos' ? t('Search YouTube or paste a YouTube/Vimeo link…') : tab === 'gifs' ? t('Search GIFs or paste a link to one…') : t('Search photos or paste a link to an image…')}
          value=${query}
          onInput=${(e) => setQuery(e.target.value)}
        />
      </div>
      <div class="media-providers">
        ${current.providers.map(
          ([id, label]) => html`<button type="button" class=${cls('chip', provider === id && 'active')} onClick=${() => setProvider(id)}>
            ${label}
          </button>`,
        )}
        ${tab === 'photos' && html`<span class="spacer" /><${Toggle} value=${credit} label=${t('Add photo credit')} onChange=${(v) => {
          setCredit(v);
          try {
            localStorage.setItem('cz-media-credit', v ? '1' : '0');
          } catch {
            /* ignore */
          }
        }} />`}
      </div>
    </div>
    <div class="media-body">
      ${needsKey && !isLink(query) && !SITE[provider] && html`<${KeyPrompt} name=${provider} onSaved=${loadKeys} />`}
      ${needsKey && !isLink(query) && SITE[provider] &&
      html`<${BrowseCard} provider=${provider} onKey=${provider !== 'vimeo' && !showKey ? () => setShowKey(true) : null} />
        ${showKey && html`<${KeyPrompt} name=${provider} onSaved=${loadKeys} />`}
        ${tab === 'videos' && html`<p class="muted small center">${t('Tip: you can also paste any YouTube or Vimeo link in the box above.')}</p>`}`}
      ${error && html`<div class="media-error"><${Icon} name="circle-alert" size=${16} />${t(error)}</div>`}
      ${preview &&
      html`<div class=${cls('video-preview', preview.kind !== 'video' && 'image')}>
        <img src=${preview.thumb} alt="" onError=${(e) => (e.currentTarget.style.visibility = 'hidden')} />
        <div>
          <b>${preview.title}</b>
          <p class="muted small">${previewSource(preview)}</p>
          <${Button}
            variant="primary"
            icon="plus"
            label=${preview.kind === 'video' ? t('Insert Video') : t('Insert Image')}
            disabled=${!!inserting}
            onClick=${() => insertItem(preview)}
          />
        </div>
      </div>`}
      ${(!needsKey || isLink(query)) && !preview && !items.length && !loading && !error &&
      html`<div class="media-empty">
        <${Icon} name=${current.icon} size=${36} />
        <p>${tab === 'videos' ? t('Search YouTube, or paste a YouTube or Vimeo link.') : t('Type to search, or paste a link.')}</p>
      </div>`}
      <div class=${cls('media-grid', tab === 'videos' && 'videos')}>
        ${items.map(
          (it) => html`<button type="button" key=${it.provider + it.id} class=${cls('media-item', inserting === it.id && 'busy')} onClick=${() => insertItem(it)} title=${it.title}>
            <img src=${it.thumb} alt=${it.title} loading="lazy" onError=${(e) => thumbFailed(e.currentTarget, it)} style=${it.width && it.height && tab !== 'videos' ? { aspectRatio: `${it.width} / ${it.height}` } : null} />
            ${tab === 'videos' && html`<span class="media-title">${it.title}</span>`}
            <span class="media-credit">${it.author ? `${it.author}` : it.license}</span>
            ${inserting === it.id && html`<span class="media-busy"><span class="spinner" /></span>`}
          </button>`,
        )}
      </div>
      <div ref=${sentinel} class="media-sentinel">${loading && html`<span class="spinner" />`}</div>
    </div>
    <div class="media-foot">
      ${provider === 'giphy' && html`<span class="powered">${withBold(t('Powered by {service}'), 'service', 'GIPHY')}</span>`}
      ${provider === 'pexels' && html`<span class="powered">${withBold(t('Powered by {service}'), 'service', 'Pexels', 'https://www.pexels.com/')}</span>`}
      ${provider === 'pixabay' && html`<span class="powered">${withBold(t('Powered by {service}'), 'service', 'Pixabay', 'https://pixabay.com/')}</span>`}
      ${provider?.startsWith('openverse') &&
      html`<span class="powered">${withBold(t('Openly licensed media via {service} — check each license'), 'service', 'Openverse')}</span>`}
      ${provider === 'wikimedia' &&
      html`<span class="powered">${withBold(t('Openly licensed media via {service} — check each license'), 'service', 'Wikimedia Commons')}</span>`}
    </div>
  <//>`;
}

/**
 * Some services' thumbnails fail (Openverse answers 424 for many GIFs): show
 * the full image instead, and drop the result if that fails too.
 */
function thumbFailed(img, it) {
  if (it.kind !== 'video' && it.url && img.src !== it.url) img.src = it.url;
  else img.closest('.media-item')?.remove();
}

const site = (url) => {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return '';
  }
};

/** Where a previewed link comes from, under its title. */
function previewSource(it) {
  if (it.provider === 'youtube' || it.provider === 'vimeo') return `${it.author} · ${it.provider === 'vimeo' ? 'Vimeo' : 'YouTube'}`;
  const size = it.width && it.height ? `${it.width} × ${it.height}` : '';
  const from = it.provider === 'wikimedia' ? [it.author, it.license, 'Wikimedia Commons'] : [site(it.pageUrl)];
  return [...from, size].filter(Boolean).join(' · ');
}

/** Credit line for the image's description (and optional caption). */
function attribution(it) {
  if (it.provider === 'pexels') return t('Photo by {author} on Pexels', { author: it.author });
  if (it.provider === 'pixabay') return t('Image by {author} from Pixabay', { author: it.author });
  if (it.provider === 'giphy') return it.title || t('GIF via GIPHY');
  if (it.provider === 'link') return t('Image from {site}', { site: site(it.pageUrl) });
  if (it.provider === 'openverse' || it.provider === 'wikimedia') {
    const { title, author } = it;
    const work = title
      ? author ? t('“{title}” by {author}', { title, author }) : t('“{title}”', { title })
      : author ? t('Image by {author}', { author }) : t('Image');
    return `${work}${it.license ? `, ${it.license}` : ''}`;
  }
  return it.title || '';
}

async function addCredit(imageId, it) {
  const s = currentSlide();
  const img = s?.elements.find((e) => e.id === imageId);
  if (!img) return;
  const d = deck();
  const label = escapeHtml(attribution(it));
  const link = it.pageUrl ? `<a href="${escapeHtml(it.pageUrl)}">${label}</a>` : label;
  await A.insert('text', 'caption', {
    x: img.x,
    y: Math.min(d.height - 30, img.y + img.h + 6),
    w: img.w,
    h: 26,
    content: { kind: 'text', html: `<p>${link}</p>`, style: { fontSize: 14, align: 'right' } },
  });
  set({ selection: [imageId] });
}

export function openMediaLibrary(tab = 'photos', replaceId = null) {
  set({ modal: { type: 'media', props: { tab, replaceId } }, menu: null });
}

// ---------------------------------------------------------------------------
// Items picked in a provider's website (browse mode)
// ---------------------------------------------------------------------------

export async function insertPicked(p) {
  if (!state.doc) return;
  if (p.kind === 'video') {
    await A.insert('video', '', { name: N_('Video'), content: { kind: 'video', src: p.url, autoplay: false } });
    return;
  }
  set({ busy: t('Adding GIF…') });
  try {
    const asset = await invoke('media_import', { url: p.url });
    await A.insertAsset(asset, null, 0, { alt: p.title || t('GIF via GIPHY') });
  } catch (e) {
    toast(String(e), 'error');
  } finally {
    set({ busy: null });
  }
}
