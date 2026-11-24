use anyhow::{Context, Result};
use glyphon::*;
use jam_core::vt::{Cell, ColorSource, DEFAULT_BACKGROUND, DEFAULT_FOREGROUND, TerminalFrame};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::{
    collections::HashMap,
    ptr::NonNull,
    sync::{Arc, Mutex, OnceLock},
};
static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
static GPU: Mutex<Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)>> = Mutex::new(None);
static FONTS: OnceLock<Arc<Mutex<FontSystem>>> = OnceLock::new();

fn terminal_fonts() -> FontSystem {
    FontSystem::new_with_fonts(
        [
            include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/JetBrainsMono-Italic.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/JetBrainsMono-BoldItalic.ttf").as_slice(),
        ]
        .into_iter()
        .map(|bytes| fontdb::Source::Binary(Arc::new(bytes.to_vec()))),
    )
}
fn embedded_weights(fonts: &FontSystem) -> [Weight; 4] {
    // Match the actual embedded faces: the bundled bold font declares weight 558,
    // so requesting CSS 700 makes the fallback matcher choose a proportional system font.
    static WEIGHTS: OnceLock<[Weight; 4]> = OnceLock::new();
    *WEIGHTS.get_or_init(|| {
        [
            "JetBrainsMono-Regular",
            "JetBrainsMono-Italic",
            "JetBrainsMono-Bold",
            "JetBrainsMono-BoldItalic",
        ]
        .map(|name| {
            fonts
                .db()
                .faces()
                .find(|f| f.post_script_name == name)
                .expect("bundled terminal font")
                .weight
        })
    })
}
fn shape_row(fonts: &mut FontSystem, cells: &[Cell], scale: f32) -> (Buffer, Vec<(usize, usize)>) {
    let weights = embedded_weights(fonts);
    let mut buffer = Buffer::new(fonts, Metrics::new(14.0 * scale, 19.0 * scale));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, Some(19.0 * scale));
    let strings: Vec<String> = cells
        .iter()
        .map(|c| {
            if c.wide >= 2 {
                String::new()
            } else if c.text.is_empty() {
                " ".into()
            } else {
                c.text.clone()
            }
        })
        .collect();
    let mut mapping = Vec::new();
    let mut bytes = 0;
    for (i, s) in strings.iter().enumerate() {
        if !s.is_empty() {
            mapping.push((bytes, i));
            bytes += s.len();
        }
    }
    buffer.set_rich_text(
        strings.iter().zip(cells).map(|(s, c)| {
            (
                s.as_str(),
                Attrs::new()
                    .family(Family::Name("JetBrains Mono"))
                    .weight(weights[usize::from(c.bold) * 2 + usize::from(c.italic)])
                    .style(if c.italic {
                        Style::Italic
                    } else {
                        Style::Normal
                    })
                    .color(Color::rgb(c.fg[0], c.fg[1], c.fg[2])),
            )
        }),
        &Attrs::new().family(Family::Name("JetBrains Mono")),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    (buffer, mapping)
}
fn instance() -> wgpu::Instance {
    INSTANCE.get_or_init(wgpu::Instance::default).clone()
}
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}
struct Raster {
    data: Vec<u8>,
    kind: ContentType,
    width: u32,
    height: u32,
}
struct RowCache {
    cells: Vec<Cell>,
    scale: f32,
    glyphs: Vec<CustomGlyph>,
}
#[derive(Clone, Copy, PartialEq)]
struct Theme {
    foreground: [u8; 3],
    background: [u8; 3],
}
impl Theme {
    fn resolve(
        self,
        frame: &TerminalFrame,
        color: [u8; 3],
        source: ColorSource,
        faint: bool,
    ) -> [u8; 3] {
        let replacement = match source {
            ColorSource::Foreground if frame.foreground == DEFAULT_FOREGROUND => self.foreground,
            ColorSource::Background if frame.background == DEFAULT_BACKGROUND => self.background,
            _ => return color,
        };
        if faint {
            replacement.map(|v| v / 2)
        } else {
            replacement
        }
    }
}
pub struct Renderer {
    theme: Option<Theme>,
    presented: u64,
    surface: Option<wgpu::Surface<'static>>,
    output: Option<wgpu::Texture>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    fonts: Arc<Mutex<FontSystem>>,
    swash: SwashCache,
    atlas: TextAtlas,
    text: TextRenderer,
    viewport: Viewport,
    rects: wgpu::RenderPipeline,
    rasters: Vec<Raster>,
    glyph_ids: HashMap<CacheKey, u16>,
    rows: Vec<RowCache>,
    pub cell_width: f32,
    pub cell_height: f32,
}
// Draw the common TUI line alphabet on cell edges, avoiding font-advance seams.
fn box_lines(text: &str) -> Option<[bool; 4]> {
    Some(match text {
        "─" | "━" => [true, true, false, false],
        "│" | "┃" => [false, false, true, true],
        "┌" | "┏" => [false, true, false, true],
        "┐" | "┓" => [true, false, false, true],
        "└" | "┗" => [false, true, true, false],
        "┘" | "┛" => [true, false, true, false],
        "├" | "┣" => [false, true, true, true],
        "┤" | "┫" => [true, false, true, true],
        "┬" | "┳" => [true, true, false, true],
        "┴" | "┻" => [true, true, true, false],
        "┼" | "╋" => [true, true, true, true],
        _ => return None,
    })
}
impl Renderer {
    pub fn prepare() -> Result<()> {
        let _ = Self::headless(1, 1)?;
        Ok(())
    }
    /// # Safety
    /// The caller keeps the NSView alive while attaching the surface on AppKit's main thread.
    pub unsafe fn appkit(
        view: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<impl FnOnce() -> Result<Self> + Send + 'static> {
        let instance = instance();
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::AppKit(AppKitWindowHandle::new(
                    NonNull::new(view).context("null view")?,
                )),
            })
        }?;
        Ok(move || Self::with_surface(instance, Some(surface), width, height))
    }
    pub fn headless(width: u32, height: u32) -> Result<Self> {
        Self::with_surface(instance(), None, width, height)
    }
    pub fn with_surface(
        instance: wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut cached = GPU.lock().unwrap();
        let compatible = cached.as_ref().is_some_and(|(adapter, _, _)| {
            surface
                .as_ref()
                .is_none_or(|s| !s.get_capabilities(adapter).formats.is_empty())
        });
        if !compatible {
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: surface.as_ref(),
                    ..Default::default()
                }))?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
            *cached = Some((adapter, device, queue));
        }
        let (adapter, device, queue) = cached.as_ref().unwrap().clone();
        drop(cached);
        let config = if let Some(s) = &surface {
            let mut config = s
                .get_default_config(&adapter, width.max(1), height.max(1))
                .context("unsupported surface")?;
            config.format = s
                .get_capabilities(&adapter)
                .formats
                .into_iter()
                .find(|f| f.is_srgb())
                .context("surface requires an sRGB format")?;
            config
        } else {
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                width: width.max(1),
                height: height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![],
                color_space: wgpu::SurfaceColorSpace::Auto,
            }
        };
        if let Some(s) = &surface {
            s.configure(&device, &config);
        }
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, config.format);
        let text = TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("terminal rectangles"),source:wgpu::ShaderSource::Wgsl("struct Out{@builtin(position)p:vec4f,@location(0)c:vec4f};@vertex fn vs(@location(0)p:vec2f,@location(1)c:vec4f)->Out{var o:Out;o.p=vec4f(p,0.,1.);o.c=c;return o;}@fragment fn fs(i:Out)->@location(0)vec4f{let rgb=select(pow((i.c.rgb+vec3f(0.055))/1.055,vec3f(2.4)),i.c.rgb/12.92,i.c.rgb<=vec3f(0.04045));return vec4f(rgb,i.c.a);}".into())});
        let rects = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terminal backgrounds"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let fonts = FONTS
            .get_or_init(|| {
                let fonts = terminal_fonts();
                Arc::new(Mutex::new(fonts))
            })
            .clone();
        Ok(Self {
            theme: None,
            presented: 0,
            surface,
            output: None,
            device,
            queue,
            config,
            fonts,
            swash: SwashCache::new(),
            atlas,
            text,
            viewport,
            rects,
            rasters: Vec::new(),
            glyph_ids: HashMap::new(),
            rows: Vec::new(),
            cell_width: 8.4,
            cell_height: 19.0,
        })
    }
    pub fn set_theme(&mut self, foreground: [u8; 3], background: [u8; 3]) -> bool {
        let theme = Some(Theme {
            foreground,
            background,
        });
        if self.theme == theme {
            return false;
        }
        self.theme = theme;
        self.rows.clear();
        true
    }
    fn row(&mut self, cells: &[Cell], frame: &TerminalFrame, scale: f32) -> Result<RowCache> {
        let shared_fonts = self.fonts.clone();
        let mut fonts = shared_fonts.lock().unwrap();
        let themed: Vec<Cell>;
        let text_cells = if let Some(theme) = self.theme {
            themed = cells
                .iter()
                .map(|cell| {
                    let mut cell = cell.clone();
                    cell.fg = theme.resolve(frame, cell.fg, cell.fg_source, cell.faint);
                    cell
                })
                .collect();
            &themed
        } else {
            cells
        };
        let (buffer, mapping) = shape_row(&mut fonts, text_cells, scale);
        let mut glyphs = Vec::new();
        for run in buffer.layout_runs() {
            let mut origins = HashMap::<usize, f32>::new();
            for g in run.glyphs {
                origins
                    .entry(g.start)
                    .and_modify(|x| *x = x.min(g.x))
                    .or_insert(g.x);
            }
            for original in run.glyphs {
                let col = mapping
                    .get(
                        mapping
                            .partition_point(|(b, _)| *b <= original.start)
                            .saturating_sub(1),
                    )
                    .map(|(_, c)| *c)
                    .unwrap_or(0);
                if cells.get(col).is_some_and(|c| box_lines(&c.text).is_some()) {
                    continue;
                }
                let mut g = original.clone();
                g.x = col as f32 * self.cell_width + (original.x - origins[&original.start]);
                let physical = g.physical((0.0, 0.0), 1.0);
                let image = self.swash.get_image(&mut fonts, physical.cache_key);
                let Some(image) = image else { continue };
                if image.placement.width == 0 || image.placement.height == 0 {
                    continue;
                }
                let id = if let Some(id) = self.glyph_ids.get(&physical.cache_key) {
                    *id
                } else {
                    let id = u16::try_from(self.rasters.len()).context("glyph cache capacity")?;
                    let kind = if image.content == SwashContent::Color {
                        ContentType::Color
                    } else {
                        ContentType::Mask
                    };
                    let data = if image.content == SwashContent::SubpixelMask {
                        image
                            .data
                            .chunks(3)
                            .map(|p| p.iter().copied().max().unwrap_or(0))
                            .collect()
                    } else {
                        image.data.clone()
                    };
                    self.rasters.push(Raster {
                        data,
                        kind,
                        width: image.placement.width,
                        height: image.placement.height,
                    });
                    self.glyph_ids.insert(physical.cache_key, id);
                    id
                };
                let raster = &self.rasters[id as usize];
                glyphs.push(CustomGlyph {
                    id,
                    left: (physical.x + image.placement.left) as f32,
                    top: (run.line_y as i32 + physical.y - image.placement.top) as f32,
                    width: raster.width as f32,
                    height: raster.height as f32,
                    color: Some(g.color_opt.unwrap_or(Color::rgb(220, 224, 235))),
                    snap_to_physical_pixel: true,
                    metadata: 0,
                });
            }
        }
        Ok(RowCache {
            cells: cells.to_vec(),
            scale,
            glyphs,
        })
    }
    pub fn render(
        &mut self,
        frame: &TerminalFrame,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<()> {
        self.render_with_cursor(frame, width, height, scale, true)
    }
    pub fn render_with_cursor(
        &mut self,
        frame: &TerminalFrame,
        width: u32,
        height: u32,
        scale: f32,
        show_cursor: bool,
    ) -> Result<()> {
        if width == 0 || height == 0 || frame.cols == 0 {
            return Ok(());
        }
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            if let Some(s) = &self.surface {
                s.configure(&self.device, &self.config)
            }
        }
        self.cell_width = (8.4 * scale).round();
        self.cell_height = (19.0 * scale).round();
        self.viewport
            .update(&self.queue, Resolution { width, height });
        if self.rasters.len() > 60000 {
            self.rows.clear();
            self.rasters.clear();
            self.glyph_ids.clear();
            self.swash = SwashCache::new();
            let cache = Cache::new(&self.device);
            self.atlas = TextAtlas::new(&self.device, &self.queue, &cache, self.config.format);
        }
        for (y, cells) in frame.cells.chunks(frame.cols as usize).enumerate() {
            // Resizing usually changes only trailing blank columns. Their backgrounds
            // and decorations are drawn separately; keep glyph shaping cached.
            let end = cells
                .iter()
                .rposition(|c| !c.text.is_empty() && c.text != " ")
                .map_or(0, |i| i + 1);
            let cells = &cells[..end];
            if self
                .rows
                .get(y)
                .is_none_or(|r| r.scale != scale || r.cells != cells)
            {
                let row = self.row(cells, frame, scale)?;
                if y < self.rows.len() {
                    self.rows[y] = row
                } else {
                    self.rows.push(row)
                }
            }
        }
        self.rows.truncate(frame.rows as usize);
        let mut custom = Vec::new();
        for (y, row) in self.rows.iter().enumerate() {
            for g in &row.glyphs {
                let mut g = *g;
                g.top += y as f32 * self.cell_height;
                custom.push(g)
            }
        }
        let shared_fonts = self.fonts.clone();
        let mut fonts = shared_fonts.lock().unwrap();
        let empty = Buffer::new(&mut fonts, Metrics::new(14.0 * scale, self.cell_height));
        let areas = [TextArea {
            buffer: &empty,
            left: 0.0,
            top: 0.0,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            },
            default_color: Color::rgb(220, 224, 235),
            custom_glyphs: &custom,
        }];
        let rasters = &self.rasters;
        self.text.prepare_with_custom(
            &self.device,
            &self.queue,
            &mut fonts,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash,
            |r| {
                let g = rasters.get(r.id as usize)?;
                Some(RasterizedCustomGlyph {
                    data: g.data.clone(),
                    content_type: g.kind,
                })
            },
        )?;
        let mut vertices = Vec::new();
        let mut rect = |x: f32, y: f32, w: f32, h: f32, c: [u8; 3], alpha: f32| {
            let color = [
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                alpha,
            ];
            let (x0, x1) = (
                x / width as f32 * 2.0 - 1.0,
                (x + w) / width as f32 * 2.0 - 1.0,
            );
            let (y0, y1) = (
                1.0 - y / height as f32 * 2.0,
                1.0 - (y + h) / height as f32 * 2.0,
            );
            for p in [[x0, y0], [x1, y0], [x0, y1], [x0, y1], [x1, y0], [x1, y1]] {
                vertices.push(Vertex { position: p, color })
            }
        };
        for (i, c) in frame.cells.iter().enumerate() {
            let fg = self
                .theme
                .map_or(c.fg, |t| t.resolve(frame, c.fg, c.fg_source, c.faint));
            let bg = self
                .theme
                .map_or(c.bg, |t| t.resolve(frame, c.bg, c.bg_source, false));
            let x = (i % frame.cols as usize) as f32 * self.cell_width;
            let y = (i / frame.cols as usize) as f32 * self.cell_height;
            rect(x, y, self.cell_width, self.cell_height, bg, 1.0);
            if frame
                .selection
                .is_some_and(|(a, b)| i >= a.min(b) && i <= a.max(b))
            {
                rect(
                    x,
                    y,
                    self.cell_width,
                    self.cell_height,
                    [75, 124, 190],
                    0.65,
                )
            }
            if let Some([left, right, up, down]) = box_lines(&c.text) {
                let thick = if ["━", "┃", "┏", "┓", "┗", "┛", "┣", "┫", "┳", "┻", "╋"]
                    .contains(&c.text.as_str())
                {
                    2.0 * scale
                } else {
                    scale
                };
                let cx = (x + self.cell_width * 0.5).floor();
                let cy = (y + self.cell_height * 0.5).floor();
                if left {
                    rect(x, cy, self.cell_width * 0.5 + thick, thick, fg, 1.0)
                }
                if right {
                    rect(cx, cy, self.cell_width - (cx - x), thick, fg, 1.0)
                }
                if up {
                    rect(cx, y, thick, self.cell_height * 0.5 + thick, fg, 1.0)
                }
                if down {
                    rect(cx, cy, thick, self.cell_height - (cy - y), fg, 1.0)
                }
            }
            if c.underline {
                rect(
                    x,
                    y + self.cell_height - 2.0 * scale,
                    self.cell_width,
                    scale,
                    fg,
                    1.0,
                )
            }
            if c.strike {
                rect(
                    x,
                    y + self.cell_height * 0.55,
                    self.cell_width,
                    scale,
                    fg,
                    1.0,
                )
            }
        }
        if frame.cursor_visible && show_cursor {
            let x = frame.cursor.0 as f32 * self.cell_width;
            let y = frame.cursor.1 as f32 * self.cell_height;
            match frame.cursor_style {
                1 => rect(
                    x,
                    y,
                    self.cell_width,
                    self.cell_height,
                    [170, 190, 235],
                    0.45,
                ),
                2 => rect(
                    x,
                    y + self.cell_height - 2.0 * scale,
                    self.cell_width,
                    2.0 * scale,
                    [170, 190, 235],
                    1.0,
                ),
                3 => {
                    for (rx, ry, rw, rh) in [
                        (x, y, self.cell_width, scale),
                        (x, y + self.cell_height - scale, self.cell_width, scale),
                        (x, y, scale, self.cell_height),
                        (x + self.cell_width - scale, y, scale, self.cell_height),
                    ] {
                        rect(rx, ry, rw, rh, [170, 190, 235], 1.0)
                    }
                }
                _ => rect(x, y, 2.0 * scale, self.cell_height, [170, 190, 235], 1.0),
            }
        }
        let vertex = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terminal rectangles"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let output = if let Some(surface) = &self.surface {
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    surface.configure(&self.device, &self.config);
                    return Ok(());
                }
                e => anyhow::bail!("surface unavailable: {e:?}"),
            }
        } else {
            None
        };
        let texture = if output.is_none() {
            Some(self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("terminal readback"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }))
        } else {
            None
        };
        let view = output
            .as_ref()
            .map(|o| &o.texture)
            .or(texture.as_ref())
            .unwrap()
            .create_view(&Default::default());
        let background = self.theme.map_or(frame.background, |t| {
            t.resolve(frame, frame.background, ColorSource::Background, false)
        });
        let linear = background.map(|v| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jam terminal"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: linear[0],
                            g: linear[1],
                            b: linear[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&self.rects);
                pass.set_vertex_buffer(0, vertex.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
            self.text.render(&self.atlas, &self.viewport, &mut pass)?;
        }
        self.queue.submit(Some(enc.finish()));
        if let Some(output) = output {
            self.queue.present(output);
            self.presented += 1;
        } else {
            self.output = texture;
        }
        self.atlas.trim();
        Ok(())
    }
    pub fn presented_frames(&self) -> u64 {
        self.presented
    }
    pub fn read_pixels(&self) -> Result<Vec<u8>> {
        let texture = self
            .output
            .as_ref()
            .context("render a headless frame first")?;
        let width = self.config.width;
        let height = self.config.height;
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pixel readback"),
            size: (stride * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(enc.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(stride as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_changes_preserve_explicit_colors_and_program_overrides() {
        let mut terminal = jam_core::vt::terminal(3, 1).unwrap();
        terminal.vt_write(b" \x1b[48;2;18;21;28m \x1b[0m ");
        let mut frame = jam_core::vt::frame(&terminal).unwrap();
        frame.cursor_visible = false;
        let mut renderer = Renderer::headless(32, 25).unwrap();
        for background in [[230, 230, 230], [30, 30, 30]] {
            renderer.set_theme([80, 90, 100], background);
            renderer.render(&frame, 32, 25, 1.).unwrap();
            let pixels = renderer.read_pixels().unwrap();
            assert_eq!(&pixels[..3], &background);
            assert_eq!(&pixels[9 * 4..9 * 4 + 3], &DEFAULT_BACKGROUND);
            assert_eq!(&pixels[28 * 4..28 * 4 + 3], &background);
            let theme = renderer.theme.unwrap();
            assert_eq!(
                theme.resolve(&frame, DEFAULT_FOREGROUND, ColorSource::Foreground, true),
                [40, 45, 50]
            );
            assert_eq!(
                theme.resolve(&frame, DEFAULT_FOREGROUND, ColorSource::Foreground, false),
                [80, 90, 100]
            );
        }
        terminal.vt_write(b"\x1b]11;rgb:12/34/56\x07");
        frame = jam_core::vt::frame(&terminal).unwrap();
        frame.cursor_visible = false;
        renderer.render(&frame, 32, 25, 1.).unwrap();
        assert_eq!(
            &renderer.read_pixels().unwrap()[28 * 4..28 * 4 + 3],
            &[18, 52, 86]
        );
    }
    #[test]
    fn all_terminal_styles_use_monospaced_embedded_faces() {
        let mut fonts = terminal_fonts();
        for (sgr, expected) in [
            ("0", "JetBrainsMono-Regular"),
            ("1", "JetBrainsMono-Bold"),
            ("3", "JetBrainsMono-Italic"),
            ("1;3", "JetBrainsMono-BoldItalic"),
        ] {
            let mut t = jam_core::vt::terminal(40, 2).unwrap();
            t.vt_write(format!("\x1b[{sgr}m~/wt/jam/test-wt").as_bytes());
            let f = jam_core::vt::frame(&t).unwrap();
            let (buffer, _) = shape_row(&mut fonts, &f.cells[..16], 2.0);
            for run in buffer.layout_runs() {
                for g in run.glyphs {
                    let face = fonts.db().face(g.font_id).unwrap();
                    assert_eq!(face.post_script_name, expected, "wrong face for {sgr}");
                    assert!(
                        (g.w - 16.8).abs() < 0.01,
                        "proportional advance for {sgr}: {}",
                        g.w
                    );
                }
            }
        }
    }
}
