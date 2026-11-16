//! Actual renderer snapshot used by the browser/OOXML PPTX regression test.
use cadenza_lib::{model::Deck, render};
use serde_json::json;

fn main() {
    let deck: Deck = serde_json::from_value(json!({
        "title":"PPTX interoperability", "width":1280,"height":720,"theme":"cadenza-midnight",
        "settings":{"transition":"none"},
        "slides":[{"id":"editable","notes":"Notes survive export: ç, α, 日本語.","elements":[
            {"id":"text","x":48,"y":30,"w":700,"h":160,"content":{"kind":"text","html":"<p>Editable <strong>bold</strong> <em>italic</em> <u>underlined</u></p><p><a href=\"https://example.com\">A safe link</a></p>","style":{"fontFamily":"Arial","fontSize":32,"color":"#ffffff","autoHeight":false}}},
            {"id":"shape","action":{"kind":"url","url":"https://example.org/shape"},"x":850,"y":50,"w":300,"h":120,"style":{"fill":"#ff00ff","borderWidth":4,"borderColor":"#ffffff"},"content":{"kind":"shape","shape":"rect","text":{"html":"Native shape","style":{"fontFamily":"Arial","fontSize":22,"color":"#ffffff"}}}},
            {"id":"table","x":48,"y":220,"w":500,"h":200,"style":{"fill":"transparent"},"content":{"kind":"table","rows":[["Name","Value"],["Zero","0"],["Markup","<strong>Bold</strong>"]],"fontSize":24}},
            {"id":"chart","x":600,"y":220,"w":620,"h":400,"content":{"kind":"chart","chart":"line","labels":["A","B","C"],"series":[{"name":"Series","values":[0,null,8]}],"fontSize":20}},
            {"id":"picture","action":{"kind":"next"},"x":48,"y":460,"w":300,"h":200,"style":{"fill":"rgba(0,0,0,0)"},"content":{"kind":"image","src":"print-crop.png","naturalWidth":640,"naturalHeight":480,"crop":{"x":0,"y":0,"w":0.5,"h":1},"fit":"fill","flipX":true}},
            {"id":"equation","x":370,"y":480,"w":190,"h":150,"content":{"kind":"math","latex":"x^2+y^2"}}
        ]},
        {"id":"fallback","elements":[
            {"id":"unsupported-shape","x":48,"y":48,"w":300,"h":200,"rotation":30,"opacity":0.5,"content":{"kind":"shape","shape":"star"}},
            {"id":"diagram","x":400,"y":48,"w":500,"h":200,"content":{"kind":"diagram","source":"graph TD; A-->B","svgLight":"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><rect width=\"100\" height=\"100\" fill=\"red\"/></svg>","svgDark":"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><rect width=\"100\" height=\"100\" fill=\"red\"/></svg>","renderedFor":{"source":"old","style":"classic","look":"classic"}}},
            {"id":"remote-image","x":48,"y":350,"w":300,"h":200,"content":{"kind":"image","src":"https://example.com/photo.png","naturalWidth":640,"naturalHeight":480}},
            {"id":"camera","x":400,"y":350,"w":300,"h":200,"content":{"kind":"camera"}},
            {"id":"rich-text","x":750,"y":350,"w":450,"h":300,"opacity":0.5,"contentScale":1.5,"action":{"kind":"slide","slide":"editable"},"content":{"kind":"text","html":"<ul><li>One <strong>bold</strong></li><li>Two</li></ul><p>X<sub>2</sub> + Y<sup>3</sup></p>","style":{"fontFamily":"Arial","fontSize":24,"autoHeight":false}}}
        ]},
        {"id":"round-charts","elements":[
            {"id":"pie","x":40,"y":80,"w":570,"h":520,"content":{"kind":"chart","chart":"pie","labels":["A","B","C","D","E","F","G"],"series":[{"name":"Pie","values":[1,2,3,4,5,6,7]}],"fontSize":18}},
            {"id":"donut","x":670,"y":80,"w":570,"h":520,"content":{"kind":"chart","chart":"donut","labels":["A","B","C"],"series":[{"name":"Donut","values":[1,2,3]}],"fontSize":18}}
        ]},
        {"id":"skipped","hidden":true,"elements":[]}]
    })).expect("valid fixture deck");
    let ctx = render::Ctx {
        mode: render::Mode::Editor,
        asset_url: &|name| format!("/tools/fixtures/{name}"),
        youtube_host: None,
    };
    let slides: Vec<_> = deck
        .slides
        .iter()
        .map(|slide| {
            let rendered = render::slide_for_editor(&deck, slide, &ctx);
            json!({"id":slide.id,"html":rendered.html,"classes":rendered.classes})
        })
        .collect();
    println!(
        "{}",
        json!({"deck":deck,"width":deck.width,"height":deck.height,
        "themeCss":deck.theme_href(),"deckCss":render::deck_css(&deck,&ctx),"slides":slides,"jobId":"test"})
    );
}
