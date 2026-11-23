//! The CraftOffice Sketch window: menus, tools, the canvas, colour, brush and layer panels.

use std::sync::mpsc::{channel, Receiver, Sender};

use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, Sense, Stroke as UiStroke, TextureHandle, TextureOptions, Ui, Vec2};

use crate::files;
use crate::i18n::{self, t, Lang};
use crate::raster::{self, Blend, Brush, Doc, Gradient, History, IRect, Mode, Moving, Shape, Stroke, Tip, Transform, Undo};
use crate::text;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Move,
    Pencil,
    Pen,
    Brush,
    Marker,
    Airbrush,
    Eraser,
    Fill,
    Gradient,
    Eyedropper,
    Line,
    Rect,
    Ellipse,
    Text,
    Hand,
}

const TOOLS: [(Tool, &str, &str); 16] = [
    (Tool::Select, "Select", "S"),
    (Tool::Move, "Move", "V"),
    (Tool::Pencil, "Pencil", "P"),
    (Tool::Pen, "Pen", "N"),
    (Tool::Brush, "Brush", "B"),
    (Tool::Marker, "Marker", "M"),
    (Tool::Airbrush, "Airbrush", "A"),
    (Tool::Eraser, "Eraser", "E"),
    (Tool::Fill, "Fill", "G"),
    (Tool::Gradient, "Gradient", "D"),
    (Tool::Eyedropper, "Eyedropper", "I"),
    (Tool::Line, "Line", "L"),
    (Tool::Rect, "Rectangle", "U"),
    (Tool::Ellipse, "Ellipse", "O"),
    (Tool::Text, "Text", "T"),
    (Tool::Hand, "Hand", "H"),
];

#[derive(Clone, Copy)]
struct ToolSettings {
    size: f32,
    opacity: f32,
    hardness: f32,
}

impl Tool {
    fn defaults(self) -> ToolSettings {
        let (size, opacity, hardness) = match self {
            Tool::Pencil => (3.0, 1.0, 1.0),
            Tool::Pen => (6.0, 1.0, 0.9),
            Tool::Brush => (20.0, 1.0, 0.5),
            Tool::Marker => (16.0, 0.5, 0.95),
            Tool::Airbrush => (60.0, 1.0, 0.0),
            Tool::Eraser => (24.0, 1.0, 0.8),
            Tool::Line | Tool::Rect | Tool::Ellipse => (4.0, 1.0, 1.0),
            Tool::Fill | Tool::Gradient => (1.0, 1.0, 1.0),
            Tool::Eyedropper | Tool::Hand | Tool::Select | Tool::Move | Tool::Text => (1.0, 1.0, 1.0),
        };
        ToolSettings { size, opacity, hardness }
    }
    fn paints(self) -> bool {
        matches!(self, Tool::Pencil | Tool::Pen | Tool::Brush | Tool::Marker | Tool::Airbrush | Tool::Eraser)
    }
    fn shape(self) -> Option<Shape> {
        match self {
            Tool::Line => Some(Shape::Line),
            Tool::Rect => Some(Shape::Rect),
            Tool::Ellipse => Some(Shape::Ellipse),
            _ => None,
        }
    }
    fn has_size(self) -> bool {
        self.paints() || self.shape().is_some()
    }
}

/// What a file dialog or a read came back with.
pub enum Io {
    /// A file to open as the drawing (or to import as a layer).
    Opened { name: String, path: Option<String>, bytes: Vec<u8>, as_layer: bool },
    /// Where to save (native).
    SaveTo { path: String, kind: SaveKind },
    /// A font file for the Text tool.
    Font { name: String, bytes: Vec<u8> },
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SaveKind {
    Project,
    Png,
    Jpeg,
}

impl SaveKind {
    fn ext(self) -> &'static str {
        match self {
            SaveKind::Project => "cosketch",
            SaveKind::Png => "png",
            SaveKind::Jpeg => "jpg",
        }
    }
}

struct NewDlg {
    w: u32,
    h: u32,
    white: bool,
}

struct TextDlg {
    pos: (f32, f32),
    text: String,
    size: f32,
    font: usize,
    align: text::Align,
    /// What the preview was made from, and the preview itself.
    key: String,
    mask: Option<text::Mask>,
    tex: Option<TextureHandle>,
}

struct SizeDlg {
    w: u32,
    h: u32,
    anchor: (f32, f32),
}

pub struct SketchApp {
    doc: Doc,
    hist: History,
    /// The composited picture (all layers with their blend modes) shown on the canvas.
    comp: Option<TextureHandle>,
    /// The part of the picture to composite again.
    comp_dirty: IRect,
    rebuild: bool,
    checker: Option<TextureHandle>,
    name: String,
    path: Option<String>,
    dirty: bool,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    tool: Tool,
    settings: std::collections::HashMap<Tool, ToolSettings>,
    color: Color32,
    secondary: Color32,
    recent: Vec<Color32>,
    smoothing: f32,
    pressure_size: bool,
    pressure_alpha: bool,
    tolerance: u8,
    shape_fill: bool,
    stroke: Option<Stroke>,
    smooth: Option<Pos2>,
    shape_start: Option<(f32, f32)>,
    shape_end: Option<(f32, f32)>,
    pressure: f32,
    show_panels: bool,
    new_dlg: Option<NewDlg>,
    size_dlg: Option<SizeDlg>,
    about: bool,
    shortcuts: bool,
    confirm_close: bool,
    allow_close: bool,
    rename: Option<(usize, String)>,
    toast: Option<(String, f64)>,
    io_tx: Sender<Io>,
    io_rx: Receiver<Io>,
    max_side: u32,
    hover: Option<(f32, f32)>,
    tips: std::collections::HashMap<Tool, Tip>,
    selection: Option<IRect>,
    sel_start: Option<(f32, f32)>,
    moving: Option<Moving>,
    move_start: Option<(f32, f32)>,
    grad_kind: Gradient,
    grad_fade: bool,
    grad_start: Option<(f32, f32)>,
    grad_end: Option<(f32, f32)>,
    text_dlg: Option<TextDlg>,
    text_size: f32,
    text_font: usize,
    fonts: Vec<(String, ab_glyph::FontArc)>,
    clipboard: Option<(IRect, Vec<u8>)>,
    /// 0 copy, 1 cut, 2 paste, from the keyboard.
    clip_cmd: Option<u8>,
    move_off: (i64, i64),
}

const PALETTE: [[u8; 3]; 18] = [
    [0, 0, 0],
    [64, 64, 64],
    [128, 128, 128],
    [192, 192, 192],
    [255, 255, 255],
    [128, 64, 32],
    [220, 40, 40],
    [255, 128, 0],
    [255, 210, 0],
    [120, 200, 40],
    [20, 140, 60],
    [0, 180, 200],
    [30, 110, 230],
    [20, 40, 140],
    [130, 60, 200],
    [230, 80, 170],
    [255, 190, 160],
    [250, 235, 200],
];

