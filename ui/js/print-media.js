/* Print-only media preparation. Reveal must finish pagination first: while
 * building its pages it temporarily detaches slides from the document. */
(function () {
  'use strict';

  /** Subscribe before Reveal.initialize, which starts pagination asynchronously. */
  function layoutReady(deck) {
    return new Promise(function (resolve, reject) {
      var ready = function () {
        clearTimeout(timer);
        deck.off('pdf-ready', ready);
        resolve();
      };
      var timer = setTimeout(function () {
        deck.off('pdf-ready', ready);
        reject(new Error('Could not prepare the slides for printing.'));
      }, 30000);
      deck.on('pdf-ready', ready);
    });
  }

  // The bitmap resolution and its authored CSS dimensions are independent.
  // Copy resolved visual properties so replacing an img/video with canvas
  // also preserves CSS filters, fit, rounded corners and custom styling.
  function copyMediaStyle(from, to) {
    var cs = getComputedStyle(from);
    ['display', 'position', 'top', 'right', 'bottom', 'left', 'width', 'height',
      'min-width', 'min-height', 'max-width', 'max-height', 'margin', 'padding',
      'border', 'border-radius', 'box-sizing', 'object-fit', 'object-position',
      'filter', 'opacity', 'transform', 'transform-origin', 'clip-path', 'z-index', 'visibility'].forEach(function (p) {
      to.style.setProperty(p, cs.getPropertyValue(p));
    });
    to.className = from.className;
  }

  /**
   * Chromium drops nested image overflow clips on intermediate PDF pages.
   * Rasterize just the selected image region, keeping the outer flip/rotation
   * wrappers and all other slide content. A canvas can print cross-origin pixels
   * without reading them back through toDataURL (which would taint-fail).
   */
  function rasterizeCrops() {
    var images = [].slice.call(document.querySelectorAll('.reveal .cz-crop-viewport img'));
    // At most 256MiB of crop pixel buffers for the whole document, with a
    // fair resolution reduction across slides rather than dropping pictures.
    var totalArea = images.reduce(function (sum, img) {
      var viewport = img.closest('.cz-crop-viewport');
      return sum + viewport.clientWidth * viewport.clientHeight;
    }, 0);
    var documentScale = Math.min(2, Math.sqrt(67108864 / Math.max(1, totalArea)));
    return Promise.all(images.map(function (img) {
      return new Promise(function (resolve) {
        var done = false;
        var finish = function () {
          if (done) return;
          done = true;
          clearTimeout(timer);
          img.removeEventListener('load', draw);
          img.removeEventListener('error', finish);
          resolve();
        };
        var timer = setTimeout(finish, 5000);
        var draw = function () {
          if (done) return;
          try {
            var viewport = img.closest('.cz-crop-viewport');
            var region = img.parentElement;
            var width = viewport.clientWidth;
            var height = viewport.clientHeight;
            if (!(width > 0 && height > 0 && img.naturalWidth > 0)) { finish(); return; }
            var style = getComputedStyle(img);
            // 192dpi at slide size, capped to 16MP and 8192px per side.
            var scale = Math.min(documentScale, 8192 / Math.max(width, height), Math.sqrt(16777216 / (width * height)));
            var canvas = document.createElement('canvas');
            canvas.width = Math.max(1, Math.floor(width * scale));
            canvas.height = Math.max(1, Math.floor(height * scale));
            var context = canvas.getContext('2d');
            if (style.filter && style.filter !== 'none' && !('filter' in context)) { finish(); return; }
            context.scale(canvas.width / width, canvas.height / height);
            // Both clips are baked into pixels: the inner selected region
            // (smaller than the viewport for Contain), then the viewport.
            context.beginPath();
            context.rect(region.offsetLeft, region.offsetTop, region.clientWidth, region.clientHeight);
            context.clip();
            if (style.filter && style.filter !== 'none') {
              context.filter = style.filter.replace(/blur\(([\d.]+)px\)/g, function (_, px) { return 'blur(' + Number(px) * scale + 'px)'; });
            }
            context.drawImage(img, region.offsetLeft + img.offsetLeft, region.offsetTop + img.offsetTop, img.offsetWidth, img.offsetHeight);
            canvas.style.cssText = 'display:block;position:absolute;left:0;top:0;width:100%;height:100%;max-width:none;max-height:none;';
            canvas.style.opacity = style.opacity;
            canvas.className = img.className;
            canvas.setAttribute('role', 'img');
            canvas.setAttribute('aria-label', img.alt || '');
            viewport.replaceChildren(canvas);
          } catch (e) {
            /* keep an image the engine could not decode */
          }
          finish();
        };
        img.addEventListener('error', finish, { once: true });
        if (img.complete) draw();
        else img.addEventListener('load', draw, { once: true });
      });
    }));
  }

  /**
   * Printouts don't show video frames (they came out black): each video
   * becomes a picture of its first frame. Waits at most a few seconds.
   */
  function stillVideos() {
    var videos = [].slice.call(document.querySelectorAll('.reveal video'));
    return Promise.all(videos.map(function (v) {
      return new Promise(function (resolve) {
        var done = false;
        var finish = function () {
          if (done) return;
          done = true;
          clearTimeout(timer);
          v.removeEventListener('seeked', draw);
          v.removeEventListener('loadedmetadata', seek);
          v.removeEventListener('error', finish);
          resolve();
        };
        var timer = setTimeout(finish, 5000);
        var draw = function () {
          if (done) return;
          try {
            var c = document.createElement('canvas');
            c.width = v.videoWidth;
            c.height = v.videoHeight;
            c.getContext('2d').drawImage(v, 0, 0);
            copyMediaStyle(v, c);
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
        var seek = function () {
          if (done) return;
          try { v.currentTime = Math.min(0.1, (v.duration || 1) / 2); }
          catch (e) { finish(); }
        };
        if (v.readyState >= 1) seek();
        else {
          v.addEventListener('loadedmetadata', seek, { once: true });
          v.preload = 'auto';
          if (!v.getAttribute('src') && v.dataset.src) v.src = v.dataset.src;
          try { v.load(); } catch (e) { finish(); }
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

  window.CadenzaPrint = {
    layoutReady: layoutReady,
    prepareMedia: function () { return Promise.all([rasterizeCrops(), stillVideos(), stillEmbeds()]); },
  };
})();
