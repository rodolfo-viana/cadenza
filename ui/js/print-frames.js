// Injected into the Windows print window's frames, including external pages.
// Chromium omits an iframe's document canvas background when printing, even
// with background printing enabled. Paint that background as an ordinary box
// and preserve the page's colors. Text and other content remain vector output.
(function () {
  if (window === window.top) return;
  var layer = null;
  var style = null;
  function clear() {
    if (layer) layer.remove();
    if (style) style.remove();
    layer = style = null;
  }
  window.addEventListener('beforeprint', function () {
    clear();
    var root = document.documentElement;
    if (!root || !document.body) return;
    var background = getComputedStyle(root);
    // With a transparent root, the body supplies the document canvas.
    if (background.backgroundImage === 'none' &&
        /^(transparent|rgba\([^)]*,\s*0\)|.*\/\s*0\))$/.test(background.backgroundColor)) {
      background = getComputedStyle(document.body);
    }
    var paint = background.background;
    style = document.createElement('style');
    style.textContent = '@media print {html {isolation:isolate!important} ' +
      '*,*::before,*::after {-webkit-print-color-adjust:exact!important;print-color-adjust:exact!important}}';
    root.appendChild(style);
    layer = document.createElement('div');
    layer.setAttribute('aria-hidden', 'true');
    layer.style.cssText = 'all:initial!important;position:fixed!important;inset:0!important;z-index:-2147483648!important;' +
      'pointer-events:none!important;-webkit-print-color-adjust:exact!important;print-color-adjust:exact!important;' +
      'background:' + paint + '!important;';
    root.appendChild(layer);
  });
  window.addEventListener('afterprint', clear);
})();
