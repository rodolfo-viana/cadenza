//! Opt-in AI assistant: creates and edits presentations with a language model
//! (Anthropic Claude, OpenAI, or any OpenAI-compatible server such as Ollama).
//!
//! The model answers with a JSON "plan" (structured output): a short reply
//! plus editing actions. Cadenza turns those actions into real slides built
//! from its own layouts, so results look native, stay editable and can be
//! undone in one step. Nothing is sent anywhere unless the user turned the
//! assistant on and asked it something.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use pulldown_cmark::{Options, Parser, html};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::layouts;
use crate::model::*;
use crate::sanitize;
use crate::util::esc;

const UA: &str = concat!("Cadenza/", env!("CARGO_PKG_VERSION"));
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const OPENAI_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5-5";
pub const DEFAULT_COMPATIBLE_URL: &str = "http://localhost:11434/v1";

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Provider {
    /// "anthropic", "openai" or "compatible".
    pub kind: String,
    pub model: String,
    /// Base URL of an OpenAI-compatible server (e.g. Ollama).
    pub base_url: String,
    pub key: Option<String>,
}

/// Anthropic API address (debug builds can point it at a local mock server
/// with `CADENZA_ANTHROPIC_URL` for testing).
fn anthropic_url() -> String {
    if cfg!(debug_assertions) {
        if let Ok(u) = std::env::var("CADENZA_ANTHROPIC_URL") {
            return u;
        }
    }
    ANTHROPIC_URL.into()
}

impl Provider {
    fn openai_base(&self) -> String {
        if self.kind == "openai" {
            return OPENAI_URL.into();
        }
        match self.base_url.trim().trim_end_matches('/') {
            "" => DEFAULT_COMPATIBLE_URL.into(),
            b => b.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    /// "user" or "assistant".
    pub role: String,
    pub text: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// A request the server rejected as invalid: worth retrying without
/// optional features (structured outputs, effort, fallbacks).
#[derive(Debug)]
struct BadRequest(String);

impl std::fmt::Display for BadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The AI service rejected the request: {}", self.0)
    }
}

impl std::error::Error for BadRequest {}

pub const CANCELLED: &str = "Cancelled";

fn agent(streaming: bool) -> ureq::Agent {
    let (first_byte, body) = if streaming { (300, 1800) } else { (60, 60) };
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(first_byte)))
            .timeout_recv_body(Some(Duration::from_secs(body)))
            .build(),
    )
}

fn api_error(status: u16, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or(v["error"].as_str())
                .or(v["message"].as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status {
        400 | 422 => anyhow!(BadRequest(msg)),
        401 | 403 => anyhow!("The AI service rejected the API key (HTTP {status}). Check it in Settings ▸ AI Assistant."),
        404 => anyhow!("Model or address not found (HTTP 404). Check the model in Settings ▸ AI Assistant. {msg}"),
        429 => anyhow!("Too many requests, or your quota is used up (HTTP 429). {msg}"),
        500..=599 => anyhow!("The AI service is having trouble right now (HTTP {status}). Please try again in a moment."),
        _ => anyhow!("The AI service answered with HTTP {status}: {msg}"),
    }
}

fn get_json(url: &str, headers: &[(&str, String)]) -> anyhow::Result<Value> {
    let mut req = agent(false).get(url).header("User-Agent", UA);
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let mut resp = req
        .call()
        .context("Could not reach the AI service. Check your internet connection (or that the server is running).")?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(api_error(status, &body));
    }
    serde_json::from_str(&body).context("Unexpected answer from the AI service")
}

fn anthropic_headers(key: &str) -> Vec<(&'static str, String)> {
    vec![("x-api-key", key.to_string()), ("anthropic-version", ANTHROPIC_VERSION.to_string())]
}

fn bearer(key: &Option<String>) -> Vec<(&'static str, String)> {
    key.as_deref()
        .filter(|k| !k.is_empty())
        .map(|k| vec![("Authorization", format!("Bearer {k}"))])
        .unwrap_or_default()
}

/// Models offered by the provider, for the model picker.
pub fn list_models(p: &Provider) -> anyhow::Result<Vec<ModelInfo>> {
    if p.kind == "anthropic" {
        let key = p.key.as_deref().context("Add your Anthropic API key first")?;
        let v = get_json(&format!("{}/models?limit=100", anthropic_url()), &anthropic_headers(key))?;
        return Ok(v["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| {
                        let id = m["id"].as_str()?.to_string();
                        Some(ModelInfo {
                            name: m["display_name"].as_str().unwrap_or(&id).to_string(),
                            id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default());
    }
    if p.kind == "openai" && p.key.is_none() {
        bail!("Add your OpenAI API key first");
    }
    let v = get_json(&format!("{}/models", p.openai_base()), &bearer(&p.key))?;
    // OpenAI lists every model it serves; keep the ones that can chat.
    const NOT_CHAT: &[&str] = &[
        "embed",
        "whisper",
        "tts",
        "dall-e",
        "moderation",
        "davinci",
        "babbage",
        "transcribe",
        "realtime",
        "audio",
        "image",
        "search",
    ];
    let mut models: Vec<(i64, ModelInfo)> = v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    if p.kind == "openai" && NOT_CHAT.iter().any(|x| id.contains(x)) {
                        return None;
                    }
                    Some((m["created"].as_i64().unwrap_or(0), ModelInfo { name: id.clone(), id }))
                })
                .collect()
        })
        .unwrap_or_default();
    models.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
    Ok(models.into_iter().map(|(_, m)| m).collect())
}

/// Effort control exists on current Claude models only.
fn anthropic_effort(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5", "claude-haiku-5"]
        .iter()
        .any(|p| model.starts_with(p))
}

/// Models whose safety classifiers can decline a request; server-side
/// fallbacks re-run a declined request on Anthropic's recommended model.
fn anthropic_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"].iter().any(|p| model.starts_with(p))
}

fn anthropic_max_tokens(key: &str, model: &str) -> u32 {
    get_json(
        &format!("{}/models/{}", anthropic_url(), crate::util::encode_uri_component(model)),
        &anthropic_headers(key),
    )
    .ok()
    .and_then(|v| v["max_tokens"].as_u64())
    .map(|m| m.min(32_000) as u32)
    .unwrap_or(16_000)
}

