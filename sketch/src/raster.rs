//! The drawing itself: layers of straight-alpha RGBA pixels, brush strokes stamped through a
//! per-stroke coverage mask (so a stroke never darkens where it crosses itself, like a real
//! marker), flood fill, shapes, transforms, and undo by changed regions.

/// A rectangle of pixels: x0..x1, y0..y1 (exclusive ends).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl IRect {
    pub fn empty() -> Self {
        IRect { x0: u32::MAX, y0: u32::MAX, x1: 0, y1: 0 }
    }
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
    pub fn union(&mut self, o: IRect) {
        if o.is_empty() {
            return;
        }
        self.x0 = self.x0.min(o.x0);
        self.y0 = self.y0.min(o.y0);
        self.x1 = self.x1.max(o.x1);
        self.y1 = self.y1.max(o.y1);
    }
    pub fn width(&self) -> u32 {
        self.x1.saturating_sub(self.x0)
    }
    pub fn height(&self) -> u32 {
        self.y1.saturating_sub(self.y0)
    }
    pub fn intersect(&self, o: IRect) -> IRect {
        IRect { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
    /// The pixels between two canvas points, kept inside a w × h canvas.
    pub fn between(a: (f32, f32), b: (f32, f32), w: u32, h: u32) -> IRect {
        let c = |v: f32, m: u32| v.round().clamp(0.0, m as f32) as u32;
        IRect { x0: c(a.0.min(b.0), w), y0: c(a.1.min(b.1), h), x1: c(a.0.max(b.0), w), y1: c(a.1.max(b.1), h) }
    }
}

/// How a layer mixes with the layers under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Add,
}

impl Blend {
    pub const ALL: [Blend; 12] = [
        Blend::Normal,
        Blend::Multiply,
        Blend::Screen,
        Blend::Overlay,
        Blend::Darken,
        Blend::Lighten,
        Blend::ColorDodge,
        Blend::ColorBurn,
        Blend::HardLight,
        Blend::SoftLight,
        Blend::Difference,
        Blend::Add,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Multiply => "Multiply",
            Blend::Screen => "Screen",
            Blend::Overlay => "Overlay",
            Blend::Darken => "Darken",
            Blend::Lighten => "Lighten",
            Blend::ColorDodge => "Color Dodge",
            Blend::ColorBurn => "Color Burn",
            Blend::HardLight => "Hard Light",
            Blend::SoftLight => "Soft Light",
            Blend::Difference => "Difference",
            Blend::Add => "Add",
        }
    }

    /// One channel (0..1) of the layer `s` over the backdrop `d`.
    pub fn mix(self, d: f32, s: f32) -> f32 {
        match self {
            Blend::Normal => s,
            Blend::Multiply => s * d,
            Blend::Screen => s + d - s * d,
            Blend::Overlay => Blend::HardLight.mix(s, d),
            Blend::Darken => s.min(d),
            Blend::Lighten => s.max(d),
            Blend::ColorDodge => {
                if d <= 0.0 {
                    0.0
                } else if s >= 1.0 {
                    1.0
                } else {
                    (d / (1.0 - s)).min(1.0)
                }
            }
            Blend::ColorBurn => {
                if d >= 1.0 {
                    1.0
                } else if s <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - d) / s).min(1.0)
                }
            }
            Blend::HardLight => {
                if s <= 0.5 {
                    d * 2.0 * s
                } else {
                    Blend::Screen.mix(d, 2.0 * s - 1.0)
                }
            }
            Blend::SoftLight => {
                if s <= 0.5 {
                    d - (1.0 - 2.0 * s) * d * (1.0 - d)
                } else {
                    let dd = if d <= 0.25 { ((16.0 * d - 12.0) * d + 4.0) * d } else { d.sqrt() };
                    d + (2.0 * s - 1.0) * (dd - d)
                }
            }
            Blend::Difference => (s - d).abs(),
            Blend::Add => (s + d).min(1.0),
        }
    }
}

/// Put one straight-alpha pixel `s` (alpha `sa`, 0..1) over the backdrop `out` (0..1 RGBA).
#[inline]
pub fn blend_into(out: &mut [f32], s: &[u8], sa: f32, mode: Blend) {
    if sa <= 0.0 {
        return;
    }
    let da = out[3];
    let oa = sa + da * (1.0 - sa);
    for c in 0..3 {
        let sc = s[c] as f32 / 255.0;
        let dc = out[c];
        let b = if mode == Blend::Normal || da <= 0.0 { sc } else { mode.mix(dc, sc) };
        out[c] = (sa * (1.0 - da) * sc + sa * da * b + (1.0 - sa) * da * dc) / oa;
    }
    out[3] = oa;
}

#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: Blend,
    /// width × height × RGBA, straight alpha. While the mask is being painted (`on_mask`), the
    /// mask is here instead, so every tool paints on it as on a picture.
    pub px: Vec<u8>,
    /// The layer mask: a grey picture of the same size, white shows the layer, black hides it
    /// (value = brightness × alpha). While `on_mask`, this holds the layer's colours.
    pub mask: Option<Vec<u8>>,
    pub on_mask: bool,
    /// Clipping mask: the layer shows only where the layer under it has paint.
    pub clip: bool,
}

/// How much of a layer a mask pixel lets through (0..1).
#[inline]
pub fn mask_value(m: &[u8]) -> f32 {
    let lum = 0.299 * m[0] as f32 + 0.587 * m[1] as f32 + 0.114 * m[2] as f32;
    lum / 255.0 * (m[3] as f32 / 255.0)
}

impl Layer {
    pub fn new(name: String, w: u32, h: u32, fill: [u8; 4]) -> Self {
        let mut px = vec![0u8; (w * h * 4) as usize];
        if fill != [0, 0, 0, 0] {
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&fill);
            }
        }
        Layer { name, visible: true, opacity: 1.0, blend: Blend::Normal, px, mask: None, on_mask: false, clip: false }
    }

    /// The layer's own colours (whichever buffer holds them).
    pub fn colors(&self) -> &[u8] {
        match (&self.mask, self.on_mask) {
            (Some(m), true) => m,
            _ => &self.px,
        }
    }

    /// The mask, if the layer has one (whichever buffer holds it).
    pub fn mask_px(&self) -> Option<&[u8]> {
        match (&self.mask, self.on_mask) {
            (Some(_), true) => Some(&self.px),
            (Some(m), false) => Some(m),
            (None, _) => None,
        }
    }

    /// Switch between painting the layer and painting its mask.
    pub fn swap_mask(&mut self) {
        if let Some(m) = self.mask.as_mut() {
            std::mem::swap(&mut self.px, m);
            self.on_mask = !self.on_mask;
        }
    }

    /// Back to painting the layer (colours in `px`).
    pub fn leave_mask(&mut self) {
        if self.on_mask {
            self.swap_mask();
        }
    }

    /// A new mask: white (all shown), or from a selection (0..255 per pixel).
    pub fn add_mask(&mut self, sel: Option<&[u8]>) {
        self.leave_mask();
        let n = self.px.len() / 4;
        let mut m = vec![255u8; n * 4];
        if let Some(sel) = sel {
            for (p, &v) in m.chunks_exact_mut(4).zip(sel) {
                p[0] = v;
                p[1] = v;
                p[2] = v;
            }
        }
        self.mask = Some(m);
    }

    /// The colours with the mask applied to their alpha.
    pub fn baked(&self) -> Vec<u8> {
        let mut out = self.colors().to_vec();
        if let Some(m) = self.mask_px() {
            for (p, mp) in out.chunks_exact_mut(4).zip(m.chunks_exact(4)) {
                p[3] = (p[3] as f32 * mask_value(mp) + 0.5) as u8;
            }
        }
        out
    }

    /// Put the mask into the layer's alpha and drop it.
    pub fn apply_mask(&mut self) {
        if self.mask.is_some() {
            self.px = self.baked();
            self.mask = None;
            self.on_mask = false;
        }
    }

    /// Drop the mask without using it.
    pub fn delete_mask(&mut self) {
        self.leave_mask();
        self.mask = None;
    }

    /// The pixel buffers (colours and mask) for whole-picture changes such as rotating.
    pub fn buffers_mut(&mut self) -> impl Iterator<Item = &mut Vec<u8>> {
        std::iter::once(&mut self.px).chain(self.mask.as_mut())
    }
}

#[derive(Clone)]
pub struct Doc {
    pub w: u32,
    pub h: u32,
    pub layers: Vec<Layer>,
    pub active: usize,
}

impl Doc {
    pub fn new(w: u32, h: u32, background: Option<[u8; 4]>, bg_name: &str, layer_name: &str) -> Self {
        let mut layers = Vec::new();
        if let Some(c) = background {
            layers.push(Layer::new(bg_name.to_string(), w, h, c));
        }
        layers.push(Layer::new(layer_name.to_string(), w, h, [0, 0, 0, 0]));
        let active = layers.len() - 1;
        Doc { w, h, layers, active }
    }

    pub fn full(&self) -> IRect {
        IRect { x0: 0, y0: 0, x1: self.w, y1: self.h }
    }

    /// All visible layers composited (straight alpha), optionally over an opaque colour.
    pub fn flatten(&self, under: Option<[u8; 3]>) -> Vec<u8> {
        self.composite(self.full(), under)
    }

    /// The visible layers composited with their opacity and blend mode, in a region.
    pub fn composite(&self, r: IRect, under: Option<[u8; 3]>) -> Vec<u8> {
        self.composite_with(r, under, None)
    }

    /// [`Doc::composite`], with each layer's opacity times `mult` (the flipbook's frames and
    /// onion skin) in place of the layers' visibility.
    pub fn composite_with(&self, r: IRect, under: Option<[u8; 3]>, mult: Option<&[f32]>) -> Vec<u8> {
        let r = r.intersect(self.full());
        let mut out = Vec::with_capacity((r.width() * r.height() * 4) as usize);
        let layers: Vec<(&Layer, f32)> = match mult {
            None => self.layers.iter().filter(|l| l.visible && l.opacity > 0.0).map(|l| (l, 1.0)).collect(),
            Some(m) => self.layers.iter().zip(m).filter(|(l, &k)| k > 0.0 && l.opacity > 0.0).map(|(l, &k)| (l, k)).collect(),
        };
        let base = match under {
            Some(c) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0],
            None => [0.0; 4],
        };
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let i = ((y * self.w + x) * 4) as usize;
                let mut acc = base;
                // the coverage of the last layer that is not clipped, for the clipped ones above it
                let mut under = 1.0;
                for &(l, k) in &layers {
                    let p = &l.colors()[i..i + 4];
                    let m = l.mask_px().map_or(1.0, |m| mask_value(&m[i..i + 4]));
                    let a = p[3] as f32 / 255.0 * l.opacity * k * m;
                    if l.clip {
                        blend_into(&mut acc, p, a * under, l.blend);
                    } else {
                        under = p[3] as f32 / 255.0 * m;
                        blend_into(&mut acc, p, a, l.blend);
                    }
                }
                out.extend(acc.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
            }
        }
        out
    }

    /// The colour at a point of the composited picture.
    pub fn sample(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.w || y >= self.h {
            return [0, 0, 0, 0];
        }
        let v = self.composite(IRect { x0: x, y0: y, x1: x + 1, y1: y + 1 }, None);
        [v[0], v[1], v[2], v[3]]
    }
}

