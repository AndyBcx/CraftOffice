//! Files: the drawing with its layers (`.cosketch`), pictures to open or import, and export.
//!
//! `.cosketch`: the line `COSKETCH 1`, a line of JSON (size, active layer, and per layer its name,
//! visibility, opacity and the byte length of its PNG), then the layers' PNGs one after another.

use serde::{Deserialize, Serialize};

use crate::raster::{Blend, Doc, Layer};

const MAGIC: &[u8] = b"COSKETCH 1\n";

#[derive(Serialize, Deserialize)]
struct Header {
    w: u32,
    h: u32,
    active: usize,
    layers: Vec<LayerHeader>,
}

#[derive(Serialize, Deserialize)]
struct LayerHeader {
    name: String,
    visible: bool,
    opacity: f32,
    #[serde(default)]
    blend: Blend,
    png: usize,
}

fn png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    image::ImageEncoder::write_image(enc, rgba, w, h, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn save_project(doc: &Doc) -> Result<Vec<u8>, String> {
    let mut pngs = Vec::new();
    for l in &doc.layers {
        pngs.push(png(doc.w, doc.h, &l.px)?);
    }
    let header = Header {
        w: doc.w,
        h: doc.h,
        active: doc.active,
        layers: doc
            .layers
            .iter()
            .zip(&pngs)
            .map(|(l, p)| LayerHeader { name: l.name.clone(), visible: l.visible, opacity: l.opacity, blend: l.blend, png: p.len() })
            .collect(),
    };
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(serde_json::to_string(&header).map_err(|e| e.to_string())?.as_bytes());
    out.push(b'\n');
    for p in pngs {
        out.extend_from_slice(&p);
    }
    Ok(out)
}

pub fn is_project(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

pub fn load_project(bytes: &[u8]) -> Result<Doc, String> {
    let rest = bytes.strip_prefix(MAGIC).ok_or("not a CraftOffice Sketch file")?;
    let nl = rest.iter().position(|&b| b == b'\n').ok_or("damaged file")?;
    let header: Header = serde_json::from_slice(&rest[..nl]).map_err(|e| e.to_string())?;
    if header.w == 0 || header.h == 0 || header.w > 16384 || header.h > 16384 || header.layers.is_empty() {
        return Err("damaged file".into());
    }
    let mut at = nl + 1;
    let mut layers = Vec::new();
    for lh in header.layers {
        let end = at.checked_add(lh.png).filter(|&e| e <= rest.len()).ok_or("damaged file")?;
        let img = image::load_from_memory(&rest[at..end]).map_err(|e| e.to_string())?.to_rgba8();
        if img.width() != header.w || img.height() != header.h {
            return Err("damaged file".into());
        }
        layers.push(Layer { name: lh.name, visible: lh.visible, opacity: lh.opacity.clamp(0.0, 1.0), blend: lh.blend, px: img.into_raw() });
        at = end;
    }
    let active = header.active.min(layers.len() - 1);
    Ok(Doc { w: header.w, h: header.h, layers, active })
}

/// A picture file as straight RGBA.
pub fn decode_image(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return Err("the picture is too large".into());
    }
    Ok((w, h, img.into_raw()))
}

pub fn export_png(doc: &Doc) -> Result<Vec<u8>, String> {
    png(doc.w, doc.h, &doc.flatten(None))
}

/// JPEG has no transparency: the picture goes over white.
pub fn export_jpeg(doc: &Doc) -> Result<Vec<u8>, String> {
    let flat = doc.flatten(Some([255, 255, 255]));
    let rgb: Vec<u8> = flat.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
    image::ImageEncoder::write_image(enc, &rgb, doc.w, doc.h, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Lay a picture into a new layer of the drawing, centred and scaled down to fit if needed.
pub fn picture_layer(doc: &Doc, name: String, iw: u32, ih: u32, rgba: &[u8]) -> Layer {
    let mut l = Layer::new(name, doc.w, doc.h, [0, 0, 0, 0]);
    let scale = (doc.w as f32 / iw as f32).min(doc.h as f32 / ih as f32).min(1.0);
    let (sw, sh) = ((iw as f32 * scale).round().max(1.0) as u32, (ih as f32 * scale).round().max(1.0) as u32);
    let (ox, oy) = ((doc.w - sw.min(doc.w)) / 2, (doc.h - sh.min(doc.h)) / 2);
    for y in 0..sh.min(doc.h) {
        for x in 0..sw.min(doc.w) {
            let (sx, sy) = (((x as f32 + 0.5) / scale) as u32, ((y as f32 + 0.5) / scale) as u32);
            let (sx, sy) = (sx.min(iw - 1), sy.min(ih - 1));
            let a = ((sy * iw + sx) * 4) as usize;
            let b = (((y + oy) * doc.w + x + ox) * 4) as usize;
            l.px[b..b + 4].copy_from_slice(&rgba[a..a + 4]);
        }
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawing_round_trips_with_its_layers() {
        let mut d = Doc::new(8, 6, Some([255, 255, 255, 255]), "Background", "Ink");
        d.layers[1].px[0..4].copy_from_slice(&[10, 20, 30, 200]);
        d.layers[1].opacity = 0.5;
        d.layers[0].visible = false;
        d.layers[1].blend = Blend::Multiply;
        let bytes = save_project(&d).unwrap();
        assert!(is_project(&bytes));
        let e = load_project(&bytes).unwrap();
        assert_eq!((e.w, e.h, e.layers.len(), e.active), (8, 6, 2, 1));
        assert_eq!(e.layers[1].name, "Ink");
        assert_eq!(&e.layers[1].px[0..4], &[10, 20, 30, 200]);
        assert!(!e.layers[0].visible);
        assert_eq!(e.layers[1].blend, Blend::Multiply);
        assert!(load_project(&bytes[..bytes.len() - 3]).is_err());
        let p = export_png(&d).unwrap();
        let (w, h, _) = decode_image(&p).unwrap();
        assert_eq!((w, h), (8, 6));
        assert!(export_jpeg(&d).unwrap().starts_with(&[0xFF, 0xD8]));
    }
}