/// Runs a completion that must produce JSON matching `schema`, streaming the
/// text so far to `on_text`. Retries without optional features when the
/// server rejects them (older models, local servers).
pub fn complete(
    p: &Provider,
    system: &str,
    messages: &[Message],
    schema: &Value,
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(&str),
) -> anyhow::Result<String> {
    let max_tokens = match (p.kind.as_str(), p.key.as_deref()) {
        ("anthropic", Some(k)) => anthropic_max_tokens(k, &p.model),
        _ => 16_000,
    };
    let mut level = 0;
    loop {
        let r = if p.kind == "anthropic" {
            anthropic_stream(p, system, messages, schema, level, max_tokens, cancel, on_text)
        } else {
            openai_stream(p, system, messages, schema, level, cancel, on_text)
        };
        match r {
            Err(e) if level < 2 && e.downcast_ref::<BadRequest>().is_some() => level += 1,
            other => return other,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn anthropic_stream(
    p: &Provider,
    system: &str,
    messages: &[Message],
    schema: &Value,
    level: u8,
    max_tokens: u32,
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(&str),
) -> anyhow::Result<String> {
    let key = p.key.as_deref().context("Add your Anthropic API key in Settings ▸ AI Assistant")?;
    let mut body = json!({
        "model": p.model,
        "max_tokens": max_tokens,
        "stream": true,
        "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }],
        "messages": messages.iter().map(|m| json!({ "role": m.role, "content": m.text })).collect::<Vec<_>>(),
    });
    let mut output_config = serde_json::Map::new();
    if level < 2 {
        output_config.insert("format".into(), json!({ "type": "json_schema", "schema": schema }));
    }
    if level == 0 && anthropic_effort(&p.model) {
        output_config.insert("effort".into(), json!("medium"));
    }
    if !output_config.is_empty() {
        body["output_config"] = Value::Object(output_config);
    }
    let fallbacks = level == 0 && anthropic_fallbacks(&p.model);
    if fallbacks {
        body["fallbacks"] = json!("default");
    }
    let mut req = agent(true)
        .post(&format!("{}/messages", anthropic_url()))
        .header("User-Agent", UA)
        .header("content-type", "application/json")
        .header("x-api-key", key)
        .header("anthropic-version", ANTHROPIC_VERSION);
    if fallbacks {
        req = req.header("anthropic-beta", "server-side-fallback-2026-07-01");
    }
    let mut resp = req
        .send(body.to_string())
        .context("Could not reach Anthropic. Check your internet connection.")?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        return Err(api_error(status, &body));
    }
    let mut text = String::new();
    let mut stop = String::new();
    for line in BufReader::new(resp.body_mut().as_reader()).lines() {
        if cancel.load(Ordering::SeqCst) {
            bail!(CANCELLED);
        }
        let line = line.context("The connection to Anthropic was interrupted")?;
        let Some(data) = line.strip_prefix("data:") else { continue };
        let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { continue };
        match v["type"].as_str() {
            // Text continues across a server-side fallback: the fallback model
            // picks up where the declined one stopped.
            Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
                text.push_str(v["delta"]["text"].as_str().unwrap_or(""));
                on_text(&text);
            }
            Some("message_delta") => {
                if let Some(s) = v["delta"]["stop_reason"].as_str() {
                    stop = s.to_string();
                }
            }
            Some("error") => bail!("{}", v["error"]["message"].as_str().unwrap_or("The AI service reported an error")),
            _ => {}
        }
    }
    match stop.as_str() {
        "refusal" => bail!("The model declined this request. Try rephrasing it."),
        "max_tokens" => bail!("The answer was too long and got cut off. Try asking for fewer slides."),
        _ => Ok(text),
    }
}

fn openai_stream(
    p: &Provider,
    system: &str,
    messages: &[Message],
    schema: &Value,
    level: u8,
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(&str),
) -> anyhow::Result<String> {
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    msgs.extend(messages.iter().map(|m| json!({ "role": m.role, "content": m.text })));
    let mut body = json!({ "model": p.model, "stream": true, "messages": msgs });
    match level {
        0 => body["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "cadenza_plan", "strict": true, "schema": schema } }),
        1 => body["response_format"] = json!({ "type": "json_object" }),
        _ => {}
    }
    let mut req = agent(true)
        .post(&format!("{}/chat/completions", p.openai_base()))
        .header("User-Agent", UA)
        .header("content-type", "application/json");
    for (k, v) in bearer(&p.key) {
        req = req.header(k, v.as_str());
    }
    let what = if p.kind == "openai" { "OpenAI" } else { "the AI server" };
    let mut resp = req
        .send(body.to_string())
        .with_context(|| format!("Could not reach {what}. Check your internet connection (or that the server is running)."))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        return Err(api_error(status, &body));
    }
    let mut text = String::new();
    let mut finish = String::new();
    for line in BufReader::new(resp.body_mut().as_reader()).lines() {
        if cancel.load(Ordering::SeqCst) {
            bail!(CANCELLED);
        }
        let line = line.with_context(|| format!("The connection to {what} was interrupted"))?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(m) = v["error"]["message"].as_str() {
            bail!("{m}");
        }
        let choice = &v["choices"][0];
        if choice["delta"]["refusal"].as_str().is_some_and(|r| !r.is_empty()) {
            bail!("The model declined this request. Try rephrasing it.");
        }
        if let Some(t) = choice["delta"]["content"].as_str() {
            text.push_str(t);
            on_text(&text);
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            finish = f.to_string();
        }
    }
    match finish.as_str() {
        "length" => bail!("The answer was too long and got cut off. Try asking for fewer slides."),
        "content_filter" => bail!("The model declined this request. Try rephrasing it."),
        _ => Ok(text),
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Accepts `null` for any field (models sometimes send it) as the default.
fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct Plan {
    #[serde(deserialize_with = "nullable")]
    pub reply: String,
    #[serde(deserialize_with = "nullable")]
    pub actions: Vec<Action>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct Action {
    #[serde(deserialize_with = "nullable")]
    pub action: String,
    #[serde(deserialize_with = "nullable")]
    pub slide_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub element_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub slides: Vec<AiSlide>,
    #[serde(deserialize_with = "nullable")]
    pub text: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub value: Option<String>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct AiSlide {
    #[serde(deserialize_with = "nullable")]
    pub layout: String,
    #[serde(deserialize_with = "nullable")]
    pub title: String,
    #[serde(deserialize_with = "nullable")]
    pub subtitle: String,
    #[serde(deserialize_with = "nullable")]
    pub body: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub body2: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub code: String,
    #[serde(deserialize_with = "nullable")]
    pub code_language: String,
    #[serde(deserialize_with = "nullable")]
    pub chart_type: String,
    #[serde(deserialize_with = "nullable")]
    pub chart_labels: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    pub chart_series: Vec<AiSeries>,
    #[serde(deserialize_with = "nullable")]
    pub table: Vec<Vec<String>>,
    #[serde(deserialize_with = "nullable")]
    pub diagram: String,
    #[serde(deserialize_with = "nullable")]
    pub image_query: String,
    #[serde(deserialize_with = "nullable")]
    pub notes: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct AiSeries {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub values: Vec<f64>,
}

const SLIDE_LAYOUTS: &[&str] = &[
    "title",
    "section",
    "bullets",
    "two_column",
    "statement",
    "quote",
    "big_number",
    "code",
    "chart",
    "table",
    "diagram",
    "image_text",
];
const ACTIONS: &[&str] = &["insert", "replace", "set_text", "notes", "delete", "set_theme", "set_title"];

/// JSON schema of the plan (strict-mode compatible: every field required,
/// no recursion, `additionalProperties: false` everywhere).
pub fn plan_schema() -> Value {
    let s = json!({ "type": "string" });
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    let nullable_s = json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] });
    let slide_props = json!({
        "layout": { "type": "string", "enum": SLIDE_LAYOUTS },
        "title": s, "subtitle": s, "body": strings, "body2": strings,
        "code": s, "code_language": s,
        "chart_type": { "type": "string", "enum": ["", "column", "bar", "line", "area", "pie", "donut"] },
        "chart_labels": strings,
        "chart_series": { "type": "array", "items": {
            "type": "object", "additionalProperties": false,
            "properties": { "name": s, "values": { "type": "array", "items": { "type": "number" } } },
            "required": ["name", "values"],
        } },
        "table": { "type": "array", "items": strings },
        "diagram": s, "image_query": s, "notes": s,
    });
    let slide_required: Vec<&String> = slide_props.as_object().map(|o| o.keys().collect()).unwrap_or_default();
    let slide = json!({ "type": "object", "additionalProperties": false, "properties": slide_props, "required": slide_required });
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "reply": s,
            "actions": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "properties": {
                    "action": { "type": "string", "enum": ACTIONS },
                    "slide_id": nullable_s, "element_id": nullable_s,
                    "slides": { "type": "array", "items": slide },
                    "text": strings, "value": nullable_s,
                },
                "required": ["action", "slide_id", "element_id", "slides", "text", "value"],
            } },
        },
        "required": ["reply", "actions"],
    })
}

