// Live camera elements in the editor. The camera is used only while
// presenting (runtime.js), or during a preview the user asks for: then the
// picture is shown inside the element on the canvas.

import { set } from './store.js';

let preview = { id: null, device: '', stream: null };

export const previewing = (id) => preview.id === id;

/** Turns the camera on to show it in element `el`. */
export async function startPreview(el) {
  stopPreview();
  const device = el.content.deviceId || '';
  const video = { width: { ideal: 1280 }, height: { ideal: 720 } };
  if (device) video.deviceId = { ideal: device };
  const stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
  preview = { id: el.id, device, stream };
  set({});
}

// The show opens the camera itself; some systems allow one user at a time.
window.addEventListener('cz-present-start', () => stopPreview());

export function stopPreview() {
  if (!preview.id) return;
  preview.stream?.getTracks().forEach((t) => t.stop());
  preview = { id: null, device: '', stream: null };
  set({});
}

/**
 * Puts the preview into its element after the canvas renders (re-rendering
 * replaces the element's markup). Stops it when the element is gone.
 */
export function attachPreview(view, slide) {
  if (!preview.id) return;
  const el = slide?.elements.find((e) => e.id === preview.id);
  const box = view?.node(preview.id)?.querySelector('.cz-camera-box');
  if (!el || !box) {
    stopPreview();
    return;
  }
  if ((el.content.deviceId || '') !== preview.device) {
    startPreview(el).catch(() => stopPreview());
    return;
  }
  let v = box.querySelector('video.cz-camera');
  if (!v) {
    v = document.createElement('video');
    v.muted = true;
    v.playsInline = true;
    box.appendChild(v);
  }
  v.className = `cz-camera${el.content.mirror ? ' cz-mirror' : ''}`;
  if (v.srcObject !== preview.stream) v.srcObject = preview.stream;
  v.play().catch(() => {});
}
