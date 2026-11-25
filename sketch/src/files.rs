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
    /// The layer mask's PNG, right after the layer's (bytes; absent: no mask).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mask_png: Option<usize>,
    /// Clipping mask (clipped to the layer under it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    clip: bool,
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
    let mut masks = Vec::new();
    for l in &doc.layers {
        pngs.push(png(doc.w, doc.h, l.colors())?);
        masks.push(match l.mask_px() {
            Some(m) => Some(png(doc.w, doc.h, m)?),
            None => None,
        });
    }
    let header = Header {
        w: doc.w,
        h: doc.h,
        active: doc.active,
        layers: doc
            .layers
            .iter()
            .zip(pngs.iter().zip(&masks))
            .map(|(l, (p, m))| LayerHeader {
                name: l.name.clone(),
                visible: l.visible,
                opacity: l.opacity,
                blend: l.blend,
                png: p.len(),
                mask_png: m.as_ref().map(Vec::len),
                clip: l.clip,
            })
            .collect(),
    };
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(serde_json::to_string(&header).map_err(|e| e.to_string())?.as_bytes());
    out.push(b'\n');
    for (p, m) in pngs.into_iter().zip(masks) {
        out.extend_from_slice(&p);
        if let Some(m) = m {
            out.extend_from_slice(&m);
        }
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
        at = end;
        let mask = match lh.mask_png {
            Some(n) => {
                let end = at.checked_add(n).filter(|&e| e <= rest.len()).ok_or("damaged file")?;
                let m = image::load_from_memory(&rest[at..end]).map_err(|e| e.to_string())?.to_rgba8();
                if m.width() != header.w || m.height() != header.h {
                    return Err("damaged file".into());
                }
                at = end;
                Some(m.into_raw())
            }
            None => None,
        };
        layers.push(Layer {
            name: lh.name,
            visible: lh.visible,
            opacity: lh.opacity.clamp(0.0, 1.0),
            blend: lh.blend,
            px: img.into_raw(),
            mask,
            on_mask: false,
            clip: lh.clip,
        });
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

/// The flipbook as an animated GIF: one frame per entry of `frames` (each a layer opacity list
/// for [`Doc::composite_with`]), over white, `fps` frames a second, looping.
pub fn export_gif(doc: &Doc, frames: &[Vec<f32>], fps: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        let delay = image::Delay::from_numer_denom_ms(1000, fps.clamp(1, 60));
        for m in frames {
            let px = doc.composite_with(doc.full(), Some([255, 255, 255]), Some(m));
            let img = image::RgbaImage::from_raw(doc.w, doc.h, px).ok_or("frame")?;
            enc.encode_frame(image::Frame::from_parts(img, 0, 0, delay)).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// A recorded time-lapse as an animated GIF: frames of `w`×`h` RGBA (stored as PNG), the last
/// one held for two seconds.
pub fn export_timelapse(w: u32, h: u32, frames: &[Vec<u8>], fps: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        let n = frames.len();
        for (i, f) in frames.iter().enumerate() {
            let img = image::load_from_memory(f).map_err(|e| e.to_string())?.to_rgba8();
            if img.width() != w || img.height() != h {
                continue;
            }
            let ms = if i + 1 == n { 2000 } else { 1000 / fps.clamp(1, 60) };
            enc.encode_frame(image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(ms, 1))).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// A small copy of a picture (the longer side at most `max`), as PNG: a time-lapse frame.
pub fn thumb_png(w: u32, h: u32, rgba: &[u8], max: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or("frame")?;
    let k = (max as f32 / w.max(h) as f32).min(1.0);
    let (tw, th) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
    let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
    Ok((tw, th, png(tw, th, small.as_raw())?))
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

/// A layered Photoshop file (PSD, uncompressed): every layer with its name, visibility, opacity
/// and blend mode, and the flattened picture.
pub fn export_psd(doc: &Doc) -> Vec<u8> {
    fn be16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn be32(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    let (w, h) = (doc.w, doc.h);
    let n = (w * h) as usize;
    let mut out = Vec::with_capacity(n * 4 * (doc.layers.len() + 1) + 1024);
    out.extend_from_slice(b"8BPS");
    be16(&mut out, 1);
    out.extend_from_slice(&[0; 6]);
    be16(&mut out, 4); // RGB + alpha
    be32(&mut out, h);
    be32(&mut out, w);
    be16(&mut out, 8);
    be16(&mut out, 3); // RGB
    be32(&mut out, 0); // colour mode data
    be32(&mut out, 0); // image resources
    // layer info
    let mut li = Vec::new();
    be16(&mut li, (-(doc.layers.len() as i16)) as u16); // negative: the merged alpha is transparency
    for l in &doc.layers {
        be32(&mut li, 0);
        be32(&mut li, 0);
        be32(&mut li, h);
        be32(&mut li, w);
        be16(&mut li, 4);
        for id in [-1i16, 0, 1, 2] {
            be16(&mut li, id as u16);
            be32(&mut li, 2 + n as u32);
        }
        li.extend_from_slice(b"8BIM");
        let key: &[u8; 4] = match l.blend {
            Blend::Normal => b"norm",
            Blend::Multiply => b"mul ",
            Blend::Screen => b"scrn",
            Blend::Overlay => b"over",
            Blend::Darken => b"dark",
            Blend::Lighten => b"lite",
            Blend::ColorDodge => b"div ",
            Blend::ColorBurn => b"idiv",
            Blend::HardLight => b"hLit",
            Blend::SoftLight => b"sLit",
            Blend::Difference => b"diff",
            Blend::Add => b"lddg",
        };
        li.extend_from_slice(key);
        li.push((l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
        li.push(u8::from(l.clip)); // clipping (1: clipped to the layer under it)
        li.push(if l.visible { 0 } else { 2 });
        li.push(0);
        // extra data: no mask, no blending ranges, the name (Pascal, padded to 4)
        let mut name: Vec<u8> = l.name.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).take(255).collect();
        let mut pas = vec![name.len() as u8];
        pas.append(&mut name);
        while pas.len() % 4 != 0 {
            pas.push(0);
        }
        // and in full (Unicode), which Photoshop shows
        let u16s: Vec<u16> = l.name.encode_utf16().collect();
        let mut luni = Vec::new();
        luni.extend_from_slice(b"8BIMluni");
        let len = 4 + 2 * u16s.len() as u32;
        let pad = len % 4;
        be32(&mut luni, len + if pad > 0 { 4 - pad } else { 0 });
        be32(&mut luni, u16s.len() as u32);
        for c in &u16s {
            be16(&mut luni, *c);
        }
        if pad > 0 {
            luni.extend(std::iter::repeat_n(0u8, (4 - pad) as usize));
        }
        be32(&mut li, 8 + pas.len() as u32 + luni.len() as u32);
        be32(&mut li, 0);
        be32(&mut li, 0);
        li.extend_from_slice(&pas);
        li.extend_from_slice(&luni);
    }
    for l in &doc.layers {
        // a layer mask goes into the layer's transparency
        let px = l.baked();
        for c in [3usize, 0, 1, 2] {
            be16(&mut li, 0); // raw
            li.extend(px.chunks_exact(4).map(|p| p[c]));
        }
    }
    if li.len() % 2 != 0 {
        li.push(0);
    }
    be32(&mut out, li.len() as u32 + 4 + 4);
    be32(&mut out, li.len() as u32);
    out.extend_from_slice(&li);
    be32(&mut out, 0); // global layer mask
    // the flattened picture, planar
    be16(&mut out, 0);
    let flat = doc.flatten(None);
    for c in 0..4 {
        out.extend(flat.chunks_exact(4).map(|p| p[c]));
    }
    out
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
        // a layer mask is kept, also when saved while it is being painted
        d.layers[1].add_mask(None);
        d.layers[1].swap_mask();
        d.layers[1].px[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let e = load_project(&save_project(&d).unwrap()).unwrap();
        assert!(!e.layers[1].on_mask);
        assert_eq!(&e.layers[1].px[0..4], &[10, 20, 30, 200]);
        assert_eq!(&e.layers[1].mask.as_ref().unwrap()[0..4], &[0, 0, 0, 255]);
        assert_eq!(e.layers[1].baked()[3], 0);
        assert_eq!(e.layers[0].mask, None);
        assert!(!e.layers[0].visible);
        assert_eq!(e.layers[1].blend, Blend::Multiply);
        assert!(load_project(&bytes[..bytes.len() - 3]).is_err());
        let p = export_png(&d).unwrap();
        let (w, h, _) = decode_image(&p).unwrap();
        assert_eq!((w, h), (8, 6));
        assert!(export_jpeg(&d).unwrap().starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn psd_layout() {
        let mut d = Doc::new(3, 2, Some([255, 255, 255, 255]), "Háttér", "L");
        d.layers[1].blend = Blend::Multiply;
        let b = export_psd(&d);
        assert_eq!(&b[0..4], b"8BPS");
        assert_eq!(u32::from_be_bytes(b[14..18].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(b[18..22].try_into().unwrap()), 3);
        let lm = u32::from_be_bytes(b[34..38].try_into().unwrap()) as usize;
        // header 26 + 4 + 4 + the layer section's length field and body, then the merged image
        assert_eq!(b.len(), 34 + 4 + lm + 2 + 3 * 2 * 4);
        assert!(b.windows(4).any(|w| w == b"mul "));
    }
}