/// Parses the model's answer (tolerating code fences or stray prose when
/// the server could not enforce the schema).
pub fn parse_plan(text: &str) -> anyhow::Result<Plan> {
    let t = text.trim();
    if let Ok(p) = serde_json::from_str::<Plan>(t) {
        return Ok(p);
    }
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        if a < b {
            if let Ok(p) = serde_json::from_str::<Plan>(&t[a..=b]) {
                return Ok(p);
            }
        }
    }
    bail!("The AI's answer could not be understood. Please try again (or pick a more capable model).")
}

/// Progress while the answer streams in: slides started so far and the
/// title of the latest one.
pub fn progress(text: &str) -> (usize, String) {
    let count = text.matches("\"layout\"").count();
    // The latest title whose string is complete.
    let complete = |i: usize| -> Option<String> {
        let rest = text[i + 7..].trim_start().strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let mut escaped = false;
        for (j, c) in rest.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return serde_json::from_str::<String>(&format!("\"{}\"", &rest[..j])).ok(),
                _ => escaped = false,
            }
        }
        None
    };
    let positions: Vec<usize> = text.match_indices("\"title\"").map(|(i, _)| i).collect();
    let title = positions.iter().rev().find_map(|&i| complete(i)).unwrap_or_default();
    (count, title)
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

pub const SYSTEM_PROMPT: &str = r#"You are the assistant inside Cadenza, a presentation app built on reveal.js. You create and edit slides by answering with a JSON object that follows the provided schema: a short `reply` for the user and a list of `actions` that Cadenza applies to the presentation. Cadenza designs each slide from your content, so focus on clear, well-structured content.

Slide layouts (field `layout`) and the fields each one uses:
- "title": opening slide: `title`, `subtitle`.
- "section": divider between parts: `title`; `subtitle` is a short kicker shown above it (e.g. "PART 2").
- "bullets": the workhorse: `title` and `body` (3 to 6 short bullet points).
- "two_column": comparisons: `title`, `body` (left column) and `body2` (right column). Start each column with a bold label such as "**Before**".
- "statement": one bold sentence in `title`, nothing else.
- "quote": the quotation in `title` (without quote marks) and who said it in `subtitle`.
- "big_number": a striking figure in `title` (e.g. "73%") and what it means in `subtitle`.
- "code": `title`, `code` and `code_language` (e.g. "python").
- "chart": `title`, `chart_type` (column, bar, line, area, pie or donut), `chart_labels` (the categories) and `chart_series` (each with a `name` and exactly one number per label). Optionally 2 or 3 takeaways in `body`.
- "table": `title` and `table` (the first row is the header; at most 6 rows and 5 columns). Optional takeaways in `body`.
- "diagram": `title` and `diagram` with valid Mermaid source (flowchart, sequenceDiagram, timeline, mindmap, gantt, pie...). Keep it small (12 nodes or fewer) and do not add styling directives.
- "image_text": `title`, `body`, and `image_query`: 2 to 5 English keywords for a stock-photo search (e.g. "solar panels rooftop").
Leave every field a layout doesn't use as an empty string or an empty array.

