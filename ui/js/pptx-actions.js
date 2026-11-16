import { state, set, toast } from './store.js';
import { invoke, dialog } from './api.js';
import { t } from './i18n.js';

/** Import opens a new, dirty document; the current presentation stays intact. */
export async function importPowerPoint() {
  const docId = state.doc?.docId ?? null;
  let importing = false;
  try {
    const path = await dialog.open({ title: t('Import PowerPoint'), multiple: false, filters: [{ name: 'PowerPoint', extensions: ['pptx'] }] });
    if (!path || (state.doc?.docId ?? null) !== docId) return;
    importing = true;
    set({ busy: t('Importing PowerPoint…') });
    await invoke('import_pptx', { path, docId });
  }
  catch (error) { toast(t(String(error)), 'error'); }
  finally { if (importing) set({ busy: null }); }
}