/// Copy a region of a layer's pixels.
pub fn crop(px: &[u8], w: u32, r: IRect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.width() * r.height() * 4) as usize);
    for y in r.y0..r.y1 {
        let a = ((y * w + r.x0) * 4) as usize;
        let b = ((y * w + r.x1) * 4) as usize;
        out.extend_from_slice(&px[a..b]);
    }
    out
}

/// Put a region back.
pub fn paste(px: &mut [u8], w: u32, r: IRect, data: &[u8]) {
    let rw = (r.width() * 4) as usize;
    for (k, y) in (r.y0..r.y1).enumerate() {
        let a = ((y * w + r.x0) * 4) as usize;
        px[a..a + rw].copy_from_slice(&data[k * rw..(k + 1) * rw]);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Paint the colour.
    Paint,
    /// Take away alpha.
    Erase,
}

/// The shape of the brush's tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tip {
    Round,
    Square,
    /// A flat nib at 45°, like a calligraphy pen.
    Flat,
    /// Round, broken up by the grain of the paper.
    Chalk,
    /// Scattered dots.
    Spray,
    /// Graphite on paper: the tooth of the paper shows through.
    Graphite,
    /// A ballpoint: ink that skips a little.
    Ballpoint,
}

impl Tip {
    pub const ALL: [Tip; 7] = [Tip::Round, Tip::Square, Tip::Flat, Tip::Chalk, Tip::Spray, Tip::Graphite, Tip::Ballpoint];
    pub fn name(self) -> &'static str {
        match self {
            Tip::Round => "Round",
            Tip::Square => "Square",
            Tip::Flat => "Calligraphy",
            Tip::Chalk => "Chalk",
            Tip::Spray => "Spray",
            Tip::Graphite => "Graphite",
            Tip::Ballpoint => "Ballpoint",
        }
    }
}

/// A fixed pseudo-random value 0..1 for a pixel (the paper grain) or a stamp.
#[inline]
fn hash01(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

/// Smooth noise 0..1 (the paper's tooth for graphite), `scale` pixels per cell.
fn value_noise(x: f32, y: f32, scale: f32, seed: u32) -> f32 {
    let (fx, fy) = (x / scale, y / scale);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - ix, fy - iy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (ix as i64 as u32, iy as i64 as u32);
    let a = hash01(ix, iy, seed);
    let b = hash01(ix.wrapping_add(1), iy, seed);
    let c = hash01(ix, iy.wrapping_add(1), seed);
    let d = hash01(ix.wrapping_add(1), iy.wrapping_add(1), seed);
    (a + (b - a) * sx) + ((c + (d - c) * sx) - (a + (b - a) * sx)) * sy
}

/// Symmetry: each point of a stroke is also painted at its mirror images.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SymKind {
    Off,
    /// Mirrored left–right (about a vertical line).
    LeftRight,
    /// Mirrored top–bottom (about a horizontal line).
    TopBottom,
    /// Both: four copies.
    Both,
    /// Turned around the centre, this many copies.
    Radial(u8),
    /// Turned and mirrored (a kaleidoscope), twice this many copies.
    Kaleido(u8),
    /// Mirrored about a line through the centre at any angle (`Sym::angle`).
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sym {
    pub kind: SymKind,
    /// The centre (the line(s) of symmetry go through it), in canvas pixels.
    pub cx: f32,
    pub cy: f32,
    /// Free symmetry: the line's angle (radians, 0 = level).
    pub angle: f32,
}

impl Sym {
    pub const OFF: Sym = Sym { kind: SymKind::Off, cx: 0.0, cy: 0.0, angle: 0.0 };

    /// The point and its images; the point itself comes first.
    pub fn images(&self, x: f32, y: f32) -> Vec<(f32, f32)> {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let (cx, cy) = (self.cx, self.cy);
        match self.kind {
            SymKind::Off => vec![(x, y)],
            SymKind::LeftRight => vec![(x, y), (cx - dx, y)],
            SymKind::TopBottom => vec![(x, y), (x, cy - dy)],
            SymKind::Both => vec![(x, y), (cx - dx, y), (x, cy - dy), (cx - dx, cy - dy)],
            SymKind::Free => {
                let (uy, ux) = self.angle.sin_cos();
                let d = dx * ux + dy * uy;
                vec![(x, y), (cx + 2.0 * d * ux - dx, cy + 2.0 * d * uy - dy)]
            }
            SymKind::Radial(n) | SymKind::Kaleido(n) => {
                let n = n.clamp(2, 16) as usize;
                let mut v = Vec::with_capacity(n * 2);
                for k in 0..n {
                    let a = k as f32 * std::f32::consts::TAU / n as f32;
                    let (s, c) = a.sin_cos();
                    v.push((cx + dx * c - dy * s, cy + dx * s + dy * c));
                }
                if matches!(self.kind, SymKind::Kaleido(_)) {
                    for k in 0..n {
                        let a = k as f32 * std::f32::consts::TAU / n as f32;
                        let (s, c) = a.sin_cos();
                        // mirror across the vertical line, then turn
                        let (mx, my) = (-dx, dy);
                        v.push((cx + mx * c - my * s, cy + mx * s + my * c));
                    }
                }
                v
            }
        }
    }

    /// The guide: rays from the centre, as angles (radians).
    pub fn rays(&self) -> Vec<f32> {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        match self.kind {
            SymKind::Off => vec![],
            SymKind::LeftRight => vec![-FRAC_PI_2, FRAC_PI_2],
            SymKind::TopBottom => vec![0.0, PI],
            SymKind::Both => vec![0.0, FRAC_PI_2, PI, -FRAC_PI_2],
            SymKind::Free => vec![self.angle, self.angle + PI],
            SymKind::Radial(n) => {
                let n = n.clamp(2, 16) as usize;
                (0..n).map(|k| -FRAC_PI_2 + (k as f32 + 0.5) * TAU / n as f32).collect()
            }
            SymKind::Kaleido(n) => {
                let n = n.clamp(2, 16) as usize;
                (0..2 * n).map(|k| -FRAC_PI_2 + k as f32 * PI / n as f32).collect()
            }
        }
    }
}

/// How one brush lays down coverage.
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    /// Diameter in pixels.
    pub size: f32,
    /// 0 soft … 1 hard edge.
    pub hardness: f32,
    /// Stroke opacity 0..1.
    pub opacity: f32,
    /// Coverage per stamp; below 1 builds up (airbrush).
    pub flow: f32,
    /// Builds up over itself instead of taking the maximum.
    pub buildup: bool,
    pub mode: Mode,
    pub color: [u8; 3],
    /// Distance between stamps, as a fraction of the size.
    pub spacing: f32,
    pub tip: Tip,
    /// The stroke starts thin and swells over this much of its length (0: no taper).
    pub taper: f32,
    /// A fast stroke draws thinner, like a brush pen (0: no change).
    pub speed: f32,
}

/// A stroke in progress on one layer.
pub struct Stroke {
    pub layer: usize,
    pub before: Vec<u8>,
    pub mask: Vec<u8>,
    pub brush: Brush,
    pub dirty: IRect,
    /// Paint only inside this rectangle (the selection).
    pub clip: Option<IRect>,
    /// …and only as much as this selection mask (doc-sized, 0..255) allows.
    pub sel: Option<std::sync::Arc<Vec<u8>>>,
    stamps: u32,
    /// Symmetry: the stroke is also painted at the images of each point.
    pub sym: Sym,
    /// Per image: the last point, and the distance left over from the previous segment so
    /// stamps stay evenly spaced.
    last: Vec<Option<(f32, f32, f32)>>,
    carry: Vec<f32>,
    /// How far the stroke has come, and how fast it moves (smoothed), for taper and speed.
    travel: f32,
    pace: f32,
}

impl Stroke {
    pub fn begin(doc: &Doc, layer: usize, brush: Brush) -> Self {
        Stroke {
            layer,
            before: doc.layers[layer].px.clone(),
            mask: vec![0u8; (doc.w * doc.h) as usize],
            brush,
            dirty: IRect::empty(),
            clip: None,
            sel: None,
            stamps: 0,
            sym: Sym::OFF,
            last: Vec::new(),
            carry: Vec::new(),
            travel: 0.0,
            pace: 0.0,
        }
    }

    /// Continue the stroke to (x, y) with pen pressure 0..1; returns the pixels it changed.
    pub fn to(&mut self, doc: &mut Doc, x: f32, y: f32, pressure_size: f32, pressure_alpha: f32) -> IRect {
        let mut changed = IRect::empty();
        // taper and speed thin the line, from the first image's movement
        let seg = match self.last.first().copied().flatten() {
            Some((lx, ly, _)) => ((x - lx).powi(2) + (y - ly).powi(2)).sqrt(),
            None => 0.0,
        };
        self.travel += seg;
        self.pace = self.pace * 0.7 + seg * 0.3;
        let mut f = pressure_size;
        if self.brush.taper > 0.0 {
            let len = (self.brush.size * 8.0 * self.brush.taper).max(1.0);
            let ramp = (self.travel / len).clamp(0.0, 1.0);
            f *= 0.2 + 0.8 * ramp.powf(0.6);
        }
        if self.brush.speed > 0.0 {
            let rel = self.pace / self.brush.size.max(1.0);
            f *= (1.0 / (1.0 + self.brush.speed * rel * 0.8)).max(0.3);
        }
        let size = (self.brush.size * f).max(1.0);
        let step = (size * self.brush.spacing).max(0.5);
        let pts = self.sym.images(x, y);
        if self.last.len() != pts.len() {
            self.last = vec![None; pts.len()];
            self.carry = vec![0.0; pts.len()];
        }
        for (k, (x, y)) in pts.into_iter().enumerate() {
            match self.last[k] {
                None => {
                    changed.union(self.stamp(doc.w, doc.h, x, y, size, pressure_alpha));
                }
                Some((lx, ly, _)) => {
                    let (dx, dy) = (x - lx, y - ly);
                    let dist = (dx * dx + dy * dy).sqrt();
                    let mut d = step - self.carry[k];
                    while d <= dist {
                        let t = d / dist.max(1e-6);
                        changed.union(self.stamp(doc.w, doc.h, lx + dx * t, ly + dy * t, size, pressure_alpha));
                        d += step;
                    }
                    self.carry[k] = dist - (d - step);
                }
            }
            self.last[k] = Some((x, y, pressure_size));
        }
        if !changed.is_empty() {
            self.apply(doc, changed);
            self.dirty.union(changed);
        }
        changed
    }