Writing rules:
- Slides support a talk; they are not documents. Use short phrases: at most 12 words per bullet and 6 bullets per slide.
- Inline formatting in text: **bold**, *italic*, `code`. Write math as LaTeX between dollar signs: $E = mc^2$ inline, $$\int_0^1 x^2\,dx$$ on its own.
- Put the detail in `notes` (speaker notes): 2 to 4 natural sentences the presenter can say.
- Vary the layouts. Use charts and tables only with data the user provided or well-established facts; never present invented statistics as real. Label illustrative numbers as such.
- Write in the language of the user's request unless they ask for another language.

Actions:
- "insert": add `slides` after the slide whose id is `slide_id` ("START" inserts at the beginning, null at the end).
- "replace": rebuild the slide `slide_id` from scratch with the single slide in `slides`.
- "set_text": rewrite the text element `element_id` on slide `slide_id` with `text` (one entry per paragraph or bullet), keeping the slide's design. Prefer this for wording changes, translation, shortening and proofreading.
- "notes": set the speaker notes of slide `slide_id` to `value`.
- "delete": remove the slide `slide_id`.
- "set_theme": switch to the theme whose id is `value` (only ids from the list you are given).
- "set_title": rename the presentation to `value`.
Fill the fields an action doesn't use with null or empty arrays. Only use ids that exist in the presentation you were given. Keep `reply` to one or two sentences, in the user's language, saying what you changed. If the request is unclear or not about the presentation, answer in `reply` and return no actions."#;

/// Plain text of rich-text HTML (lists as "• " lines).
pub fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if in_tag {
            if c == '>' {
                in_tag = false;
                let t = tag.trim().to_ascii_lowercase();
                let name = t.split(|c: char| c.is_whitespace() || c == '/').find(|s| !s.is_empty()).unwrap_or("");
                if t.starts_with("li") {
                    out.push_str("\n• ");
                } else if matches!(name, "p" | "br" | "div" | "h1" | "h2" | "h3" | "h4" | "tr" | "ul" | "ol") && (t.starts_with('/') || name == "br") {
                    out.push('\n');
                } else if name == "td" || name == "th" {
                    out.push_str(" | ");
                }
            } else {
                tag.push(c);
            }
            continue;
        }
        if c == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        out.push(c);
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let lines: Vec<&str> = out.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    lines.join("\n").trim().to_string()
}

/// A compact description of the deck for the model.
pub fn deck_context(deck: &Deck, current: Option<&str>, selected: &[String]) -> Value {
    let slides: Vec<Value> = deck
        .slides
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let elements: Vec<Value> = s
                .elements
                .iter()
                .filter_map(|e| match &e.content {
                    Content::Text(_) | Content::Shape(_) => {
                        let t = e.content.text()?;
                        let text = plain_text(&t.html);
                        if text.is_empty() {
                            return None;
                        }
                        Some(json!({ "id": e.id, "kind": e.kind(), "role": t.style.role, "text": text }))
                    }
                    Content::Chart(c) => Some(json!({ "id": e.id, "kind": "chart", "chart_type": c.chart, "labels": c.labels,
                        "series": c.series.iter().map(|x| json!({ "name": x.name, "values": x.values })).collect::<Vec<_>>() })),
                    Content::Table(t) => Some(json!({ "id": e.id, "kind": "table",
                        "rows": t.rows.iter().take(8).map(|r| r.iter().map(|c| plain_text(c)).collect::<Vec<_>>()).collect::<Vec<_>>() })),
                    Content::Code(c) => {
                        Some(json!({ "id": e.id, "kind": "code", "language": c.language, "code": c.code.chars().take(1500).collect::<String>() }))
                    }
                    Content::Diagram(d) => Some(json!({ "id": e.id, "kind": "diagram", "mermaid": d.source })),
                    Content::Math(m) => Some(json!({ "id": e.id, "kind": "equation", "latex": m.latex })),
                    Content::Image(img) => Some(json!({ "id": e.id, "kind": "image", "description": img.alt })),
                    Content::Video(_) => Some(json!({ "id": e.id, "kind": "video" })),
                    _ => None,
                })
                .collect();
            let mut v = json!({ "id": s.id, "number": i + 1, "layout": s.layout, "elements": elements });
            if !s.notes.trim().is_empty() {
                v["notes"] = json!(s.notes);
            }
            if s.hidden {
                v["skipped"] = json!(true);
            }
            v
        })
        .collect();
    json!({
        "title": deck.title,
        "theme": deck.theme,
        "slide_size": format!("{}x{}", deck.width, deck.height),
        "current_slide": current,
        "selected_slides": selected,
        "slides": slides,
    })
}

// ---------------------------------------------------------------------------
// Building slides
// ---------------------------------------------------------------------------

/// A photo found for an `image_text` slide: asset name, size and credit.
#[derive(Clone, Debug)]
pub struct FoundImage {
    pub src: String,
    pub width: u32,
    pub height: u32,
    pub alt: String,
}

/// Strips list markers and heading hashes a model may add.
fn clean_item(s: &str) -> &str {
    let t = s.trim();
    let t = t.trim_start_matches('#').trim_start();
    for m in ["- ", "* ", "• ", "– ", "· "] {
        if let Some(r) = t.strip_prefix(m) {
            return r.trim_start();
        }
    }
    // "1. " / "2) "
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 3 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r.trim_start();
        }
    }
    t
}

/// Finds `$…$` / `$$…$$` spans (same spirit as the math renderer: no space
/// just inside single dollars, so "$5 and $10" stays text).
fn math_spans(chars: &[char]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' || (i > 0 && chars[i - 1] == '\\') {
            i += 1;
            continue;
        }
        let dbl = chars.get(i + 1) == Some(&'$');
        let start = i + if dbl { 2 } else { 1 };
        if !dbl && chars.get(start).is_none_or(|c| c.is_whitespace()) {
            i += 1;
            continue;
        }
        let mut j = start;
        let mut end = None;
        while j < chars.len() {
            if chars[j] == '$' && chars[j - 1] != '\\' {
                if dbl {
                    if chars.get(j + 1) == Some(&'$') {
                        end = Some(j + 2);
                        break;
                    }
                } else if !chars[j - 1].is_whitespace() && !chars.get(j + 1).is_some_and(|c| c.is_ascii_digit()) {
                    end = Some(j + 1);
                    break;
                }
            }
            j += 1;
        }
        match end {
            Some(e) if e > start + if dbl { 2 } else { 1 } => {
                spans.push((i, e));
                i = e;
            }
            _ => i += 1,
        }
    }
    spans
}

