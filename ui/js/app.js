// Application shell.

import { html } from './lib.js';
import { useStore, settings } from './store.js';
import { Toolbar, ShapePicker } from './toolbar.js';
import { Navigator } from './navigator.js';
import { Canvas } from './canvas.js';
import { Inspector } from './inspector.js';
import { NotesPanel } from './notes.js';
import { Welcome, Logo } from './welcome.js';
import { ModalHost } from './dialogs.js';
import { MenuHost, Toasts, Busy } from './ui.js';
import { PainterBar } from './formatpainter.js';
import { FindReplace } from './findreplace.js';

function Editor() {
  const st = useStore();
  const s = settings();
  return html`<div class="editor">
    <${Toolbar} />
    <${PainterBar} />
    ${st.findReplace && html`<${FindReplace} key=${st.doc.docId} />`}
    <div class="workspace">
      <${Navigator} />
      <main class="center">
        <${Canvas} />
        ${s.showNotes && html`<${NotesPanel} />`}
      </main>
      ${s.showInspector && html`<${Inspector} />`}
    </div>
  </div>`;
}

export function App() {
  const st = useStore();
  if (!st.ready) {
    return html`<div class="splash"><${Logo} size=${64} /></div>`;
  }
  return html`
    ${st.doc ? html`<${Editor} />` : html`<${Welcome} />`}
    <${ShapePicker} />
    <${MenuHost} />
    <${ModalHost} />
    <${Toasts} />
    <${Busy} />
    ${st.dropHint && html`<div class="drop-hint"><div>${st.dropHint}</div></div>`}
  `;
}
