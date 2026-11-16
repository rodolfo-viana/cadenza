#!/usr/bin/env python3
"""Measure production PPTX snapshots in isolated WebKitGTK using Tauri's scheme.

Needs PyGObject, Gtk 3, WebKit2 4.1 and Xvfb. No user display or profile is used.
Run after generating pptx_fixture; pass --xvfb if it is not on PATH.
"""
import argparse
import json
import mimetypes
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from urllib.parse import urlparse

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("snapshot", type=Path)
parser.add_argument("--xvfb", default=shutil.which("Xvfb"))
args = parser.parse_args()
if not args.xvfb:
    parser.error("Xvfb is required; specify --xvfb /path/to/Xvfb")
root = Path(__file__).resolve().parent.parent
snapshot = json.loads(args.snapshot.read_text())
server = subprocess.Popen(
    [args.xvfb, "-displayfd", "1", "-screen", "0", "1280x900x24", "-nolisten", "tcp"],
    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
)
display = server.stdout.readline().strip()
if not display:
    raise RuntimeError("Xvfb failed: " + server.stderr.read())
os.environ.update(DISPLAY=":" + display, GDK_BACKEND="x11", WEBKIT_DISABLE_COMPOSITING_MODE="1")

import gi  # noqa: E402 (display must be isolated before loading GTK)
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gio, GLib, Gtk, WebKit2  # noqa: E402

script = r"""
import { measureSnapshot } from '/js/pptx-layout.js';
const snapshot = SNAPSHOT;
let assertions=0;
function ok(value,message){if(!value)throw new Error(message);assertions++;}
const active=new Set(), originalSet=window.setInterval, originalClear=window.clearInterval;
window.setInterval=(...args)=>{const id=originalSet(...args);active.add(id);return id;};
window.clearInterval=id=>{active.delete(id);originalClear(id);};
async function run(){
  const start=performance.now();
  const frame=await measureSnapshot(snapshot);
  const read=await frame.slide(snapshot.slides[0]);
  const text=read.root.querySelector('.cz-text:not(.cz-el)');
  ok(read.style(text).fontSize==='32px','Authored text size is measured');
  ok(read.style(read.root.querySelector('.cz-text-box')).display==='flex','Content CSS is applied');
  ok(read.style(read.root).getPropertyValue('--cz-chart-1').trim()!=='','Theme CSS is applied');
  ok([...read.root.ownerDocument.styleSheets].filter(s=>s.href).every(s=>s.cssRules.length>0),'Every bundled stylesheet was parsed');
  ok(active.size===0,'Successful CSS loading cleans every readiness poll');
  frame.dispose();
  ok(document.querySelectorAll('iframe').length===0,'Successful measurement is disposable');
  const elapsed=performance.now()-start;
  const controller=new AbortController();
  const cancel=measureSnapshot({...snapshot,themeCss:'pending-measure-theme.css'},controller.signal);
  setTimeout(()=>controller.abort(),100);
  let cancelled=false;
  try{await cancel;}catch(e){cancelled=e.name==='AbortError';}
  ok(cancelled,'Cancellation interrupts a stylesheet that never answers');
  ok(active.size===0&&document.querySelectorAll('iframe').length===0,'Cancellation removes polls and frame');
  let failed=false;
  try{await measureSnapshot({...snapshot,themeCss:'missing-measure-theme.css'});}
  catch(e){failed=/stylesheet/.test(e.message);}
  ok(failed,'Missing CSS rejects instead of passing the readiness check');
  ok(active.size===0&&document.querySelectorAll('iframe').length===0,'Failed CSS loading removes polls and frame');
  return {pass:true,assertions,elapsed};
}
run().then(result=>window.result=result,error=>window.result={error:String(error)});
""".replace("SNAPSHOT", json.dumps(snapshot).replace("</", "<\\/"))
html = ("<!doctype html><html><head></head><body><script type='module'>" + script + "</script></body></html>").encode()
context = WebKit2.WebContext.new_ephemeral()
# Matches wry's Linux custom-protocol registration, including secure context.
context.get_security_manager().register_uri_scheme_as_secure("tauri")
requests = []
pending_requests = []


def resource(request):
    name = urlparse(request.get_uri()).path
    requests.append(name)
    if name == "/pending-measure-theme.css":
        pending_requests.append(request)
        return
    path = (root / "ui" / name.lstrip("/")).resolve()
    try:
        if not path.is_relative_to(root / "ui"):
            raise FileNotFoundError(name)
        data = html if name == "/test.html" else path.read_bytes()
        mime = mimetypes.guess_type(name)[0] or "application/octet-stream"
        if name.endswith(".js"):
            mime = "text/javascript"
        stream = Gio.MemoryInputStream.new_from_bytes(GLib.Bytes.new(data))
        request.finish(stream, len(data), mime)
    except FileNotFoundError:
        response = WebKit2.URISchemeResponse.new(Gio.MemoryInputStream.new(), 0)
        response.set_status(404, "Not Found")
        response.set_content_type("text/plain")
        request.finish_with_response(response)


context.register_uri_scheme("tauri", resource)
view = WebKit2.WebView.new_with_context(context)
window = Gtk.Window()
window.set_default_size(1280, 900)
window.add(view)
window.show_all()
started = time.monotonic()
result = None


def evaluated(webview, task, _data):
    global result
    value = webview.evaluate_javascript_finish(task).to_string()
    if value and value != "undefined":
        result = json.loads(value)
        Gtk.main_quit()


def poll():
    if time.monotonic() - started > 40:
        Gtk.main_quit()
        return False
    view.evaluate_javascript("JSON.stringify(window.result)", -1, None, None, None, evaluated, None)
    return True


GLib.timeout_add(100, poll)
view.load_uri("tauri://localhost/test.html")
try:
    Gtk.main()
finally:
    window.destroy()
    server.terminate()
    server.wait(timeout=5)
print(json.dumps({"result": result, "requests": requests, "webkit": [
    WebKit2.get_major_version(), WebKit2.get_minor_version(), WebKit2.get_micro_version()
]}, indent=2))
sys.exit(0 if result and result.get("pass") else 1)
