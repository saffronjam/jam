//! Cross-platform fixture used by remote-check, never linked into the app.
use anyhow::{Context, Result};
use jam_core::vt;
use libghostty_vt::snapshot::Decoder;
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().context("write|verify")?;
    let path = args.next().context("fixture path")?;
    let mut expected = vt::terminal(43, 8)?;
    expected.vt_write("hello 界🙂\r\n".as_bytes());
    for _ in 0..100 {
        expected.vt_write(b"scrollback\r\n");
    }
    expected.vt_write(b"\x1b[?1049h\x1b[38;2;1;2;");
    if mode == "write" {
        std::fs::write(
            path,
            expected
                .encode_snapshot_alloc(None)?
                .context("empty snapshot")?,
        )?;
    } else {
        let bytes = std::fs::read(path)?;
        let mut restored = Decoder::new_buf(&bytes)?.decode()?;
        expected.vt_write(b"3mcolored\x1b[?1049l");
        restored.vt_write(b"3mcolored\x1b[?1049l");
        assert_eq!(vt::frame(&expected)?, vt::frame(&restored)?);
        assert_eq!(vt::text(&expected)?, vt::text(&restored)?);
    }
    Ok(())
}
