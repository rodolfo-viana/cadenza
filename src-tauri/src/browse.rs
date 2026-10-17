//! "Browse" mode for online media: the real YouTube / Vimeo / GIPHY website
//! opens in a Cadenza window, the user searches as usual and clicks "Insert"
//! on the item they want. No API keys involved.
//!
//! The site never gets access to Cadenza: an injected bar triggers a
//! navigation to `https://cadenza.invalid/...`, which the window's
//! navigation handler intercepts (and cancels).

use tauri::Url;

pub const PICK_HOST: &str = "cadenza.invalid";

pub fn start_url(provider: &str) -> Option<&'static str> {
    Some(match provider {
        "youtube" => "https://www.youtube.com/",
        "vimeo" => "https://vimeo.com/watch",
        "giphy" => "https://giphy.com/",
        _ => return None,
    })
}

/// Script injected into the provider's pages: a floating Cadenza bar.
/// The bar's script; `tr` translates interface text.
pub fn bar_script(provider: &str, tr: &dyn Fn(&str) -> String) -> String {
    let (item_re, label, hint) = match provider {
        "youtube" => (r"youtube\.com\/(watch\?|shorts\/)", "Insert this video", "Open a video to insert it"),
        "vimeo" => (r"vimeo\.com\/(\d+)(\/|\?|$)", "Insert this video", "Open a video to insert it"),
        _ => (r"giphy\.com\/(gifs|stickers)\/", "Insert this GIF", "Open a GIF to insert it"),
    };
    // JSON string literals: translations may contain quotes.
    let js = |s: &str| serde_json::to_string(&tr(s)).unwrap_or_else(|_| "\"\"".into());
    let (label, hint, cancel_label) = (js(label), js(hint), js("Cancel"));
    format!(
        r#"(function () {{
  if (window.top !== window) return;
  var ITEM = /{item_re}/;
  var host, btn, hintEl;
  function go(path) {{ location.href = 'https://{PICK_HOST}/' + path; }}
  function mount() {{
    if (host || !document.body) return;
    host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:50%;bottom:18px;transform:translateX(-50%);z-index:2147483647;';
    var root = host.attachShadow({{ mode: 'closed' }});
    // Built with DOM APIs: sites like YouTube enforce Trusted Types, which
    // forbid assigning HTML strings.
    var style = document.createElement('style');
    style.textContent =
      '.bar{{display:flex;align-items:center;gap:10px;padding:8px 8px 8px 14px;border-radius:14px;background:rgba(20,20,26,.94);color:#eee;font:500 13px/1.2 system-ui,sans-serif;box-shadow:0 10px 40px rgba(0,0,0,.45);}}' +
      '.logo{{width:22px;height:22px;border-radius:6px;background:linear-gradient(135deg,#7c6cff,#e2489f);}}' +
      'button{{font:600 13px system-ui,sans-serif;border:0;border-radius:9px;padding:9px 14px;cursor:pointer;}}' +
      '.ins{{background:#7c6cff;color:#fff;}} .ins:disabled{{background:#3f3f46;color:#9ca3af;cursor:default;}}' +
      '.cancel{{background:transparent;color:#ccc;}} .cancel:hover{{background:rgba(255,255,255,.08);}}' +
      '.hint{{opacity:.7;}}';
    root.appendChild(style);
    var bar = document.createElement('div');
    bar.className = 'bar';
    var logo = document.createElement('span');
    logo.className = 'logo';
    var name = document.createElement('b');
    name.textContent = 'Cadenza';
    hintEl = document.createElement('span');
    hintEl.className = 'hint';
    btn = document.createElement('button');
    btn.className = 'ins';
    btn.textContent = {label};
    var cancel = document.createElement('button');
    cancel.className = 'cancel';
    cancel.textContent = {cancel_label};
    [logo, name, hintEl, btn, cancel].forEach(function (n) {{ bar.appendChild(n); }});
    root.appendChild(bar);
    btn.addEventListener('click', function () {{
      var title = (document.title || '').replace(/ - YouTube$| on Vimeo$| - Find & Share on GIPHY$/, '');
      go('pick?u=' + encodeURIComponent(location.href) + '&t=' + encodeURIComponent(title));
    }});
    cancel.addEventListener('click', function () {{ go('cancel'); }});
    document.documentElement.appendChild(host);
    update();
  }}
  function update() {{
    if (!btn) return;
    var ok = ITEM.test(location.href);
    btn.disabled = !ok;
    hintEl.textContent = ok ? '' : {hint};
  }}
  setInterval(function () {{ if (!host || !host.isConnected) {{ host = null; mount(); }} update(); }}, 500);
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount); else mount();
}})();"#
    )
}

