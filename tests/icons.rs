use sequoia::icons::{self, Icon};
#[test]
fn every_svg_renders_at_toolbar_and_hidpi_sizes() {
    let mut names = std::collections::HashSet::new();
    for icon in Icon::ALL {
        assert!(names.insert(icon.name()), "duplicate icon name");
        assert!(
            icon.svg().contains("viewBox=\"0 0 24 24\""),
            "{}",
            icon.name()
        );
        assert!(
            !icon.svg().contains("<image"),
            "SVG must not embed raster data"
        );
        assert!(!icon.svg().contains("<script"), "SVG must be declarative");
        for size in [16, 18, 20, 24, 32, 48, 64] {
            let rendered =
                icons::raster(&icon.svg().replace("currentColor", "#ffffff"), size).unwrap();
            let visible = rendered.pixels().iter().filter(|p| p.alpha() > 0).count();
            assert!(
                visible > 8 && visible < (size * size) as usize,
                "empty/opaque icon {} at {size}",
                icon.name()
            );
        }
    }
}
#[test]
fn application_svg_has_visible_transparent_shell_icon() {
    let icon = icons::app_icon();
    assert_eq!((icon.width, icon.height), (256, 256));
    assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    assert!(icon.rgba.chunks_exact(4).any(|p| p[3] == 0));
    assert!(icon.rgba.chunks_exact(4).filter(|p| p[3] > 0).count() > 20000);
}
