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
    // New presentations are titled in English by Rust.
    document.title = (data.title === 'Untitled Presentation' ? t('Untitled Presentation') : data.title) || t('Presentation');
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
    // A printout is read later: live video has no place in it.
    if (mode === 'print') {
      var cameras = document.querySelectorAll('.reveal .cz-el.cz-camera');
      for (var i = 0; i < cameras.length; i++) cameras[i].remove();
    }

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
      // X: with one screen, bring the presenter display to the front.
      cfg.keyboard = { 27: endShow, 81: endShow, 88: function () { tauri.core.invoke('present_swap').catch(function () {}); } };
      cfg.help = true;
    } else {
      cfg.controls = false;
    }

    return Promise.all(waits).then(function () {
      return window.CadenzaRuntime.start(cfg, { notes: false, camera: mode === 'show' });
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
      // With the camera on, cameras have names: the editor lists them (it
      // never uses the camera itself).
      window.addEventListener('cz-camera-on', function () {
        navigator.mediaDevices.enumerateDevices().then(function (list) {
          var cams = list.filter(function (d) { return d.kind === 'videoinput' && d.label; })
            .map(function (d) { return { id: d.deviceId, label: d.label }; });
          if (cams.length) localStorage.setItem('cz-cameras', JSON.stringify(cams));
        }).catch(function () {});
      });
      emitState({ started: true });
    }).catch(function (e) {
      var message = t('Could not start the presentation: {error}', { error: t(String(e)) });
      document.body.innerHTML = '<p style="color:#eee;font:16px system-ui;padding:2em">' + message + '</p>';
    });
  }

  function bootPrint() {
    var size = null;
    var title = '';
    tauri.core.invoke('present_data').then(function (data) {
      size = { width: data.width, height: data.height };
      title = data.title;
      return loadLanguage(data.language).then(function () { return boot(data); });
    }).then(function () {
      return Promise.all([stillVideos(), stillEmbeds()]);
    }).then(function () {
      var bar = document.createElement('div');
      bar.className = 'cz-print-bar';
      // Windows' print dialog puts slides on the printer's paper, with the
      // browser's headers: Cadenza saves PDFs itself there, a slide per page.
      var windows = /Windows/.test(navigator.userAgent);
      bar.innerHTML = windows
        ? '<span id="cz-print-msg">' + t('Save the slides as a PDF, a slide per page, or print them on paper.') + '</span>' +
          '<button id="cz-pdf">' + t('Save PDF…') + '</button><button class="secondary" id="cz-print">' + t('Print…') + '</button>' +
          '<button class="secondary" id="cz-close">' + t('Close') + '</button>'
        : '<span>' + t('Choose “Print to File” (PDF) or “Save as PDF” in the dialog.') + '</span>' +
          '<button id="cz-print">' + t('Print / Save PDF…') + '</button><button class="secondary" id="cz-close">' + t('Close') + '</button>';
      document.body.appendChild(bar);
      // Linux prints through Cadenza, which sets pages the size of a slide.
      var print = function () {
        tauri.core.invoke('print_slides', size)
          .then(function (done) { if (!done) window.print(); })
          .catch(function () { window.print(); });
      };
      document.getElementById('cz-print').onclick = print;
      document.getElementById('cz-close').onclick = function () { tauri.window.getCurrentWindow().close(); };
      if (windows) {
        document.getElementById('cz-pdf').onclick = function () { savePdf(size, title); };
        return;
      }
      var printed = false;
      var go = function () {
        if (printed) return;
        printed = true;
        setTimeout(print, 400);
      };
      Reveal.on('pdf-ready', go);
      setTimeout(go, 2500);
    });
  }

  /**
   * Printouts don't show video frames (they came out black): each video
   * becomes a picture of its first frame. Waits at most a few seconds.
   */
  function stillVideos() {
    var videos = [].slice.call(document.querySelectorAll('.reveal video'));
    return Promise.all(videos.map(function (v) {
      return new Promise(function (resolve) {
        var finish = function () {
          clearTimeout(timer);
          resolve();
        };
        var timer = setTimeout(finish, 5000);
        var draw = function () {
          try {
            var c = document.createElement('canvas');
            c.width = v.videoWidth;
            c.height = v.videoHeight;
            c.getContext('2d').drawImage(v, 0, 0);
            var cs = getComputedStyle(v);
            ['position', 'top', 'left', 'width', 'height', 'object-fit', 'object-position', 'max-width', 'max-height'].forEach(function (p) {
              c.style.setProperty(p, cs.getPropertyValue(p));
            });
            c.className = v.className;
            v.pause();
            v.replaceWith(c);
          } catch (e) {
            /* keep the video */
          }
          finish();
        };
        v.muted = true;
        v.addEventListener('seeked', draw, { once: true });
        v.addEventListener('error', finish, { once: true });
        var seek = function () { v.currentTime = Math.min(0.1, (v.duration || 1) / 2); };
        if (v.readyState >= 1) seek();
        else {
          v.addEventListener('loadedmetadata', seek, { once: true });
          v.preload = 'auto';
          if (!v.getAttribute('src') && v.dataset.src) v.src = v.dataset.src;
          v.load();
        }
      });
    }));
  }

  /** YouTube players print empty: they show the video's picture instead. */
  function stillEmbeds() {
    var frames = [].slice.call(document.querySelectorAll('.reveal .cz-video-box iframe'));
    return Promise.all(frames.map(function (f) {
      var m = /youtube\.com\/embed\/(?:\?v=)?([\w-]{6,})/.exec(f.getAttribute('data-src') || f.getAttribute('src') || '');
      if (!m) return null;
      return new Promise(function (resolve) {
        var img = new Image();
        var timer = setTimeout(resolve, 5000);
        img.alt = '';
        img.style.cssText = 'display:block;width:100%;height:100%;object-fit:cover';
        img.onload = function () {
          clearTimeout(timer);
          resolve();
        };
        img.onerror = function () {
          img.remove();
          clearTimeout(timer);
          resolve();
        };
        img.src = 'https://img.youtube.com/vi/' + m[1] + '/hqdefault.jpg';
        f.replaceWith(img);
      });
    }));
  }

  /** Asks where to save, then writes the PDF (Windows). */
  function savePdf(size, title) {
    var msg = document.getElementById('cz-print-msg');
    // New presentations are titled in English by Rust.
    var name = (title === 'Untitled Presentation' ? t('Untitled Presentation') : title) || t('Presentation');
    name = name.replace(/[\\/:*?"<>|]+/g, '-').trim() || t('Presentation');
    tauri.dialog.save({ title: t('Save PDF…'), defaultPath: name + '.pdf', filters: [{ name: 'PDF', extensions: ['pdf'] }] }).then(function (path) {
      if (!path) return;
      if (!/\.pdf$/i.test(path)) path += '.pdf';
      msg.textContent = t('Saving…');
      return tauri.core.invoke('print_to_pdf', { path: path, width: size.width, height: size.height }).then(function () {
        msg.textContent = t('Saved {file}', { file: path.split(/[\\/]/).pop() });
        var show = document.getElementById('cz-show') || document.createElement('button');
        show.id = 'cz-show';
        show.className = 'secondary';
        show.textContent = t('Show in Folder');
        show.onclick = function () { tauri.opener.revealItemInDir(path).catch(function () {}); };
        msg.after(show);
      });
    }).catch(function (e) {
      msg.textContent = t(String(e));
    });
  }

  window.CadenzaPresent = { boot: boot, handleCommand: handleCommand };

  if (mode === 'show') bootShow();
  else if (mode === 'print') bootPrint();
})();
