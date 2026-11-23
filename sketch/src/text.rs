//! Text on the drawing: lines laid out with a font and turned into pixels.

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};

use crate::raster::{crop, Doc, IRect, Undo};

/// The fonts that come with Sketch; more can be loaded from .ttf / .otf files.
pub fn built_in() -> Vec<(String, FontArc)> {
    let mut v = Vec::new();
    if let Ok(f) = FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT) {
        v.push(("Ubuntu Light".to_string(), f));
    }
    if let Ok(f) = FontArc::try_from_slice(epaint_default_fonts::HACK_REGULAR) {
        v.push(("Hack".to_string(), f));
    }
    v
}

pub fn load(bytes: Vec<u8>) -> Result<FontArc, String> {
    FontArc::try_from_vec(bytes).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Text coverage: a w × h mask placed at (x, y) on the canvas.
pub struct Mask {
    pub x: i64,
    pub y: i64,
    pub w: u32,
    pub h: u32,
    pub a: Vec<u8>,
}

/// Lay out `text` (lines split at newlines) with its top-left at `at`; `size` is the pixel height.
pub fn layout(font: &FontArc, text: &str, size: f32, at: (f32, f32), align: Align, line_gap: f32) -> Option<Mask> {
    let sf = font.as_scaled(PxScale::from(size.max(1.0)));
    let line_h = (sf.ascent() - sf.descent() + sf.line_gap()) * line_gap;
    let lines: Vec<&str> = text.split('\n').collect();
    let width_of = |line: &str| {
        let mut w = 0.0;
        let mut prev = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        w
    };
    let widths: Vec<f32> = lines.iter().map(|l| width_of(l)).collect();
    let block_w = widths.iter().cloned().fold(0.0f32, f32::max);
    let mut glyphs = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let off = match align {
            Align::Left => 0.0,
            Align::Center => (block_w - widths[n]) / 2.0,
            Align::Right => block_w - widths[n],
        };
        let mut x = at.0 + off;
        let y = at.1 + sf.ascent() + n as f32 * line_h;
        let mut prev = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            glyphs.push(id.with_scale_and_position(sf.scale(), point(x, y)));
            x += sf.h_advance(id);
            prev = Some(id);
        }
    }
    let outlined: Vec<_> = glyphs.into_iter().filter_map(|g| font.outline_glyph(g)).collect();
    if outlined.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for g in &outlined {
        let b = g.px_bounds();
        x0 = x0.min(b.min.x);
        y0 = y0.min(b.min.y);
        x1 = x1.max(b.max.x);
        y1 = y1.max(b.max.y);
    }
    let (mx, my) = (x0.floor() as i64, y0.floor() as i64);
    let (w, h) = ((x1.ceil() as i64 - mx).max(1) as u32, (y1.ceil() as i64 - my).max(1) as u32);
    if (w as u64) * (h as u64) > 64_000_000 {
        return None;
    }
    let mut a = vec![0u8; (w * h) as usize];
    for g in &outlined {
        let b = g.px_bounds();
        let (gx, gy) = (b.min.x as i64 - mx, b.min.y as i64 - my);
        g.draw(|px, py, c| {
            let (x, y) = (gx + px as i64, gy + py as i64);
            if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                let i = (y as u32 * w + x as u32) as usize;
                let v = (c.clamp(0.0, 1.0) * 255.0) as u8;
                a[i] = a[i].max(v);
            }
        });
    }
    Some(Mask { x: mx, y: my, w, h, a })
}

/// Paint a text mask onto a layer in a colour.
pub fn paint(doc: &mut Doc, layer: usize, m: &Mask, color: [u8; 3], opacity: f32, clip: Option<IRect>) -> Option<Undo> {
    let cl = |v: i64, max: u32| v.clamp(0, max as i64) as u32;
    let r = IRect { x0: cl(m.x, doc.w), y0: cl(m.y, doc.h), x1: cl(m.x + m.w as i64, doc.w), y1: cl(m.y + m.h as i64, doc.h) };
    let r = r.intersect(clip.unwrap_or(doc.full()));
    if r.is_empty() {
        return None;
    }
    let before = crop(&doc.layers[layer].px, doc.w, r);
    let w = doc.w;
    let px = &mut doc.layers[layer].px;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let mi = ((y as i64 - m.y) as u32 * m.w + (x as i64 - m.x) as u32) as usize;
            let s = m.a[mi] as f32 / 255.0 * opacity;
            if s <= 0.0 {
                continue;
            }
            let k = ((y * w + x) * 4) as usize;
            let da = px[k + 3] as f32 / 255.0;
            let oa = s + da * (1.0 - s);
            for c in 0..3 {
                px[k + c] = ((color[c] as f32 * s + px[k + c] as f32 * da * (1.0 - s)) / oa) as u8;
            }
            px[k + 3] = (oa * 255.0 + 0.5) as u8;
        }
    }
    Some(Undo::Pixels { layer, rect: r, before, after: crop(&doc.layers[layer].px, doc.w, r) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_with_accents_paints() {
        let fonts = built_in();
        assert_eq!(fonts.len(), 2);
        let m = layout(&fonts[0].1, "Árvíztűrő\nșțăâî", 32.0, (4.0, 4.0), Align::Center, 1.0).unwrap();
        assert!(m.w > 50 && m.h > 40);
        assert!(m.a.iter().any(|&v| v > 200));
        let mut d = Doc::new(300, 120, None, "", "L");
        assert!(paint(&mut d, 0, &m, [0, 0, 0], 1.0, None).is_some());
        assert!(d.layers[0].px.chunks_exact(4).filter(|p| p[3] > 0).count() > 100);
    }
}