    fn stamp(&mut self, w: u32, h: u32, cx: f32, cy: f32, size: f32, alpha: f32) -> IRect {
        let r = size / 2.0;
        let mut x0 = (cx - r - 1.0).floor().max(0.0) as i64;
        let mut y0 = (cy - r - 1.0).floor().max(0.0) as i64;
        let mut x1 = ((cx + r + 1.0).ceil() as i64).min(w as i64);
        let mut y1 = ((cy + r + 1.0).ceil() as i64).min(h as i64);
        if let Some(c) = self.clip {
            x0 = x0.max(c.x0 as i64);
            y0 = y0.max(c.y0 as i64);
            x1 = x1.min(c.x1 as i64);
            y1 = y1.min(c.y1 as i64);
        }
        if x0 >= x1 || y0 >= y1 {
            return IRect::empty();
        }
        self.stamps = self.stamps.wrapping_add(1);
        let hard = self.brush.hardness.clamp(0.0, 1.0);
        let inner = r * hard;
        let flow = (self.brush.flow * alpha).clamp(0.0, 1.0);
        let tip = self.brush.tip;
        // a ballpoint skips now and then
        let flow = if tip == Tip::Ballpoint { flow * (0.82 + 0.18 * hash01(self.stamps, 5, 41)) } else { flow };
        let buildup = self.brush.buildup;
        let sel = self.sel.clone();
        let put = |mask: &mut Vec<u8>, x: i64, y: i64, c: f32| {
            let i = (y as u32 * w + x as u32) as usize;
            let c = match &sel {
                Some(s) => c * s[i] as f32 / 255.0,
                None => c,
            };
            let v = (c * flow * 255.0) as u32;
            let m = mask[i] as u32;
            mask[i] = if buildup { (m + (255 - m) * v / 255).min(255) } else { m.max(v) } as u8;
        };
        if tip == Tip::Spray {
            // dots scattered over the disc, more for a bigger brush
            let n = ((r * r) * 0.12 * (0.3 + 0.7 * hard)).clamp(2.0, 600.0) as u32;
            for k in 0..n {
                let a = hash01(k, self.stamps, 11) * std::f32::consts::TAU;
                let d = hash01(k, self.stamps, 23).sqrt() * r;
                let (x, y) = ((cx + d * a.cos()).floor() as i64, (cy + d * a.sin()).floor() as i64);
                if x >= x0 && x < x1 && y >= y0 && y < y1 {
                    put(&mut self.mask, x, y, 1.0);
                }
            }
            return IRect { x0: x0 as u32, y0: y0 as u32, x1: x1 as u32, y1: y1 as u32 };
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let (fx, fy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = match tip {
                    Tip::Square => fx.abs().max(fy.abs()),
                    Tip::Flat => {
                        // a nib a quarter as thick as it is wide, at 45°
                        let u = (fx + fy) * std::f32::consts::FRAC_1_SQRT_2;
                        let v = (fy - fx) * std::f32::consts::FRAC_1_SQRT_2;
                        (u * u + (v * 4.0) * (v * 4.0)).sqrt()
                    }
                    _ => (fx * fx + fy * fy).sqrt(),
                };
                // anti-aliased edge for hard brushes, a smooth falloff for soft ones
                let mut c = if d <= inner {
                    1.0
                } else if r - inner < 1.0 {
                    (r + 0.5 - d).clamp(0.0, 1.0)
                } else {
                    let t = ((d - inner) / (r - inner)).clamp(0.0, 1.0);
                    let s = 1.0 - t;
                    s * s * (3.0 - 2.0 * s)
                };
                if tip == Tip::Graphite && c > 0.0 {
                    // the paper's tooth: fine grain over a softer weave; light strokes catch only the peaks
                    let n = 0.6 * value_noise(x as f32, y as f32, 2.2, 17) + 0.4 * hash01(x as u32, y as u32, 29);
                    c *= (n * 1.6 - 0.35).clamp(0.0, 1.0) * 0.85 + 0.1;
                }
                if tip == Tip::Ballpoint && c > 0.0 {
                    c *= 0.85 + 0.15 * hash01(x as u32, y as u32, 31);
                }
                if tip == Tip::Chalk && c > 0.0 {
                    let g = hash01(x as u32, y as u32, 7);
                    c *= if g < 0.3 { 0.0 } else { 0.45 + 0.55 * hash01(x as u32, y as u32, 3) };
                }
                if c <= 0.0 {
                    continue;
                }
                put(&mut self.mask, x, y, c);
            }
        }
        IRect { x0: x0 as u32, y0: y0 as u32, x1: x1 as u32, y1: y1 as u32 }
    }

    /// Recompute the layer in `r` from its pixels before the stroke and the stroke's coverage.
    fn apply(&self, doc: &mut Doc, r: IRect) {
        let w = doc.w;
        let px = &mut doc.layers[self.layer].px;
        let op = self.brush.opacity.clamp(0.0, 1.0);
        let [cr, cg, cb] = self.brush.color;
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let i = (y * w + x) as usize;
                let k = i * 4;
                let m = self.mask[i] as f32 / 255.0 * op;
                let b = &self.before[k..k + 4];
                if m <= 0.0 {
                    px[k..k + 4].copy_from_slice(b);
                    continue;
                }
                match self.brush.mode {
                    Mode::Erase => {
                        px[k..k + 3].copy_from_slice(&b[..3]);
                        px[k + 3] = (b[3] as f32 * (1.0 - m)) as u8;
                    }
                    Mode::Paint => {
                        let da = b[3] as f32 / 255.0;
                        let oa = m + da * (1.0 - m);
                        let mix = |s: u8, d: u8| ((s as f32 * m + d as f32 * da * (1.0 - m)) / oa) as u8;
                        px[k] = mix(cr, b[0]);
                        px[k + 1] = mix(cg, b[1]);
                        px[k + 2] = mix(cb, b[2]);
                        px[k + 3] = (oa * 255.0 + 0.5) as u8;
                    }
                }
            }
        }
    }

    /// Finish: the undo step for this stroke, if it changed anything.
    pub fn end(self, doc: &Doc) -> Option<Undo> {
        if self.dirty.is_empty() {
            return None;
        }
        let r = self.dirty;
        Some(Undo::Pixels {
            layer: self.layer,
            rect: r,
            before: crop(&self.before, doc.w, r),
            after: crop(&doc.layers[self.layer].px, doc.w, r),
        })
    }
}

/// Stamp a whole path at once (shapes): a stroke from the first point through the rest.
pub fn stroke_path(doc: &mut Doc, layer: usize, brush: Brush, pts: &[(f32, f32)], clip: Option<IRect>) -> Option<Undo> {
    stroke_path_sym(doc, layer, brush, pts, clip, Sym::OFF)
}

/// [`stroke_path`] with symmetry.
pub fn stroke_path_sym(doc: &mut Doc, layer: usize, brush: Brush, pts: &[(f32, f32)], clip: Option<IRect>, sym: Sym) -> Option<Undo> {
    let mut s = Stroke::begin(doc, layer, brush);
    s.clip = clip;
    s.sym = sym;
    for &(x, y) in pts {
        s.to(doc, x, y, 1.0, 1.0);
    }
    s.end(doc)
}

/// Predictive stroke: a freehand stroke that is nearly a straight line, a circle or ellipse, a
/// triangle or a rectangle, as that shape (its outline as points), or `None` to keep it as drawn.
/// `snap`: straight lines near 0°, 45° or 90° are set to it.
pub fn predict(pts: &[(f32, f32)], snap: bool) -> Option<Vec<(f32, f32)>> {
    if pts.len() < 4 {
        return None;
    }
    let (a, b) = (pts[0], *pts.last().unwrap());
    let path: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    if path < 12.0 {
        return None;
    }
    let chord = dist(a, b);
    // a straight line: every point close to the chord
    if chord > path * 0.7 {
        let dev = pts.iter().map(|&p| seg_dist(p, a, b)).fold(0.0f32, f32::max);
        if dev < (chord * 0.04).max(2.0) {
            let mut b = b;
            if snap {
                let ang = (b.1 - a.1).atan2(b.0 - a.0);
                let q = std::f32::consts::FRAC_PI_4;
                let snapped = (ang / q).round() * q;
                if (ang - snapped).abs() < 5f32.to_radians() {
                    b = (a.0 + chord * snapped.cos(), a.1 + chord * snapped.sin());
                }
            }
            return Some(vec![a, b]);
        }
        return None;
    }
    // closed shapes: the ends meet
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let diag = dist((x0, y0), (x1, y1));
    if diag < 10.0 || chord > diag * 0.25 {
        return None;
    }
    // an ellipse: the points sit at nearly the same "radius" of the box's ellipse
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, ((x1 - x0) / 2.0).max(1.0), ((y1 - y0) / 2.0).max(1.0));
    let rs: Vec<f32> = pts.iter().map(|&(x, y)| (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt()).collect();
    let mean = rs.iter().sum::<f32>() / rs.len() as f32;
    let sd = (rs.iter().map(|r| (r - mean).powi(2)).sum::<f32>() / rs.len() as f32).sqrt();
    // and they go all the way around
    let turned = {
        let mut seen = [false; 12];
        for &(x, y) in pts {
            let a = (y - cy).atan2(x - cx) + std::f32::consts::PI;
            seen[((a / std::f32::consts::TAU * 12.0) as usize).min(11)] = true;
        }
        seen.iter().all(|&v| v)
    };
    // a polygon: the corners of the simplified outline
    let mut closed: Vec<(f32, f32)> = pts.to_vec();
    closed.push(a);
    let corners = simplify(&closed, diag * 0.07);
    let n = corners.len().saturating_sub(1); // the last repeats the first
    if (3..=4).contains(&n) && sd > 0.06 {
        let mut c = corners[..n].to_vec();
        if n == 4 {
            // a rectangle: nearly square to the axes → exactly so
            let axis = c.iter().zip(c.iter().cycle().skip(1)).all(|(p, q)| {
                let ang = (q.1 - p.1).atan2(q.0 - p.0).abs() % std::f32::consts::FRAC_PI_2;
                ang < 0.17 || ang > std::f32::consts::FRAC_PI_2 - 0.17
            });
            if axis {
                c = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
            }
        }
        c.push(c[0]);
        return Some(c);
    }
    if turned && sd < 0.09 {
        let (rx, ry) = if (rx - ry).abs() < (rx + ry) * 0.06 { ((rx + ry) / 2.0, (rx + ry) / 2.0) } else { (rx, ry) };
        let k = ((rx + ry) * 0.8).clamp(24.0, 720.0) as usize;
        return Some(
            (0..=k)
                .map(|i| {
                    let t = i as f32 / k as f32 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect(),
        );
    }
    None
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Distance from p to the segment a–b.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    if l2 <= 1e-6 {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0);
    dist(p, (a.0 + vx * t, a.1 + vy * t))
}

/// Douglas–Peucker: the points that keep the outline within `eps`.
fn simplify(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], *pts.last().unwrap());
    // a closed outline: split at the point farthest from the start
    let (mut far, mut fd) = (0, 0.0);
    for (i, &p) in pts.iter().enumerate() {
        let d = if dist(a, b) < 1e-3 { dist(p, a) } else { seg_dist(p, a, b) };
        if d > fd {
            far = i;
            fd = d;
        }
    }
    if fd <= eps || far == 0 || far == pts.len() - 1 {
        return vec![a, b];
    }
    let mut l = simplify(&pts[..=far], eps);
    let r = simplify(&pts[far..], eps);
    l.pop();
    l.extend(r);
    l
}

