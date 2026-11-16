// Prepare a pair of the actual Show-rendered slides. This module never reads
// editor state or changes a document; it also runs in the isolated preview.

/** Only bundled and document assets may load in this noninteractive preview. */
export function localPreviewUrl(value, base = location.href) {
  try {
    const url = new URL(value, base);
    return ['data:', 'blob:', 'asset:'].includes(url.protocol) ||
      ['asset.localhost', 'tauri.localhost'].includes(url.hostname) || url.origin === new URL(base).origin;
  } catch { return false; }
}

/** Remove live content before the template is attached, not after playback starts. */
export function inertPreview(root, placeholder) {
  root.querySelectorAll('script, link, object, embed, base, meta, .notes').forEach((node) => node.remove());
  // Mermaid's self-contained SVG styles are part of the drawing. Removing
  // them would silently change the diagram in previews and raster exports.
  root.querySelectorAll('style').forEach((node) => {
    if (node.namespaceURI !== 'http://www.w3.org/2000/svg') node.remove();
  });
  root.querySelectorAll('iframe, video, audio, .cz-camera-box').forEach((node) => {
    const box = document.createElement('div');
    box.className = 'cz-preview-media';
    box.textContent = placeholder;
    node.replaceWith(box);
  });
  for (const node of root.querySelectorAll('*')) {
    for (const attr of [...node.attributes]) {
      const name = attr.name;
      if (name === 'href' || name === 'xlink:href') {
        const drawingReference = node.namespaceURI === 'http://www.w3.org/2000/svg' && ['use', 'image'].includes(node.localName);
        if (!drawingReference || !localPreviewUrl(attr.value)) node.removeAttribute(name);
      } else if (name.startsWith('on') || ['autoplay', 'srcset', 'data-autoslide', 'data-background-video', 'data-background-iframe', 'data-cz-bg-video', 'data-cz-bg-embed'].includes(name)) node.removeAttribute(name);
      else if (['src', 'data-src', 'data-background-image', 'data-background'].includes(name) && !localPreviewUrl(attr.value)) node.removeAttribute(name);
    }
  }
}

/** Preserve horizontal/vertical entry direction and both slides' transition attributes. */
export function previewPair(data, slideId, placeholder = '') {
  const index = data.slides.findIndex((s) => s.id === slideId);
  if (index < 0) throw new Error('The selected slide is not visible in the presentation.');
  const template = document.createElement('template');
  template.innerHTML = data.sections;
  const ids = data.slides.slice(Math.max(0, index - 1), index + 1);
  const fragment = document.createDocumentFragment();
  const vertical = ids.length === 2 && ids[0].h === ids[1].h;
  const wrapper = vertical ? fragment.appendChild(document.createElement('section')) : fragment;
  for (const item of ids) {
    const node = [...template.content.querySelectorAll('section[id]')].find((el) => el.id === item.id);
    if (!node) throw new Error('The rendered slide was not found.');
    wrapper.appendChild(node.cloneNode(true));
  }
  inertPreview(fragment, placeholder);
  return { fragment, hasPrevious: index > 0, target: { h: vertical || index === 0 ? 0 : 1, v: vertical ? 1 : 0 } };
}

/** Preserve the deck's transition configuration, disabling only autonomous interaction. */
export function previewConfig(config) {
  return { ...config, controls: false, progress: false, slideNumber: false,
    keyboard: false, touch: false, overview: false, help: false, hash: false,
    respondToHashChanges: false, history: false, autoSlide: 0, autoSlideStoppable: false,
    autoPlayMedia: false, embedded: true, hideInactiveCursor: false, pause: false,
    loop: false, shuffle: false, navigationMode: 'default' };
}
