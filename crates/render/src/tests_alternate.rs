//! Preferences › Type › Highlight Alternate Glyphs (#394): OpenType substitutions get a pink
//! cell behind them on screen when the preference is on.

use super::*;

#[test]
fn is_opentype_alternate_marks_ligatures_and_forms() {
    assert!(is_opentype_alternate(100, 100, 2), "ligature cluster");
    assert!(is_opentype_alternate(50, 99, 1), "contextual / stylistic form");
    assert!(!is_opentype_alternate(50, 50, 1), "nominal cmap glyph");
    assert!(!is_opentype_alternate(50, 0, 2), "missing glyph");
}

#[test]
fn highlight_alternate_glyphs_paints_pink_behind_a_ligature() {
    use vectorcraft_doc::text::{CharStyle, TextObject};
    use vectorcraft_doc::{Node, NodeKind};
    use vectorcraft_geom::Point;

    // Source Serif 4 ships with standard ligatures for "fi".
    let mut d = Document::new(200.0, 100.0);
    let style = CharStyle { size: 48.0, font_family: "Source Serif 4".into(), ..Default::default() };
    let mut n = Node::new(NodeId(0), NodeKind::Text(Box::new(TextObject::point(Point::new(20.0, 70.0), "fi", style))));
    n.id = d.alloc_id();
    let l = d.layers[0].id;
    d.insert(Some(l), 0, n).unwrap();

    let off = RenderOptions {
        background: Some([255, 255, 255, 255]),
        highlight_substitutions: true,
        highlight_alternate_glyphs: false,
        ..Default::default()
    };
    let on = RenderOptions { highlight_alternate_glyphs: true, ..off.clone() };
    let mut r = Renderer::new();
    let without = r.render(&d, 200, 100, Affine::IDENTITY, &off);
    let with = r.render(&d, 200, 100, Affine::IDENTITY, &on);
    assert_ne!(without.pixels, with.pixels, "the preference changes the frame");
    // Pink SUBSTITUTED = (255, 120, 190) at alpha 110, blended over white / ink.
    let pinkish = with.pixels.chunks(4).filter(|p| p[0] > 200 && p[1] < 180 && p[2] > 150 && p[2] < 230).count();
    assert!(pinkish > 10, "pink highlight cells behind the ligature ({pinkish} pixels)");
}