impl SketchApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<(String, Option<String>, Vec<u8>)>) -> Self {
        i18n::set(i18n::detect());
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let (io_tx, io_rx) = channel();
        let max_side = if cfg!(target_arch = "wasm32") { 4096 } else { 8192 };
        let mut app = SketchApp {
            doc: Doc::new(1600, 1200, Some([255, 255, 255, 255]), t("Background"), &format!("{} 1", t("Layer"))),
            hist: History::default(),
            comp: None,
            comp_dirty: IRect::empty(),
            rebuild: true,
            checker: None,
            name: t("Untitled").to_string(),
            path: None,
            dirty: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit: true,
            tool: Tool::Pen,
            settings: TOOLS.iter().map(|(tl, _, _)| (*tl, tl.defaults())).collect(),
            color: Color32::from_rgb(20, 20, 20),
            secondary: Color32::WHITE,
            recent: Vec::new(),
            smoothing: 0.3,
            pressure_size: true,
            pressure_alpha: false,
            tolerance: 32,
            shape_fill: false,
            stroke: None,
            smooth: None,
            shape_start: None,
            shape_end: None,
            pressure: 1.0,
            show_panels: true,
            new_dlg: None,
            size_dlg: None,
            about: false,
            shortcuts: false,
            confirm_close: false,
            allow_close: false,
            rename: None,
            toast: None,
            io_tx,
            io_rx,
            max_side,
            hover: None,
            tips: TOOLS.iter().map(|(tl, _, _)| (*tl, Tip::Round)).collect(),
            selection: None,
            sel_start: None,
            moving: None,
            move_start: None,
            grad_kind: Gradient::Linear,
            grad_fade: false,
            grad_start: None,
            grad_end: None,
            text_dlg: None,
            text_size: 64.0,
            text_font: 0,
            fonts: text::built_in(),
            clipboard: None,
            clip_cmd: None,
            move_off: (0, 0),
        };
        if let Some((name, path, bytes)) = open {
            app.open_bytes(name, path, bytes, false);
        }
        app
    }

    pub fn io_sender(&self) -> Sender<Io> {
        self.io_tx.clone()
    }

    fn set_toast(&mut self, ctx: &egui::Context, s: impl Into<String>) {
        self.toast = Some((s.into(), ctx.input(|i| i.time) + 3.0));
    }

    // ---- document ----

    fn reset_doc(&mut self, doc: Doc, name: String, path: Option<String>) {
        self.doc = doc;
        self.hist.clear();
        self.name = name;
        self.path = path;
        self.dirty = false;
        self.rebuild = true;
        self.fit = true;
        self.stroke = None;
        self.moving = None;
        self.selection = None;
        self.text_dlg = None;
    }

    fn open_bytes(&mut self, name: String, path: Option<String>, bytes: Vec<u8>, as_layer: bool) {
        if !as_layer && files::is_project(&bytes) {
            match files::load_project(&bytes) {
                Ok(d) => {
                    let stem = name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string());
                    self.reset_doc(d, stem, path);
                }
                Err(e) => self.toast = Some((format!("{}: {e}", t("Could not open the file")), f64::MAX)),
            }
            return;
        }
        match files::decode_image(&bytes) {
            Ok((w, h, px)) => {
                let stem = name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string());
                if as_layer {
                    let layer = files::picture_layer(&self.doc, stem, w, h, &px);
                    self.doc_op(|d| {
                        let at = d.active + 1;
                        d.layers.insert(at, layer);
                        d.active = at;
                    });
                } else {
                    let (w2, h2, px2) = self.fit_size(w, h, px);
                    let mut d = Doc::new(w2, h2, None, t("Background"), t("Background"));
                    d.layers[0].px = px2;
                    d.layers.push(raster::Layer::new(format!("{} 1", t("Layer")), w2, h2, [0, 0, 0, 0]));
                    d.active = 1;
                    // a picture opened to draw on is saved as a drawing, not over the picture
                    self.reset_doc(d, stem, None);
                }
            }
            Err(e) => self.toast = Some((format!("{}: {e}", t("Could not open the file")), f64::MAX)),
        }
    }

    /// Scale an opened picture down to the largest canvas this device can show.
    fn fit_size(&self, w: u32, h: u32, px: Vec<u8>) -> (u32, u32, Vec<u8>) {
        let m = self.max_side;
        if w <= m && h <= m {
            return (w, h, px);
        }
        let s = (m as f32 / w as f32).min(m as f32 / h as f32);
        let (nw, nh) = (((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1));
        let img = image::RgbaImage::from_raw(w, h, px).unwrap_or_default();
        let r = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
        (nw, nh, r.into_raw())
    }

    /// A structural change (layers, size) as one undo step.
    fn doc_op(&mut self, f: impl FnOnce(&mut Doc)) {
        let before = Box::new(self.doc.clone());
        f(&mut self.doc);
        self.doc.active = self.doc.active.min(self.doc.layers.len().saturating_sub(1));
        self.hist.push(Undo::Doc { before, after: Box::new(self.doc.clone()) });
        self.dirty = true;
        self.rebuild = true;
    }

    fn pixels(&mut self, u: Option<Undo>) {
        if let Some(u) = u {
            if let Undo::Pixels { rect, .. } = &u {
                self.mark(*rect);
            }
            self.hist.push(u);
            self.dirty = true;
        }
    }

    fn mark(&mut self, r: IRect) {
        self.comp_dirty.union(r);
    }

    fn undo(&mut self) {
        self.cancel_stroke();
        if self.hist.undo(&mut self.doc).is_some() {
            self.rebuild = true;
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        if self.hist.redo(&mut self.doc).is_some() {
            self.rebuild = true;
            self.dirty = true;
        }
    }

    fn cancel_stroke(&mut self) {
        if let Some(s) = self.stroke.take() {
            let l = s.layer;
            self.doc.layers[l].px = s.before;
            self.rebuild = true;
        }
        if let Some(m) = self.moving.take() {
            m.cancel(&mut self.doc);
            self.rebuild = true;
        }
        self.move_start = None;
        self.shape_start = None;
        self.shape_end = None;
        self.grad_start = None;
        self.grad_end = None;
        self.sel_start = None;
    }

    /// Finish a move in progress.
    fn drop_moving(&mut self) {
        if let Some(m) = self.moving.take() {
            let dst = m.dst;
            let u = m.end(&self.doc);
            self.pixels(u);
            if self.selection.is_some() {
                self.selection = Some(dst).filter(|r| !r.is_empty());
            }
        }
        self.move_start = None;
    }

    // ---- selection and clipboard ----

    /// The selection, or the whole canvas.
    fn sel_or_all(&self) -> IRect {
        self.selection.unwrap_or(self.doc.full())
    }

    fn copy(&mut self) {
        let r = self.sel_or_all();
        let data = raster::crop(&self.doc.layers[self.doc.active].px, self.doc.w, r);
        self.clipboard = Some((r, data));
    }

    fn delete_selection(&mut self) {
        if let Some(r) = self.selection {
            let l = self.doc.active;
            let u = raster::clear_rect(&mut self.doc, l, r);
            self.pixels(u);
        }
    }

    fn cut(&mut self) {
        self.copy();
        let r = self.sel_or_all();
        let l = self.doc.active;
        let u = raster::clear_rect(&mut self.doc, l, r);
        self.pixels(u);
    }

    /// Paste as a new layer, where it was copied from (or centred if that is off the canvas).
    fn paste(&mut self) {
        let Some((r, data)) = self.clipboard.clone() else { return };
        let (w, h) = (r.width(), r.height());
        let (mut x0, mut y0) = (r.x0, r.y0);
        if x0 + w > self.doc.w || y0 + h > self.doc.h {
            x0 = self.doc.w.saturating_sub(w) / 2;
            y0 = self.doc.h.saturating_sub(h) / 2;
        }
        let dst = IRect { x0, y0, x1: (x0 + w).min(self.doc.w), y1: (y0 + h).min(self.doc.h) };
        let name = t("Pasted").to_string();
        self.doc_op(|d| {
            let mut l = raster::Layer::new(name, d.w, d.h, [0, 0, 0, 0]);
            raster::over(&mut l.px, d.w, dst, &data, w, 0, 0);
            let at = d.active + 1;
            d.layers.insert(at, l);
            d.active = at;
        });
        self.selection = Some(dst);
        self.tool = Tool::Move;
    }

    // ---- textures ----

    fn sync_textures(&mut self, ctx: &egui::Context) {
        let opts = TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, ..Default::default() };
        let size = [self.doc.w as usize, self.doc.h as usize];
        if self.rebuild || self.comp.as_ref().is_none_or(|t| t.size() != size) {
            let px = self.doc.composite(self.doc.full(), None);
            let img = egui::ColorImage::from_rgba_unmultiplied(size, &px);
            self.comp = Some(ctx.load_texture("picture", img, opts));
            self.comp_dirty = IRect::empty();
            self.rebuild = false;
            return;
        }
        let r = self.comp_dirty.intersect(self.doc.full());
        self.comp_dirty = IRect::empty();
        if r.is_empty() {
            return;
        }
        if let Some(tex) = self.comp.as_mut() {
            let data = self.doc.composite(r, None);
            let img = egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], &data);
            tex.set_partial([r.x0 as usize, r.y0 as usize], img, opts);
        }
    }

    fn checker(&mut self, ctx: &egui::Context) -> egui::TextureId {
        self.checker
            .get_or_insert_with(|| {
                let (a, b) = (Color32::from_gray(205), Color32::from_gray(245));
                let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
                ctx.load_texture(
                    "checker",
                    img,
                    TextureOptions { magnification: egui::TextureFilter::Nearest, wrap_mode: egui::TextureWrapMode::Repeat, ..Default::default() },
                )
            })
            .id()
    }

    // ---- brush ----

    fn brush(&self) -> Brush {
        let s = self.settings[&self.tool];
        let c = self.color;
        let (flow, buildup, spacing, mode) = match self.tool {
            Tool::Airbrush => (0.06, true, 0.08, Mode::Paint),
            Tool::Eraser => (1.0, false, 0.12, Mode::Erase),
            Tool::Pencil => (1.0, false, 0.08, Mode::Paint),
            _ => (1.0, false, 0.12, Mode::Paint),
        };
        let tip = self.tips.get(&self.tool).copied().unwrap_or(Tip::Round);
        let spacing = if tip == Tip::Spray { f32::max(spacing, 0.25) } else { spacing };
        Brush { size: s.size, hardness: s.hardness, opacity: s.opacity, flow, buildup, mode, color: [c.r(), c.g(), c.b()], spacing, tip }
    }

    fn remember_colour(&mut self) {
        let c = self.color;
        self.recent.retain(|&r| r != c);
        self.recent.insert(0, c);
        self.recent.truncate(12);
    }

    // ---- files ----

    fn pick_open(&self, as_layer: bool) {
        let tx = self.io_tx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let mut d = rfd::FileDialog::new().add_filter("CraftOffice Sketch / images", &["cosketch", "png", "jpg", "jpeg", "bmp", "gif", "webp"]);
                if as_layer {
                    d = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp"]);
                }
                if let Some(p) = d.pick_file() {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    match std::fs::read(&p) {
                        Ok(bytes) => {
                            let _ = tx.send(Io::Opened { name, path: Some(p.to_string_lossy().into_owned()), bytes, as_layer });
                        }
                        Err(e) => {
                            let _ = tx.send(Io::Failed(e.to_string()));
                        }
                    }
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let d = rfd::AsyncFileDialog::new().add_filter("CraftOffice Sketch / images", &["cosketch", "png", "jpg", "jpeg", "bmp", "gif", "webp"]);
                if let Some(f) = d.pick_file().await {
                    let bytes = f.read().await;
                    let _ = tx.send(Io::Opened { name: f.file_name(), path: None, bytes, as_layer });
                }
            });
        }
    }

    fn pick_font(&self) {
        let tx = self.io_tx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            if let Some(p) = rfd::FileDialog::new().add_filter("Fonts", &["ttf", "otf", "ttc"]).pick_file() {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                match std::fs::read(&p) {
                    Ok(bytes) => {
                        let _ = tx.send(Io::Font { name, bytes });
                    }
                    Err(e) => {
                        let _ = tx.send(Io::Failed(e.to_string()));
                    }
                }
            }
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(f) = rfd::AsyncFileDialog::new().add_filter("Fonts", &["ttf", "otf", "ttc"]).pick_file().await {
                let bytes = f.read().await;
                let _ = tx.send(Io::Font { name: f.file_name(), bytes });
            }
        });
    }

    fn save(&mut self, ctx: &egui::Context, kind: SaveKind, ask: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if kind == SaveKind::Project && !ask {
                if let Some(p) = self.path.clone().filter(|p| p.to_ascii_lowercase().ends_with(".cosketch")) {
                    self.write_to(ctx, &p, kind);
                    return;
                }
            }
            let tx = self.io_tx.clone();
            let name = format!("{}.{}", self.name, kind.ext());
            std::thread::spawn(move || {
                let filter = match kind {
                    SaveKind::Project => ("CraftOffice Sketch", "cosketch"),
                    SaveKind::Png => ("PNG", "png"),
                    SaveKind::Jpeg => ("JPEG", "jpg"),
                };
                if let Some(mut p) = rfd::FileDialog::new().set_file_name(&name).add_filter(filter.0, &[filter.1]).save_file() {
                    if p.extension().is_none() {
                        p.set_extension(filter.1);
                    }
                    let _ = tx.send(Io::SaveTo { path: p.to_string_lossy().into_owned(), kind });
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ask;
            let name = format!("{}.{}", self.name, kind.ext());
            match self.encode(kind) {
                Ok(bytes) => {
                    crate::web::download(&name, &bytes, mime(kind));
                    if kind == SaveKind::Project {
                        self.dirty = false;
                    }
                    self.set_toast(ctx, format!("{} · {name}", if kind == SaveKind::Project { t("Saved") } else { t("Exported") }));
                }
                Err(e) => self.set_toast(ctx, format!("{}: {e}", t("Could not save the file"))),
            }
        }
    }

    fn encode(&self, kind: SaveKind) -> Result<Vec<u8>, String> {
        match kind {
            SaveKind::Project => files::save_project(&self.doc),
            SaveKind::Png => files::export_png(&self.doc),
            SaveKind::Jpeg => files::export_jpeg(&self.doc),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn write_to(&mut self, ctx: &egui::Context, path: &str, kind: SaveKind) {
        match self.encode(kind).and_then(|b| std::fs::write(path, b).map_err(|e| e.to_string())) {
            Ok(()) => {
                let file = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if kind == SaveKind::Project {
                    self.path = Some(path.to_string());
                    self.name = file.rsplit_once('.').map_or(file.clone(), |(s, _)| s.to_string());
                    self.dirty = false;
                    self.set_toast(ctx, format!("{} · {file}", t("Saved")));
                    if self.allow_close {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                } else {
                    self.set_toast(ctx, format!("{} · {file}", t("Exported")));
                }
            }
            Err(e) => self.toast = Some((format!("{}: {e}", t("Could not save the file")), f64::MAX)),
        }
    }

    fn poll_io(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.io_rx.try_recv() {
            match m {
                Io::Opened { name, path, bytes, as_layer } => self.open_bytes(name, path, bytes, as_layer),
                Io::SaveTo { path, kind } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    self.write_to(ctx, &path, kind);
                    #[cfg(target_arch = "wasm32")]
                    let _ = (path, kind);
                }
                Io::Font { name, bytes } => match text::load(bytes) {
                    Ok(f) => {
                        let stem = name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string());
                        self.fonts.push((stem, f));
                        self.text_font = self.fonts.len() - 1;
                        if let Some(d) = self.text_dlg.as_mut() {
                            d.font = self.text_font;
                        }
                    }
                    Err(e) => self.set_toast(ctx, format!("{}: {e}", t("Could not open the file"))),
                },
                Io::Failed(e) => self.set_toast(ctx, format!("{}: {e}", t("Could not open the file"))),
            }
        }
    }

    // ---- keyboard ----

    fn shortcuts_input(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.new_dlg.is_some() || self.size_dlg.is_some() || self.rename.is_some() || self.text_dlg.is_some() {
            return;
        }
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        if cmd {
            if pressed(Key::Z) {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if pressed(Key::Y) {
                self.redo();
            }
            if pressed(Key::S) {
                self.save(ctx, SaveKind::Project, shift);
            }
            if pressed(Key::O) {
                self.pick_open(false);
            }
            if pressed(Key::N) {
                self.new_dlg = Some(NewDlg { w: self.doc.w, h: self.doc.h, white: true });
            }
            if pressed(Key::E) {
                self.save(ctx, SaveKind::Png, true);
            }
            if pressed(Key::Num0) {
                self.fit = true;
            }
            if pressed(Key::Num1) {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            if pressed(Key::A) {
                self.drop_moving();
                self.selection = Some(self.doc.full());
            }
            if pressed(Key::D) {
                self.drop_moving();
                self.selection = None;
            }
            if pressed(Key::C) {
                self.clip_cmd = Some(0);
            }
            if pressed(Key::X) {
                self.clip_cmd = Some(1);
            }
            if pressed(Key::V) {
                self.clip_cmd = Some(2);
            }
            return;
        }
        for (tl, _, key) in TOOLS {
            if let Some(k) = Key::from_name(key) {
                if pressed(k) {
                    self.tool = tl;
                }
            }
        }
        if pressed(Key::X) {
            std::mem::swap(&mut self.color, &mut self.secondary);
        }
        if self.tool.has_size() {
            let s = self.settings.get_mut(&self.tool).map(|s| &mut s.size);
            if let Some(size) = s {
                if pressed(Key::OpenBracket) {
                    *size = (*size / 1.15).max(1.0);
                }
                if pressed(Key::CloseBracket) {
                    *size = (*size * 1.15).min(500.0);
                }
            }
        }
        if pressed(Key::Plus) || pressed(Key::Equals) {
            self.zoom = (self.zoom * 1.25).min(32.0);
        }
        if pressed(Key::Minus) {
            self.zoom = (self.zoom / 1.25).max(0.05);
        }
        if pressed(Key::Escape) {
            self.cancel_stroke();
        }
        if pressed(Key::Delete) || pressed(Key::Backspace) {
            self.drop_moving();
            self.delete_selection();
        }
        if pressed(Key::Enter) {
            self.drop_moving();
        }
    }

    /// Clipboard commands from the keyboard (they arrive as events, so they are handled once here).
    fn clip_commands(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Copy => self.clip_cmd = Some(0),
                    egui::Event::Cut => self.clip_cmd = Some(1),
                    egui::Event::Paste(_) => self.clip_cmd = Some(2),
                    _ => {}
                }
            }
        });
        if ctx.egui_wants_keyboard_input() || self.text_dlg.is_some() {
            self.clip_cmd = None;
            return;
        }
        match self.clip_cmd.take() {
            Some(0) => self.copy(),
            Some(1) => {
                self.drop_moving();
                self.cut();
            }
            Some(2) => {
                self.drop_moving();
                self.paste();
            }
            _ => {}
        }
    }

    // ---- panels ----

    fn menu(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t("File"), |ui| {
                if ui.button(t("New…")).clicked() {
                    self.new_dlg = Some(NewDlg { w: self.doc.w, h: self.doc.h, white: true });
                    ui.close();
                }
                if ui.button(t("Open…")).clicked() {
                    self.pick_open(false);
                    ui.close();
                }
                if ui.button(t("Import as Layer…")).clicked() {
                    self.pick_open(true);
                    ui.close();
                }
                ui.separator();
                if ui.button(t("Save")).clicked() {
                    self.save(&ctx, SaveKind::Project, false);
                    ui.close();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button(t("Save As…")).clicked() {
                    self.save(&ctx, SaveKind::Project, true);
                    ui.close();
                }
                ui.separator();
                if ui.button(t("Export PNG…")).clicked() {
                    self.save(&ctx, SaveKind::Png, true);
                    ui.close();
                }
                if ui.button(t("Export JPEG…")).clicked() {
                    self.save(&ctx, SaveKind::Jpeg, true);
                    ui.close();
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.separator();
                    if ui.button(t("Quit")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                }
            });
            ui.menu_button(t("Edit"), |ui| {
                if ui.add_enabled(self.hist.can_undo(), egui::Button::new(t("Undo"))).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(self.hist.can_redo(), egui::Button::new(t("Redo"))).clicked() {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button(format!("{}   Ctrl+X", t("Cut"))).clicked() {
                    self.drop_moving();
                    self.cut();
                    ui.close();
                }
                if ui.button(format!("{}   Ctrl+C", t("Copy"))).clicked() {
                    self.copy();
                    ui.close();
                }
                if ui.add_enabled(self.clipboard.is_some(), egui::Button::new(format!("{}   Ctrl+V", t("Paste as New Layer")))).clicked() {
                    self.drop_moving();
                    self.paste();
                    ui.close();
                }
                if ui.add_enabled(self.selection.is_some(), egui::Button::new(format!("{}   Del", t("Delete Selection")))).clicked() {
                    self.drop_moving();
                    self.delete_selection();
                    ui.close();
                }
                ui.separator();
                if ui.button(format!("{}   Ctrl+A", t("Select All"))).clicked() {
                    self.drop_moving();
                    self.selection = Some(self.doc.full());
                    ui.close();
                }
                if ui.add_enabled(self.selection.is_some(), egui::Button::new(format!("{}   Ctrl+D", t("Deselect")))).clicked() {
                    self.drop_moving();
                    self.selection = None;
                    ui.close();
                }
                ui.separator();
                if ui.button(t("Clear Layer")).clicked() {
                    self.doc_op(|d| {
                        let a = d.active;
                        d.layers[a].px.iter_mut().for_each(|v| *v = 0);
                    });
                    ui.close();
                }
                if ui.button(t("Fill Layer with Colour")).clicked() {
                    let c = self.color;
                    self.doc_op(|d| {
                        let a = d.active;
                        for p in d.layers[a].px.chunks_exact_mut(4) {
                            p.copy_from_slice(&[c.r(), c.g(), c.b(), 255]);
                        }
                    });
                    ui.close();
                }
            });
            ui.menu_button(t("Image"), |ui| {
                for (label, tr) in [
                    ("Flip Horizontal", Transform::FlipH),
                    ("Flip Vertical", Transform::FlipV),
                    ("Rotate 90° Clockwise", Transform::RotCw),
                    ("Rotate 90° Counter-clockwise", Transform::RotCcw),
                ] {
                    if ui.button(t(label)).clicked() {
                        self.doc_op(|d| raster::transform(d, tr));
                        self.fit = true;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.add_enabled(self.selection.is_some(), egui::Button::new(t("Crop to Selection"))).clicked() {
                    self.drop_moving();
                    if let Some(r) = self.selection.take() {
                        self.doc_op(|d| raster::crop_doc(d, r));
                        self.fit = true;
                    }
                    ui.close();
                }
                if ui.button(t("Canvas Size…")).clicked() {
                    self.size_dlg = Some(SizeDlg { w: self.doc.w, h: self.doc.h, anchor: (0.5, 0.5) });
                    ui.close();
                }
                if ui.button(t("Flatten Image")).clicked() {
                    self.doc_op(|d| {
                        let flat = d.flatten(None);
                        let mut l = raster::Layer::new(t("Background").to_string(), d.w, d.h, [0, 0, 0, 0]);
                        l.px = flat;
                        d.layers = vec![l];
                        d.active = 0;
                    });
                    ui.close();
                }
            });
            ui.menu_button(t("Layer"), |ui| {
                self.layer_actions(ui, true);
            });
            ui.menu_button(t("View"), |ui| {
                if ui.button(format!("{}   +", t("Zoom In"))).clicked() {
                    self.zoom = (self.zoom * 1.25).min(32.0);
                    ui.close();
                }
                if ui.button(format!("{}   -", t("Zoom Out"))).clicked() {
                    self.zoom = (self.zoom / 1.25).max(0.05);
                    ui.close();
                }
                if ui.button(t("Fit to Window")).clicked() {
                    self.fit = true;
                    ui.close();
                }
                if ui.button(t("Actual Size")).clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut self.show_panels, t("Panels"));
                ui.menu_button(t("Language"), |ui| {
                    for (l, n) in [(Lang::Hu, "Magyar"), (Lang::Ro, "Română"), (Lang::En, "English")] {
                        if ui.selectable_label(i18n::current() == l, n).clicked() {
                            i18n::set(l);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button(t("Help"), |ui| {
                if ui.button(t("Keyboard Shortcuts")).clicked() {
                    self.shortcuts = true;
                    ui.close();
                }
                if ui.button(t("About CraftOffice Sketch")).clicked() {
                    self.about = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(self.show_panels, t("Panels")).clicked() {
                    self.show_panels = !self.show_panels;
                }
                ui.label(egui::RichText::new(format!("{}{}", self.name, if self.dirty { " •" } else { "" })).weak());
            });
        });
    }

    fn layer_actions(&mut self, ui: &mut Ui, menu: bool) {
        let n = self.doc.layers.len();
        let a = self.doc.active;
        let mut close = false;
        let row = |ui: &mut Ui, enabled: bool, label: &'static str, icon: &str| -> bool {
            let _ = icon;
            ui.add_enabled(enabled, egui::Button::new(t(label))).clicked()
        };
        if row(ui, true, "New Layer", "＋") {
            let name = format!("{} {}", t("Layer"), n + 1);
            self.doc_op(|d| {
                let l = raster::Layer::new(name, d.w, d.h, [0, 0, 0, 0]);
                d.layers.insert(a + 1, l);
                d.active = a + 1;
            });
            close = true;
        }
        if row(ui, true, "Duplicate Layer", "⧉") {
            self.doc_op(|d| {
                let mut l = d.layers[a].clone();
                l.name = format!("{} 2", l.name);
                d.layers.insert(a + 1, l);
                d.active = a + 1;
            });
            close = true;
        }
        if row(ui, n > 1, "Delete Layer", "🗑") {
            self.doc_op(|d| {
                d.layers.remove(a);
                d.active = a.saturating_sub(1);
            });
            close = true;
        }
        if row(ui, a > 0, "Merge Down", "⤓") {
            self.doc_op(|d| {
                let upper = d.layers.remove(a);
                raster::merge(&mut d.layers[a - 1], &upper);
                d.active = a - 1;
            });
            close = true;
        }
        if row(ui, a + 1 < n, "Move Up", "▲") {
            self.doc_op(|d| {
                d.layers.swap(a, a + 1);
                d.active = a + 1;
            });
            close = true;
        }
        if row(ui, a > 0, "Move Down", "▼") {
            self.doc_op(|d| {
                d.layers.swap(a, a - 1);
                d.active = a - 1;
            });
            close = true;
        }
        if menu && close {
            ui.close();
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        for (tl, label, key) in TOOLS {
            let selected = self.tool == tl;
            let (rect, r) = ui.allocate_exact_size(vec2(40.0, 36.0), Sense::click());
            let vis = ui.visuals();
            let bg = if selected {
                vis.selection.bg_fill
            } else if r.hovered() {
                vis.widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect.shrink(2.0), CornerRadius::same(6), bg);
            let fg = if selected { Color32::WHITE } else { vis.text_color() };
            tool_icon(ui.painter(), rect.shrink(9.0), tl, fg);
            if r.on_hover_text(format!("{}  ({key})", t(label))).clicked() {
                self.tool = tl;
            }
        }
        ui.add_space(8.0);
        // primary and secondary colour
        let (rect, r) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
        let back = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(20.0, 20.0));
        let front = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(20.0, 20.0));
        ui.painter().rect_filled(back, CornerRadius::same(3), self.secondary);
        ui.painter().rect_stroke(back, CornerRadius::same(3), UiStroke::new(1.0, Color32::GRAY), egui::StrokeKind::Middle);
        ui.painter().rect_filled(front, CornerRadius::same(3), self.color);
        ui.painter().rect_stroke(front, CornerRadius::same(3), UiStroke::new(1.0, Color32::GRAY), egui::StrokeKind::Middle);
        if r.on_hover_text(format!("{}  (X)", t("Swap colours"))).clicked() {
            std::mem::swap(&mut self.color, &mut self.secondary);
        }
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading(t("Colour"));
            let mut c = self.color;
            if egui::widgets::color_picker::color_picker_color32(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque) {
                self.color = c;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                for p in PALETTE {
                    let col = Color32::from_rgb(p[0], p[1], p[2]);
                    if swatch(ui, col, self.color == col).clicked() {
                        self.color = col;
                    }
                }
            });
            if !self.recent.is_empty() {
                ui.label(egui::RichText::new(t("Recent colours")).small().weak());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                    for col in self.recent.clone() {
                        if swatch(ui, col, self.color == col).clicked() {
                            self.color = col;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(t("Secondary colour"));
                ui.color_edit_button_srgba(&mut self.secondary);
            });
            ui.separator();
            ui.heading(t(TOOLS.iter().find(|x| x.0 == self.tool).map_or("Tool", |x| x.1)));
            let tool = self.tool;
            if let Some(s) = self.settings.get_mut(&tool) {
                if tool.has_size() {
                    ui.add(egui::Slider::new(&mut s.size, 1.0..=500.0).logarithmic(true).text(t("Size")));
                }
                if tool.paints() || tool.shape().is_some() || tool == Tool::Fill {
                    let mut op = s.opacity * 100.0;
                    if ui.add(egui::Slider::new(&mut op, 1.0..=100.0).suffix("%").text(t("Opacity"))).changed() {
                        s.opacity = op / 100.0;
                    }
                }
                if tool.paints() && tool != Tool::Pencil {
                    let mut hd = s.hardness * 100.0;
                    if ui.add(egui::Slider::new(&mut hd, 0.0..=100.0).suffix("%").text(t("Hardness"))).changed() {
                        s.hardness = hd / 100.0;
                    }
                }
            }
            if tool.paints() {
                let mut tip = self.tips.get(&tool).copied().unwrap_or(Tip::Round);
                ui.horizontal(|ui| {
                    ui.label(t("Brush tip"));
                    egui::ComboBox::from_id_salt("tip").selected_text(t(tip.name())).show_ui(ui, |ui| {
                        for k in Tip::ALL {
                            ui.selectable_value(&mut tip, k, t(k.name()));
                        }
                    });
                });
                self.tips.insert(tool, tip);
                let mut sm = self.smoothing * 100.0;
                if ui.add(egui::Slider::new(&mut sm, 0.0..=90.0).suffix("%").text(t("Smoothing"))).changed() {
                    self.smoothing = sm / 100.0;
                }
                ui.checkbox(&mut self.pressure_size, t("Pressure changes size"));
                ui.checkbox(&mut self.pressure_alpha, t("Pressure changes opacity"));
            }
            if tool == Tool::Fill {
                let mut tol = self.tolerance as f32;
                if ui.add(egui::Slider::new(&mut tol, 0.0..=255.0).text(t("Tolerance"))).changed() {
                    self.tolerance = tol as u8;
                }
            }
            if matches!(tool, Tool::Rect | Tool::Ellipse) {
                ui.checkbox(&mut self.shape_fill, t("Filled shape"));
            }
            if tool == Tool::Gradient {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.grad_kind, Gradient::Linear, t("Linear"));
                    ui.radio_value(&mut self.grad_kind, Gradient::Radial, t("Radial"));
                });
                ui.radio_value(&mut self.grad_fade, false, t("Colour to secondary colour"));
                ui.radio_value(&mut self.grad_fade, true, t("Colour to transparent"));
                ui.label(egui::RichText::new(t("Drag on the canvas to lay the gradient.")).small().weak());
            }
            if matches!(tool, Tool::Select | Tool::Move) {
                ui.label(egui::RichText::new(t(if tool == Tool::Select {
                    "Drag to select a rectangle; painting stays inside it. Click to deselect."
                } else {
                    "Drag to move the selection, or the whole layer when nothing is selected."
                }))
                .small()
                .weak());
                if let Some(r) = self.selection {
                    ui.label(format!("{} × {} {}", r.width(), r.height(), t("pixels")));
                }
            }
            if tool == Tool::Text {
                ui.add(egui::Slider::new(&mut self.text_size, 6.0..=600.0).logarithmic(true).text(t("Size")));
                ui.label(egui::RichText::new(t("Click on the canvas where the text should go.")).small().weak());
            }
            ui.separator();
            ui.heading(t("Layers"));
            ui.horizontal_wrapped(|ui| self.layer_actions(ui, false));
            let a = self.doc.active;
            let mut op = self.doc.layers[a].opacity * 100.0;
            if ui.add(egui::Slider::new(&mut op, 0.0..=100.0).suffix("%").text(t("Opacity"))).changed() {
                self.doc.layers[a].opacity = op / 100.0;
                self.dirty = true;
                self.rebuild = true;
            }
            let mut blend = self.doc.layers[a].blend;
            egui::ComboBox::from_id_salt("blend").selected_text(t(blend.name())).show_ui(ui, |ui| {
                for b in Blend::ALL {
                    ui.selectable_value(&mut blend, b, t(b.name()));
                }
            });
            if blend != self.doc.layers[a].blend {
                self.doc.layers[a].blend = blend;
                self.dirty = true;
                self.rebuild = true;
            }
            let mut pick = None;
            let mut toggle = None;
            let mut start_rename = None;
            for i in (0..self.doc.layers.len()).rev() {
                let (visible, lname) = (self.doc.layers[i].visible, self.doc.layers[i].name.clone());
                ui.horizontal(|ui| {
                    let mut v = visible;
                    if ui.checkbox(&mut v, "").on_hover_text(t("Visible")).changed() {
                        toggle = Some(i);
                    }
                    if let Some((ri, text)) = self.rename.as_mut().filter(|(ri, _)| *ri == i) {
                        let r = ui.text_edit_singleline(text);
                        if r.lost_focus() {
                            let (ri, text) = (*ri, text.clone());
                            self.rename = None;
                            if !text.trim().is_empty() {
                                self.doc.layers[ri].name = text;
                                self.dirty = true;
                            }
                        } else {
                            r.request_focus();
                        }
                    } else {
                        let r = ui.selectable_label(i == a, lname.as_str());
                        if r.clicked() {
                            pick = Some(i);
                        }
                        if r.double_clicked() {
                            start_rename = Some(i);
                        }
                        r.on_hover_text(t("Rename"));
                    }
                });
            }
            if let Some(i) = toggle {
                self.doc.layers[i].visible = !self.doc.layers[i].visible;
                self.dirty = true;
                self.rebuild = true;
            }
            if let Some(i) = pick {
                self.drop_moving();
                self.cancel_stroke();
                self.doc.active = i;
            }
            if let Some(i) = start_rename {
                self.rename = Some((i, self.doc.layers[i].name.clone()));
            }
        });
    }

    fn status(&mut self, ui: &mut Ui, hover: Option<(f32, f32)>) {
        ui.horizontal(|ui| {
            ui.label(format!("{} × {} {}", self.doc.w, self.doc.h, t("pixels")));
            ui.separator();
            ui.label(format!("{:.0}%", self.zoom * 100.0));
            if let Some((x, y)) = hover {
                ui.separator();
                ui.label(format!("{:.0}, {:.0}", x, y));
            }
            ui.separator();
            ui.label(format!("{}: {}", t("Layer"), self.doc.layers[self.doc.active].name));
        });
    }

    // ---- canvas ----

    fn canvas(&mut self, ui: &mut Ui) -> Option<(f32, f32)> {
        let ctx = ui.ctx().clone();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, Color32::from_gray(48));
        let size = vec2(self.doc.w as f32, self.doc.h as f32);
        if self.fit && rect.width() > 10.0 {
            self.zoom = ((rect.width() - 40.0) / size.x).min((rect.height() - 40.0) / size.y).clamp(0.05, 4.0);
            self.pan = Vec2::ZERO;
            self.fit = false;
        }
        // zoom and pan: wheel scrolls, Ctrl+wheel or pinch zooms around the pointer
        let hover_screen = resp.hover_pos();
        let (scroll, zoom_d, multi) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.multi_touch()));
        if resp.hovered() || multi.is_some() {
            let anchor = hover_screen.or(multi.as_ref().map(|m| m.center_pos)).unwrap_or(rect.center());
            if (zoom_d - 1.0).abs() > 1e-4 {
                let before = self.to_canvas(rect, anchor);
                self.zoom = (self.zoom * zoom_d).clamp(0.05, 32.0);
                let after = self.to_screen(rect, before);
                self.pan += anchor - after;
            } else if multi.is_none() {
                self.pan += scroll;
            }
            if let Some(m) = &multi {
                self.pan += m.translation_delta;
            }
        }
        let origin = self.origin(rect);
        let img_rect = Rect::from_min_size(origin, size * self.zoom);
        // transparency checkerboard, then the layers
        let checker = self.checker(&ctx);
        let cells = size / 16.0;
        painter.image(checker, img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(cells.x / 2.0, cells.y / 2.0)), Color32::WHITE);
        if let Some(tex) = &self.comp {
            painter.image(tex.id(), img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        painter.rect_stroke(img_rect, 0, UiStroke::new(1.0, Color32::from_black_alpha(120)), egui::StrokeKind::Outside);

        // pressure from a pen or touch, when the device reports it
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Touch { force: Some(f), .. } = e {
                    self.pressure = f.clamp(0.05, 1.0);
                }
            }
        });

        let pointer = resp.interact_pointer_pos().or(hover_screen);
        let hover = hover_screen.map(|p| self.to_canvas(rect, p));
        let space = ctx.input(|i| i.key_down(Key::Space)) && !ctx.egui_wants_keyboard_input();
        let middle = ctx.input(|i| i.pointer.middle_down());
        let alt = ctx.input(|i| i.modifiers.alt);
        let shift = ctx.input(|i| i.modifiers.shift);
        let panning = self.tool == Tool::Hand || space || middle;

        if multi.is_some() {
            // two fingers move the view; a stroke begun by the first finger is taken back
            if self.stroke.is_some() {
                self.cancel_stroke();
            }
        } else if panning {
            if resp.dragged() {
                self.pan += resp.drag_delta();
            }
        } else if let Some(p) = pointer {
            let c = self.to_canvas(rect, p);
            let tool = self.tool;
            if (tool == Tool::Eyedropper || (alt && tool.paints())) && (resp.clicked() || resp.dragged()) {
                if c.0 >= 0.0 && c.1 >= 0.0 {
                    let s = self.doc.sample(c.0 as u32, c.1 as u32);
                    if s[3] > 0 {
                        self.color = Color32::from_rgb(s[0], s[1], s[2]);
                    }
                }
            } else if tool == Tool::Select {
                if resp.drag_started() {
                    let o = ctx.input(|i| i.pointer.press_origin()).unwrap_or(p);
                    self.sel_start = Some(self.to_canvas(rect, o));
                }
                if let Some(a) = self.sel_start {
                    let b = if shift { constrain(Shape::Rect, a, c) } else { c };
                    self.selection = Some(IRect::between(a, b, self.doc.w, self.doc.h)).filter(|r| !r.is_empty());
                    if resp.drag_stopped() || !ctx.input(|i| i.pointer.any_down()) {
                        self.sel_start = None;
                    }
                } else if resp.clicked() {
                    self.selection = None;
                }
            } else if tool == Tool::Move {
                if resp.drag_started() && self.moving.is_none() && self.doc.layers[self.doc.active].visible {
                    let o = ctx.input(|i| i.pointer.press_origin()).unwrap_or(p);
                    self.move_start = Some(self.to_canvas(rect, o));
                    self.move_off = (0, 0);
                    let src = self.sel_or_all();
                    let layer = self.doc.active;
                    self.moving = Some(Moving::begin(&mut self.doc, layer, src));
                    self.mark(src);
                }
                if let Some(st) = self.move_start {
                    let off = ((c.0 - st.0).round() as i64, (c.1 - st.1).round() as i64);
                    if off != self.move_off {
                        self.move_off = off;
                        let ch = self.moving.as_mut().map(|m| m.place(&mut self.doc, off.0, off.1));
                        if let Some(ch) = ch {
                            self.mark(ch);
                        }
                    }
                    if resp.drag_stopped() || !ctx.input(|i| i.pointer.any_down()) {
                        self.drop_moving();
                    }
                }
            } else if tool == Tool::Gradient {
                if resp.drag_started() {
                    let o = ctx.input(|i| i.pointer.press_origin()).unwrap_or(p);
                    self.grad_start = Some(self.to_canvas(rect, o));
                }
                if let Some(a) = self.grad_start {
                    let b = if shift { constrain(Shape::Line, a, c) } else { c };
                    self.grad_end = Some(b);
                    if resp.drag_stopped() || !ctx.input(|i| i.pointer.any_down()) {
                        let (c0, c1) = (self.color, self.secondary);
                        let start = [c0.r(), c0.g(), c0.b(), 255];
                        let end = if self.grad_fade { [c0.r(), c0.g(), c0.b(), 0] } else { [c1.r(), c1.g(), c1.b(), 255] };
                        let op = self.settings[&Tool::Gradient].opacity;
                        let layer = self.doc.active;
                        let u = raster::gradient(&mut self.doc, layer, self.grad_kind, a, b, start, end, op, self.selection);
                        self.pixels(u);
                        self.remember_colour();
                        self.grad_start = None;
                        self.grad_end = None;
                    }
                }
            } else if tool == Tool::Text {
                if resp.clicked() || resp.drag_started() || resp.dragged() {
                    let at = if resp.drag_started() { self.to_canvas(rect, ctx.input(|i| i.pointer.press_origin()).unwrap_or(p)) } else { c };
                    if let Some(d) = self.text_dlg.as_mut() {
                        d.pos = at;
                    } else {
                        self.text_dlg = Some(TextDlg {
                            pos: at,
                            text: String::new(),
                            size: self.text_size,
                            font: self.text_font.min(self.fonts.len().saturating_sub(1)),
                            align: text::Align::Left,
                            key: String::new(),
                            mask: None,
                            tex: None,
                        });
                    }
                }
            } else if tool == Tool::Fill {
                if resp.clicked() && c.0 >= 0.0 && c.1 >= 0.0 {
                    let col = self.color;
                    let op = self.settings[&Tool::Fill].opacity;
                    let layer = self.doc.active;
                    let u = raster::flood_fill(&mut self.doc, layer, c.0 as u32, c.1 as u32, [col.r(), col.g(), col.b()], op, self.tolerance, self.selection);
                    self.pixels(u);
                    self.remember_colour();
                }
            } else if tool.paints() {
                if resp.drag_started() || (resp.is_pointer_button_down_on() && self.stroke.is_none() && !resp.dragged()) {
                    if self.stroke.is_none() && self.doc.layers[self.doc.active].visible {
                        let mut st = Stroke::begin(&self.doc, self.doc.active, self.brush());
                        st.clip = self.selection;
                        self.stroke = Some(st);
                        self.smooth = Some(ctx.input(|i| i.pointer.press_origin()).unwrap_or(p));
                        self.remember_colour();
                    }
                }
                if let Some(mut s) = self.stroke.take() {
                    // smoothing: the brush trails the pointer like a lazy string
                    let sp = self.smooth.unwrap_or(p);
                    let k = 1.0 - self.smoothing.clamp(0.0, 0.95);
                    let np = sp + (p - sp) * k;
                    self.smooth = Some(np);
                    let c = self.to_canvas(rect, np);
                    let ps = if self.pressure_size { self.pressure } else { 1.0 };
                    let pa = if self.pressure_alpha { self.pressure } else { 1.0 };
                    let changed = s.to(&mut self.doc, c.0, c.1, ps, pa);
                    if !changed.is_empty() {
                        self.mark(changed);
                    }
                    if resp.drag_stopped() || resp.clicked() || !ctx.input(|i| i.pointer.any_down()) {
                        // finish the line at the pointer itself
                        let end = self.to_canvas(rect, p);
                        let changed = s.to(&mut self.doc, end.0, end.1, ps, pa);
                        if !changed.is_empty() {
                            self.mark(changed);
                        }
                        let u = s.end(&self.doc);
                        self.pixels(u);
                        self.smooth = None;
                    } else {
                        self.stroke = Some(s);
                    }
                }
            } else if let Some(kind) = tool.shape() {
                let shift = ctx.input(|i| i.modifiers.shift);
                if resp.drag_started() {
                    // from where the button went down, not where the drag was noticed
                    let o = ctx.input(|i| i.pointer.press_origin()).unwrap_or(p);
                    self.shape_start = Some(self.to_canvas(rect, o));
                }
                if let Some(a) = self.shape_start {
                    let mut b = c;
                    if shift {
                        b = constrain(kind, a, b);
                    }
                    self.shape_end = Some(b);
                    if resp.drag_stopped() {
                        let mut brush = self.brush();
                        brush.mode = Mode::Paint;
                        let layer = self.doc.active;
                        if self.shape_fill && kind != Shape::Line {
                            let col = self.color;
                            let u = raster::fill_shape(&mut self.doc, layer, kind, a, b, [col.r(), col.g(), col.b()], brush.opacity, self.selection);
                            self.pixels(u);
                        } else {
                            let u = raster::stroke_path(&mut self.doc, layer, brush, &raster::shape_points(kind, a, b), self.selection);
                            self.pixels(u);
                        }
                        self.remember_colour();
                        self.shape_start = None;
                        self.shape_end = None;
                    }
                }
            }
        }

        // the shape being drawn
        if let (Some(a), Some(b), Some(kind)) = (self.shape_start, self.shape_end, self.tool.shape()) {
            let pts: Vec<Pos2> = raster::shape_points(kind, a, b).into_iter().map(|p| self.to_screen(rect, p)).collect();
            let w = (self.settings[&self.tool].size * self.zoom).max(1.0);
            if self.shape_fill && kind != Shape::Line {
                let (sa, sb) = (self.to_screen(rect, a), self.to_screen(rect, b));
                if kind == Shape::Rect {
                    painter.rect_filled(Rect::from_two_pos(sa, sb), 0, self.color);
                } else {
                    painter.add(egui::Shape::convex_polygon(pts, self.color, UiStroke::NONE));
                }
            } else {
                painter.add(egui::Shape::line(pts, UiStroke::new(w, self.color)));
            }
        }
        // the gradient being laid
        if let (Some(a), Some(b)) = (self.grad_start, self.grad_end) {
            let (sa, sb) = (self.to_screen(rect, a), self.to_screen(rect, b));
            painter.line_segment([sa, sb], UiStroke::new(3.0, Color32::from_black_alpha(140)));
            painter.line_segment([sa, sb], UiStroke::new(1.5, Color32::WHITE));
            painter.circle(sa, 5.0, self.color, UiStroke::new(1.5, Color32::WHITE));
            let end = if self.grad_fade { Color32::TRANSPARENT } else { self.secondary };
            painter.circle(sb, 5.0, end, UiStroke::new(1.5, Color32::WHITE));
        }
        // the text being placed
        self.text_preview(&ctx);
        if let Some(d) = &self.text_dlg {
            if let (Some(m), Some(tex)) = (&d.mask, &d.tex) {
                let r = Rect::from_min_max(self.to_screen(rect, (m.x as f32, m.y as f32)), self.to_screen(rect, ((m.x + m.w as i64) as f32, (m.y + m.h as i64) as f32)));
                painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            let p = self.to_screen(rect, d.pos);
            let h = d.size * self.zoom;
            painter.line_segment([p, p + vec2(0.0, h)], UiStroke::new(1.5, self.color));
            painter.rect_stroke(Rect::from_min_size(p - vec2(3.0, 3.0), vec2(6.0, 6.0)), 0, UiStroke::new(1.0, Color32::WHITE), egui::StrokeKind::Middle);
        }
        // the selection, and where the moved pixels are
        let sel = self.moving.as_ref().filter(|_| self.selection.is_some()).map(|m| m.dst).or(self.selection);
        if let Some(r) = sel.filter(|r| !r.is_empty()) {
            let sr = Rect::from_min_max(self.to_screen(rect, (r.x0 as f32, r.y0 as f32)), self.to_screen(rect, (r.x1 as f32, r.y1 as f32)));
            let pts = [sr.left_top(), sr.right_top(), sr.right_bottom(), sr.left_bottom(), sr.left_top()];
            painter.add(egui::Shape::line(pts.to_vec(), UiStroke::new(1.0, Color32::WHITE)));
            // marching ants
            let phase = (ctx.input(|i| i.time) * 8.0) as f32 % 8.0;
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                let len = (b - a).length();
                let dir = (b - a) / len.max(1e-3);
                let mut d = -phase;
                while d < len {
                    let s0 = d.max(0.0);
                    let s1 = (d + 4.0).min(len);
                    if s1 > s0 {
                        painter.line_segment([a + dir * s0, a + dir * s1], UiStroke::new(1.0, Color32::BLACK));
                    }
                    d += 8.0;
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        // the brush outline
        if let Some(hp) = hover_screen {
            if self.tool == Tool::Move && !panning {
                ctx.set_cursor_icon(egui::CursorIcon::Move);
            } else if self.tool == Tool::Text && !panning {
                ctx.set_cursor_icon(egui::CursorIcon::Text);
            } else if self.tool.has_size() && !panning {
                let r = (self.settings[&self.tool].size * self.zoom / 2.0).max(1.5);
                painter.circle_stroke(hp, r, UiStroke::new(1.0, Color32::from_black_alpha(160)));
                painter.circle_stroke(hp, r + 1.0, UiStroke::new(1.0, Color32::from_white_alpha(160)));
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            } else if panning {
                ctx.set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            } else {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        if self.stroke.is_some() || self.moving.is_some() {
            ctx.request_repaint();
        }
        hover
    }

    /// Lay out the text being placed again when it, its font, size or colour changed.
    fn text_preview(&mut self, ctx: &egui::Context) {
        let color = self.color;
        let Some(d) = self.text_dlg.as_mut() else { return };
        let Some((_, font)) = self.fonts.get(d.font) else { return };
        let key = format!("{}|{}|{}|{:?}|{:?}|{:?}", d.text, d.size, d.font, d.align, d.pos, color);
        if key == d.key {
            return;
        }
        d.key = key;
        d.mask = text::layout(font, &d.text, d.size, d.pos, d.align, 1.0);
        d.tex = d.mask.as_ref().map(|m| {
            let px: Vec<Color32> = m.a.iter().map(|&a| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)).collect();
            let img = egui::ColorImage::new([m.w as usize, m.h as usize], px);
            ctx.load_texture("text-preview", img, TextureOptions::LINEAR)
        });
    }

    fn origin(&self, rect: Rect) -> Pos2 {
        let size = vec2(self.doc.w as f32, self.doc.h as f32) * self.zoom;
        rect.center() + self.pan - size / 2.0
    }

    fn to_canvas(&self, rect: Rect, p: Pos2) -> (f32, f32) {
        let o = self.origin(rect);
        ((p.x - o.x) / self.zoom, (p.y - o.y) / self.zoom)
    }

    fn to_screen(&self, rect: Rect, c: (f32, f32)) -> Pos2 {
        self.origin(rect) + vec2(c.0, c.1) * self.zoom
    }

    // ---- dialogs ----

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut d) = self.new_dlg.take() {
            let mut keep = true;
            let mut create = false;
            egui::Modal::new(egui::Id::unique("new-drawing")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading(t("New Drawing"));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, w, h) in [("HD 1920×1080", 1920, 1080), ("1600×1200", 1600, 1200), ("1080×1080", 1080, 1080), ("A4 150 dpi", 1240, 1754), ("A4 300 dpi", 2480, 3508), ("4K 3840×2160", 3840, 2160)] {
                        if ui.small_button(label).clicked() {
                            d.w = w;
                            d.h = h;
                        }
                    }
                });
                egui::Grid::new("new-grid").num_columns(2).show(ui, |ui| {
                    ui.label(t("Width"));
                    ui.add(egui::DragValue::new(&mut d.w).range(1..=self.max_side).suffix(" px"));
                    ui.end_row();
                    ui.label(t("Height"));
                    ui.add(egui::DragValue::new(&mut d.h).range(1..=self.max_side).suffix(" px"));
                    ui.end_row();
                });
                ui.radio_value(&mut d.white, true, t("White background"));
                ui.radio_value(&mut d.white, false, t("Transparent background"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("Create")).clicked() {
                        create = true;
                    }
                    if ui.button(t("Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
            if create {
                let bg = if d.white { Some([255, 255, 255, 255]) } else { None };
                let doc = Doc::new(d.w.min(self.max_side), d.h.min(self.max_side), bg, t("Background"), &format!("{} 1", t("Layer")));
                self.reset_doc(doc, t("Untitled").to_string(), None);
            } else if keep && !ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.new_dlg = Some(d);
            }
        }
        if let Some(mut d) = self.size_dlg.take() {
            let mut keep = true;
            let mut apply = false;
            egui::Modal::new(egui::Id::unique("canvas-size")).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.heading(t("Canvas Size…").trim_end_matches('…'));
                egui::Grid::new("size-grid").num_columns(2).show(ui, |ui| {
                    ui.label(t("Width"));
                    ui.add(egui::DragValue::new(&mut d.w).range(1..=self.max_side).suffix(" px"));
                    ui.end_row();
                    ui.label(t("Height"));
                    ui.add(egui::DragValue::new(&mut d.h).range(1..=self.max_side).suffix(" px"));
                    ui.end_row();
                });
                ui.label(t("Anchor"));
                egui::Grid::new("anchor").show(ui, |ui| {
                    for y in 0..3 {
                        for x in 0..3 {
                            let a = (x as f32 / 2.0, y as f32 / 2.0);
                            if ui.selectable_label(d.anchor == a, if d.anchor == a { "[X]" } else { "[ ]" }).clicked() {
                                d.anchor = a;
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.button(t("Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
            if apply {
                let (w, h, an) = (d.w, d.h, d.anchor);
                self.doc_op(|doc| raster::resize_canvas(doc, w, h, an));
                self.fit = true;
            } else if keep && !ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.size_dlg = Some(d);
            }
        }
        if let Some(mut d) = self.text_dlg.take() {
            let mut keep = true;
            let mut place = false;
            let mut load_font = false;
            let mut open = true;
            egui::Window::new(t("Text")).collapsible(false).resizable(false).open(&mut open).default_pos(ctx.content_rect().right_top() + vec2(-320.0, 80.0)).show(ctx, |ui| {
                ui.set_width(280.0);
                let r = ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(3).desired_width(f32::INFINITY).hint_text(t("Type the text here")));
                if d.text.is_empty() && !r.has_focus() {
                    r.request_focus();
                }
                ui.horizontal(|ui| {
                    ui.label(t("Font"));
                    egui::ComboBox::from_id_salt("font").selected_text(self.fonts.get(d.font).map_or("", |f| f.0.as_str())).width(150.0).show_ui(ui, |ui| {
                        for (i, (name, _)) in self.fonts.iter().enumerate() {
                            ui.selectable_value(&mut d.font, i, name.as_str());
                        }
                    });
                    if ui.small_button(t("Load Font…")).clicked() {
                        load_font = true;
                    }
                });
                ui.add(egui::Slider::new(&mut d.size, 6.0..=600.0).logarithmic(true).suffix(" px").text(t("Size")));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.align, text::Align::Left, t("Left"));
                    ui.radio_value(&mut d.align, text::Align::Center, t("Centre"));
                    ui.radio_value(&mut d.align, text::Align::Right, t("Right"));
                });
                ui.label(egui::RichText::new(t("Click or drag on the canvas to move it. The colour is the current colour.")).small().weak());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!d.text.trim().is_empty(), egui::Button::new(t("Place"))).clicked() {
                        place = true;
                    }
                    if ui.button(t("Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
            if load_font {
                self.pick_font();
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Enter)) && !d.text.trim().is_empty() {
                place = true;
            }
            self.text_size = d.size;
            self.text_font = d.font;
            if place {
                let col = self.color;
                let op = 1.0;
                let layer = self.doc.active;
                let mask = self.fonts.get(d.font).and_then(|(_, f)| text::layout(f, &d.text, d.size, d.pos, d.align, 1.0));
                if let Some(m) = mask {
                    let u = text::paint(&mut self.doc, layer, &m, [col.r(), col.g(), col.b()], op, None);
                    self.pixels(u);
                    self.remember_colour();
                }
            } else if keep && open && !ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.text_dlg = Some(d);
            }
        }
        if self.about {
            let mut open = true;
            egui::Window::new(t("About CraftOffice Sketch")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
                ui.heading("CraftOffice Sketch");
                ui.label(format!("{} {}", t("Version"), env!("CARGO_PKG_VERSION")));
                ui.label(t("Drawing and sketching with layers, brushes and shapes."));
                ui.add_space(6.0);
                ui.label(t("CraftOffice edition by AndyBcx: Hungarian and Romanian interface."));
                ui.hyperlink_to("CraftOffice: github.com/AndyBcx/CraftOffice", "https://github.com/AndyBcx/CraftOffice");
                ui.add_space(4.0);
                ui.label(egui::RichText::new("MIT OR Apache-2.0 · Rust · egui").small().weak());
            });
            self.about = open;
        }
        if self.shortcuts {
            let mut open = true;
            egui::Window::new(t("Keyboard Shortcuts")).collapsible(false).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("keys").striped(true).show(ui, |ui| {
                    for (tl, label, key) in TOOLS {
                        let _ = tl;
                        ui.label(t(label));
                        ui.label(key);
                        ui.end_row();
                    }
                    for (label, key) in [
                        ("Brush size", "[  ]"),
                        ("Select All", "Ctrl+A"),
                        ("Deselect", "Ctrl+D"),
                        ("Copy", "Ctrl+C"),
                        ("Cut", "Ctrl+X"),
                        ("Paste as New Layer", "Ctrl+V"),
                        ("Delete Selection", "Del"),
                        ("Swap colours", "X"),
                        ("Eyedropper", "Alt"),
                        ("Pan", "Space, middle button"),
                        ("Zoom", "Ctrl + wheel, + -, pinch"),
                        ("Fit to Window", "Ctrl+0"),
                        ("Undo", "Ctrl+Z"),
                        ("Redo", "Ctrl+Y, Ctrl+Shift+Z"),
                        ("Save", "Ctrl+S"),
                        ("Open…", "Ctrl+O"),
                        ("New…", "Ctrl+N"),
                        ("Export PNG…", "Ctrl+E"),
                    ] {
                        ui.label(t(label));
                        ui.label(key);
                        ui.end_row();
                    }
                });
            });
            self.shortcuts = open;
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::unique("confirm-close")).show(ctx, |ui| {
                ui.heading(t("Unsaved changes"));
                ui.label(t("Save the drawing before closing?"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("Save")).clicked() {
                        self.confirm_close = false;
                        self.allow_close = true;
                        self.save(ctx, SaveKind::Project, false);
                    }
                    if ui.button(t("Don't Save")).clicked() {
                        self.confirm_close = false;
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button(t("Cancel")).clicked() {
                        self.confirm_close = false;
                    }
                });
            });
        }
        if let Some((text, until)) = &self.toast {
            if ctx.input(|i| i.time) > *until {
                self.toast = None;
            } else {
                let text = text.clone();
                let r = ctx.content_rect();
                egui::Area::new(egui::Id::unique("toast")).fixed_pos(pos2(r.center().x, r.bottom() - 60.0)).pivot(Align2::CENTER_BOTTOM).show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(text);
                            if ui.small_button("x").clicked() {
                                self.toast = None;
                            }
                        });
                    });
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn mime(kind: SaveKind) -> &'static str {
    match kind {
        SaveKind::Project => "application/octet-stream",
        SaveKind::Png => "image/png",
        SaveKind::Jpeg => "image/jpeg",
    }
}

