import { previewPair, previewConfig, localPreviewUrl } from './transition-preview.js';

// Each replay owns a fresh frame. Removing it cancels Reveal's timers, observers,
// listeners and pending assets together; no presentation events or native IPC.
window.CadenzaTransitionPreview = {
  async load(data, slideId, placeholder) {
    const pair = previewPair(data, slideId, placeholder);
    const theme = document.getElementById('cz-theme');
    const waitTheme = data.themeCss && localPreviewUrl(data.themeCss) ? new Promise((resolve) => {
      theme.onload = resolve; theme.onerror = resolve; theme.href = data.themeCss;
    }) : Promise.resolve();
    document.getElementById('cz-deck-css').textContent = data.deckCss || '';
    document.querySelector('.slides').appendChild(pair.fragment);
    await waitTheme;
    const reveal = await window.CadenzaRuntime.start(previewConfig(data.config), { camera: false, notes: false });
    // Outgoing objects have finished all builds and paths; the incoming slide
    // starts before its first build, just as in the audience view.
    document.documentElement.classList.add('cz-preview-setup');
    reveal.slide(0, 0, Number.MAX_SAFE_INTEGER);
    await document.fonts.ready;
    // Force final path geometry before enabling transitions. A short timer also
    // settles Reveal in WebViews where animation frames pause for inactive tabs.
    document.querySelector('.slides').getBoundingClientRect();
    await new Promise((resolve) => setTimeout(resolve, 50));
    document.documentElement.classList.remove('cz-preview-setup');
    if (pair.hasPrevious) {
      await new Promise((resolve) => setTimeout(resolve, 200));
      reveal.slide(pair.target.h, pair.target.v, -1);
    }
    return pair.hasPrevious;
  },
};