/// What the user picked, resolved from the page address.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub provider: String,
    /// "video" or "gif".
    pub kind: String,
    /// Video page URL (embedded) or the file to download.
    pub url: String,
    pub title: String,
    pub page_url: String,
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned())
}

/// Interprets a `https://cadenza.invalid/pick?u=…` navigation.
pub fn resolve(provider: &str, nav: &Url) -> Option<Picked> {
    if nav.path() != "/pick" {
        return None;
    }
    let page = query_param(nav, "u")?;
    let title = query_param(nav, "t").unwrap_or_default();
    let page_url = Url::parse(&page).ok()?;
    let on = |domain: &str| crate::util::on_domain(&page_url, domain);
    match provider {
        "youtube" if on("youtube.com") => {
            let id = query_param(&page_url, "v").or_else(|| page_url.path().strip_prefix("/shorts/").map(str::to_string))?;
            let id: String = id.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            (!id.is_empty()).then(|| Picked {
                provider: "youtube".into(),
                kind: "video".into(),
                url: format!("https://www.youtube.com/watch?v={id}"),
                title,
                page_url: page.clone(),
            })
        }
        "vimeo" if on("vimeo.com") => {
            let id: String = page_url.path().trim_start_matches('/').chars().take_while(|c| c.is_ascii_digit()).collect();
            (!id.is_empty()).then(|| Picked {
                provider: "vimeo".into(),
                kind: "video".into(),
                url: format!("https://vimeo.com/{id}"),
                title,
                page_url: page.clone(),
            })
        }
        "giphy" if on("giphy.com") => {
            // /gifs/some-title-ID or /stickers/some-title-ID
            let last = page_url.path_segments()?.rfind(|s| !s.is_empty())?.to_string();
            let id = last.rsplit('-').next()?.to_string();
            (id.chars().all(|c| c.is_ascii_alphanumeric()) && !id.is_empty()).then(|| Picked {
                provider: "giphy".into(),
                kind: "gif".into(),
                url: format!("https://media.giphy.com/media/{id}/giphy.gif"),
                title,
                page_url: page.clone(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(u: &str, t: &str) -> Url {
        Url::parse(&format!(
            "https://{PICK_HOST}/pick?u={}&t={}",
            crate::util::encode_uri_component(u),
            crate::util::encode_uri_component(t)
        ))
        .unwrap()
    }

    #[test]
    fn resolves_picks() {
        let p = resolve("youtube", &nav("https://www.youtube.com/watch?v=aqz-KE-bpKQ&t=10s", "Big Buck Bunny")).unwrap();
        assert_eq!(p.url, "https://www.youtube.com/watch?v=aqz-KE-bpKQ");
        assert_eq!(p.title, "Big Buck Bunny");
        assert_eq!(
            resolve("vimeo", &nav("https://vimeo.com/22439234", "")).unwrap().url,
            "https://vimeo.com/22439234"
        );
        assert_eq!(
            resolve("giphy", &nav("https://giphy.com/gifs/happy-dance-l0MYt5jPR6QX5pnqM", "")).unwrap().url,
            "https://media.giphy.com/media/l0MYt5jPR6QX5pnqM/giphy.gif"
        );
        assert!(resolve("unsplash", &nav("https://unsplash.com/photos/a-lake-AbC12_xyz", "")).is_none());
    }

    #[test]
    fn rejects_foreign_or_non_item_pages() {
        assert!(resolve("youtube", &nav("https://evil.example/watch?v=x", "")).is_none());
        assert!(resolve("youtube", &nav("https://notyoutube.com/watch?v=x", "")).is_none());
        assert!(resolve("vimeo", &nav("https://evilvimeo.com/22439234", "")).is_none());
        assert!(resolve("giphy", &nav("https://notgiphy.com/gifs/happy-dance-l0MYt5jPR6QX5pnqM", "")).is_none());
        assert!(resolve("youtube", &nav("https://m.youtube.com/watch?v=x", "")).is_some());
        assert!(resolve("youtube", &nav("https://www.youtube.com/", "")).is_none());
        assert!(resolve("youtube", &Url::parse("https://cadenza.invalid/cancel").unwrap()).is_none());
    }

    #[test]
    fn bar_script_dump() {
        if let Ok(path) = std::env::var("CZ_DUMP_BAR") {
            std::fs::write(path, bar_script("youtube", &|s: &str| s.to_string())).unwrap();
        }
    }

    #[test]
    fn bar_script_is_specific() {
        assert!(bar_script("giphy", &|s: &str| s.to_string()).contains("Insert this GIF"));
        assert!(bar_script("youtube", &|s: &str| s.to_string()).contains(PICK_HOST));
    }
}
