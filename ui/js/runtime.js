/*
 * Cadenza runtime — boots reveal.js for a Cadenza deck.
 *
 * Used by the app's presentation windows and by exported presentations, so
 * it must stay dependency-free and work from file:// URLs.
 */
(function () {
  'use strict';

  /** Renders every LaTeX element with KaTeX (when it is loaded). */
  function renderMath(root) {
    if (!window.katex) return;
    var nodes = (root || document).querySelectorAll('.cz-math[data-latex], .cz-math-inline[data-latex], .cz-math-display[data-latex]');
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      try {
        window.katex.render(el.getAttribute('data-latex') || '', el, {
          displayMode: !el.classList.contains('cz-math-inline'),
          throwOnError: false,
          output: 'html',
        });
      } catch (e) {
        /* keep the source text */
      }
    }
  }

  /**
   * Magic Move: match elements between slides only by their Cadenza link id.
   * reveal.js would otherwise also pair inner paragraphs by text content,
   * animating them twice. `this` is reveal's AutoAnimate controller.
   */
  function autoAnimateMatcher(fromSlide, toSlide) {
    var pairs = [];
    this.findAutoAnimateMatches(pairs, fromSlide, toSlide, '.cz-el[data-id]', function (node) {
      return 'cz:::' + node.getAttribute('data-id');
    }, { measure: measureElement });
    return pairs;
  }

  /**
   * An element's box for Magic Move, moved as far as its motion path has
   * taken it (reveal.js would measure where the path started). Same units
   * as reveal's own measure with `center: false`.
   */
  function measureElement(el) {
    var scale = window.Reveal.getScale();
    var box = { x: el.offsetLeft * scale, y: el.offsetTop * scale, width: el.offsetWidth * scale, height: el.offsetHeight * scale };
    var path = el.querySelector(':scope > .cz-path');
    if (path) {
      var a = el.getBoundingClientRect();
      var b = path.getBoundingClientRect();
      box.x += b.left - a.left;
      box.y += b.top - a.top;
    }
    return box;
  }

  // YouTube refuses to play without a Referer (error 153), and pages opened
  // from disk send none. There, videos play through a small page published
  // with Cadenza's code (docs/youtube.com/embed/ in its repository); its path
  // keeps the `youtube.com/embed/` that reveal.js looks for.
  var YOUTUBE_WRAPPER = 'https://rodolfo-viana.github.io/cadenza/youtube.com/embed/';

  function wrapYouTube(root) {
    if (location.protocol !== 'file:') return;
    var frames = root.querySelectorAll('iframe[data-src^="https://www.youtube.com/embed/"]');
    for (var i = 0; i < frames.length; i++) {
      var m = /^https:\/\/www\.youtube\.com\/embed\/([\w-]+)\??(.*)$/.exec(frames[i].getAttribute('data-src'));
      if (m) frames[i].setAttribute('data-src', YOUTUBE_WRAPPER + '?v=' + m[1] + (m[2] ? '&' + m[2] : ''));
    }
  }

  // Live camera: on while a slide with live video is shown, off otherwise.
  // One stream serves every live video element of the slide.
  var camera = { stream: null, device: null, timer: 0, request: 0 };

  function stopCamera() {
    camera.request++;
    if (camera.stream) camera.stream.getTracks().forEach(function (t) { t.stop(); });
    camera.stream = null;
    camera.device = null;
  }

  function syncCameras(deck) {
    var slide = deck.getCurrentSlide();
    var videos = slide ? Array.prototype.slice.call(slide.querySelectorAll('video.cz-camera')) : [];
    clearTimeout(camera.timer);
    if (!videos.length) {
      // A short grace period keeps it on between two slides that both show it.
      camera.timer = setTimeout(stopCamera, 1500);
      return;
    }
    var device = videos[0].getAttribute('data-device') || '';
    var size = { width: { ideal: 1280 }, height: { ideal: 720 } };
    if (device) size.deviceId = { ideal: device };
    var ready;
    if (camera.stream && camera.device === device) {
      ready = Promise.resolve(camera.stream);
    } else {
      stopCamera();
      var request = camera.request;
      ready = navigator.mediaDevices.getUserMedia({ video: size, audio: false }).then(function (stream) {
        if (request !== camera.request) {
          stream.getTracks().forEach(function (t) { t.stop(); });
          throw new Error('superseded');
        }
        camera.stream = stream;
        camera.device = device;
        try { window.dispatchEvent(new CustomEvent('cz-camera-on')); } catch (e) { /* old browsers */ }
        return stream;
      });
    }
    ready.then(
      function (stream) {
        videos.forEach(function (v) {
          v.parentNode.classList.remove('cz-camera-off');
          if (v.srcObject !== stream) v.srcObject = stream;
          v.play().catch(function () {});
        });
      },
      function (e) {
        if (e && e.message === 'superseded') return;
        videos.forEach(function (v) { v.parentNode.classList.add('cz-camera-off'); });
      }
    );
  }

  /** Shows a camera icon in place of live video that won't be turned on. */
  function camerasOff(root) {
    var videos = root.querySelectorAll('video.cz-camera');
    for (var i = 0; i < videos.length; i++) videos[i].parentNode.classList.add('cz-camera-off');
  }

  /**
   * A click on a YouTube or Vimeo player moves the keyboard into its frame,
   * where reveal.js no longer hears it. The show takes it back, so the arrows
   * and Escape keep working (the click still reaches the player).
   */
  function keepKeyboard() {
    window.addEventListener('blur', function () {
      setTimeout(function () {
        var active = document.activeElement;
        if (active && active.tagName === 'IFRAME' && active.closest('.cz-video-box')) {
          active.blur();
          window.focus();
        }
      }, 0);
    });
  }

  /**
   * Starts reveal.js. `config` is the deck configuration generated by
   * Cadenza; `options.notes === false` leaves out the speaker-notes plugin
   * and `options.camera === false` keeps live video off (previews, printing).
   */
  function start(config, options) {
    options = options || {};
    renderMath(document.querySelector('.reveal'));
    wrapYouTube(document);
    var plugins = [];
    if (window.RevealHighlight) plugins.push(window.RevealHighlight);
    if (window.RevealNotes && options.notes !== false) plugins.push(window.RevealNotes);
    var cfg = {};
    for (var k in config) cfg[k] = config[k];
    cfg.plugins = plugins;
    cfg.autoAnimateMatcher = autoAnimateMatcher;
    var deck = window.Reveal;
    if (cfg.keyboard !== false) keepKeyboard();
    return Promise.resolve(deck.initialize(cfg)).then(function () {
      if (!document.querySelector('video.cz-camera')) return deck;
      if (options.camera !== false && navigator.mediaDevices) {
        deck.on('slidechanged', function () { syncCameras(deck); });
        window.addEventListener('pagehide', stopCamera);
        syncCameras(deck);
      } else {
        camerasOff(document);
      }
      return deck;
    });
  }

  window.CadenzaRuntime = { start: start, renderMath: renderMath };
})();
