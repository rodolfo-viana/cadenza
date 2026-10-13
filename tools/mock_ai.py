"""Mock AI server for testing Cadenza's assistant without real API keys or costs.

Speaks both wire formats Cadenza uses:
  - Anthropic Messages API (SSE):  GET /v1/models, GET /v1/models/<id>, POST /v1/messages
  - OpenAI-compatible (SSE):       GET /v1/models, POST /v1/chat/completions
Every request (headers + body) is appended to $CZ_OUT/mock-requests.jsonl
(default: the current folder).

    python3 tools/mock_ai.py 8765

Then, in Cadenza's Settings ▸ AI Assistant, pick "Other" with the server
http://127.0.0.1:8765/v1, or start a debug build with
CADENZA_ANTHROPIC_URL=http://127.0.0.1:8765/v1 to exercise the Claude path.
"""

import json
import os
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

OUT = os.environ.get("CZ_OUT", ".")
LOG = os.path.join(OUT, "mock-requests.jsonl")


def slide(layout, title="", **kw):
    s = {"layout": layout, "title": title, "subtitle": "", "body": [], "body2": [], "code": "", "code_language": "",
         "chart_type": "", "chart_labels": [], "chart_series": [], "table": [], "diagram": "", "image_query": "", "notes": ""}
    s.update(kw)
    return s


def action(kind, slide_id=None, element_id=None, slides=None, text=None, value=None):
    return {"action": kind, "slide_id": slide_id, "element_id": element_id, "slides": slides or [], "text": text or [], "value": value}


def new_deck_plan():
    slides = [
        slide("title", "Photosynthesis", subtitle="How plants turn light into life", notes="Welcome everyone. Today we look at how plants make food from light."),
        slide("section", "The Big Picture", subtitle="PART 1", notes="Let's start with the overall equation."),
        slide("bullets", "What happens in a leaf", body=["Light is captured by **chlorophyll**", "Water is split, releasing oxygen", "CO₂ is fixed into sugars", "Overall: $6CO_2 + 6H_2O \\rightarrow C_6H_{12}O_6 + 6O_2$"],
              notes="These four steps summarize the process."),
        slide("diagram", "Light reactions at a glance", diagram="flowchart LR\n  L[Light] --> P[Photosystem II]\n  P --> E[Electron chain]\n  E --> A[ATP + NADPH]\n  W[Water] --> P\n  P --> O[Oxygen]",
              notes="Follow the energy from light to ATP and NADPH."),
        slide("chart", "Rate vs. light intensity", chart_type="line", chart_labels=["0", "200", "400", "600", "800"],
              chart_series=[{"name": "Rate (illustrative)", "values": [0, 8, 14, 17, 18]}], body=["Rises fast, then plateaus", "Other factors become limiting"],
              notes="These numbers are illustrative; the shape is what matters."),
        slide("table", "C3 vs. C4 vs. CAM", table=[["Type", "Example", "Best in"], ["C3", "Wheat", "Cool, wet"], ["C4", "Maize", "Hot, sunny"], ["CAM", "Cactus", "Dry"]],
              notes="Plants evolved different strategies for different climates."),
        slide("two_column", "Light vs. dark reactions", body=["**Light reactions**", "Thylakoid membranes", "Make ATP and NADPH"], body2=["**Calvin cycle**", "Stroma", "Builds sugars"],
              notes="Two stages, two places, one goal."),
        slide("big_number", "~120 Gt", subtitle="carbon fixed by land plants every year", notes="An enormous amount of carbon moves through leaves."),
        slide("image_text", "Leaves are solar panels", body=["Broad, flat and thin", "Packed with chloroplasts"], image_query="green leaf sunlight", notes="Leaf shape maximizes light capture."),
        slide("code", "Modelling it in Python", code="def rate(light, k=0.01, vmax=18):\n    return vmax * light / (1 / k + light)\n\nprint(rate(400))", code_language="python",
              notes="A simple saturating model reproduces the curve."),
        slide("quote", "Nature does nothing uselessly", subtitle="Aristotle", notes="A fitting closing thought."),
        slide("bullets", "Key takeaways", body=["Light energy becomes chemical energy", "Oxygen is a by-product", "Climate shapes the strategy"], notes="Let's recap before questions."),
    ]
    return {"reply": "Here is a 12-slide presentation on photosynthesis.", "actions": [action("set_title", value="Photosynthesis"), action("insert", slides=slides)]}


