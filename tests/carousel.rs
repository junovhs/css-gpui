use css_gpui::carousel::compile;

const HTML: &str = include_str!("../fixtures/carousel-rotate/source.html");
const CSS: &str = include_str!("../fixtures/carousel-rotate/source.css");

#[test]
fn authored_carousel_cascades_and_recompiles() {
    let original = compile(HTML, CSS).unwrap();
    assert_eq!(original.panels.len(), 9);
    assert_eq!(original.panels[0].fill, [255, 0, 0, 204]);
    assert_eq!(original.panels[8].label, "9");
    assert_eq!(original.panels[8].rotation_degrees, 320.0);
    let edited = format!(
        "{CSS}\n.carousel__cell:nth-child(9n + 1) {{ background: #00ffaa; width: 180px; }}"
    );
    let edited = compile(HTML, &edited).unwrap();
    assert_eq!(edited.panels[0].fill, [0, 255, 170, 255]);
    assert_eq!(edited.panels[0].size[0], 180.0);
    assert_eq!(edited.panels[1].fill, original.panels[1].fill);
}
