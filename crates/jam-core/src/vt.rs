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
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Hyperlink {
    start: usize,
    end: usize,
    uri: String,
}

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct TerminalFrame {
    pub background: [u8; 3],
    #[serde(default)]
    pub foreground: [u8; 3],
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub wrapped_rows: Vec<bool>,
    #[serde(default)]
    hyperlinks: Vec<Hyperlink>,
    pub cursor: (u16, u16),
    pub cursor_visible: bool,
    pub cursor_blink: bool,
    pub cursor_style: u8,
    pub mouse_tracking: bool,
    pub alternate: bool,
    pub selection: Option<(usize, usize)>,
}
impl TerminalFrame {
    /// AppKit's link detector uses UTF-16 offsets into the visible logical line.
    pub fn link_at(&self, x: usize, y: usize) -> Option<serde_json::Value> {
        let cols = usize::from(self.cols);
        if x >= cols || y >= self.rows.into() {
            return None;
        }
        let mut index = y * cols + x;
        if self.cells.get(index)?.wide == 2 && x > 0 {
            index -= 1;
        }
        if let Some(link) = self
            .hyperlinks
            .iter()
            .find(|l| (l.start..l.end).contains(&index))
        {
            return Some(
                serde_json::json!({"url":link.uri,"start":link.start,"end":link.end,"cols":cols}),
            );
        }
        let mut first = y;
        while first > 0 && self.wrapped_rows.get(first - 1) == Some(&true) {
            first -= 1;
        }
        let mut last = y;
        while last + 1 < self.rows.into() && self.wrapped_rows.get(last) == Some(&true) {
            last += 1;
        }
        let mut line = String::new();
        let mut offsets = Vec::with_capacity((last + 1 - first) * cols);
        let mut units = 0;
        for i in first * cols..(last + 1) * cols {
            let cell = self.cells.get(i)?;
            offsets.push(if cell.wide == 2 {
                offsets.last().copied().unwrap_or(units)
            } else {
                units
            });
            if cell.wide >= 2 {
                continue;
            }
            let text = if cell.text.is_empty() {
                " "
            } else {
                &cell.text
            };
            units += text.encode_utf16().count();
            line.push_str(text);
        }
        Some(
            serde_json::json!({"line":line,"offset":offsets[index-first*cols],"offsets":offsets,"start":first*cols,"cols":cols}),
        )
    }
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
pub fn frame(t: &Terminal<'_, '_>) -> Result<TerminalFrame> {
    let mut state = RenderState::new()?;
    let snap = state.update(t)?;
    let colors = snap.colors()?;
    let mut ri = RowIterator::new()?;
    let mut rows = ri.update(&snap)?;
    let mut cells = Vec::new();
    let mut ci = CellIterator::new()?;
    let cols = snap.cols()?;
    let mut hyperlinks: Vec<Hyperlink> = Vec::new();
    let mut uri_buf = Vec::new();
    let mut wrapped_rows = Vec::new();
    while rows.next().is_some() {
        let row = rows.raw_row()?;
        let linked = row.has_hyperlink()?;
        wrapped_rows.push(row.is_wrapped()?);
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
            let raw = c.raw_cell()?;
            let wide = raw.wide()? as u8;
            if linked && raw.has_hyperlink()? {
                use libghostty_vt::point::{Coordinate, Point};
                let index = cells.len();
                let grid = t.grid_ref(Point::Viewport(Coordinate {
                    x: (index % usize::from(cols)) as u16,
                    y: (index / usize::from(cols)) as u32,
                }))?;
                let length = match grid.hyperlink_uri(&mut uri_buf) {
                    Ok(n) => n,
                    Err(libghostty_vt::Error::OutOfSpace { required }) if required <= 32768 => {
                        uri_buf.resize(required, 0);
                        grid.hyperlink_uri(&mut uri_buf)?
                    }
                    Err(libghostty_vt::Error::OutOfSpace { .. }) => 0,
                    Err(e) => return Err(e.into()),
                };
                if let Ok(uri) = std::str::from_utf8(&uri_buf[..length])
                    && !uri.is_empty()
                {
                    if let Some(link) = hyperlinks.last_mut()
                        && link.end == index
                        && link.uri == uri
                    {
                        link.end += 1;
                    } else {
                        hyperlinks.push(Hyperlink {
                            start: index,
                            end: index + 1,
                            uri: uri.to_owned(),
                        });
                    }
                }
            }
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
        cols,
        hyperlinks,
        wrapped_rows,
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
    fn hyperlink_hit_test_preserves_labels_wraps_and_unicode_offsets() {
        let mut t = terminal(9, 4).unwrap();
        t.vt_write("界 https://example.org".as_bytes());
        let f = frame(&t).unwrap();
        let target = f.link_at(1, 1).unwrap();
        assert!(
            target["line"]
                .as_str()
                .unwrap()
                .starts_with("界 https://example.org")
        );
        assert_eq!(target["offset"], 9);
        t.vt_write(b"\r\n\x1b]8;;https://example.org/page\x1b\\Label\x1b]8;;\x1b\\");
        let f = frame(&t).unwrap();
        assert_eq!(f.link_at(2, 3).unwrap()["url"], "https://example.org/page");
        assert!(f.link_at(9, 0).is_none());
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
