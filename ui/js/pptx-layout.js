import { N_ } from './i18n.js';
// Read computed presentation styles from the immutable Rust-rendered snapshot.
// The isolated frame has no scripts or media. It is a measuring surface, not
// an alternate slide renderer or a source of raster export images.

export function checkAbort(signal) {
  if (signal?.aborted) throw signal.reason || new DOMException(N_('Export cancelled'), 'AbortError');
}

export function bounded(promise, signal, timeout = 15000, message = N_('Presentation resources did not finish loading.')) {
  return new Promise((resolve, reject) => {
    let timer;
    const finish = (fn, value) => { clearTimeout(timer); signal?.removeEventListener('abort', cancel); fn(value); };
    const cancel = () => finish(reject, signal.reason || new DOMException(N_('Export cancelled'), 'AbortError'));
    if (signal?.aborted) { cancel(); return; }
    signal?.addEventListener('abort', cancel, { once: true });
    timer = setTimeout(() => finish(reject, new Error(message)), timeout);
    Promise.resolve(promise).then(value => finish(resolve, value), error => finish(reject, error));
  });
}

/** Browser-resolved colors include CSS variables, named colors and alpha. */
export function color(value, fallback = '000000') {
  const s = String(value || '').trim();
  const hex = /^#([\da-f]{3,8})$/i.exec(s);
  if (hex) {
    let h = hex[1];
    if (h.length === 3 || h.length === 4) h = [...h].map(c => c + c).join('');
    if (h.length === 6 || h.length === 8) return { color: h.slice(0,6).toUpperCase(), transparency: h.length === 8 ? (1-parseInt(h.slice(6),16)/255)*100 : 0 };
  }
  const rgb = /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+))?\s*\)$/i.exec(s);
  if (rgb) return { color: rgb.slice(1,4).map(n => Math.max(0,Math.min(255,Math.round(Number(n)))).toString(16).padStart(2,'0')).join('').toUpperCase(), transparency: (1-Number(rgb[4] ?? 1))*100 };
  if (s === 'transparent' || s === 'none') return { color: fallback, transparency: 100 };
  const srgb=/^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+))?\s*\)$/i.exec(s);
  if(srgb)return {color:srgb.slice(1,4).map(v=>Math.max(0,Math.min(255,Math.round(Number(v)*255))).toString(16).padStart(2,'0')).join('').toUpperCase(),transparency:(1-Number(srgb[4]??1))*100};
  return { color: fallback, transparency: 0 };
}