/// The outline of a shape between two corners, as points along it.
pub fn shape_points(kind: Shape, a: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    match kind {
        Shape::Line => vec![a, b],
        Shape::Rect => vec![a, (b.0, a.1), b, (a.0, b.1), a],
        Shape::Ellipse => {
            let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let (rx, ry) = (((b.0 - a.0) / 2.0).abs(), ((b.1 - a.1) / 2.0).abs());
            let n = ((rx + ry) * 0.8).clamp(24.0, 720.0) as usize;
            (0..=n)
                .map(|k| {
                    let t = k as f32 / n as f32 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Line,
    Rect,
    Ellipse,
}

/// Fill the inside of a rectangle or ellipse with the colour (anti-aliased edge for ellipses).
#[allow(clippy::too_many_arguments)]
pub fn fill_shape(doc: &mut Doc, layer: usize, kind: Shape, a: (f32, f32), b: (f32, f32), color: [u8; 3], opacity: f32, clip: Option<IRect>) -> Option<Undo> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let r = IRect {
        x0: x0.floor().max(0.0) as u32,
        y0: y0.floor().max(0.0) as u32,
        x1: (x1.ceil().max(0.0) as u32).min(doc.w),
        y1: (y1.ceil().max(0.0) as u32).min(doc.h),
    };
    let r = r.intersect(clip.unwrap_or(doc.full()));
    if r.is_empty() {
        return None;
    }
    let before = crop(&doc.layers[layer].px, doc.w, r);
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, ((x1 - x0) / 2.0).max(0.5), ((y1 - y0) / 2.0).max(0.5));
    let w = doc.w;
    let px = &mut doc.layers[layer].px;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let cov = match kind {
                Shape::Ellipse => {
                    let (nx, ny) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                    let d = (nx * nx + ny * ny).sqrt();
                    ((1.0 - d) * rx.min(ry) + 0.5).clamp(0.0, 1.0)
                }
                _ => 1.0,
            };
            let m = cov * opacity;
            if m <= 0.0 {
                continue;
            }
            let k = ((y * w + x) * 4) as usize;
            let da = px[k + 3] as f32 / 255.0;
            let oa = m + da * (1.0 - m);
            for c in 0..3 {
                px[k + c] = ((color[c] as f32 * m + px[k + c] as f32 * da * (1.0 - m)) / oa) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
    Some(Undo::Pixels { layer, rect: r, before, after: crop(&doc.layers[layer].px, doc.w, r) })
}

/// Flood fill from (x, y) on a layer, matching the composited colour within `tolerance` (0..255).
#[allow(clippy::too_many_arguments)]
pub fn flood_fill(doc: &mut Doc, layer: usize, x: u32, y: u32, color: [u8; 3], opacity: f32, tolerance: u8, clip: Option<IRect>) -> Option<Undo> {
    let clip = clip.unwrap_or(doc.full()).intersect(doc.full());
    if x >= doc.w || y >= doc.h || !clip.contains(x, y) {
        return None;
    }
    let (w, h) = (doc.w as usize, doc.h as usize);
    // Match on the layer itself: what the person sees on it is what bounds the fill.
    let src = &doc.layers[layer].px;
    let at = |i: usize| [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]];
    let start = at(y as usize * w + x as usize);
    let tol = tolerance as i32;
    let inside = |i: usize| clip.contains((i % w) as u32, (i / w) as u32);
    let close = |c: [u8; 4]| {
        // transparent pixels match each other whatever their hidden colour
        if c[3] == 0 && start[3] == 0 {
            return true;
        }
        (0..4).all(|k| (c[k] as i32 - start[k] as i32).abs() <= tol)
    };
    let mut seen = vec![false; w * h];
    let mut stack = vec![(x as usize, y as usize)];
    let mut r = IRect::empty();
    let mut hits = Vec::new();
    while let Some((sx, sy)) = stack.pop() {
        let i = sy * w + sx;
        if seen[i] || !inside(i) || !close(at(i)) {
            continue;
        }
        // scan the run left and right
        let mut l = sx;
        while l > 0 && !seen[sy * w + l - 1] && inside(sy * w + l - 1) && close(at(sy * w + l - 1)) {
            l -= 1;
        }
        let mut rr = sx;
        while rr + 1 < w && !seen[sy * w + rr + 1] && inside(sy * w + rr + 1) && close(at(sy * w + rr + 1)) {
            rr += 1;
        }
        for xx in l..=rr {
            seen[sy * w + xx] = true;
            hits.push(sy * w + xx);
            if sy > 0 && !seen[(sy - 1) * w + xx] {
                stack.push((xx, sy - 1));
            }
            if sy + 1 < h && !seen[(sy + 1) * w + xx] {
                stack.push((xx, sy + 1));
            }
        }
        r.union(IRect { x0: l as u32, y0: sy as u32, x1: rr as u32 + 1, y1: sy as u32 + 1 });
    }
    if hits.is_empty() {
        return None;
    }
    // grow the filled area by a pixel so it tucks under anti-aliased outlines
    let mut grow = seen.clone();
    for &i in &hits {
        let (hx, hy) = (i % w, i / w);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (hx as i64 + dx, hy as i64 + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && clip.contains(nx as u32, ny as u32) {
                grow[ny as usize * w + nx as usize] = true;
            }
        }
    }
    let r = IRect { x0: r.x0.saturating_sub(1), y0: r.y0.saturating_sub(1), x1: (r.x1 + 1).min(doc.w), y1: (r.y1 + 1).min(doc.h) };
    let before = crop(&doc.layers[layer].px, doc.w, r);
    let px = &mut doc.layers[layer].px;
    for yy in r.y0..r.y1 {
        for xx in r.x0..r.x1 {
            let i = yy as usize * w + xx as usize;
            if !grow[i] {
                continue;
            }
            let k = i * 4;
            let m = opacity;
            let da = px[k + 3] as f32 / 255.0;
            let oa = m + da * (1.0 - m);
            for c in 0..3 {
                px[k + c] = ((color[c] as f32 * m + px[k + c] as f32 * da * (1.0 - m)) / oa.max(1e-6)) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
    Some(Undo::Pixels { layer, rect: r, before, after: crop(&doc.layers[layer].px, doc.w, r) })
}

/// The kind of colour gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gradient {
    Linear,
    Radial,
}

/// Lay a gradient from `c0` at `a` to `c1` at `b` over the layer (inside `clip`).
#[allow(clippy::too_many_arguments)]
pub fn gradient(doc: &mut Doc, layer: usize, kind: Gradient, a: (f32, f32), b: (f32, f32), c0: [u8; 4], c1: [u8; 4], opacity: f32, clip: Option<IRect>) -> Option<Undo> {
    let r = clip.unwrap_or(doc.full()).intersect(doc.full());
    if r.is_empty() {
        return None;
    }
    let before = crop(&doc.layers[layer].px, doc.w, r);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    let len = len2.sqrt();
    // premultiplied ends, so a fade to transparent keeps its colour
    let pm = |c: [u8; 4]| {
        let a = c[3] as f32 / 255.0;
        [c[0] as f32 / 255.0 * a, c[1] as f32 / 255.0 * a, c[2] as f32 / 255.0 * a, a]
    };
    let (p0, p1) = (pm(c0), pm(c1));
    let w = doc.w;
    let px = &mut doc.layers[layer].px;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let (fx, fy) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
            let mut t = match kind {
                Gradient::Linear => (fx * dx + fy * dy) / len2,
                Gradient::Radial => (fx * fx + fy * fy).sqrt() / len,
            }
            .clamp(0.0, 1.0);
            // a little dither against banding
            t = (t + (hash01(x, y, 5) - 0.5) / 255.0).clamp(0.0, 1.0);
            let c: Vec<f32> = (0..4).map(|k| p0[k] + (p1[k] - p0[k]) * t).collect();
            let m = c[3] * opacity;
            if m <= 0.0 {
                continue;
            }
            let sc = [c[0] / c[3], c[1] / c[3], c[2] / c[3]];
            let k = ((y * w + x) * 4) as usize;
            let da = px[k + 3] as f32 / 255.0;
            let oa = m + da * (1.0 - m);
            for ch in 0..3 {
                px[k + ch] = ((sc[ch] * 255.0 * m + px[k + ch] as f32 * da * (1.0 - m)) / oa).clamp(0.0, 255.0) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
    Some(Undo::Pixels { layer, rect: r, before, after: crop(&doc.layers[layer].px, doc.w, r) })
}

/// Make a region of a layer transparent.
pub fn clear_rect(doc: &mut Doc, layer: usize, r: IRect) -> Option<Undo> {
    let r = r.intersect(doc.full());
    if r.is_empty() {
        return None;
    }
    let before = crop(&doc.layers[layer].px, doc.w, r);
    let zero = vec![0u8; before.len()];
    paste(&mut doc.layers[layer].px, doc.w, r, &zero);
    Some(Undo::Pixels { layer, rect: r, before, after: zero })
}

/// Put straight-alpha pixels `data` (r-sized) over a layer's pixels at `r`.
pub fn over(px: &mut [u8], w: u32, r: IRect, data: &[u8], rw: u32, sx: u32, sy: u32) {
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let s = (((y - r.y0 + sy) * rw + (x - r.x0 + sx)) * 4) as usize;
            let sa = data[s + 3] as f32 / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let k = ((y * w + x) * 4) as usize;
            let da = px[k + 3] as f32 / 255.0;
            let oa = sa + da * (1.0 - sa);
            for c in 0..3 {
                px[k + c] = ((data[s + c] as f32 * sa + px[k + c] as f32 * da * (1.0 - sa)) / oa) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
}

/// Paint a colour into a layer as much as a coverage mask (doc-sized, 0..255) says.
pub fn fill_mask(doc: &mut Doc, layer: usize, cover: &[u8], color: [u8; 3], opacity: f32) -> Option<Undo> {
    let w = doc.w;
    let mut r = IRect::empty();
    for (i, &v) in cover.iter().enumerate() {
        if v > 0 {
            let (x, y) = (i as u32 % w, i as u32 / w);
            r.union(IRect { x0: x, y0: y, x1: x + 1, y1: y + 1 });
        }
    }
    if r.is_empty() {
        return None;
    }
    let px = &mut doc.layers[layer].px;
    let before = crop(px, w, r);
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let i = (y * w + x) as usize;
            let m = cover[i] as f32 / 255.0 * opacity.clamp(0.0, 1.0);
            if m <= 0.0 {
                continue;
            }
            let k = i * 4;
            let da = px[k + 3] as f32 / 255.0;
            let oa = m + da * (1.0 - m);
            for c in 0..3 {
                px[k + c] = ((color[c] as f32 * m + px[k + c] as f32 * da * (1.0 - m)) / oa) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
    Some(Undo::Pixels { layer, rect: r, before, after: crop(px, w, r) })
}

/// The edge of a selection mask as a band `width` pixels wide, centred on the edge (Draw Outline).
pub fn outline_mask(sel: &[u8], w: u32, h: u32, width: f32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let rad = ((width / 2.0).round() as usize).max(1);
    let inside: Vec<u8> = sel.iter().map(|&v| if v >= 128 { 255 } else { 0 }).collect();
    // dilate and erode with a square, one axis at a time
    let pass = |src: &[u8], grow: bool| -> Vec<u8> {
        let pick = |a: u8, b: u8| if grow { a.max(b) } else { a.min(b) };
        let mut tmp = src.to_vec();
        for y in 0..h {
            for x in 0..w {
                let mut v = src[y * w + x];
                for d in 1..=rad {
                    if x >= d {
                        v = pick(v, src[y * w + x - d]);
                    } else if !grow {
                        v = 0;
                    }
                    if x + d < w {
                        v = pick(v, src[y * w + x + d]);
                    } else if !grow {
                        v = 0;
                    }
                }
                tmp[y * w + x] = v;
            }
        }
        let mut out = tmp.clone();
        for y in 0..h {
            for x in 0..w {
                let mut v = tmp[y * w + x];
                for d in 1..=rad {
                    if y >= d {
                        v = pick(v, tmp[(y - d) * w + x]);
                    } else if !grow {
                        v = 0;
                    }
                    if y + d < h {
                        v = pick(v, tmp[(y + d) * w + x]);
                    } else if !grow {
                        v = 0;
                    }
                }
                out[y * w + x] = v;
            }
        }
        out
    };
    let grown = pass(&inside, true);
    let shrunk = pass(&inside, false);
    grown.iter().zip(&shrunk).map(|(&g, &s)| g.saturating_sub(s)).collect()
}

/// The homography taking the unit square's corners (0,0) (1,0) (1,1) (0,1) to `q`.
fn square_to_quad(q: [(f32, f32); 4]) -> [f64; 9] {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = q.map(|(a, b)| (a as f64, b as f64));
    let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
    let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
    if dx3.abs() < 1e-9 && dy3.abs() < 1e-9 {
        return [x1 - x0, x3 - x0, x0, y1 - y0, y3 - y0, y0, 0.0, 0.0, 1.0];
    }
    let den = dx1 * dy2 - dx2 * dy1;
    let g = (dx3 * dy2 - dx2 * dy3) / den;
    let hh = (dx1 * dy3 - dx3 * dy1) / den;
    [x1 - x0 + g * x1, x3 - x0 + hh * x3, x0, y1 - y0 + g * y1, y3 - y0 + hh * y3, y0, g, hh, 1.0]
}

fn invert3(m: [f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let k = 1.0 / det;
    Some([
        (e * i - f * h) * k,
        (c * h - b * i) * k,
        (b * f - c * e) * k,
        (f * g - d * i) * k,
        (a * i - c * g) * k,
        (c * d - a * f) * k,
        (d * h - e * g) * k,
        (b * g - a * h) * k,
        (a * e - b * d) * k,
    ])
}

/// Whether a point is inside a polygon (even–odd).
pub fn point_in_polygon(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    inside
}

/// Transform: a part of a layer (the selection, or all of it) moved, scaled, turned or distorted
/// to any four corners.
pub struct Warp {
    pub layer: usize,
    before: Vec<u8>,
    base: Vec<u8>,
    pub src: IRect,
    data: Vec<u8>,
    /// Where the corners of `src` are now (top-left, top-right, bottom-right, bottom-left).
    pub quad: [(f32, f32); 4],
    shown: IRect,
    touched: IRect,
}

impl Warp {
    pub fn begin(doc: &mut Doc, layer: usize, src: IRect, sel: Option<&[u8]>) -> Self {
        let m = Moving::begin(doc, layer, src, sel);
        let (x0, y0, x1, y1) = (m.src.x0 as f32, m.src.y0 as f32, m.src.x1 as f32, m.src.y1 as f32);
        Warp { layer, before: m.before, base: m.base, src: m.src, data: m.data, quad: [(x0, y0), (x1, y0), (x1, y1), (x0, y1)], shown: m.src, touched: m.src }
    }

    /// Draw the part at `quad`; returns the region that changed.
    pub fn place(&mut self, doc: &mut Doc, quad: [(f32, f32); 4]) -> IRect {
        self.quad = quad;
        let (w, h) = (doc.w, doc.h);
        let px = &mut doc.layers[self.layer].px;
        let old = self.shown;
        paste(px, w, old, &crop(&self.base, w, old));
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in quad {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let r = IRect {
            x0: x0.floor().clamp(0.0, w as f32) as u32,
            y0: y0.floor().clamp(0.0, h as f32) as u32,
            x1: x1.ceil().clamp(0.0, w as f32) as u32,
            y1: y1.ceil().clamp(0.0, h as f32) as u32,
        };
        let (sw, sh) = (self.src.width(), self.src.height());
        if let (false, Some(inv)) = (r.is_empty() || sw == 0 || sh == 0, invert3(square_to_quad(quad))) {
            let mut layer = vec![0u8; (r.width() * r.height() * 4) as usize];
            let fetch = |x: i64, y: i64| -> [f32; 4] {
                if x < 0 || y < 0 || x >= sw as i64 || y >= sh as i64 {
                    return [0.0; 4];
                }
                let k = ((y as u32 * sw + x as u32) * 4) as usize;
                let a = self.data[k + 3] as f32;
                [self.data[k] as f32 * a, self.data[k + 1] as f32 * a, self.data[k + 2] as f32 * a, a]
            };
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                    let d = inv[6] * fx + inv[7] * fy + inv[8];
                    let u = (inv[0] * fx + inv[1] * fy + inv[2]) / d;
                    let v = (inv[3] * fx + inv[4] * fy + inv[5]) / d;
                    if !(-0.01..=1.01).contains(&u) || !(-0.01..=1.01).contains(&v) {
                        continue;
                    }
                    // bilinear, premultiplied
                    let (sx, sy) = (u as f32 * sw as f32 - 0.5, v as f32 * sh as f32 - 0.5);
                    let (ix, iy) = (sx.floor(), sy.floor());
                    let (tx, ty) = (sx - ix, sy - iy);
                    let (ix, iy) = (ix as i64, iy as i64);
                    let (a, b, c, e) = (fetch(ix, iy), fetch(ix + 1, iy), fetch(ix, iy + 1), fetch(ix + 1, iy + 1));
                    let mut o = [0f32; 4];
                    for n in 0..4 {
                        o[n] = (a[n] * (1.0 - tx) + b[n] * tx) * (1.0 - ty) + (c[n] * (1.0 - tx) + e[n] * tx) * ty;
                    }
                    if o[3] <= 0.0 {
                        continue;
                    }
                    let k = (((y - r.y0) * r.width() + (x - r.x0)) * 4) as usize;
                    layer[k] = (o[0] / o[3]).clamp(0.0, 255.0) as u8;
                    layer[k + 1] = (o[1] / o[3]).clamp(0.0, 255.0) as u8;
                    layer[k + 2] = (o[2] / o[3]).clamp(0.0, 255.0) as u8;
                    layer[k + 3] = o[3].clamp(0.0, 255.0) as u8;
                }
            }
            over(px, w, r, &layer, r.width(), 0, 0);
        }
        self.shown = r;
        self.touched.union(r);
        let mut changed = old;
        changed.union(r);
        changed
    }

    pub fn end(self, doc: &Doc) -> Option<Undo> {
        let r = self.touched;
        if r.is_empty() {
            return None;
        }
        let before = crop(&self.before, doc.w, r);
        let after = crop(&doc.layers[self.layer].px, doc.w, r);
        (before != after).then_some(Undo::Pixels { layer: self.layer, rect: r, before, after })
    }

    pub fn cancel(self, doc: &mut Doc) {
        doc.layers[self.layer].px = self.before;
    }
}

/// Moving a part of a layer (the selection, or all of it) with the Move tool.
pub struct Moving {
    pub layer: usize,
    before: Vec<u8>,
    base: Vec<u8>,
    pub src: IRect,
    data: Vec<u8>,
    pub dst: IRect,
    touched: IRect,
}

impl Moving {
    pub fn begin(doc: &mut Doc, layer: usize, src: IRect, sel: Option<&[u8]>) -> Self {
        let src = src.intersect(doc.full());
        let before = doc.layers[layer].px.clone();
        let mut data = crop(&before, doc.w, src);
        let mut base = before.clone();
        match sel {
            None => paste(&mut base, doc.w, src, &vec![0u8; data.len()]),
            Some(m) => {
                // the selected part moves, the rest stays
                for y in src.y0..src.y1 {
                    for x in src.x0..src.x1 {
                        let mi = (y * doc.w + x) as usize;
                        let k = mi * 4 + 3;
                        let d = (((y - src.y0) * src.width() + (x - src.x0)) * 4 + 3) as usize;
                        let a = before[k] as u32;
                        let sm = m[mi] as u32;
                        data[d] = (a * sm / 255) as u8;
                        base[k] = (a * (255 - sm) / 255) as u8;
                    }
                }
            }
        }
        let mut m = Moving { layer, before, base, src, data, dst: IRect::empty(), touched: src };
        m.place(doc, 0, 0);
        m
    }

    /// Show the moved pixels at offset (dx, dy); returns the region that changed.
    pub fn place(&mut self, doc: &mut Doc, dx: i64, dy: i64) -> IRect {
        let w = doc.w;
        let px = &mut doc.layers[self.layer].px;
        let mut changed = self.dst;
        if self.dst.is_empty() {
            changed = self.src;
            let r = self.src;
            paste(px, w, r, &crop(&self.base, w, r));
        } else {
            let r = self.dst;
            paste(px, w, r, &crop(&self.base, w, r));
        }
        let (x0, y0) = (self.src.x0 as i64 + dx, self.src.y0 as i64 + dy);
        let (x1, y1) = (self.src.x1 as i64 + dx, self.src.y1 as i64 + dy);
        let clampx = |v: i64| v.clamp(0, doc.w as i64) as u32;
        let clampy = |v: i64| v.clamp(0, doc.h as i64) as u32;
        let dst = IRect { x0: clampx(x0), y0: clampy(y0), x1: clampx(x1), y1: clampy(y1) };
        if !dst.is_empty() {
            let (sx, sy) = ((dst.x0 as i64 - x0) as u32, (dst.y0 as i64 - y0) as u32);
            over(px, w, dst, &self.data, self.src.width(), sx, sy);
        }
        self.dst = dst;
        self.touched.union(dst);
        changed.union(dst);
        changed
    }

    pub fn end(self, doc: &Doc) -> Option<Undo> {
        let r = self.touched;
        if r.is_empty() {
            return None;
        }
        let before = crop(&self.before, doc.w, r);
        let after = crop(&doc.layers[self.layer].px, doc.w, r);
        if before == after {
            return None;
        }
        Some(Undo::Pixels { layer: self.layer, rect: r, before, after })
    }

    /// Put everything back as it was.
    pub fn cancel(self, doc: &mut Doc) {
        doc.layers[self.layer].px = self.before;
    }
}

/// Cut the whole drawing down to a rectangle.
pub fn crop_doc(doc: &mut Doc, r: IRect) {
    let r = r.intersect(doc.full());
    if r.is_empty() {
        return;
    }
    for l in &mut doc.layers {
        for b in l.buffers_mut() {
            *b = crop(b, doc.w, r);
        }
    }
    doc.w = r.width();
    doc.h = r.height();
}

// ---------- Selections with a shape (lasso, magic wand) ----------

/// The bounds of a selection mask's selected pixels.
pub fn mask_bounds(mask: &[u8], w: u32, h: u32) -> IRect {
    let mut r = IRect::empty();
    for y in 0..h {
        let row = &mask[(y * w) as usize..((y + 1) * w) as usize];
        if let Some(x0) = row.iter().position(|&v| v > 0) {
            let x1 = row.iter().rposition(|&v| v > 0).unwrap_or(x0);
            r.union(IRect { x0: x0 as u32, y0: y, x1: x1 as u32 + 1, y1: y + 1 });
        }
    }
    r
}

/// A rectangle as a selection mask.
pub fn rect_mask(r: IRect, w: u32, h: u32) -> Vec<u8> {
    let mut m = vec![0u8; (w * h) as usize];
    let r = r.intersect(IRect { x0: 0, y0: 0, x1: w, y1: h });
    for y in r.y0..r.y1 {
        m[(y * w + r.x0) as usize..(y * w + r.x1) as usize].fill(255);
    }
    m
}

/// A closed polygon (canvas points) as an anti-aliased selection mask.
pub fn polygon_mask(pts: &[(f32, f32)], w: u32, h: u32) -> Vec<u8> {
    let mut acc = vec![0f32; (w * h) as usize];
    if pts.len() < 3 {
        return vec![0u8; (w * h) as usize];
    }
    const SUB: u32 = 4;
    let ymin = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
    let ymax = (pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().max(0.0) as u32).min(h);
    let mut xs = Vec::new();
    for y in ymin..ymax {
        for s in 0..SUB {
            let sy = y as f32 + (s as f32 + 0.5) / SUB as f32;
            xs.clear();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                    xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                let (xa, xb) = (pair[0].clamp(0.0, w as f32), pair[1].clamp(0.0, w as f32));
                if xb <= xa {
                    continue;
                }
                let (pa, pb) = (xa.floor() as u32, (xb.ceil() as u32).min(w));
                for px in pa..pb {
                    let cov = (xb.min(px as f32 + 1.0) - xa.max(px as f32)).max(0.0);
                    acc[(y * w + px) as usize] += cov / SUB as f32;
                }
            }
        }
    }
    acc.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect()
}

/// The pixels of a layer like the one at (x, y), within `tolerance`, joined to it (or anywhere
/// on the layer, `contiguous` off): the Magic Wand.
pub fn wand_mask(doc: &Doc, layer: usize, x: u32, y: u32, tolerance: u8, contiguous: bool) -> Vec<u8> {
    let (w, h) = (doc.w as usize, doc.h as usize);
    let mut out = vec![0u8; w * h];
    if x >= doc.w || y >= doc.h {
        return out;
    }
    let src = &doc.layers[layer].px;
    let at = |i: usize| [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]];
    let start = at(y as usize * w + x as usize);
    let tol = tolerance as i32;
    let close = |c: [u8; 4]| (c[3] == 0 && start[3] == 0) || (0..4).all(|k| (c[k] as i32 - start[k] as i32).abs() <= tol);
    if !contiguous {
        for (i, o) in out.iter_mut().enumerate() {
            if close(at(i)) {
                *o = 255;
            }
        }
        return out;
    }
    let mut stack = vec![(x as usize, y as usize)];
    while let Some((sx, sy)) = stack.pop() {
        let i = sy * w + sx;
        if out[i] != 0 || !close(at(i)) {
            continue;
        }
        let mut l = sx;
        while l > 0 && out[sy * w + l - 1] == 0 && close(at(sy * w + l - 1)) {
            l -= 1;
        }
        let mut r = sx;
        while r + 1 < w && out[sy * w + r + 1] == 0 && close(at(sy * w + r + 1)) {
            r += 1;
        }
        for xx in l..=r {
            out[sy * w + xx] = 255;
            if sy > 0 && out[(sy - 1) * w + xx] == 0 {
                stack.push((xx, sy - 1));
            }
            if sy + 1 < h && out[(sy + 1) * w + xx] == 0 {
                stack.push((xx, sy + 1));
            }
        }
    }
    out
}

/// How a new selection joins the one there is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelOp {
    Replace,
    Add,
    Subtract,
}

/// Join a new selection mask to the old one.
pub fn combine_masks(old: &[u8], new: &[u8], op: SelOp) -> Vec<u8> {
    match op {
        SelOp::Replace => new.to_vec(),
        SelOp::Add => old.iter().zip(new).map(|(a, b)| *a.max(b)).collect(),
        SelOp::Subtract => old.iter().zip(new).map(|(a, b)| (*a).min(255 - *b)).collect(),
    }
}

/// Keep a change only as much as the selection mask allows: the pixels of `u` become a mix of
/// before and after by the mask, on the layer and in the undo step.
pub fn mask_undo(doc: &mut Doc, u: Option<Undo>, mask: &[u8]) -> Option<Undo> {
    let Some(Undo::Pixels { layer, rect, before, mut after }) = u else { return u };
    let w = doc.w;
    let px = &mut doc.layers[layer].px;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let m = mask[(y * w + x) as usize];
            if m == 255 {
                continue;
            }
            let k = (((y - rect.y0) * rect.width() + (x - rect.x0)) * 4) as usize;
            let t = m as f32 / 255.0;
            let (ba, aa) = (before[k + 3] as f32, after[k + 3] as f32);
            let oa = ba + (aa - ba) * t;
            for c in 0..3 {
                let v = if oa > 0.0 { (before[k + c] as f32 * ba + (after[k + c] as f32 * aa - before[k + c] as f32 * ba) * t) / oa } else { 0.0 };
                after[k + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            after[k + 3] = oa.round().clamp(0.0, 255.0) as u8;
            let d = ((y * w + x) * 4) as usize;
            px[d..d + 4].copy_from_slice(&after[k..k + 4]);
        }
    }
    if before == after {
        return None;
    }
    Some(Undo::Pixels { layer, rect, before, after })
}

/// One edge of a selection outline, from one canvas corner to another.
pub type Segment = ((u32, u32), (u32, u32));

/// The outline of a selection mask (at 50%), as canvas-space segments; `None` when there would
/// be more than `max` of them.
pub fn mask_outline(mask: &[u8], w: u32, h: u32, max: usize) -> Option<Vec<Segment>> {
    let on = |x: i64, y: i64| x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h && mask[(y as u32 * w + x as u32) as usize] >= 128;
    let mut segs = Vec::new();
    // horizontal edges between rows y-1 and y
    for y in 0..=h as i64 {
        let mut run: Option<u32> = None;
        for x in 0..=w as i64 {
            let edge = x < w as i64 && on(x, y - 1) != on(x, y);
            match (edge, run) {
                (true, None) => run = Some(x as u32),
                (false, Some(x0)) => {
                    segs.push(((x0, y as u32), (x as u32, y as u32)));
                    run = None;
                }
                _ => {}
            }
        }
        if segs.len() > max {
            return None;
        }
    }
    for x in 0..=w as i64 {
        let mut run: Option<u32> = None;
        for y in 0..=h as i64 {
            let edge = y < h as i64 && on(x - 1, y) != on(x, y);
            match (edge, run) {
                (true, None) => run = Some(y as u32),
                (false, Some(y0)) => {
                    segs.push(((x as u32, y0), (x as u32, y as u32)));
                    run = None;
                }
                _ => {}
            }
        }
        if segs.len() > max {
            return None;
        }
    }
    Some(segs)
}

// ---------- Filters and adjustments ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// Gaussian-like blur of this radius.
    Blur(f32),
    /// Unsharp mask: amount (0..3), radius.
    Sharpen(f32, f32),
    /// Brightness and contrast, -1..1 each.
    BrightnessContrast(f32, f32),
    /// Hue (degrees), saturation and lightness (-1..1).
    HueSaturation(f32, f32, f32),
    Invert,
    Desaturate,
}

impl Filter {
    pub fn name(&self) -> &'static str {
        match self {
            Filter::Blur(_) => "Blur",
            Filter::Sharpen(..) => "Sharpen",
            Filter::BrightnessContrast(..) => "Brightness / Contrast",
            Filter::HueSaturation(..) => "Hue / Saturation",
            Filter::Invert => "Invert Colours",
            Filter::Desaturate => "Black and White",
        }
    }
}

/// Three box blurs over premultiplied RGBA (`pw` × `ph`), about a Gaussian of this radius.
fn blur_premul(buf: &mut [f32], pw: usize, ph: usize, radius: f32) {
    let r = ((radius * radius / 3.0 + 1.0).sqrt() - 1.0).max(0.0).round() as usize + if radius >= 1.0 { 1 } else { 0 };
    if r == 0 {
        return;
    }
    let mut tmp = vec![0f32; buf.len()];
    for _ in 0..3 {
        // horizontal
        for y in 0..ph {
            for c in 0..4 {
                let mut sum = 0f32;
                let at = |x: i64| buf[(y * pw + x.clamp(0, pw as i64 - 1) as usize) * 4 + c];
                for k in -(r as i64)..=r as i64 {
                    sum += at(k);
                }
                for x in 0..pw {
                    tmp[(y * pw + x) * 4 + c] = sum / (2 * r + 1) as f32;
                    sum += at(x as i64 + r as i64 + 1) - at(x as i64 - r as i64);
                }
            }
        }
        // vertical
        for x in 0..pw {
            for c in 0..4 {
                let mut sum = 0f32;
                let at = |y: i64| tmp[(y.clamp(0, ph as i64 - 1) as usize * pw + x) * 4 + c];
                for k in -(r as i64)..=r as i64 {
                    sum += at(k);
                }
                for y in 0..ph {
                    buf[(y * pw + x) * 4 + c] = sum / (2 * r + 1) as f32;
                    sum += at(y as i64 + r as i64 + 1) - at(y as i64 - r as i64);
                }
            }
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

/// Apply a filter to a region of a layer, without an undo step (the caller keeps the pixels
/// from before). Reads `src` (the layer as it was), writes the layer.
pub fn filter_region(doc: &mut Doc, layer: usize, f: Filter, r: IRect, src: &[u8]) {
    let r = r.intersect(doc.full());
    if r.is_empty() {
        return;
    }
    let w = doc.w;
    let px = &mut doc.layers[layer].px;
    match f {
        Filter::Blur(radius) | Filter::Sharpen(_, radius) => {
            let pad = (radius * 3.0).ceil() as u32 + 2;
            let e = IRect { x0: r.x0.saturating_sub(pad), y0: r.y0.saturating_sub(pad), x1: (r.x1 + pad).min(doc.w), y1: (r.y1 + pad).min(doc.h) };
            let (pw, ph) = (e.width() as usize, e.height() as usize);
            let mut buf = vec![0f32; pw * ph * 4];
            for y in 0..ph {
                for x in 0..pw {
                    let k = (((e.y0 as usize + y) * w as usize) + e.x0 as usize + x) * 4;
                    let a = src[k + 3] as f32 / 255.0;
                    let o = (y * pw + x) * 4;
                    for c in 0..3 {
                        buf[o + c] = src[k + c] as f32 / 255.0 * a;
                    }
                    buf[o + 3] = a;
                }
            }
            blur_premul(&mut buf, pw, ph, radius);
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let o = (((y - e.y0) as usize * pw) + (x - e.x0) as usize) * 4;
                    let k = ((y * w + x) * 4) as usize;
                    let mut v = [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]];
                    if let Filter::Sharpen(amount, _) = f {
                        let a0 = src[k + 3] as f32 / 255.0;
                        let orig = [src[k] as f32 / 255.0 * a0, src[k + 1] as f32 / 255.0 * a0, src[k + 2] as f32 / 255.0 * a0, a0];
                        for c in 0..4 {
                            v[c] = orig[c] + (orig[c] - v[c]) * amount;
                        }
                        v[3] = v[3].clamp(0.0, 1.0);
                        for c in 0..3 {
                            v[c] = v[c].clamp(0.0, v[3]);
                        }
                    }
                    let a = v[3].clamp(0.0, 1.0);
                    for c in 0..3 {
                        px[k + c] = if a > 0.0 { (v[c] / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
                    }
                    px[k + 3] = (a * 255.0).round() as u8;
                }
            }
        }
        _ => {
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let k = ((y * w + x) * 4) as usize;
                    let (mut cr, mut cg, mut cb) = (src[k] as f32 / 255.0, src[k + 1] as f32 / 255.0, src[k + 2] as f32 / 255.0);
                    match f {
                        Filter::BrightnessContrast(b, c) => {
                            let k2 = if c >= 0.0 { 1.0 / (1.0 - c * 0.99) } else { 1.0 + c };
                            let adj = |v: f32| ((v - 0.5) * k2 + 0.5 + b * 0.5).clamp(0.0, 1.0);
                            (cr, cg, cb) = (adj(cr), adj(cg), adj(cb));
                        }
                        Filter::HueSaturation(hd, sa, li) => {
                            let (h, s, l) = rgb_to_hsl(cr, cg, cb);
                            let s = (s * (1.0 + sa)).clamp(0.0, 1.0);
                            let l = if li >= 0.0 { l + (1.0 - l) * li } else { l * (1.0 + li) };
                            (cr, cg, cb) = hsl_to_rgb(h + hd / 360.0, s, l.clamp(0.0, 1.0));
                        }
                        Filter::Invert => (cr, cg, cb) = (1.0 - cr, 1.0 - cg, 1.0 - cb),
                        Filter::Desaturate => {
                            let l = 0.2126 * cr + 0.7152 * cg + 0.0722 * cb;
                            (cr, cg, cb) = (l, l, l);
                        }
                        _ => {}
                    }
                    px[k] = (cr * 255.0).round() as u8;
                    px[k + 1] = (cg * 255.0).round() as u8;
                    px[k + 2] = (cb * 255.0).round() as u8;
                    px[k + 3] = src[k + 3];
                }
            }
        }
    }
}

/// A whole-document transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transform {
    FlipH,
    FlipV,
    RotCw,
    RotCcw,
}

pub fn transform(doc: &mut Doc, t: Transform) {
    let (w, h) = (doc.w as usize, doc.h as usize);
    for buf in doc.layers.iter_mut().flat_map(Layer::buffers_mut) {
        let src = std::mem::take(buf);
        let mut dst = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let (nx, ny, nw) = match t {
                    Transform::FlipH => (w - 1 - x, y, w),
                    Transform::FlipV => (x, h - 1 - y, w),
                    Transform::RotCw => (h - 1 - y, x, h),
                    Transform::RotCcw => (y, w - 1 - x, h),
                };
                let a = (y * w + x) * 4;
                let b = (ny * nw + nx) * 4;
                dst[b..b + 4].copy_from_slice(&src[a..a + 4]);
            }
        }
        *buf = dst;
    }
    if matches!(t, Transform::RotCw | Transform::RotCcw) {
        std::mem::swap(&mut doc.w, &mut doc.h);
    }
}

