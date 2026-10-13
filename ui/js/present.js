/*
 * Presentation window.
 *
 *  - show:    the audience view (fullscreen). Reports its state to the
 *             presenter display and obeys its commands.
 *  - preview: embedded in the presenter display; booted by the parent frame.
 *  - print:   reveal.js print-pdf layout, then the system print dialog.
 */
(function () {
  'use strict';

  window.addEventListener('error', function (e) {
    if (window.__TAURI__) window.__TAURI__.core.invoke('ui_log', { level: 'error', message: e.message + ' @ ' + e.filename + ':' + e.lineno }).catch(function () {});
  });

  var params = new URLSearchParams(location.search);
  var mode = params.has('print-pdf') ? 'print' : params.has('preview') ? 'preview' : 'show';
  var docLabel = params.get('doc') || '';
  var tauri = window.__TAURI__;

  // Interface translation (replaced by i18n.js once the language is known).
  var t = function (s, v) { return v ? s.replace(/\{(\w+)\}/g, function (m, k) { return k in v ? v[k] : m; }) : s; };

  /** Loads the interface language for this window's own texts; the slides keep the page's lang and direction. */
  function loadLanguage(code) {
    var root = document.documentElement;
    var lang = root.getAttribute('lang');
    var dir = root.getAttribute('dir');
    return import('./i18n.js').then(function (m) {
      return m.setLanguage(code).then(function () { t = m.t; });
    }).catch(function () {}).then(function () {
      if (lang === null) root.removeAttribute('lang'); else root.setAttribute('lang', lang);
      if (dir === null) root.removeAttribute('dir'); else root.setAttribute('dir', dir);
    });
  }

  function loadScript(src) {
    return new Promise(function (resolve, reject) {
      var s = document.createElement('script');
      s.src = src;
      s.onload = resolve;
      s.onerror = reject;
      document.head.appendChild(s);
    });
  }

  function addStylesheet(href) {
    var l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = href;
    document.head.appendChild(l);
    return new Promise(function (resolve) {
      l.onload = resolve;
      l.onerror = resolve;
    });
  }

  function emitState(extra) {
    if (!tauri || mode !== 'show' || !window.Reveal.isReady()) return;
    var slide = Reveal.getCurrentSlide();
    var payload = {
      doc: docLabel,
      state: Reveal.getState(),
      indices: Reveal.getIndices(),
      slideId: slide ? slide.id : null,
      index: Reveal.getSlidePastCount(),
      total: Reveal.getTotalSlides(),
      isLast: Reveal.isLastSlide() && !Reveal.availableFragments().next,
      paused: Reveal.isPaused(),
    };
    for (var k in extra || {}) payload[k] = extra[k];
    tauri.event.emit('cz-present-state', payload);
  }

  function endShow() {
    var slide = window.Reveal && Reveal.isReady() ? Reveal.getCurrentSlide() : null;
    if (tauri) {
      tauri.core.invoke('present_stop', { slide: slide ? slide.id : null });
    } else {
      window.close();
    }
  }

  function handleCommand(cmd) {
    if (!cmd || !Reveal.isReady()) return;
    switch (cmd.cmd) {
      case 'next': Reveal.next(); break;
      case 'prev': Reveal.prev(); break;
      case 'first': Reveal.slide(0, 0, -1); break;
      case 'last': Reveal.slide(Number.MAX_SAFE_INTEGER, 0); break;
      case 'goto': Reveal.slide(cmd.h, cmd.v || 0, cmd.f); break;
      case 'pause': Reveal.togglePause(); break;
      case 'overview': Reveal.toggleOverview(); break;
      case 'end': endShow(); break;
      case 'sync': emitState(); break;
    }
  }

  /** Boots reveal.js with presentation data from Rust. */
  function boot(data, opts) {
    opts = opts || {};
    document.title = data.title || t('Presentation');
    if (data.themeCss) document.getElementById('cz-theme').href = data.themeCss;
    else document.getElementById('cz-theme').removeAttribute('href');
    document.getElementById('cz-deck-css').textContent = data.deckCss;
    var waits = [];
    if (data.needs.code) {
      waits.push(addStylesheet('vendor/reveal/plugin/highlight/' + data.codeTheme + '.css'));
      waits.push(loadScript('vendor/reveal/plugin/highlight.js'));
    }
    if (data.needs.math) {
      waits.push(addStylesheet('vendor/katex/katex.min.css'));
      waits.push(loadScript('vendor/katex/katex.min.js'));
    }
    document.querySelector('.reveal .slides').innerHTML = data.sections;

    var cfg = {};
    for (var k in data.config) cfg[k] = data.config[k];
    if (mode === 'preview') {
      document.documentElement.classList.add('cz-preview');
      cfg.controls = false;
      cfg.progress = false;
      cfg.slideNumber = false;
      cfg.keyboard = false;
      cfg.touch = false;
      cfg.overview = false;
      cfg.autoSlide = 0;
      cfg.help = false;
      cfg.transition = 'none';
      cfg.backgroundTransition = 'none';
      cfg.autoAnimate = false;
      cfg.autoPlayMedia = false;
      cfg.hideInactiveCursor = false;
      cfg.previewLinks = false;
    } else if (mode === 'show') {
      cfg.keyboard = { 27: endShow, 81: endShow };
      cfg.help = true;
    } else {
      cfg.controls = false;
    }

    return Promise.all(waits).then(function () {
      return window.CadenzaRuntime.start(cfg, { notes: false });
    }).then(function () {
      var loading = document.getElementById('cz-loading');
      if (loading) loading.remove();
      if (opts.h !== undefined) Reveal.slide(opts.h, opts.v || 0);
      return Reveal;
    });
  }

  function bootShow() {
    tauri.core.invoke('present_data').then(function (data) {
      var h = parseInt(params.get('h') || '0', 10) || 0;
      var v = parseInt(params.get('v') || '0', 10) || 0;
      return loadLanguage(data.language).then(function () { return boot(data, { h: h, v: v }); });
    }).then(function () {
      ['slidechanged', 'fragmentshown', 'fragmenthidden', 'paused', 'resumed', 'overviewshown', 'overviewhidden'].forEach(function (ev) {
        Reveal.on(ev, function () { emitState(); });
      });
      tauri.event.listen('cz-present-cmd', function (e) {
        if (e.payload && e.payload.doc === docLabel) handleCommand(e.payload);
      });
      // Keynote-style: a click anywhere advances, except on interactive content.
      document.addEventListener('click', function (e) {
        if (e.button !== 0 || Reveal.isOverview()) return;
        if (e.target.closest('a, button, iframe, video, audio, input, textarea, select, .controls, .progress')) return;
        Reveal.next();
      });
      document.addEventListener('contextmenu', function (e) {
        e.preventDefault();
        Reveal.prev();
      });
      emitState({ started: true });
    }).catch(function (e) {
      var message = t('Could not start the presentation: {error}', { error: t(String(e)) });
      document.body.innerHTML = '<p style="color:#eee;font:16px system-ui;padding:2em">' + message + '</p>';
    });
  }

  function bootPrint() {
    tauri.core.invoke('present_data').then(function (data) {
      return loadLanguage(data.language).then(function () { return boot(data); });
    }).then(function () {
      var bar = document.createElement('div');
      bar.className = 'cz-print-bar';
      bar.innerHTML = '<span>' + t('Choose “Print to File” (PDF) or “Save as PDF” in the dialog.') + '</span>' +
        '<button id="cz-print">' + t('Print / Save PDF…') + '</button><button class="secondary" id="cz-close">' + t('Close') + '</button>';
      document.body.appendChild(bar);
      document.getElementById('cz-print').onclick = function () { window.print(); };
      document.getElementById('cz-close').onclick = function () { tauri.window.getCurrentWindow().close(); };
      var printed = false;
      var go = function () {
        if (printed) return;
        printed = true;
        setTimeout(function () { window.print(); }, 400);
      };
      Reveal.on('pdf-ready', go);
      setTimeout(go, 2500);
    });
  }

  window.CadenzaPresent = { boot: boot, handleCommand: handleCommand };

  if (mode === 'show') bootShow();
  else if (mode === 'print') bootPrint();
})();