export const px = value => parseFloat(value) || 0;
export const fontFace = value => String(value || 'Arial').split(',')[0].trim().replace(/^['"]|['"]$/g, '') || 'Arial';

async function loadStylesheet(doc, href, signal) {
  const link = doc.createElement('link');
  link.rel = 'stylesheet';
  let resolve, reject, poll;
  const ready = new Promise((yes, no) => { resolve=yes; reject=no; });
  const failed = () => reject(new Error(N_('A presentation stylesheet could not be loaded.')));
  // WebKitGTK suppresses resource-event callbacks in a script-disabled frame,
  // including listeners registered by its parent. Its CSSOM is still available.
  // Check actual parsed CSS instead of treating a delay as successful loading.
  const parsed = () => {
    if (!link.sheet) return;
    // Every bundled presentation sheet contains rules. WebKit can expose an
    // empty CSSStyleSheet even for a failed custom-protocol response.
    try { if (link.sheet.cssRules.length>0) resolve(); }
    catch { failed(); }
  };
  link.addEventListener('load',parsed);
  link.addEventListener('error',failed);
  link.href = href;
  doc.head.appendChild(link);
  poll = setInterval(parsed,25);
  parsed();
  try {
    await bounded(ready,signal,15000,N_('The presentation stylesheets did not finish loading for PowerPoint export.'));
  } finally {
    clearInterval(poll);
    link.removeEventListener('load',parsed);
    link.removeEventListener('error',failed);
  }
}

/** An inert, disposable frame keeps theme styles independent of the live editor. */
export async function measureSnapshot(snapshot, signal) {
  checkAbort(signal);
  const frame = document.createElement('iframe');
  frame.setAttribute('aria-hidden', 'true');
  frame.setAttribute('sandbox', 'allow-same-origin');
  frame.style.cssText = `position:fixed;left:-100000px;top:0;width:${snapshot.width}px;height:${snapshot.height}px;border:0;pointer-events:none;`;
  const loaded = new Promise(resolve => { frame.onload = resolve; });
  // A static bundled frame avoids WebKit srcdoc/custom-scheme load-event differences.
  frame.src = new URL('../pptx-measure.html',import.meta.url).href;
  document.body.appendChild(frame);
  try {
    await bounded(loaded, signal,15000,N_('The PowerPoint measuring frame did not finish loading.'));
    const doc = frame.contentDocument;
    const sheets = ['../vendor/reveal/reset.css', '../vendor/reveal/reveal.css', '../css/content.css', '../css/editor-slide.css'].map(p => new URL(p, import.meta.url).href);
    if (snapshot.themeCss) sheets.splice(2, 0, new URL(snapshot.themeCss, new URL('../', import.meta.url)).href);
    const stylesheetAbort = new AbortController();
    const cancelStylesheets = () => stylesheetAbort.abort(signal.reason);
    signal?.addEventListener('abort',cancelStylesheets,{once:true});
    if(signal?.aborted)cancelStylesheets();
    try { await Promise.all(sheets.map(href => loadStylesheet(doc,href,stylesheetAbort.signal))); }
    catch(error) { stylesheetAbort.abort(error); throw error; }
    finally { signal?.removeEventListener('abort',cancelStylesheets); }
    const css = doc.createElement('style'); css.textContent = snapshot.deckCss || ''; doc.head.appendChild(css);
    const layout = doc.createElement('style');
    layout.textContent = `html,body{margin:0;width:${snapshot.width}px;height:${snapshot.height}px}.reveal{width:100%;height:100%}.reveal .slides{position:relative;inset:0;width:100%;height:100%;margin:0;transform:none!important;perspective:none;text-align:left}.reveal .slides section{position:absolute;inset:0;display:block!important;opacity:1!important;visibility:visible!important;transform:none!important;padding:0!important;width:100%;height:100%}.cz-slide{position:relative;width:100%;height:100%}.fragment{opacity:1!important;visibility:visible!important}`;
    doc.head.appendChild(layout);
    const reveal = doc.createElement('div'); reveal.className = 'reveal reveal-viewport cz-editor-reveal';
    const slides = reveal.appendChild(doc.createElement('div')); slides.className = 'slides'; doc.body.appendChild(reveal);
    return {
      async slide(item) {
        checkAbort(signal);
        const section = doc.createElement('section'); section.className = item.classes || '';
        const host = section.appendChild(doc.createElement('div')); host.className = 'cz-slide'; host.innerHTML = item.html;
        host.querySelectorAll('script,iframe,video,audio,object,embed,link,base,meta').forEach(node => node.remove());
        for (const node of host.querySelectorAll('*')) for (const attr of [...node.attributes]) {
          if (attr.name.startsWith('on') || ['src','srcset','data-src'].includes(attr.name)) node.removeAttribute(attr.name);
        }
        slides.replaceChildren(section);
        section.getBoundingClientRect();
        await bounded(doc.fonts.ready, signal,15000,N_('The presentation fonts did not finish loading for PowerPoint export.'));
        return { root: section, style: node => frame.contentWindow.getComputedStyle(node) };
      },
      dispose() { frame.remove(); },
    };
  } catch (error) { frame.remove(); throw error; }
}

const SUPPORTED = new Set(['P','DIV','SPAN','STRONG','B','EM','I','U','S','STRIKE','SUB','SUP','BR','A','UL','OL','LI']);
const BULLETS = { disc:'2022',circle:'25CB',square:'25AA' };
const NUMBERING = { decimal:'arabicPeriod','lower-alpha':'alphaLcPeriod','upper-alpha':'alphaUcPeriod','lower-roman':'romanLcPeriod','upper-roman':'romanUcPeriod' };

function nativeTextStyle(node, root, cs) {
  if (['textShadow','filter','transform'].some(key=>cs[key] && cs[key]!=='none')) return false;
  if (cs.fontVariantCaps && cs.fontVariantCaps!=='normal') return false;
  if (cs.fontStretch && !['normal','100%'].includes(cs.fontStretch)) return false;
  if (![400,700].includes(Number(cs.fontWeight)) || !['normal','italic'].includes(cs.fontStyle)) return false;
  if (!['none','uppercase','lowercase','capitalize'].includes(cs.textTransform) || px(cs.wordSpacing)) return false;
  if (cs.writingMode && cs.writingMode!=='horizontal-tb') return false;
  if (cs.textDecorationLine!=='none' && (cs.textDecorationLine.includes('overline') || cs.textDecorationStyle!=='solid' || cs.textDecorationColor!==cs.color)) return false;
  if (px(cs.textIndent) || (['P','DIV','LI'].includes(node.tagName) && (px(cs.marginLeft)||px(cs.marginRight)))) return false;
  if (node!==root && (color(cs.backgroundColor).transparency<100 || Number(cs.opacity)!==1 || !['baseline','sub','super'].includes(cs.verticalAlign))) return false;
  if (node.tagName==='UL' && !BULLETS[cs.listStyleType]) return false;
  if (node.tagName==='OL' && !NUMBERING[cs.listStyleType]) return false;
  return true;
}

/** Rich text is mapped only when every node has a corresponding native run. */
export function textRuns(node, styleOf, scale = 1, slideNumbers = new Map()) {
  if (!node) return null;
  for (const child of node.querySelectorAll('*')) {
    if (!SUPPORTED.has(child.tagName) || child.classList.contains('cz-math-inline') || child.classList.contains('cz-math-display')) return null;
  }
  for (const child of [node,...node.querySelectorAll('*')]) if(!nativeTextStyle(child,node,styleOf(child)))return null;
  const runs = [];
  const rootStyle = styleOf(node);
  function walk(current, list = null, depth = 0) {
    if (current.nodeType === 3) {
      let text = current.textContent.replace(/\s+/g, ' ');
      if (!text) return;
      const parent = current.parentElement, cs = styleOf(parent);
      if (cs.textTransform === 'uppercase') text = text.toUpperCase();
      if (cs.textTransform === 'lowercase') text = text.toLowerCase();
      if (cs.textTransform === 'capitalize') text = text.replace(/(^|\s)(\S)/g, (_, a,b) => a+b.toUpperCase());
      const paint = color(cs.color);
      const options = { fontFace: fontFace(cs.fontFamily), fontSize: px(cs.fontSize)*scale*.75,
        bold: Number(cs.fontWeight) >= 600, italic: cs.fontStyle === 'italic', color: paint.color,
        transparency: paint.transparency, underline: { style:cs.textDecorationLine.includes('underline') || parent.closest('u') ? 'sng' : 'none' },
        strike: cs.textDecorationLine.includes('line-through') || parent.closest('s,strike') ? 'sngStrike' : undefined,
        charSpacing:px(cs.letterSpacing)*scale*.75,
        subscript: !!parent.closest('sub') || cs.verticalAlign==='sub', superscript: !!parent.closest('sup') || cs.verticalAlign==='super' };
      const anchor = parent.closest('a[href]');
      if (anchor) {
        const url = anchor.getAttribute('href');
        if (/^(https?:|mailto:)/i.test(url)) options.hyperlink = { url };
        else if (url.startsWith('#/') && slideNumbers.has(url.slice(2))) options.hyperlink = { slide: slideNumbers.get(url.slice(2)) };
      }
      const paragraph = parent.closest('p,div,li') || node;
      const ps = styleOf(paragraph);
      options.align = ['center','right','justify'].includes(ps.textAlign) ? ps.textAlign : 'left';
      options.paraSpaceAfter = px(ps.marginBottom)*scale*.75;
      options.paraSpaceBefore = px(ps.marginTop)*scale*.75;
      if (list) {
        const ls=styleOf(list), li=parent.closest('li');
        options.bullet = list.tagName==='OL' ? {type:'number',style:NUMBERING[ls.listStyleType],numberStartAt:li?.value||((list.start||1)+[...list.children].indexOf(li))} : {characterCode:BULLETS[ls.listStyleType]};
        options.bullet.indent=px(ls.paddingLeft)*scale*.75 || px(cs.fontSize)*scale*.75;
        options.indentLevel=Math.max(0,depth-1);
      }
      runs.push({ text, options }); return;
    }
    if (current.nodeType !== 1) return;
    if (current.tagName === 'BR') { if (runs.length) runs[runs.length-1].options.breakLine = true; else runs.push({ text:'', options:{breakLine:true} }); return; }
    const nextList = ['UL','OL'].includes(current.tagName) ? current : list;
    const nextDepth = ['UL','OL'].includes(current.tagName) ? depth+1 : depth;
    for (const child of current.childNodes) walk(child, nextList, nextDepth);
    if (['P','DIV','LI'].includes(current.tagName) && current !== node && runs.length) runs[runs.length-1].options.breakLine = true;
  }
  walk(node);
  if (runs.length) runs[runs.length-1].options.breakLine = false;
  return { runs, fontFace: fontFace(rootStyle.fontFamily), fontSize: px(rootStyle.fontSize)*scale*.75,
    color: color(rootStyle.color), lineSpacing: px(rootStyle.lineHeight)*scale*.75,
    charSpacing: px(rootStyle.letterSpacing)*scale*.75, rtlMode: rootStyle.direction === 'rtl' };
}