def edit_plan(user_text):
    m = re.search(r"The presentation \(JSON\):\n(\{.*\})\n\nRequest:", user_text, re.S)
    deck = json.loads(m.group(1)) if m else {"slides": []}
    current = deck.get("current_slide") or (deck["slides"][0]["id"] if deck["slides"] else None)
    acts = [action("notes", slide_id=s["id"], value=f"Speaker notes for slide {s['number']}.") for s in deck["slides"]]
    cur = next((s for s in deck["slides"] if s["id"] == current), None)
    if cur:
        title = next((e for e in cur["elements"] if e.get("role") == "title"), None)
        if title:
            acts.append(action("set_text", slide_id=current, element_id=title["id"], text=["A sharper *title*"]))
    acts.append(action("insert", slide_id=current, slides=[slide("bullets", "Pricing", body=["Free plan for students", "Pro: $9 per month", "Teams: talk to us"], notes="Explain the plans.")]))
    return {"reply": "I wrote speaker notes for every slide, sharpened this slide's title and added a pricing slide after it.", "actions": acts}


def plan_for(user_text):
    return new_deck_plan() if "Create a complete presentation" in user_text else edit_plan(user_text)


def chunks(text, n=48):
    for i in range(0, len(text), n):
        yield text[i:i + n]


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def record(self, body):
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "method": self.command, "headers": dict(self.headers), "body": body}) + "\n")

    def send_json(self, obj, status=200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self.record(None)
        if self.path.startswith("/v1/models/"):
            self.send_json({"id": self.path.rsplit("/", 1)[1], "max_tokens": 64000})
        elif self.path.startswith("/v1/models"):
            self.send_json({"data": [{"id": "mock-large", "display_name": "Mock Large", "created": 2}, {"id": "mock-small", "display_name": "Mock Small", "created": 1}]})
        else:
            self.send_json({"error": {"message": "not found"}}, 404)

    def sse(self):
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.end_headers()

    def event(self, data, name=None):
        msg = (f"event: {name}\n" if name else "") + f"data: {json.dumps(data) if not isinstance(data, str) else data}\n\n"
        self.wfile.write(msg.encode())
        self.wfile.flush()

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        self.record(body)
        user = body["messages"][-1]["content"]
        text = json.dumps(plan_for(user if isinstance(user, str) else json.dumps(user)), ensure_ascii=False)
        if self.path == "/v1/messages":
            self.sse()
            self.event({"type": "message_start", "message": {"id": "msg_mock", "type": "message", "role": "assistant", "model": body["model"], "content": []}}, "message_start")
            self.event({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}, "content_block_start")
            time.sleep(0.8)
            self.event({"type": "content_block_stop", "index": 0}, "content_block_stop")
            self.event({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}, "content_block_start")
            for c in chunks(text):
                self.event({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": c}}, "content_block_delta")
                time.sleep(0.03)
            self.event({"type": "content_block_stop", "index": 1}, "content_block_stop")
            self.event({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 999}}, "message_delta")
            self.event({"type": "message_stop"}, "message_stop")
        elif self.path == "/v1/chat/completions":
            self.sse()
            time.sleep(0.5)
            for c in chunks(text):
                self.event({"choices": [{"index": 0, "delta": {"content": c}, "finish_reason": None}]})
                time.sleep(0.03)
            self.event({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
            self.event("[DONE]")
        else:
            self.send_json({"error": {"message": "not found"}}, 404)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
