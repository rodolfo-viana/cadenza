// The crop rectangle is edited in original-image coordinates. Rotation and
// flips remain on the slide; they never change the stored source region.
import { html, useRef, useState } from './lib.js';
import * as S from './store.js';
import { endAllEditing } from './textedit.js';
import { Modal, Button } from './ui.js';
import { t } from './i18n.js';

export const FULL_CROP = { x: 0, y: 0, w: 1, h: 1 };
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

export function cropDrag(c, handle, dx, dy, ratio = 0) {
  if (handle === 'move') return { ...c, x: clamp(c.x + dx, 0, 1 - c.w), y: clamp(c.y + dy, 0, 1 - c.h) };
  const west = handle.includes('w'),
    north = handle.includes('n');
  const ax = west ? c.x + c.w : c.x,
    ay = north ? c.y + c.h : c.y;
  let w = clamp(c.w + (west ? -dx : dx), 0.005, west ? ax : 1 - ax);
  let h = clamp(c.h + (north ? -dy : dy), 0.005, north ? ay : 1 - ay);
  if (ratio) {
    h = w / ratio;
    const maxH = north ? ay : 1 - ay;
    if (h > maxH) {
      h = maxH;
      w = h * ratio;
    }
  }
  return { x: west ? ax - w : ax, y: north ? ay - h : ay, w, h };
}

export function cropAspect(c, pixelRatio, width, height) {
  if (!pixelRatio) return c;
  const ratio = (pixelRatio * height) / width;
  let w = c.w,
    h = w / ratio;
  if (h > c.h) {
    h = c.h;
    w = h * ratio;
  }
  return { x: c.x + (c.w - w) / 2, y: c.y + (c.h - h) / 2, w, h };
}

export async function openCrop(el = S.selectedElements()[0]) {
  const docId = S.state.doc?.docId,
    slide = S.state.slideId;
  if (!el || el.locked || el.content.kind !== 'image' || !el.content.src) return;
  if (!(await endAllEditing()) || S.state.doc?.docId !== docId) return;
  el = S.deck()
    .slides.find((s) => s.id === slide)
    ?.elements.find((e) => e.id === el.id);
  const { canvasApi } = await import('./canvas.js');
  if (S.state.doc?.docId !== docId || !el) return;
  const img = canvasApi.view?.node(el.id)?.querySelector('img');
  const width = img?.naturalWidth || el.content.naturalWidth,
    height = img?.naturalHeight || el.content.naturalHeight;
  if (!img?.src || !width || !height) {
    S.toast(t('Wait for the image to load before cropping.'), 'error');
    return;
  }
  S.openModal('crop', { docId, slide, el, url: img.src, width, height });
}

export function CropDialog({ docId, slide, el, url, width, height }) {
  const [crop, setCrop] = useState(el.content.crop || FULL_CROP);
  const [aspect, setAspect] = useState(0);
  const [busy, setBusy] = useState(false);
  const surface = useRef();
  const drag = useRef(null);
  const close = () => {
    if (!busy) S.closeModal();
  };
  async function save() {
    if (busy || S.state.doc?.docId !== docId) return;
    setBusy(true);
    const full = crop.x === 0 && crop.y === 0 && crop.w === 1 && crop.h === 1;
    const result = await S.apply(
      { op: 'cropImage', slide, id: el.id, before: el.content, crop: full ? null : crop, naturalWidth: width, naturalHeight: height },
      { docId, select: false },
    );
    if (result && S.state.doc?.docId === docId) S.closeModal();
    setBusy(false);
  }
  function start(e, handle) {
    if (busy || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const r = surface.current.getBoundingClientRect();
    drag.current = { handle, crop, x: e.clientX, y: e.clientY, width: r.width, height: r.height };
    surface.current.setPointerCapture(e.pointerId);
  }
  function move(e) {
    const d = drag.current;
    if (!d) return;
    setCrop(cropDrag(d.crop, d.handle, (e.clientX - d.x) / d.width, (e.clientY - d.y) / d.height, (aspect * height) / width));
  }
  return html`<${Modal}
    title=${t('Crop Image')}
    width=${820}
    onClose=${close}
    footer=${html`<${Button}
        label=${t('Reset Crop')}
        disabled=${busy}
        onClick=${() => {
          setCrop(FULL_CROP);
          setAspect(0);
        }}
      /><span class="spacer" /> <${Button} label=${t('Cancel')} disabled=${busy} onClick=${close} /><${Button}
        label=${t('Apply')}
        variant="primary"
        disabled=${busy}
        onClick=${save}
      />`}
  >
    <div
      class="crop-editor"
      onKeyDown=${(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          save();
        }
      }}
    >
      <p class="muted">${t('Select an area of the original image. Drag the area to reposition it. Rotation, flips and the frame stay unchanged.')}</p>
      <label
        >${t('Aspect ratio')}
        <select
          value=${aspect}
          disabled=${busy}
          onChange=${(e) => {
            const a = Number(e.target.value);
            setAspect(a);
            setCrop(cropAspect(crop, a, width, height));
          }}
        >
          <option value="0">${t('Free')}</option>
          <option value="1">1:1</option>
          <option value=${4 / 3}>4:3</option>
          <option value=${16 / 9}>16:9</option>
          <option value=${width / height}>${t('Original Ratio')}</option>
        </select></label
      >
      <div
        class="crop-stage"
        style=${{ aspectRatio: `${width}/${height}`, maxHeight: '55vh', maxWidth: `min(100%, ${(55 * width) / height}vh)` }}
        ref=${surface}
        onPointerMove=${move}
        onPointerUp=${() => {
          drag.current = null;
        }}
        onPointerCancel=${() => {
          if (drag.current) setCrop(drag.current.crop);
          drag.current = null;
        }}
      >
        <img src=${url} alt=${el.content.alt || t('Original image')} draggable="false" />
        <div
          class="crop-region"
          role="group"
          aria-label=${t('Selected crop area')}
          style=${{ left: `${crop.x * 100}%`, top: `${crop.y * 100}%`, width: `${crop.w * 100}%`, height: `${crop.h * 100}%` }}
          onPointerDown=${(e) => start(e, 'move')}
        >
          ${['nw', 'ne', 'sw', 'se'].map(
            (h) => html`<button type="button" class=${`crop-handle h-${h}`} aria-label=${t('Resize crop') + ' ' + h} onPointerDown=${(e) => start(e, h)} />`,
          )}
        </div>
      </div>
      <div class="crop-values">
        ${['x', 'y', 'w', 'h'].map(
          (key) =>
            html`<label
              >${{ x: 'X', y: 'Y', w: t('Width'), h: t('Height') }[key]} (%)
              <input
                type="number"
                min=${key === 'x' || key === 'y' ? 0 : 0.5}
                max="100"
                step="0.5"
                value=${Math.round(crop[key] * 10000) / 100}
                onChange=${(e) => {
                  const v = Number(e.target.value) / 100;
                  if (!Number.isFinite(v)) return;
                  const c = { ...crop };
                  c[key] = clamp(v, key === 'x' || key === 'y' ? 0 : 0.005, 1);
                  c.x = Math.min(c.x, 1 - c.w);
                  c.y = Math.min(c.y, 1 - c.h);
                  setAspect(0);
                  setCrop(c);
                }}
            /></label>`,
        )}
      </div>
    </div>
  <//>`;
}