/// Escapes what would start a Markdown block ("2024. Results" is not a list).
fn guard_block(s: &str) -> String {
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    let rest = &s[digits..];
    if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ") || rest == "." || rest == ")") {
        return format!("{}\\{}", &s[..digits], rest);
    }
    match s.chars().next() {
        Some('>' | '+' | '-' | '=' | '|' | '#') => format!("\\{s}"),
        _ => s.to_string(),
    }
}

/// Inline Markdown (bold, italic, code, links) to HTML, leaving math intact.
fn inline_html(s: &str) -> String {
    let s = guard_block(s.trim());
    let chars: Vec<char> = s.chars().collect();
    let spans = math_spans(&chars);
    let mut maths: Vec<String> = Vec::new();
    let mut prepared = String::new();
    let mut i = 0;
    for (a, b) in spans {
        prepared.extend(&chars[i..a]);
        prepared.push('\u{E000}');
        prepared.push_str(&maths.len().to_string());
        prepared.push('\u{E001}');
        maths.push(chars[a..b].iter().collect());
        i = b;
    }
    prepared.extend(&chars[i..]);
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(&prepared, Options::ENABLE_STRIKETHROUGH));
    let mut out = out.trim().to_string();
    if out.starts_with("<p>") && out.ends_with("</p>") && out.matches("<p>").count() == 1 {
        out = out[3..out.len() - 4].to_string();
    }
    for (k, m) in maths.iter().enumerate() {
        out = out.replace(&format!("\u{E000}{k}\u{E001}"), &esc(m));
    }
    out
}

fn items(list: &[String]) -> Vec<String> {
    list.iter().map(|s| clean_item(s).to_string()).filter(|s| !s.is_empty()).collect()
}

fn list_html(list: &[String]) -> String {
    let li: String = items(list).iter().map(|s| format!("<li>{}</li>", inline_html(s))).collect();
    if li.is_empty() { String::new() } else { format!("<ul>{li}</ul>") }
}

fn paras_html(list: &[String]) -> String {
    items(list).iter().map(|s| format!("<p>{}</p>", inline_html(s))).collect()
}

/// Bullets for several items, a paragraph for one.
fn body_html(list: &[String]) -> String {
    if items(list).len() > 1 { list_html(list) } else { paras_html(list) }
}

fn is_wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// Mostly right-to-left text (Arabic, Hebrew...).
pub fn is_rtl(s: &str) -> bool {
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    letters > 0 && s.chars().filter(|c| is_rtl_char(*c)).count() * 2 > letters
}

/// Approximate rendered width of text in em.
fn width_em(s: &str) -> f64 {
    let chars: Vec<char> = s.chars().collect();
    let spans = math_spans(&chars);
    let mut w = 0.0;
    for (i, c) in chars.iter().enumerate() {
        let in_math = spans.iter().any(|(a, b)| i >= *a && i < *b);
        w += if in_math {
            0.3
        } else if is_wide(*c) {
            1.0
        } else if matches!(c, '*' | '`') {
            0.0
        } else {
            0.52
        };
    }
    w
}

/// Shrinks a text element's font until `lines` fit its box.
fn fit_text(el: &mut Element, lines: &[String], bulleted: bool) {
    let (w, h) = (el.w, el.h);
    let Some(t) = el.content.text_mut() else { return };
    let base = t.style.font_size;
    let lh = if t.style.line_height > 0.0 { t.style.line_height } else { 1.3 };
    let min = (base * 0.5).max(14.0);
    let mut size = base;
    while size > min {
        let per_line = (w / size - if bulleted { 1.6 } else { 0.2 }).max(4.0);
        let rows: f64 = lines.iter().map(|l| (width_em(l) / per_line).ceil().max(1.0)).sum();
        let gaps = lines.len().saturating_sub(1) as f64 * 0.45;
        if (rows * lh + gaps) * size <= h {
            break;
        }
        size = (size * 0.93).floor();
    }
    t.style.font_size = size.max(min);
}

/// Sets HTML on a text element, fitting it to the box and following the
/// text direction.
fn fill(el: &mut Element, html_text: &str, lines: &[String], bulleted: bool) {
    let rtl = is_rtl(&lines.join(" "));
    if let Some(t) = el.content.text_mut() {
        t.html = sanitize::rich_text(html_text);
        if rtl && t.style.align == "left" {
            t.style.align = "right".into();
        }
    }
    fit_text(el, lines, bulleted);
}

fn text_of(s: &str) -> Vec<String> {
    vec![s.trim().to_string()]
}

fn place(el: &mut Element, f: (f64, f64), (x, y, w, h): (f64, f64, f64, f64)) {
    el.x = (f.0 * x).round();
    el.y = (f.1 * y).round();
    el.w = (f.0 * w).round();
    el.h = (f.1 * h).round();
}

fn chart_element(s: &AiSlide, width: u32, height: u32) -> Option<Element> {
    let labels: Vec<String> = s.chart_labels.iter().map(|l| l.trim().to_string()).collect();
    let series: Vec<Series> = s
        .chart_series
        .iter()
        .filter(|x| !x.values.is_empty())
        .map(|x| {
            let mut values: Vec<f64> = x.values.iter().map(|v| if v.is_finite() { *v } else { 0.0 }).collect();
            values.resize(labels.len(), 0.0);
            Series {
                name: x.name.trim().to_string(),
                values,
                color: String::new(),
            }
        })
        .collect();
    if labels.is_empty() || series.is_empty() {
        return None;
    }
    let kind = if ["column", "bar", "line", "area", "pie", "donut"].contains(&s.chart_type.as_str()) {
        s.chart_type.as_str()
    } else {
        "column"
    };
    let mut el = layouts::new_element("chart", kind, width, height);
    if let Content::Chart(c) = &mut el.content {
        c.labels = labels;
        c.show_legend = series.len() > 1 || matches!(kind, "pie" | "donut");
        c.series = series;
    }
    Some(el)
}

