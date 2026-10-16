// Updates: when Cadenza starts (and from Check for Updates…), it looks for a
// newer version on GitHub and, if the person agrees, downloads and installs
// it, then restarts. Rust does the work (src-tauri/src/updates.rs).

import { html, useEffect, useRef, useState } from './lib.js';
import { state, openModal, closeModal, updateSettings, flushEdits } from './store.js';
import { Modal, Button } from './ui.js';
import { Logo } from './welcome.js';
import { invoke, opener, currentWindow } from './api.js';
import { t, language } from './i18n.js';

const RELEASES = 'https://github.com/rodolfo-viana/cadenza/releases';

/**
 * Looks for an update. At startup it says nothing unless one is found;
 * asked for, it opens the dialog, which shows what it finds.
 */
export async function checkForUpdates({ manual = false } = {}) {
  if (manual) return openModal('update', {});
  const info = await invoke('update_check', { manual: false }).catch(() => null);
  if (!info) return;
  // Not over another dialog: after it.
  while (state.modal || state.busy) await new Promise((r) => setTimeout(r, 1000));
  openModal('update', { info });
}

const mb = (bytes) => new Intl.NumberFormat(language(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1048576);

export function UpdateDialog({ info: found = null }) {
  const [info, setInfo] = useState(found);
  // checking → available | latest → downloading → installing; or failed.
  const [phase, setPhase] = useState(found ? 'available' : 'checking');
  const [failure, setFailure] = useState(null);
  const [progress, setProgress] = useState(null);
  const open = useRef(true);
  useEffect(() => () => (open.current = false), []);

  const fail = (title, hint, e) => {
    if (!open.current) return;
    setFailure({ title, hint, detail: t(String(e)) });
    setPhase('failed');
  };

  useEffect(() => {
    if (phase !== 'checking') return;
    invoke('update_check', { manual: true })
      .then((update) => {
        if (!open.current) return;
        setInfo(update);
        setPhase(update ? 'available' : 'latest');
      })
      .catch((e) => fail(t('Could not check for updates'), t('Check your internet connection and try again.'), e));
  }, [phase]);

  useEffect(() => {
    let off = null;
    currentWindow()
      .listen('cz-update-progress', (e) => open.current && setProgress(e.payload))
      .then((f) => (open.current ? (off = f) : f()))
      .catch(() => {});
    return () => off?.();
  }, []);

  const install = async () => {
    setProgress(null);
    setPhase('downloading');
    try {
      await invoke('update_download');
    } catch (e) {
      if (String(e) !== 'Cancelled') fail(t('Could not download the update'), t('You can download the new version from the website instead.'), e);
      return;
    }
    if (!open.current) return;
    setPhase('installing');
    // Text being typed is part of what reopens after the restart.
    await flushEdits().catch(() => {});
    try {
      await invoke('update_install');
    } catch (e) {
      fail(t('Could not install the update'), t('You can download the new version from the website instead.'), e);
    }
  };

  const skip = () => {
    updateSettings({ skippedUpdate: info.version });
    closeModal();
  };
  const cancel = () => {
    invoke('update_cancel').catch(() => {});
    closeModal();
  };
  const link = (url, label) => html`<a href="#" onClick=${(e) => (e.preventDefault(), opener.openUrl(url))}>${label}</a>`;
  const alert = (title, body) => html`<div class="alert update">
    <${Logo} size=${44} />
    <div>
      <h3>${title}</h3>
      ${body}
    </div>
  </div>`;

  const done = progress?.done || 0;
  const total = progress?.total || 0;
  const views = {
    checking: () => ({
      body: html`<div class="update-status"><div class="spinner" />${t('Checking for updates…')}</div>`,
      footer: html`<span class="spacer" /><${Button} label=${t('Cancel')} onClick=${closeModal} />`,
    }),
    latest: () => ({
      body: alert(t('Cadenza is up to date'), html`<p class="muted">${t('You have the latest version, {version}.', { version: state.info.version })}</p>`),
      footer: html`<span class="spacer" /><${Button} label=${t('OK')} variant="primary" onClick=${closeModal} />`,
    }),
    available: () => ({
      body: alert(
        t('Cadenza {version} is available', { version: info.version }),
        html`<p class="muted">${t('You have version {current}.', { current: info.current })}</p>
          ${info.summary && html`<p>${info.summary}</p>`}
          <p>${link(info.url, t('What’s new'))}</p>
          <p class="ihint">${t('Cadenza restarts to finish. The presentations you have open reopen afterwards, unsaved changes included.')}</p>`,
      ),
      footer: html`<${Button} label=${t('Skip This Version')} variant="ghost" onClick=${skip} />
        <span class="spacer" />
        <${Button} label=${t('Not Now')} onClick=${closeModal} />
        <${Button} label=${t('Download and Install')} icon="download" variant="primary" onClick=${install} />`,
    }),
    downloading: () => ({
      body: alert(
        t('Downloading Cadenza {version}…', { version: info.version }),
        html`<div class="update-bar"><div style=${{ width: total ? `${Math.min(100, (done / total) * 100)}%` : '0%' }} /></div>
          <p class="muted small">${total ? t('{done} of {total} MB', { done: mb(done), total: mb(total) }) : t('{done} MB', { done: mb(done) })}</p>`,
      ),
      footer: html`<span class="spacer" /><${Button} label=${t('Cancel')} onClick=${cancel} />`,
    }),
    installing: () => ({
      body: alert(
        t('Installing Cadenza {version}…', { version: info.version }),
        html`<div class="update-status"><div class="spinner" />${t('Cadenza restarts when it’s done.')}</div>
          ${['deb', 'rpm'].includes(info.package) && html`<p class="ihint">${t('Your system asks for your password to install it.')}</p>`}
          ${info.package === 'msi' && html`<p class="ihint">${t('Windows may ask for permission to install it.')}</p>`}`,
      ),
      footer: null,
    }),
    failed: () => ({
      body: alert(failure.title, html`<p class="muted">${failure.hint}</p><p class="update-detail">${failure.detail}</p>`),
      footer: html`<${Button} label=${t('Open Download Page')} variant="ghost" onClick=${() => opener.openUrl(info?.url || RELEASES)} />
        <span class="spacer" />
        <${Button} label=${t('Close')} variant="primary" onClick=${closeModal} />`,
    }),
  };
  const view = views[phase]();
  // Nothing stops an installation under way.
  const onClose = phase === 'installing' ? null : phase === 'downloading' ? cancel : closeModal;
  return html`<${Modal} width=${480} onClose=${onClose} footer=${view.footer}>${view.body}<//>`;
}
