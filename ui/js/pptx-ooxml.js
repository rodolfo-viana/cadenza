import { checkAbort } from './pptx-layout.js';

/**
 * PptxGenJS 4.0.1 writes one a:pPr for every rich-text run. DrawingML allows
 * one, before all runs. Keep the first run's paragraph properties (all runs
 * in an authored paragraph share them). This narrow compatibility pass also
 * covers table cells, without replacing the pinned library or its renderer.
 */
export async function normalizeParagraphs(bytes, signal) {
  const zip = await window.JSZip.loadAsync(bytes);
  for (const name of Object.keys(zip.files)) {
    checkAbort(signal);
    if (!/^ppt\/slides\/slide\d+\.xml$/.test(name)) continue;
    const source = await zip.file(name).async('string');
    const doc = new DOMParser().parseFromString(source, 'application/xml');
    let changed = false;
    for (const paragraph of doc.getElementsByTagNameNS('http://schemas.openxmlformats.org/drawingml/2006/main', 'p')) {
      const properties = [...paragraph.children].filter(n => n.localName === 'pPr');
      for (const extra of properties.slice(1)) { extra.remove(); changed = true; }
    }
    if (changed) zip.file(name, new XMLSerializer().serializeToString(doc));
  }
  checkAbort(signal);
  return zip.generateAsync({ type:'arraybuffer', compression:'DEFLATE' });
}