fn table_element(s: &AiSlide, width: u32, height: u32) -> Option<Element> {
    let cols = s.table.iter().map(Vec::len).max().unwrap_or(0).min(8);
    let rows: Vec<Vec<String>> = s
        .table
        .iter()
        .take(12)
        .map(|r| {
            let mut r: Vec<String> = r.iter().take(cols).map(|c| sanitize::rich_text(&inline_html(c.trim()))).collect();
            r.resize(cols, String::new());
            r
        })
        .collect();
    if rows.len() < 2 || cols == 0 {
        return None;
    }
    let mut el = layouts::new_element("table", "", width, height);
    if let Content::Table(t) = &mut el.content {
        let scale = (6.0 / rows.len() as f64).min(5.0 / cols as f64).min(1.0);
        t.font_size = (t.font_size * scale.max(0.6)).round();
        t.rows = rows;
        t.header_row = true;
    }
    Some(el)
}

/// Builds a slide from the model's description with Cadenza's layouts.
pub fn build_slide(s: &AiSlide, width: u32, height: u32, image: Option<&FoundImage>) -> Slide {
    let size = (width as f64, height as f64);
    let mut kind = s.layout.as_str();
    let chart = (kind == "chart").then(|| chart_element(s, width, height)).flatten();
    let table = (kind == "table").then(|| table_element(s, width, height)).flatten();
    let has_code = !s.code.trim().is_empty();
    let has_diagram = !s.diagram.trim().is_empty();
    // Fall back to bullets when the content for a special layout is missing.
    if (kind == "chart" && chart.is_none()) || (kind == "table" && table.is_none()) || (kind == "code" && !has_code) || (kind == "diagram" && !has_diagram) {
        kind = "bullets";
    }
    let layout = match kind {
        "title" => "title",
        "section" => "section",
        "two_column" => "two-column",
        "statement" => "statement",
        "quote" => "quote",
        "big_number" => "big-number",
        "code" => "code",
        "image_text" => "image-text",
        "chart" | "table" | "diagram" => "title-only",
        _ => "title-content",
    };
    let mut slide = layouts::new_slide(layout, width, height);
    slide.notes = s.notes.trim().to_string();
    let title_html = format!("<p>{}</p>", inline_html(s.title.trim()));
    let els = &mut slide.elements;
    match layout {
        "title" | "big-number" => {
            fill(&mut els[0], &title_html, &text_of(&s.title), false);
            let sub = if s.subtitle.trim().is_empty() { s.body.join(" ") } else { s.subtitle.clone() };
            fill(&mut els[1], &format!("<p>{}</p>", inline_html(sub.trim())), &text_of(&sub), false);
        }
        "section" => {
            fill(&mut els[0], &format!("<p>{}</p>", inline_html(s.subtitle.trim())), &text_of(&s.subtitle), false);
            fill(&mut els[1], &title_html, &text_of(&s.title), false);
        }
        "statement" => {
            let t = if s.title.trim().is_empty() { s.body.join(" ") } else { s.title.clone() };
            fill(&mut els[0], &format!("<p>{}</p>", inline_html(t.trim())), &text_of(&t), false);
        }
        "quote" => {
            let q = s.title.trim().trim_matches(|c| matches!(c, '"' | '“' | '”' | '«' | '»' | '„'));
            let q = format!("“{q}”");
            fill(&mut els[0], &format!("<p>{}</p>", inline_html(&q)), &text_of(&q), false);
            let who = s.subtitle.trim().trim_start_matches(['—', '-', '–']).trim();
            let who = if who.is_empty() { String::new() } else { format!("— {who}") };
            fill(&mut els[1], &format!("<p>{}</p>", inline_html(&who)), &text_of(&who), false);
        }
        "two-column" => {
            fill(&mut els[0], &title_html, &text_of(&s.title), false);
            fill(&mut els[1], &list_html(&s.body), &items(&s.body), true);
            fill(&mut els[2], &list_html(&s.body2), &items(&s.body2), true);
        }
        "code" => {
            fill(&mut els[0], &title_html, &text_of(&s.title), false);
            let (ew, eh) = (els[1].w, els[1].h);
            if let Content::Code(c) = &mut els[1].content {
                c.code = s.code.trim_end().to_string();
                c.language = match s.code_language.trim() {
                    "" => "plaintext".into(),
                    l => l.to_ascii_lowercase(),
                };
                let lines = c.code.lines().count().max(1) as f64;
                let longest = c.code.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
                let fit_h = eh / (lines * 1.45);
                let fit_w = ew / (longest * 0.62 + 2.0);
                c.font_size = c.font_size.min(fit_h).min(fit_w).max(12.0).round();
            }
        }
        "image-text" => {
            if let Content::Image(img) = &mut els[0].content {
                match image {
                    Some(found) => {
                        img.src = found.src.clone();
                        img.alt = found.alt.clone();
                        img.natural_width = found.width;
                        img.natural_height = found.height;
                    }
                    // Kept as a hint for picking a photo by hand.
                    None => img.alt = s.image_query.trim().to_string(),
                }
            }
            fill(&mut els[1], &title_html, &text_of(&s.title), false);
            fill(&mut els[2], &body_html(&s.body), &items(&s.body), items(&s.body).len() > 1);
        }
        "title-only" => {
            fill(&mut els[0], &title_html, &text_of(&s.title), false);
            let side = !items(&s.body).is_empty();
            let mut main = if let Some(c) = chart {
                c
            } else if let Some(t) = table {
                t
            } else {
                Element {
                    name: "Diagram".into(),
                    content: Content::Diagram(DiagramContent {
                        source: s.diagram.trim().to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            };
            if side {
                place(&mut main, size, (0.45, 0.26, 0.48, 0.66));
                let mut text = layouts::text("body", "", (28.0 * (size.0 / 1280.0).min(size.1 / 720.0)).round(), "left");
                place(&mut text, size, (0.07, 0.28, 0.35, 0.6));
                fill(&mut text, &list_html(&s.body), &items(&s.body), true);
                els.push(text);
            } else {
                place(&mut main, size, (0.07, 0.26, 0.86, 0.66));
            }
            if let Content::Table(t) = &main.content {
                // Tables take the height their rows need.
                let need = t.rows.len() as f64 * t.font_size * 2.3;
                main.h = main.h.min(need.round());
            }
            els.push(main);
        }
        _ => {
            fill(&mut els[0], &title_html, &text_of(&s.title), false);
            fill(&mut els[1], &body_html(&s.body), &items(&s.body), items(&s.body).len() > 1);
        }
    }
    // Drop text boxes left empty (titles stay, so they can be typed into).
    slide.elements.retain(|e| match &e.content {
        Content::Text(t) => !t.text.html.is_empty() || t.text.style.role == "title",
        _ => true,
    });
    slide
}

// ---------------------------------------------------------------------------
// Applying a plan
// ---------------------------------------------------------------------------

#[derive(Serialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub inserted: usize,
    pub replaced: usize,
    pub edited: usize,
    pub notes: usize,
    pub deleted: usize,
    pub theme: bool,
    pub title: bool,
    /// Slide to show afterwards.
    pub slide: Option<String>,
    /// Slides created by the plan.
    pub created: Vec<String>,
}

impl Summary {
    pub fn is_empty(&self) -> bool {
        self.inserted + self.replaced + self.edited + self.notes + self.deleted == 0 && !self.theme && !self.title
    }
}

/// Image searches the plan asks for (deduplicated).
pub fn image_queries(plan: &Plan) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in &plan.actions {
        if a.action != "insert" && a.action != "replace" {
            continue;
        }
        for s in &a.slides {
            let q = s.image_query.trim();
            if s.layout == "image_text" && !q.is_empty() && !out.iter().any(|x| x == q) {
                out.push(q.to_string());
            }
        }
    }
    out
}

/// Applies the plan to `deck`. `theme_ok` tells which theme ids may be set.
pub fn apply_plan(deck: &mut Deck, plan: &Plan, images: &HashMap<String, FoundImage>, theme_ok: &dyn Fn(&str) -> bool) -> Summary {
    let (w, h) = (deck.width, deck.height);
    let mut sum = Summary::default();
    let build = |s: &AiSlide| build_slide(s, w, h, images.get(s.image_query.trim()));
    // Inserting after a slide keeps later inserts in reading order.
    let mut last_insert: HashMap<String, String> = HashMap::new();
    for a in &plan.actions {
        let sid = a.slide_id.as_deref().map(str::trim).unwrap_or("");
        match a.action.as_str() {
            "insert" => {
                let key = sid.to_string();
                let anchor = last_insert.get(&key).cloned().unwrap_or(key.clone());
                let start = if anchor.eq_ignore_ascii_case("start") {
                    0
                } else {
                    deck.slide_index(&anchor).map(|i| i + 1).unwrap_or(deck.slides.len())
                };
                for (offset, s) in a.slides.iter().enumerate() {
                    let slide = build(s);
                    sum.slide.get_or_insert(slide.id.clone());
                    sum.created.push(slide.id.clone());
                    last_insert.insert(key.clone(), slide.id.clone());
                    deck.slides.insert(start + offset, slide);
                    sum.inserted += 1;
                }
            }
            "replace" => {
                let (Some(i), Some(s)) = (deck.slide_index(sid), a.slides.first()) else {
                    continue;
                };
                let mut slide = build(s);
                let old = &deck.slides[i];
                slide.nested = old.nested;
                slide.hidden = old.hidden;
                slide.transition = old.transition.clone();
                slide.transition_speed = old.transition_speed.clone();
                slide.background = old.background.clone();
                if slide.notes.is_empty() {
                    slide.notes = old.notes.clone();
                }
                sum.slide.get_or_insert(slide.id.clone());
                sum.created.push(slide.id.clone());
                deck.slides[i] = slide;
                sum.replaced += 1;
            }
            "set_text" => {
                let Some(i) = deck.slide_index(sid) else { continue };
                let eid = a.element_id.as_deref().unwrap_or("");
                let Some(el) = deck.slides[i].elements.iter_mut().find(|e| e.id == eid) else {
                    continue;
                };
                let Some(t) = el.content.text() else { continue };
                let list = t.html.contains("<ul") || t.html.contains("<ol");
                let ordered = t.html.contains("<ol");
                let base = t.style.font_size;
                let lines = items(&a.text);
                if lines.is_empty() {
                    continue;
                }
                let mut html_text = if list { list_html(&a.text) } else { paras_html(&a.text) };
                if ordered {
                    html_text = html_text.replacen("<ul>", "<ol>", 1).replace("</ul>", "</ol>");
                }
                fill(el, &html_text, &lines, list);
                // Only ever shrink: the user's chosen size is the maximum.
                if let Some(t) = el.content.text_mut() {
                    t.style.font_size = t.style.font_size.min(base);
                }
                sum.slide.get_or_insert(deck.slides[i].id.clone());
                sum.edited += 1;
            }
            "notes" => {
                let Some(i) = deck.slide_index(sid) else { continue };
                let notes = a.value.clone().unwrap_or_else(|| a.text.join("\n"));
                deck.slides[i].notes = notes.trim().to_string();
                sum.notes += 1;
            }
            "delete" => {
                if deck.slides.len() <= 1 {
                    continue;
                }
                if let Some(i) = deck.slide_index(sid) {
                    deck.slides.remove(i);
                    sum.deleted += 1;
                }
            }
            "set_theme" => {
                if let Some(t) = a.value.as_deref().map(str::trim).filter(|t| theme_ok(t)) {
                    deck.theme = t.to_string();
                    sum.theme = true;
                }
            }
            "set_title" => {
                if let Some(t) = a.value.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    deck.title = t.chars().take(200).collect();
                    sum.title = true;
                }
            }
            _ => {}
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(layout: &str) -> AiSlide {
        AiSlide {
            layout: layout.into(),
            title: "Hello".into(),
            ..Default::default()
        }
    }

    #[test]
    fn schema_is_strict() {
        fn check(v: &Value) {
            if v["type"] == "object" {
                assert_eq!(v["additionalProperties"], false);
                let props: Vec<&String> = v["properties"].as_object().unwrap().keys().collect();
                let req: Vec<&str> = v["required"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
                for p in props {
                    assert!(req.contains(&p.as_str()), "{p} not required");
                }
            }
            if let Some(o) = v.as_object() {
                o.values().for_each(check);
            }
            if let Some(a) = v.as_array() {
                a.iter().for_each(check);
            }
        }
        check(&plan_schema());
    }

    #[test]
    fn parses_plans_leniently() {
        let p = parse_plan("```json\n{\"reply\": \"ok\", \"actions\": [{\"action\": \"insert\", \"slide_id\": null, \"element_id\": null, \"slides\": [{\"layout\": \"bullets\", \"title\": \"A\", \"body\": [\"x\"], \"notes\": null}], \"text\": [], \"value\": null}]}\n```").unwrap();
        assert_eq!(p.reply, "ok");
        assert_eq!(p.actions[0].slides[0].title, "A");
        assert!(parse_plan("no json here").is_err());
    }

    #[test]
    fn inline_markdown_keeps_math() {
        assert_eq!(inline_html("**Bold** and $x_1 \\{a\\}$"), "<strong>Bold</strong> and $x_1 \\{a\\}$");
        assert_eq!(inline_html("costs $5 or $10 *now*"), "costs $5 or $10 <em>now</em>");
        assert_eq!(inline_html("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(clean_item("- point"), "point");
        assert_eq!(clean_item("2. second"), "second");
        assert_eq!(inline_html("2024. A *year*"), "2024. A <em>year</em>");
    }

    #[test]
    fn builds_every_layout() {
        for l in SLIDE_LAYOUTS {
            let mut s = slide(l);
            s.body = vec!["One".into(), "Two".into()];
            s.body2 = vec!["Three".into()];
            s.code = "print(1)".into();
            s.chart_labels = vec!["A".into(), "B".into()];
            s.chart_series = vec![AiSeries {
                name: "S".into(),
                values: vec![1.0],
            }];
            s.table = vec![vec!["H1".into(), "H2".into()], vec!["a".into(), "b".into()]];
            s.diagram = "graph TD; A-->B".into();
            let built = build_slide(&s, 1280, 720, None);
            assert!(!built.elements.is_empty(), "{l}");
            for e in &built.elements {
                assert!(e.x >= 0.0 && e.y >= 0.0 && e.x + e.w <= 1281.0 && e.y + e.h <= 721.0, "{l}");
            }
        }
        // Charts pad missing values; empty charts become bullet slides.
        let mut s = slide("chart");
        s.chart_labels = vec!["A".into(), "B".into()];
        s.chart_series = vec![AiSeries {
            name: "S".into(),
            values: vec![3.0],
        }];
        let b = build_slide(&s, 1280, 720, None);
        assert!(
            b.elements
                .iter()
                .any(|e| matches!(&e.content, Content::Chart(c) if c.series[0].values == vec![3.0, 0.0]))
        );
        assert_eq!(build_slide(&slide("chart"), 1280, 720, None).layout, "title-content");
    }

    #[test]
    fn long_text_shrinks_and_rtl_aligns() {
        let mut s = slide("bullets");
        s.body = (0..7)
            .map(|i| format!("A rather long bullet point number {i} that keeps going on and on, well past the width of a single line"))
            .collect();
        let b = build_slide(&s, 1280, 720, None);
        let body = b.elements.iter().find(|e| e.content.text().is_some_and(|t| t.style.role == "body")).unwrap();
        assert!(body.content.text().unwrap().style.font_size < 32.0);
        let mut ar = slide("bullets");
        ar.title = "مرحبا بالعالم".into();
        let b = build_slide(&ar, 1280, 720, None);
        assert_eq!(b.elements[0].content.text().unwrap().style.align, "right");
    }

    #[test]
    fn applies_actions() {
        let mut deck = crate::layouts::starter_deck("cadenza-midnight", 1280, 720);
        let first = deck.slides[0].id.clone();
        let title_el = deck.slides[0].elements[0].id.clone();
        let plan = Plan {
            reply: "done".into(),
            actions: vec![
                Action {
                    action: "insert".into(),
                    slide_id: Some(first.clone()),
                    slides: vec![slide("bullets"), slide("statement")],
                    ..Default::default()
                },
                Action {
                    action: "insert".into(),
                    slide_id: Some(first.clone()),
                    slides: vec![slide("quote")],
                    ..Default::default()
                },
                Action {
                    action: "set_text".into(),
                    slide_id: Some(first.clone()),
                    element_id: Some(title_el.clone()),
                    text: vec!["New *title*".into()],
                    ..Default::default()
                },
                Action {
                    action: "notes".into(),
                    slide_id: Some(first.clone()),
                    value: Some("Say hi".into()),
                    ..Default::default()
                },
                Action {
                    action: "set_theme".into(),
                    value: Some("nope".into()),
                    ..Default::default()
                },
                Action {
                    action: "set_title".into(),
                    value: Some("Deck".into()),
                    ..Default::default()
                },
            ],
        };
        let sum = apply_plan(&mut deck, &plan, &HashMap::new(), &|t| t == "black");
        assert_eq!((sum.inserted, sum.edited, sum.notes, sum.theme, sum.title), (3, 1, 1, false, true));
        let layouts: Vec<&str> = deck.slides.iter().map(|s| s.layout.as_str()).collect();
        assert_eq!(layouts, ["title", "title-content", "statement", "quote"]);
        assert_eq!(deck.slides[0].notes, "Say hi");
        assert!(deck.slides[0].elements[0].content.text().unwrap().html.contains("<em>title</em>"));
        assert_eq!(deck.title, "Deck");
        // Deleting never empties the deck.
        let ids: Vec<String> = deck.slides.iter().map(|s| s.id.clone()).collect();
        let del = Plan {
            actions: ids
                .iter()
                .map(|id| Action {
                    action: "delete".into(),
                    slide_id: Some(id.clone()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        apply_plan(&mut deck, &del, &HashMap::new(), &|_| false);
        assert_eq!(deck.slides.len(), 1);
    }

    #[test]
    fn progress_reads_partial_json() {
        let (n, t) =
            progress(r#"{"reply":"","actions":[{"action":"insert","slides":[{"layout":"title","title":"Intro \"x\""},{"layout":"bullets","title":"Mark"#);
        assert_eq!(n, 2);
        assert_eq!(t, "Intro \"x\"");
    }

    #[test]
    fn plain_text_of_lists() {
        assert_eq!(plain_text("<p>Hi &amp; bye</p><ul><li>One</li><li>Two</li></ul>"), "Hi & bye\n• One\n• Two");
    }
}
