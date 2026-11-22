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
}

#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    /// width × height × RGBA, straight alpha.
    pub px: Vec<u8>,
}

impl Layer {
    pub fn new(name: String, w: u32, h: u32, fill: [u8; 4]) -> Self {
        let mut px = vec![0u8; (w * h * 4) as usize];
        if fill != [0, 0, 0, 0] {
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&fill);
            }
        }
        Layer { name, visible: true, opacity: 1.0, px }
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
        let n = (self.w * self.h) as usize;
        let mut out = vec![0f32; n * 4];
        if let Some(c) = under {
            for i in 0..n {
                out[i * 4] = c[0] as f32 / 255.0;
                out[i * 4 + 1] = c[1] as f32 / 255.0;
                out[i * 4 + 2] = c[2] as f32 / 255.0;
                out[i * 4 + 3] = 1.0;
            }
        }
        for l in self.layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
            for i in 0..n {
                let sa = l.px[i * 4 + 3] as f32 / 255.0 * l.opacity;
                if sa <= 0.0 {
                    continue;
                }
                let da = out[i * 4 + 3];
                let oa = sa + da * (1.0 - sa);
                for c in 0..3 {
                    let s = l.px[i * 4 + c] as f32 / 255.0;
                    let d = out[i * 4 + c];
                    out[i * 4 + c] = (s * sa + d * da * (1.0 - sa)) / oa;
                }
                out[i * 4 + 3] = oa;
            }
        }
        out.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect()
    }

    /// The colour at a point of the composited picture.
    pub fn sample(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.w || y >= self.h {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.w + x) * 4) as usize;
        let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
        for l in self.layers.iter().filter(|l| l.visible) {
            let sa = l.px[i + 3] as f32 / 255.0 * l.opacity;
            if sa <= 0.0 {
                continue;
            }
            let oa = sa + a * (1.0 - sa);
            r = (l.px[i] as f32 * sa + r * a * (1.0 - sa)) / oa;
            g = (l.px[i + 1] as f32 * sa + g * a * (1.0 - sa)) / oa;
            b = (l.px[i + 2] as f32 * sa + b * a * (1.0 - sa)) / oa;
            a = oa;
        }
        [r as u8, g as u8, b as u8, (a * 255.0) as u8]
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
}

/// A stroke in progress on one layer.
pub struct Stroke {
    pub layer: usize,
    pub before: Vec<u8>,
    pub mask: Vec<u8>,
    pub brush: Brush,
    pub dirty: IRect,
    last: Option<(f32, f32, f32)>,
    /// Distance left over from the previous segment, so stamps stay evenly spaced.
    carry: f32,
}

impl Stroke {
    pub fn begin(doc: &Doc, layer: usize, brush: Brush) -> Self {
        Stroke {
            layer,
            before: doc.layers[layer].px.clone(),
            mask: vec![0u8; (doc.w * doc.h) as usize],
            brush,
            dirty: IRect::empty(),
            last: None,
            carry: 0.0,
        }
    }

    /// Continue the stroke to (x, y) with pen pressure 0..1; returns the pixels it changed.
    pub fn to(&mut self, doc: &mut Doc, x: f32, y: f32, pressure_size: f32, pressure_alpha: f32) -> IRect {
        let mut changed = IRect::empty();
        let size = (self.brush.size * pressure_size).max(1.0);
        let step = (size * self.brush.spacing).max(0.5);
        match self.last {
            None => {
                changed.union(self.stamp(doc.w, doc.h, x, y, size, pressure_alpha));
            }
            Some((lx, ly, _)) => {
                let (dx, dy) = (x - lx, y - ly);
                let dist = (dx * dx + dy * dy).sqrt();
                let mut d = step - self.carry;
                while d <= dist {
                    let t = d / dist.max(1e-6);
                    changed.union(self.stamp(doc.w, doc.h, lx + dx * t, ly + dy * t, size, pressure_alpha));
                    d += step;
                }
                self.carry = dist - (d - step);
            }
        }
        self.last = Some((x, y, pressure_size));
        if !changed.is_empty() {
            self.apply(doc, changed);
            self.dirty.union(changed);
        }
        changed
    }

