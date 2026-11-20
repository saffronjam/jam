use jam_core::vt;
use jam_render::Renderer;
#[test]
fn renders_real_terminal_pixels() {
    let mut t = vt::terminal(40, 8).unwrap();
    t.vt_write("\x1b[31mRed\x1b[0m bold -> 界 é\r\n\x1b[44mbackground\x1b[0m".as_bytes());
    let f = vt::frame(&t).unwrap();
    let mut renderer = Renderer::headless(680, 304).unwrap();
    renderer.render(&f, 680, 304, 2.0).unwrap();
    let pixels = renderer.read_pixels().unwrap();
    assert_eq!(pixels.len(), 680 * 304 * 4);
    let background = &pixels[(295 * 680 + 650) * 4..][..3];
    for (actual, expected) in background.iter().zip([18u8, 21, 28]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "sRGB background mismatch: {background:?}"
        );
    }
    let red = pixels
        .chunks_exact(4)
        .filter(|p| p[0] > p[1].saturating_add(30) && p[0] > 100)
        .count();
    assert!(red > 20, "red glyphs must reach GPU readback");
    let blue = pixels
        .chunks_exact(4)
        .filter(|p| p[2] > p[0].saturating_add(50) && p[2] > 100)
        .count();
    assert!(blue > 100, "cell backgrounds must be drawn");
    renderer.render(&f, 680, 304, 2.0).unwrap();
    assert_eq!(
        pixels,
        renderer.read_pixels().unwrap(),
        "cached render must match first frame"
    );
}

#[test]
fn box_drawing_has_no_cell_boundary_gap() {
    let mut t = vt::terminal(4, 2).unwrap();
    t.vt_write("──".as_bytes());
    let mut r = Renderer::headless(68, 76).unwrap();
    r.render(&vt::frame(&t).unwrap(), 68, 76, 2.0).unwrap();
    let p = r.read_pixels().unwrap();
    for x in 1..33 {
        assert!(p[(19 * 68 + x) * 4] > 150, "line seam at x={x}");
    }
}

#[test]
fn adding_blank_columns_preserves_existing_glyph_pixels() {
    let mut t = vt::terminal(20, 2).unwrap();
    t.vt_write("AB界".as_bytes());
    let mut renderer = Renderer::headless(400, 76).unwrap();
    renderer
        .render(&vt::frame(&t).unwrap(), 400, 76, 2.0)
        .unwrap();
    let before = renderer.read_pixels().unwrap();
    t.resize(22, 2, 0, 0).unwrap();
    renderer
        .render(&vt::frame(&t).unwrap(), 400, 76, 2.0)
        .unwrap();
    assert_eq!(before, renderer.read_pixels().unwrap());
}