/// Change the canvas size, keeping the picture at `anchor` (0..1, 0..1).
pub fn resize_canvas(doc: &mut Doc, nw: u32, nh: u32, anchor: (f32, f32)) {
    let (ow, oh) = (doc.w as i64, doc.h as i64);
    let ox = ((nw as i64 - ow) as f32 * anchor.0).round() as i64;
    let oy = ((nh as i64 - oh) as f32 * anchor.1).round() as i64;
    for l in &mut doc.layers {
      // the mask (whichever buffer holds it) is white outside the old canvas: new space shows
      let mask_slot = if l.on_mask { 0 } else { 1 };
      for (slot, buf) in l.buffers_mut().enumerate() {
        let fill = if slot == mask_slot { 255 } else { 0 };
        let mut dst = vec![fill; (nw * nh * 4) as usize];
        for y in 0..oh {
            let ny = y + oy;
            if ny < 0 || ny >= nh as i64 {
                continue;
            }
            for x in 0..ow {
                let nx = x + ox;
                if nx < 0 || nx >= nw as i64 {
                    continue;
                }
                let a = ((y * ow + x) * 4) as usize;
                let b = ((ny * nw as i64 + nx) * 4) as usize;
                dst[b..b + 4].copy_from_slice(&buf[a..a + 4]);
            }
        }
        *buf = dst;
      }
    }
    doc.w = nw;
    doc.h = nh;
}