    fn stamp(&mut self, w: u32, h: u32, cx: f32, cy: f32, size: f32, alpha: f32) -> IRect {
        let r = size / 2.0;
        let x0 = (cx - r - 1.0).floor().max(0.0) as i64;
        let y0 = (cy - r - 1.0).floor().max(0.0) as i64;
        let x1 = ((cx + r + 1.0).ceil() as i64).min(w as i64);
        let y1 = ((cy + r + 1.0).ceil() as i64).min(h as i64);
        if x0 >= x1 || y0 >= y1 {
            return IRect::empty();
        }
        let hard = self.brush.hardness.clamp(0.0, 1.0);
        let inner = r * hard;
        let flow = (self.brush.flow * alpha).clamp(0.0, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let (fx, fy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (fx * fx + fy * fy).sqrt();
                // anti-aliased edge for hard brushes, a smooth falloff for soft ones
                let c = if d <= inner {
                    1.0
                } else if r - inner < 1.0 {
                    (r + 0.5 - d).clamp(0.0, 1.0)
                } else {
                    let t = ((d - inner) / (r - inner)).clamp(0.0, 1.0);
                    let s = 1.0 - t;
                    s * s * (3.0 - 2.0 * s)
                };
                if c <= 0.0 {
                    continue;
                }
                let v = (c * flow * 255.0) as u32;
                let i = (y as u32 * w + x as u32) as usize;
                let m = self.mask[i] as u32;
                self.mask[i] = if self.brush.buildup { (m + (255 - m) * v / 255).min(255) } else { m.max(v) } as u8;
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
pub fn stroke_path(doc: &mut Doc, layer: usize, brush: Brush, pts: &[(f32, f32)]) -> Option<Undo> {
    let mut s = Stroke::begin(doc, layer, brush);
    for &(x, y) in pts {
        s.to(doc, x, y, 1.0, 1.0);
    }
    s.end(doc)
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
pub fn fill_shape(doc: &mut Doc, layer: usize, kind: Shape, a: (f32, f32), b: (f32, f32), color: [u8; 3], opacity: f32) -> Option<Undo> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    let r = IRect {
        x0: x0.floor().max(0.0) as u32,
        y0: y0.floor().max(0.0) as u32,
        x1: (x1.ceil().max(0.0) as u32).min(doc.w),
        y1: (y1.ceil().max(0.0) as u32).min(doc.h),
    };
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
pub fn flood_fill(doc: &mut Doc, layer: usize, x: u32, y: u32, color: [u8; 3], opacity: f32, tolerance: u8) -> Option<Undo> {
    if x >= doc.w || y >= doc.h {
        return None;
    }
    let (w, h) = (doc.w as usize, doc.h as usize);
    // Match on the layer itself: what the person sees on it is what bounds the fill.
    let src = &doc.layers[layer].px;
    let at = |i: usize| [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]];
    let start = at(y as usize * w + x as usize);
    let tol = tolerance as i32;
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
        if seen[i] || !close(at(i)) {
            continue;
        }
        // scan the run left and right
        let mut l = sx;
        while l > 0 && !seen[sy * w + l - 1] && close(at(sy * w + l - 1)) {
            l -= 1;
        }
        let mut rr = sx;
        while rr + 1 < w && !seen[sy * w + rr + 1] && close(at(sy * w + rr + 1)) {
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
            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
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
    for l in &mut doc.layers {
        let src = std::mem::take(&mut l.px);
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
        l.px = dst;
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
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
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
                dst[b..b + 4].copy_from_slice(&l.px[a..a + 4]);
            }
        }
        l.px = dst;
    }
    doc.w = nw;
    doc.h = nh;
}

/// Put `upper` over `lower` with the upper layer's opacity: Merge Down.
pub fn merge(lower: &mut Layer, upper: &Layer) {
    if !upper.visible {
        return;
    }
    for (d, s) in lower.px.chunks_exact_mut(4).zip(upper.px.chunks_exact(4)) {
        let sa = s[3] as f32 / 255.0 * upper.opacity;
        if sa <= 0.0 {
            continue;
        }
        let da = d[3] as f32 / 255.0 * lower.opacity;
        let oa = sa + da * (1.0 - sa);
        for c in 0..3 {
            d[c] = ((s[c] as f32 * sa + d[c] as f32 * da * (1.0 - sa)) / oa) as u8;
        }
        d[3] = (oa / lower.opacity.max(1e-6) * 255.0).min(255.0) as u8;
    }
}

/// One step of the undo history.
#[derive(Clone)]
pub enum Undo {
    Pixels { layer: usize, rect: IRect, before: Vec<u8>, after: Vec<u8> },
    /// Anything structural (layers, size): the whole document before and after.
    Doc { before: Box<Doc>, after: Box<Doc> },
}

impl Undo {
    pub fn bytes(&self) -> usize {
        match self {
            Undo::Pixels { before, after, .. } => before.len() + after.len(),
            Undo::Doc { before, after } => {
                before.layers.iter().map(|l| l.px.len()).sum::<usize>() + after.layers.iter().map(|l| l.px.len()).sum::<usize>()
            }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush() -> Brush {
        Brush { size: 9.0, hardness: 1.0, opacity: 1.0, flow: 1.0, buildup: false, mode: Mode::Paint, color: [255, 0, 0], spacing: 0.15 }
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
        let u = stroke_path(&mut d, 0, brush(), &shape_points(Shape::Rect, (5.0, 5.0), (25.0, 25.0)));
        assert!(u.is_some());
        flood_fill(&mut d, 0, 15, 15, [0, 0, 255], 1.0, 10).unwrap();
        let inside = ((15 * 30 + 15) * 4) as usize;
        let outside = 0usize;
        assert_eq!(&d.layers[0].px[inside..inside + 4], &[0, 0, 255, 255]);
        assert_eq!(d.layers[0].px[outside + 3], 0);
        let mut e = brush();
        e.mode = Mode::Erase;
        e.size = 40.0;
        stroke_path(&mut d, 0, e, &[(15.0, 15.0)]);
        assert_eq!(d.layers[0].px[inside + 3], 0);
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
}