/// Shift: a square, a circle, or a line at a multiple of 45°.
fn constrain(kind: Shape, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    match kind {
        Shape::Line => {
            let ang = dy.atan2(dx);
            let snap = (ang / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
            let len = (dx * dx + dy * dy).sqrt();
            (a.0 + len * snap.cos(), a.1 + len * snap.sin())
        }
        _ => {
            let m = dx.abs().max(dy.abs());
            (a.0 + m * dx.signum(), a.1 + m * dy.signum())
        }
    }
}

fn swatch(ui: &mut Ui, col: Color32, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(3), col);
    let st = if selected { UiStroke::new(2.0, ui.visuals().selection.stroke.color) } else { UiStroke::new(1.0, Color32::from_gray(90)) };
    ui.painter().rect_stroke(rect, CornerRadius::same(3), st, egui::StrokeKind::Inside);
    r
}

/// The tool icons, drawn as simple lines.
fn tool_icon(p: &egui::Painter, r: Rect, tool: Tool, c: Color32) {
    let s = UiStroke::new(1.6, c);
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let line = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), s));
    };
    match tool {
        Tool::Select => {
            for (a, b) in [((0.1, 0.15), (0.9, 0.15)), ((0.9, 0.15), (0.9, 0.85)), ((0.9, 0.85), (0.1, 0.85)), ((0.1, 0.85), (0.1, 0.15))] {
                // dashed
                for k in 0..3 {
                    let t0 = k as f32 / 3.0;
                    let t1 = t0 + 0.2;
                    line(&[(a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0), (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1)]);
                }
            }
        }
        Tool::Move => {
            line(&[(0.5, 0.05), (0.5, 0.95)]);
            line(&[(0.05, 0.5), (0.95, 0.5)]);
            line(&[(0.38, 0.17), (0.5, 0.05), (0.62, 0.17)]);
            line(&[(0.38, 0.83), (0.5, 0.95), (0.62, 0.83)]);
            line(&[(0.17, 0.38), (0.05, 0.5), (0.17, 0.62)]);
            line(&[(0.83, 0.38), (0.95, 0.5), (0.83, 0.62)]);
        }
        Tool::Gradient => {
            for k in 0..5 {
                let x = 0.1 + k as f32 * 0.16;
                let g = (60 + k * 45) as u8;
                p.rect_filled(Rect::from_min_max(at(x, 0.2), at(x + 0.16, 0.8)), 0, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), g));
            }
            p.rect_stroke(Rect::from_min_max(at(0.1, 0.2), at(0.9, 0.8)), 1, s, egui::StrokeKind::Middle);
        }
        Tool::Text => {
            line(&[(0.15, 0.2), (0.85, 0.2)]);
            line(&[(0.5, 0.2), (0.5, 0.88)]);
            line(&[(0.35, 0.88), (0.65, 0.88)]);
        }
        Tool::Pencil => {
            line(&[(0.15, 0.85), (0.25, 0.6), (0.75, 0.1), (0.9, 0.25), (0.4, 0.75), (0.15, 0.85)]);
            line(&[(0.25, 0.6), (0.4, 0.75)]);
        }
        Tool::Pen => {
            line(&[(0.5, 0.05), (0.8, 0.45), (0.5, 0.95), (0.2, 0.45), (0.5, 0.05)]);
            line(&[(0.5, 0.45), (0.5, 0.95)]);
            p.circle_filled(at(0.5, 0.45), 2.0, c);
        }
        Tool::Brush => {
            line(&[(0.9, 0.1), (0.5, 0.5)]);
            p.add(egui::Shape::convex_polygon(vec![at(0.5, 0.42), at(0.58, 0.5), at(0.3, 0.9), at(0.1, 0.9), at(0.12, 0.7)], c, UiStroke::NONE));
        }
        Tool::Marker => {
            line(&[(0.3, 0.1), (0.7, 0.1), (0.7, 0.6), (0.6, 0.85), (0.4, 0.85), (0.3, 0.6), (0.3, 0.1)]);
            line(&[(0.3, 0.45), (0.7, 0.45)]);
        }
        Tool::Airbrush => {
            line(&[(0.55, 0.35), (0.9, 0.35), (0.9, 0.65), (0.55, 0.65), (0.55, 0.35)]);
            for (x, y) in [(0.15, 0.3), (0.3, 0.5), (0.12, 0.62), (0.32, 0.25), (0.2, 0.45), (0.35, 0.72), (0.08, 0.45)] {
                p.circle_filled(at(x, y), 1.4, c);
            }
        }
        Tool::Eraser => {
            line(&[(0.1, 0.65), (0.5, 0.2), (0.9, 0.55), (0.55, 0.9), (0.3, 0.9), (0.1, 0.65)]);
            line(&[(0.3, 0.45), (0.7, 0.8)]);
        }
        Tool::Fill => {
            line(&[(0.15, 0.45), (0.5, 0.1), (0.85, 0.45), (0.5, 0.8), (0.15, 0.45)]);
            p.add(egui::Shape::convex_polygon(vec![at(0.85, 0.55), at(0.95, 0.8), at(0.85, 0.9), at(0.75, 0.8)], c, UiStroke::NONE));
        }
        Tool::Eyedropper => {
            line(&[(0.15, 0.85), (0.6, 0.4)]);
            line(&[(0.5, 0.3), (0.7, 0.5)]);
            p.circle_filled(at(0.75, 0.25), r.width() * 0.16, c);
        }
        Tool::Line => line(&[(0.1, 0.9), (0.9, 0.1)]),
        Tool::Rect => {
            p.rect_stroke(Rect::from_min_max(at(0.1, 0.2), at(0.9, 0.8)), 1, s, egui::StrokeKind::Middle);
        }
        Tool::Ellipse => {
            p.add(egui::Shape::ellipse_stroke(r.center(), vec2(r.width() * 0.45, r.height() * 0.32), s));
        }
        Tool::Hand => {
            line(&[(0.3, 0.9), (0.15, 0.55), (0.25, 0.5), (0.35, 0.65), (0.35, 0.15), (0.45, 0.15), (0.45, 0.5)]);
            line(&[(0.45, 0.1), (0.55, 0.1), (0.55, 0.5)]);
            line(&[(0.55, 0.15), (0.65, 0.15), (0.65, 0.5)]);
            line(&[(0.65, 0.25), (0.75, 0.25), (0.75, 0.7), (0.6, 0.9), (0.3, 0.9)]);
        }
    }
}

