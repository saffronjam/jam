use anyhow::Result;
use libghostty_vt::{
    Terminal,
    fmt::Formatter,
    render::{CellIterator, RenderState, RowIterator},
};
use serde::{Deserialize, Serialize};
pub const DEFAULT_BACKGROUND: [u8; 3] = [18, 21, 28];
pub const DEFAULT_FOREGROUND: [u8; 3] = [220, 224, 235];

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq)]
pub enum ColorSource {
    #[default]
    Explicit,
    Foreground,
    Background,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    #[serde(default)]
    pub fg_source: ColorSource,
    #[serde(default)]
    pub bg_source: ColorSource,
    #[serde(default)]
    pub faint: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub wide: u8,
}
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct TerminalFrame {
    pub background: [u8; 3],
    #[serde(default)]
    pub foreground: [u8; 3],
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    pub cursor: (u16, u16),
    pub cursor_visible: bool,
    pub cursor_blink: bool,
    pub cursor_style: u8,
    pub mouse_tracking: bool,
    pub alternate: bool,
    pub selection: Option<(usize, usize)>,
}
pub fn terminal(cols: u16, rows: u16) -> Result<Terminal<'static, 'static>> {
    let mut t = Terminal::new(cols, rows)?;
    t.set_default_bg_color(Some(libghostty_vt::color::Rgb {
        r: DEFAULT_BACKGROUND[0],
        g: DEFAULT_BACKGROUND[1],
        b: DEFAULT_BACKGROUND[2],
    }))?;
    t.set_default_fg_color(Some(libghostty_vt::color::Rgb {
        r: DEFAULT_FOREGROUND[0],
        g: DEFAULT_FOREGROUND[1],
        b: DEFAULT_FOREGROUND[2],
    }))?;
    t.set_continuation_max_bytes(256 * 1024)?;
    t.set_scrollback_max_bytes(Some(8 * 1024 * 1024))?;
    Ok(t)
}
pub fn text(t: &Terminal<'_, '_>) -> Result<String> {
    let mut f = Formatter::new(t)?;
    let b = f.format_alloc(None)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}
pub fn frame(t: &Terminal<'static, 'static>) -> Result<TerminalFrame> {
    let mut state = RenderState::new()?;
    let snap = state.update(t)?;
    let colors = snap.colors()?;
    let mut ri = RowIterator::new()?;
    let mut rows = ri.update(&snap)?;
    let mut cells = Vec::new();
    let mut ci = CellIterator::new()?;
    while rows.next().is_some() {
        let mut c = ci.update(&rows)?;
        while c.next().is_some() {
            let foreground = c.fg_color()?;
            let background = c.bg_color()?;
            let mut fg_source =
                foreground.map_or(ColorSource::Foreground, |_| ColorSource::Explicit);
            let mut bg_source =
                background.map_or(ColorSource::Background, |_| ColorSource::Explicit);
            let mut fg = foreground.map(|v| [v.r, v.g, v.b]).unwrap_or([
                colors.foreground.r,
                colors.foreground.g,
                colors.foreground.b,
            ]);
            let mut bg = background.map(|v| [v.r, v.g, v.b]).unwrap_or([
                colors.background.r,
                colors.background.g,
                colors.background.b,
            ]);
            let style = c.style()?;
            if style.inverse {
                std::mem::swap(&mut fg, &mut bg);
                std::mem::swap(&mut fg_source, &mut bg_source);
            }
            if style.faint {
                fg = fg.map(|v| v / 2);
            }
            let wide = c.raw_cell()?.wide()? as u8;
            cells.push(Cell {
                text: if style.invisible {
                    String::new()
                } else {
                    c.graphemes()?.into_iter().collect()
                },
                bold: style.bold,
                italic: style.italic,
                underline: style.underline != libghostty_vt::style::Underline::None,
                strike: style.strikethrough,
                wide,
                fg,
                bg,
                fg_source,
                bg_source,
                faint: style.faint,
            });
        }
    }
    Ok(TerminalFrame {
        foreground: [
            colors.foreground.r,
            colors.foreground.g,
            colors.foreground.b,
        ],
        background: [
            colors.background.r,
            colors.background.g,
            colors.background.b,
        ],
        cols: snap.cols()?,
        rows: snap.rows()?,
        cells,
        cursor: snap
            .cursor_viewport()?
            .map(|c| (c.x, c.y))
            .unwrap_or_default(),
        cursor_visible: snap.cursor_visible()? && snap.cursor_viewport()?.is_some(),
        cursor_blink: snap.cursor_blinking()?,
        cursor_style: match snap.cursor_visual_style()? {
            libghostty_vt::render::CursorVisualStyle::Block => 1,
            libghostty_vt::render::CursorVisualStyle::Underline => 2,
            libghostty_vt::render::CursorVisualStyle::BlockHollow => 3,
            _ => 0,
        },
        alternate: t.active_screen()? == libghostty_vt::screen::Screen::Alternate,
        mouse_tracking: [9, 1000, 1002, 1003].iter().any(|m| {
            t.mode(libghostty_vt::mode::Mode::new(
                *m,
                libghostty_vt::mode::Kind::Dec,
            ))
            .unwrap_or(false)
        }),
        selection: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libghostty_vt::snapshot::Decoder;
    #[test]
    fn colors_retain_default_sources_and_inverse_styles() {
        let mut t = terminal(8, 1).unwrap();
        t.vt_write(b"A\x1b[48;2;18;21;28mB\x1b[0;7mC\x1b[0;2mD");
        let f = frame(&t).unwrap();
        assert_eq!(f.cells[0].fg_source, ColorSource::Foreground);
        assert_eq!(f.cells[0].bg_source, ColorSource::Background);
        assert_eq!(f.cells[1].bg, DEFAULT_BACKGROUND);
        assert_eq!(f.cells[1].bg_source, ColorSource::Explicit);
        assert_eq!(f.cells[2].fg_source, ColorSource::Background);
        assert_eq!(f.cells[2].bg_source, ColorSource::Foreground);
        assert!(f.cells[3].faint);
    }
    #[test]
    fn snapshot_preserves_partial_parser() {
        let mut original = terminal(20, 4).unwrap();
        original.vt_write(b"before\r\n\x1b[31");
        let bytes = original.encode_snapshot_alloc(None).unwrap().unwrap();
        let mut restored = Decoder::new_buf(&bytes).unwrap().decode().unwrap();
        for t in [&mut original, &mut restored] {
            t.vt_write(b"mred\x1b[0m\r\nnext");
        }
        assert_eq!(text(&original).unwrap(), text(&restored).unwrap());
        assert_eq!(original.cursor_x().unwrap(), restored.cursor_x().unwrap());
    }
    #[test]
    fn alternate_screen_and_unicode() {
        let mut t = terminal(20, 4).unwrap();
        t.vt_write("main\x1b[?1049h界é".as_bytes());
        let bytes = t.encode_snapshot_alloc(None).unwrap().unwrap();
        let mut restored = Decoder::new_buf(&bytes).unwrap().decode().unwrap();
        assert_eq!(text(&t).unwrap(), text(&restored).unwrap());
        restored.vt_write(b"\x1b[?1049l");
        assert!(text(&restored).unwrap().contains("main"));
    }
}
