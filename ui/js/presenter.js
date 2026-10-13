/*
 * Presenter display: current and next slide (live reveal.js previews with
 * the right build state), speaker notes, timer and clock. Drives the
 * audience window through Tauri events.
 */
(function () {
  'use strict';

  window.addEventListener('error', function (e) {
    if (window.__TAURI__) window.__TAURI__.core.invoke('ui_log', { level: 'error', message: e.message + ' @ ' + e.filename + ':' + e.lineno }).catch(function () {});
  });

  var tauri = window.__TAURI__;
  var docLabel = new URLSearchParams(location.search).get('doc') || '';
  var $ = function (id) { return document.getElementById(id); };
  var data = null;
  var lastState = null;
  var framesReady = 0;
  // Interface translation (replaced by i18n.js once the language is known).
  var t = function (s, v) { return v ? s.replace(/\{(\w+)\}/g, function (m, k) { return k in v ? v[k] : m; }) : s; };

  function translatePage() {
    document.querySelectorAll('[data-i18n]').forEach(function (el) { el.textContent = t(el.getAttribute('data-i18n')); });
    document.querySelectorAll('[data-i18n-title]').forEach(function (el) { el.title = t(el.getAttribute('data-i18n-title')); });
  }

  // ---- Timer & clock -------------------------------------------------------
  var started = null;
  var elapsedBefore = 0;
  var running = false;

  function fmt(ms) {
    var s = Math.floor(ms / 1000);
    var h = Math.floor(s / 3600);
    var m = Math.floor((s % 3600) / 60);
    var sec = s % 60;
    var pad = function (n) { return (n < 10 ? '0' : '') + n; };
    return (h ? h + ':' + pad(m) : pad(m)) + ':' + pad(sec);
  }

  function elapsed() {
    return elapsedBefore + (running && started ? Date.now() - started : 0);
  }

  function tick() {
    $('timer').textContent = fmt(elapsed());
    $('timer').classList.toggle('paused', !running);
    $('timer-toggle').textContent = running ? t('Pause') : t('Resume');
    $('clock').textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function startTimer() {
    if (running) return;
    running = true;
    started = Date.now();
  }

  function toggleTimer() {
    if (running) {
      elapsedBefore = elapsed();
      running = false;
    } else {
      startTimer();
    }
    tick();
  }

  function resetTimer() {
    elapsedBefore = 0;
    started = Date.now();
    tick();
  }

  setInterval(tick, 500);
  tick();

  // ---- Commands -------------------------------------------------------------
  function send(cmd, extra) {
    var payload = { doc: docLabel, cmd: cmd };
    for (var k in extra || {}) payload[k] = extra[k];
    tauri.event.emit('cz-present-cmd', payload);
  }

  $('prev').onclick = function () { send('prev'); };
  $('nextbtn').onclick = function () { send('next'); };
  $('end').onclick = function () { tauri.core.invoke('present_stop', { slide: lastState && lastState.slideId }); };
  $('timer-toggle').onclick = toggleTimer;
  $('timer-reset').onclick = resetTimer;

  var notesSize = 22;
  try { notesSize = parseInt(localStorage.getItem('cz-notes-size'), 10) || 22; } catch (e) { /* storage unavailable */ }
  function setNotesSize(n) {
    notesSize = Math.max(12, Math.min(64, n));
    document.documentElement.style.setProperty('--notes-size', notesSize + 'px');
    try { localStorage.setItem('cz-notes-size', String(notesSize)); } catch (e) { /* ignore */ }
  }
  setNotesSize(notesSize);
  $('smaller').onclick = function () { setNotesSize(notesSize - 2); };
  $('larger').onclick = function () { setNotesSize(notesSize + 2); };

  document.addEventListener('keydown', function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case 'ArrowRight': case 'ArrowDown': case ' ': case 'PageDown': case 'Enter': case 'n': case 'N':
        send('next'); break;
      case 'ArrowLeft': case 'ArrowUp': case 'PageUp': case 'Backspace': case 'p':
        send('prev'); break;
      case 'Home': send('first'); break;
      case 'End': send('last'); break;
      case 'b': case 'B': case '.': send('pause'); break;
      case 'Escape': $('end').click(); break;
      case 'P': toggleTimer(); break;
      case 'r': case 'R': resetTimer(); break;
      case '+': case '=': setNotesSize(notesSize + 2); break;
      case '-': case '_': setNotesSize(notesSize - 2); break;
      default: return;
    }
    e.preventDefault();
  });

  // ---- Previews ---------------------------------------------------------------
  function frameReveal(id) {
    var w = $(id).contentWindow;
    return w && w.Reveal && w.Reveal.isReady && w.Reveal.isReady() ? w.Reveal : null;
  }

  function applyState(st) {
    if (!st) return;
    var cur = frameReveal('current');
    var next = frameReveal('next');
    if (cur) cur.setState(st.state);
    if (next) {
      next.setState(st.state);
      next.next();
    }
    $('next-wrap').classList.toggle('at-end', !!st.isLast);
    $('count').innerHTML = t('Slide {n} of {total}', { n: '<b>' + (st.index + 1) + '</b>', total: '<b>' + st.total + '</b>' });
    $('paused').classList.toggle('on', !!st.paused);
    var slide = data && data.slides.find(function (s) { return s.id === st.slideId; });
    var notes = slide && slide.notes.trim();
    $('notes').textContent = notes || t('No notes for this slide.');
    $('notes').classList.toggle('empty', !notes);
    $('notes').scrollTop = 0;
  }

  function bootFrame(id) {
    var frame = $(id);
    var go = function () {
      var w = frame.contentWindow;
      if (!w || !w.CadenzaPresent) return setTimeout(go, 50);
      w.CadenzaPresent.boot(data).then(function () {
        framesReady++;
        applyState(lastState);
      });
    };
    if (frame.contentDocument && frame.contentDocument.readyState === 'complete') go();
    else frame.addEventListener('load', go, { once: true });
  }

  tauri.core.invoke('present_data').then(function (d) {
    data = d;
    document.documentElement.style.setProperty('--ratio', d.width + ' / ' + d.height);
    import('./i18n.js').then(function (m) {
      return m.setLanguage(d.language).then(function () {
        t = m.t;
        translatePage();
        document.title = t('Presenter — {title}', { title: d.title || t('Presentation') });
        tick();
        applyState(lastState);
      });
    });
    document.title = t('Presenter — {title}', { title: d.title || t('Presentation') });
    bootFrame('current');
    bootFrame('next');
    tauri.event.listen('cz-present-state', function (e) {
      if (!e.payload || e.payload.doc !== docLabel) return;
      lastState = e.payload;
      startTimer();
      applyState(lastState);
    });
    // Ask the audience window for its current state in case we missed it.
    setTimeout(function () { send('sync'); }, 300);
  });
})();