impl eframe::App for SketchApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_io(&ctx);
        // closing with unsaved changes asks first
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        // a file dropped on the window opens; a picture dropped with Shift becomes a layer
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let shift = ctx.input(|i| i.modifiers.shift);
            #[cfg(not(target_arch = "wasm32"))]
            if let Ok(bytes) = f.bytes() {
                let p = f.path();
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let path = if p.as_os_str().is_empty() { None } else { Some(p.to_string_lossy().into_owned()) };
                self.open_bytes(name, path, bytes, shift);
            }
            #[cfg(target_arch = "wasm32")]
            {
                // in the browser the bytes arrive asynchronously; they come back through the Io channel
                let tx = self.io_tx.clone();
                let ctx2 = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let name = f.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    match f.bytes_async().await {
                        Ok(bytes) => { let _ = tx.send(Io::Opened { name, path: None, bytes, as_layer: shift }); }
                        Err(e) => { let _ = tx.send(Io::Failed(e)); }
                    }
                    ctx2.request_repaint();
                });
            }
        }
        self.shortcuts_input(&ctx);
        self.clip_commands(&ctx);
        if let Some(r) = self.selection {
            self.selection = Some(r.intersect(self.doc.full())).filter(|r| !r.is_empty());
        }
        self.sync_textures(&ctx);
        let title = format!("{}{} – CraftOffice Sketch", self.name, if self.dirty { " •" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        let mut hover = None;
        egui::Panel::left("tools").exact_size(48.0).resizable(false).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.toolbar(ui));
        });
        let narrow = ctx.content_rect().width() < 720.0;
        if self.show_panels {
            egui::Panel::right("side").default_size(if narrow { 220.0 } else { 260.0 }).min_size(200.0).resizable(true).show(ui, |ui| self.side_panel(ui));
        }
        let last = self.hover;
        egui::Panel::bottom("status").show(ui, |ui| self.status(ui, last));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            hover = self.canvas(ui);
        });
        if hover != self.hover {
            self.hover = hover;
            ctx.request_repaint();
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let r = ctx.content_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
            p.rect_filled(r, 0, Color32::from_black_alpha(140));
            p.text(r.center(), Align2::CENTER_CENTER, t("Drop to open"), FontId::proportional(24.0), Color32::WHITE);
        }
        self.dialogs(&ctx);
    }
}
