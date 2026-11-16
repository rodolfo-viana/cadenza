//! Emits production render data for tools/test_print.cjs (no app window).
use cadenza_lib::{layouts, model::*, render};
use serde_json::json;

fn main() {
    let mut deck = layouts::starter_deck("cadenza-midnight", 1280, 720);
    deck.title = "Print regression".into();
    deck.slides = vec![
        Slide {
            elements: vec![Element {
                x: 304.0,
                y: 108.0,
                w: 672.0,
                h: 504.0,
                content: Content::Image(ImageContent {
                    src: "print-crop.svg".into(),
                    natural_width: 640,
                    natural_height: 480,
                    crop: Some(CropRect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.5,
                        h: 1.0,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        },
        Slide {
            elements: vec![Element {
                x: 256.0,
                y: 144.0,
                w: 768.0,
                h: 432.0,
                content: Content::Video(VideoContent {
                    src: "print.webm".into(),
                    muted: true,
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        },
    ];
    // Chromium's SVG overflow bug only appears after the first printed page.
    // Keep both positions: simplifying this fixture to one crop hides B05.
    let mut first_crop = deck.slides[0].clone();
    first_crop.id = "print-crop-first".into();
    let mut pages = vec![first_crop];
    pages.extend((0..6).map(|_| Slide::default()));
    pages.append(&mut deck.slides);
    deck.slides = pages;
    for (name, fit, flip_y, grayscale, radius) in [
        ("contain", "contain", false, 0.0, 0.0),
        ("fill", "fill", false, 0.0, 0.0),
        ("flip", "cover", true, 0.0, 0.0),
        ("filtered-rounded", "cover", false, 100.0, 40.0),
    ] {
        let mut variant = deck.slides[7].clone();
        variant.id = format!("print-crop-{name}");
        variant.elements[0].style.radius = radius;
        if let Content::Image(image) = &mut variant.elements[0].content {
            image.fit = fit.into();
            image.flip_y = flip_y;
            image.grayscale = grayscale;
        }
        deck.slides.push(variant);
    }
    let mut png = deck.slides[7].clone();
    png.id = "print-crop-png".into();
    if let Content::Image(image) = &mut png.elements[0].content {
        image.src = "print-crop.png".into();
    }
    deck.slides.push(png);
    // The final page can mask the clip defect too: keep every variant inside.
    deck.slides.push(Slide::default());
    let ctx = render::Ctx {
        mode: render::Mode::Export,
        asset_url: &|name| format!("/tools/fixtures/{name}"),
        youtube_host: None,
    };
    println!(
        "{}",
        json!({
            "title": deck.title, "width": deck.width, "height": deck.height, "language": "en",
            "themeCss": deck.theme_href(), "deckCss": render::deck_css(&deck, &ctx),
            "sections": render::sections(&deck, &ctx), "needs": render::needs(&deck),
            "config": render::reveal_config(&deck, json!({"hash": false, "controls": false, "progress": false}))
        })
    );
}