/// Put `upper` over `lower` with the upper layer's opacity and blend mode: Merge Down.
pub fn merge(lower: &mut Layer, upper: &Layer) {
    if !upper.visible {
        return;
    }
    lower.leave_mask();
    let upper_px = upper.baked();
    for (d, s) in lower.px.chunks_exact_mut(4).zip(upper_px.chunks_exact(4)) {
        let sa = s[3] as f32 / 255.0 * upper.opacity;
        if sa <= 0.0 {
            continue;
        }
        let mut acc = [d[0] as f32 / 255.0, d[1] as f32 / 255.0, d[2] as f32 / 255.0, d[3] as f32 / 255.0 * lower.opacity];
        blend_into(&mut acc, s, sa, upper.blend);
        for c in 0..3 {
            d[c] = (acc[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        d[3] = (acc[3] / lower.opacity.max(1e-6) * 255.0).min(255.0) as u8;
    }
}

/// One step of the undo history.
#[derive(Clone)]
pub enum Undo {
    Pixels { layer: usize, rect: IRect, before: Vec<u8>, after: Vec<u8> },
    /// Anything structural (layers, size): the whole document before and after.
    Doc { before: Box<Doc>, after: Box<Doc> },
    /// Switching between painting a layer and painting its mask.
    SwapMask { layer: usize },
}

impl Undo {
    pub fn bytes(&self) -> usize {
        match self {
            Undo::Pixels { before, after, .. } => before.len() + after.len(),
            Undo::Doc { before, after } => [before, after]
                .iter()
                .flat_map(|d| d.layers.iter())
                .map(|l| l.px.len() + l.mask.as_ref().map_or(0, Vec::len))
                .sum(),
            Undo::SwapMask { .. } => 0,
        }
    }
}

/// Undo and redo, kept under a memory budget.
#[derive(Default)]
pub struct History {
    undo: Vec<Undo>,
    redo: Vec<Undo>,
}

const BUDGET: usize = 600 << 20;

impl History {
    pub fn push(&mut self, u: Undo) {
        self.redo.clear();
        self.undo.push(u);
        let mut total: usize = self.undo.iter().map(Undo::bytes).sum();
        while total > BUDGET && self.undo.len() > 1 {
            total -= self.undo.remove(0).bytes();
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Step back; returns whether the document size or layers changed (so the view rebuilds).
    pub fn undo(&mut self, doc: &mut Doc) -> Option<bool> {
        let u = self.undo.pop()?;
        let structural = apply(doc, &u, false);
        self.redo.push(u);
        Some(structural)
    }
    pub fn redo(&mut self, doc: &mut Doc) -> Option<bool> {
        let u = self.redo.pop()?;
        let structural = apply(doc, &u, true);
        self.undo.push(u);
        Some(structural)
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

fn apply(doc: &mut Doc, u: &Undo, forward: bool) -> bool {
    match u {
        Undo::Pixels { layer, rect, before, after } => {
            if let Some(l) = doc.layers.get_mut(*layer) {
                paste(&mut l.px, doc.w, *rect, if forward { after } else { before });
            }
            false
        }
        Undo::Doc { before, after } => {
            *doc = if forward { (**after).clone() } else { (**before).clone() };
            true
        }
        Undo::SwapMask { layer } => {
            if let Some(l) = doc.layers.get_mut(*layer) {
                l.swap_mask();
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetry_images() {
        let s = Sym { kind: SymKind::LeftRight, cx: 50.0, cy: 50.0, angle: 0.0 };
        assert_eq!(s.images(40.0, 10.0), vec![(40.0, 10.0), (60.0, 10.0)]);
        let r = Sym { kind: SymKind::Radial(4), cx: 0.0, cy: 0.0, angle: 0.0 }.images(10.0, 0.0);
        assert_eq!(r.len(), 4);
        assert!((r[1].0).abs() < 1e-4 && (r[1].1 - 10.0).abs() < 1e-4);
        assert_eq!(Sym { kind: SymKind::Kaleido(3), cx: 0.0, cy: 0.0, angle: 0.0 }.images(1.0, 2.0).len(), 6);
    }

    #[test]
    fn a_symmetric_stroke_paints_the_mirror_too() {
        let mut d = Doc::new(40, 40, None, "bg", "l");
        let mut st = Stroke::begin(&d, 0, brush());
        st.sym = Sym { kind: SymKind::LeftRight, cx: 20.0, cy: 20.0, angle: 0.0 };
        st.to(&mut d, 5.0, 20.0, 1.0, 1.0);
        assert!(d.layers[0].px[((20 * 40 + 34) * 4 + 3) as usize] > 0);
    }

    #[test]
    fn predictive_stroke() {
        // a shaky straight line
        let line: Vec<(f32, f32)> = (0..50).map(|i| (i as f32 * 2.0, 10.0 + if i % 2 == 0 { 0.8 } else { -0.8 })).collect();
        let p = predict(&line, true).unwrap();
        assert_eq!(p.len(), 2);
        assert!((p[1].1 - p[0].1).abs() < 1e-3, "snapped level");
        // a wobbly circle
        let circ: Vec<(f32, f32)> = (0..=60).map(|i| {
            let t = i as f32 / 60.0 * std::f32::consts::TAU;
            let r = 40.0 + (t * 7.0).sin() * 1.2;
            (100.0 + r * t.cos(), 100.0 + r * t.sin())
        }).collect();
        assert!(predict(&circ, true).unwrap().len() > 20);
        // a rough rectangle
        let mut rect = vec![];
        for i in 0..20 { rect.push((10.0 + i as f32 * 5.0, 10.0 + (i % 3) as f32 * 0.6)); }
        for i in 0..12 { rect.push((110.0, 10.0 + i as f32 * 5.0)); }
        for i in 0..20 { rect.push((110.0 - i as f32 * 5.0, 70.0)); }
        for i in 0..12 { rect.push((10.0, 70.0 - i as f32 * 5.0)); }
        let r = predict(&rect, true).unwrap();
        assert_eq!(r.len(), 5, "{r:?}");
        // a scribble stays a scribble
        let scrib: Vec<(f32, f32)> = (0..80).map(|i| ((i as f32 * 1.7).sin() * 30.0 + i as f32, (i as f32 * 0.9).cos() * 25.0)).collect();
        assert!(predict(&scrib, true).is_none());
    }

    #[test]
    fn warp_identity_and_scale() {
        let mut d = Doc::new(20, 20, None, "bg", "l");
        for y in 5..10 {
            for x in 5..10 {
                let k = ((y * 20 + x) * 4) as usize;
                d.layers[0].px[k..k + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let mut wp = Warp::begin(&mut d, 0, IRect { x0: 5, y0: 5, x1: 10, y1: 10 }, None);
        wp.place(&mut d, [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]);
        assert_eq!(d.layers[0].px[((12 * 20 + 12) * 4 + 3) as usize], 255);
        assert!(wp.end(&d).is_some());
    }

    #[test]
    fn outline_and_clip() {
        let sel = rect_mask(IRect { x0: 5, y0: 5, x1: 15, y1: 15 }, 20, 20);
        let o = outline_mask(&sel, 20, 20, 2.0);
        assert_eq!(o[5 * 20 + 5], 255);
        assert_eq!(o[10 * 20 + 10], 0);
        let mut d = Doc::new(4, 1, None, "bg", "l");
        d.layers[0].px[..4].copy_from_slice(&[0, 0, 255, 255]);
        let mut top = Layer::new("c".into(), 4, 1, [255, 0, 0, 255]);
        top.clip = true;
        d.layers.push(top);
        let c = d.composite(d.full(), None);
        assert_eq!(&c[..4], &[255, 0, 0, 255]);
        assert_eq!(c[7], 0, "clipped away where nothing is under it");
    }

    fn brush() -> Brush {
        Brush { size: 9.0, hardness: 1.0, opacity: 1.0, flow: 1.0, buildup: false, mode: Mode::Paint, color: [255, 0, 0], spacing: 0.15, tip: Tip::Round, taper: 0.0, speed: 0.0 }
    }

    #[test]
    fn a_stroke_paints_and_undoes() {
        let mut d = Doc::new(64, 32, Some([255, 255, 255, 255]), "Background", "Layer 1");
        let mut s = Stroke::begin(&d, 1, brush());
        s.to(&mut d, 5.0, 16.0, 1.0, 1.0);
        s.to(&mut d, 50.0, 16.0, 1.0, 1.0);
        let u = s.end(&d).unwrap();
        let i = ((16 * 64 + 30) * 4) as usize;
        assert_eq!(&d.layers[1].px[i..i + 4], &[255, 0, 0, 255]);
        assert_eq!(d.sample(30, 2), [255, 255, 255, 255]);
        let mut h = History::default();
        h.push(u);
        h.undo(&mut d);
        assert_eq!(d.layers[1].px[i + 3], 0);
        h.redo(&mut d);
        assert_eq!(d.layers[1].px[i + 3], 255);
    }

    #[test]
    fn a_half_opaque_marker_does_not_build_up_over_itself() {
        let mut d = Doc::new(40, 40, None, "", "L");
        let mut b = brush();
        b.opacity = 0.5;
        let mut s = Stroke::begin(&d, 0, b);
        for x in [5.0, 35.0, 5.0, 35.0] {
            s.to(&mut d, x, 20.0, 1.0, 1.0);
        }
        let i = ((20 * 40 + 20) * 4 + 3) as usize;
        assert!((d.layers[0].px[i] as i32 - 128).abs() <= 1, "{}", d.layers[0].px[i]);
    }

    #[test]
    fn fill_stops_at_lines_and_erase_clears() {
        let mut d = Doc::new(30, 30, None, "", "L");
        let u = stroke_path(&mut d, 0, brush(), &shape_points(Shape::Rect, (5.0, 5.0), (25.0, 25.0)), None);
        assert!(u.is_some());
        flood_fill(&mut d, 0, 15, 15, [0, 0, 255], 1.0, 10, None).unwrap();
        let inside = ((15 * 30 + 15) * 4) as usize;
        let outside = 0usize;
        assert_eq!(&d.layers[0].px[inside..inside + 4], &[0, 0, 255, 255]);
        assert_eq!(d.layers[0].px[outside + 3], 0);
        let mut e = brush();
        e.mode = Mode::Erase;
        e.size = 40.0;
        stroke_path(&mut d, 0, e, &[(15.0, 15.0)], None);
        assert_eq!(d.layers[0].px[inside + 3], 0);
    }

    #[test]
    fn layer_mask() {
        let mut d = Doc::new(4, 2, Some([255, 255, 255, 255]), "bg", "l");
        for p in d.layers[1].px.chunks_exact_mut(4) {
            p.copy_from_slice(&[255, 0, 0, 255]);
        }
        let mut sel = vec![255u8; 8];
        sel[0] = 0;
        d.layers[1].add_mask(Some(&sel));
        assert_eq!(&d.flatten(None)[0..4], &[255, 255, 255, 255]);
        assert_eq!(&d.flatten(None)[4..8], &[255, 0, 0, 255]);
        // painting the mask: the picture is unchanged until the mask changes
        d.layers[1].swap_mask();
        assert_eq!(&d.flatten(None)[0..4], &[255, 255, 255, 255]);
        d.layers[1].px[4..8].copy_from_slice(&[0, 0, 0, 255]);
        assert_eq!(&d.flatten(None)[4..8], &[255, 255, 255, 255]);
        // the mask follows the picture when it turns, and new canvas space shows the layer
        transform(&mut d, Transform::RotCw);
        resize_canvas(&mut d, 3, 5, (0.0, 0.0));
        assert_eq!(d.layers[1].mask.as_ref().unwrap().len(), d.layers[1].px.len());
        d.layers[1].apply_mask();
        assert!(d.layers[1].mask.is_none() && !d.layers[1].on_mask);
        assert_eq!(d.layers[1].px.len(), 3 * 5 * 4);
    }

    #[test]
    fn transforms_and_canvas_size() {
        let mut d = Doc::new(4, 2, None, "", "L");
        d.layers[0].px[0..4].copy_from_slice(&[1, 2, 3, 255]);
        transform(&mut d, Transform::RotCw);
        assert_eq!((d.w, d.h), (2, 4));
        assert_eq!(&d.layers[0].px[(1 * 4)..(1 * 4 + 4)], &[1, 2, 3, 255]);
        transform(&mut d, Transform::RotCcw);
        transform(&mut d, Transform::FlipH);
        assert_eq!(&d.layers[0].px[12..16], &[1, 2, 3, 255]);
        resize_canvas(&mut d, 6, 2, (0.0, 0.0));
        assert_eq!(d.w, 6);
        assert_eq!(&d.layers[0].px[12..16], &[1, 2, 3, 255]);
        let flat = d.flatten(Some([255, 255, 255]));
        assert_eq!(&flat[20..24], &[255, 255, 255, 255]);
    }

    #[test]
    fn blend_modes_selection_move_and_gradient() {
        // multiply: red over grey gives dark red
        let mut d = Doc::new(4, 4, Some([128, 128, 128, 255]), "B", "L");
        for p in d.layers[1].px.chunks_exact_mut(4) {
            p.copy_from_slice(&[255, 0, 0, 255]);
        }
        d.layers[1].blend = Blend::Multiply;
        let c = d.sample(1, 1);
        assert!((c[0] as i32 - 128).abs() <= 1 && c[1] == 0 && c[3] == 255, "{c:?}");
        d.layers[1].blend = Blend::Screen;
        assert_eq!(d.sample(1, 1)[0], 255);
        // painting stays inside the selection
        let mut d = Doc::new(20, 20, None, "", "L");
        let mut s = Stroke::begin(&d, 0, brush());
        s.clip = Some(IRect { x0: 0, y0: 0, x1: 10, y1: 20 });
        s.to(&mut d, 10.0, 10.0, 1.0, 1.0);
        assert!(d.layers[0].px[((10 * 20 + 8) * 4 + 3) as usize] > 0);
        assert_eq!(d.layers[0].px[((10 * 20 + 12) * 4 + 3) as usize], 0);
        // moving a region and undoing it
        let mut d = Doc::new(10, 10, None, "", "L");
        d.layers[0].px[0..4].copy_from_slice(&[9, 9, 9, 255]);
        let mut m = Moving::begin(&mut d, 0, IRect { x0: 0, y0: 0, x1: 2, y1: 2 }, None);
        m.place(&mut d, 5, 3);
        let u = m.end(&d).unwrap();
        assert_eq!(d.layers[0].px[3], 0);
        assert_eq!(&d.layers[0].px[((3 * 10 + 5) * 4) as usize..][..4], &[9, 9, 9, 255]);
        let mut h = History::default();
        h.push(u);
        h.undo(&mut d);
        assert_eq!(&d.layers[0].px[0..4], &[9, 9, 9, 255]);
        // a gradient from black to white
        let mut d = Doc::new(11, 1, None, "", "L");
        gradient(&mut d, 0, Gradient::Linear, (0.0, 0.5), (11.0, 0.5), [0, 0, 0, 255], [255, 255, 255, 255], 1.0, None);
        assert!(d.layers[0].px[0] < 20 && d.layers[0].px[40] > 235);
        // every tip paints something
        for tip in Tip::ALL {
            let mut d = Doc::new(40, 40, None, "", "L");
            let mut b = brush();
            b.tip = tip;
            b.size = 20.0;
            stroke_path(&mut d, 0, b, &[(10.0, 20.0), (30.0, 20.0)], None);
            assert!(d.layers[0].px.chunks_exact(4).any(|p| p[3] > 0), "{tip:?}");
        }
    }

    #[test]
    fn shaped_selections_and_filters() {
        // a triangle lasso covers about half its box
        let m = polygon_mask(&[(0.0, 0.0), (20.0, 0.0), (0.0, 20.0)], 20, 20);
        let sum: u32 = m.iter().map(|&v| v as u32).sum();
        assert!((sum as f32 / 255.0 - 200.0).abs() < 4.0, "{}", sum as f32 / 255.0);
        assert_eq!(mask_bounds(&m, 20, 20), IRect { x0: 0, y0: 0, x1: 20, y1: 20 });
        // the wand picks the red square only
        let mut d = Doc::new(10, 10, None, "", "L");
        for y in 2..5 {
            for x in 2..5 {
                let k = ((y * 10 + x) * 4) as usize;
                d.layers[0].px[k..k + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let w = wand_mask(&d, 0, 3, 3, 10, true);
        assert_eq!(w.iter().filter(|&&v| v == 255).count(), 9);
        let o = mask_outline(&w, 10, 10, 1000).unwrap();
        assert_eq!(o.len(), 4);
        // a fill masked by the selection stays inside it
        let full = d.full();
        let u = clear_rect(&mut d, 0, full);
        let u = mask_undo(&mut d, u, &w);
        assert!(u.is_some());
        assert_eq!(d.layers[0].px[((3 * 10 + 3) * 4 + 3) as usize], 0);
        // subtracting leaves nothing
        assert!(combine_masks(&w, &w, SelOp::Subtract).iter().all(|&v| v == 0));
        // moving only the selected part
        let mut d = Doc::new(4, 1, None, "", "L");
        d.layers[0].px.copy_from_slice(&[9, 9, 9, 255, 8, 8, 8, 255, 0, 0, 0, 0, 0, 0, 0, 0]);
        let sel = rect_mask(IRect { x0: 0, y0: 0, x1: 1, y1: 1 }, 4, 1);
        let mut mv = Moving::begin(&mut d, 0, IRect { x0: 0, y0: 0, x1: 2, y1: 1 }, Some(&sel));
        mv.place(&mut d, 2, 0);
        assert!(mv.end(&d).is_some());
        assert_eq!(&d.layers[0].px[4..8], &[8, 8, 8, 255]);
        assert_eq!(&d.layers[0].px[8..12], &[9, 9, 9, 255]);
        // filters
        let mut d = Doc::new(9, 9, Some([0, 0, 0, 255]), "", "L");
        d.layers[0].px[((4 * 9 + 4) * 4) as usize] = 255;
        let src = d.layers[0].px.clone();
        let full = d.full();
        filter_region(&mut d, 0, Filter::Blur(2.0), full, &src);
        assert!(d.layers[0].px[((4 * 9 + 4) * 4) as usize] < 200);
        assert!(d.layers[0].px[((4 * 9 + 5) * 4) as usize] > 0);
        let src = d.layers[0].px.clone();
        filter_region(&mut d, 0, Filter::Invert, full, &src);
        assert_eq!(d.layers[0].px[1], 255);
        let (h, s2, l) = rgb_to_hsl(1.0, 0.0, 0.0);
        let (r, g, b) = hsl_to_rgb(h, s2, l);
        assert!((r - 1.0).abs() < 1e-4 && g.abs() < 1e-4 && b.abs() < 1e-4);
    }
}
