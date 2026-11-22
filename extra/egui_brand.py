#!/usr/bin/env python3
"""CraftOffice: in the browser (and Android) builds the apps start without the desktop program's
start-up code, so their interface translator (and with it the CraftOffice names) is never
installed. This gives egui's text hook a default for that case: the apps' original names
(GridCraft, LightCraft…) are shown as their CraftOffice names, with Hungarian case endings.

Usage: egui_brand.py <widget_text.rs>...  (files that already have the CraftOffice hook)
"""
import os
import sys

OLD = """    match UI_TEXT_TRANSLATOR.get() {
        Some(f) if !s.is_empty() => f(s),
        _ => None,
    }"""

NEW = """    match UI_TEXT_TRANSLATOR.get() {
        Some(f) if !s.is_empty() => f(s),
        Some(_) => None,
        None => craftoffice_default_brand(s),
    }"""

CODE = r'''

// ---------- CraftOffice: the apps' names when no translator is installed (web builds) ----------

/// (original name, CraftOffice name, the last word's Hungarian case forms: accusative, -ban,
/// -ból, -tól, -val, -nak, -hoz, -ra, -on, -ról, -ba, -ig)
const CRAFTOFFICE_NAMES: &[(&str, &str, [&str; 12])] = &[
    ("WordCraft", "CraftOffice Writer", ["Writert", "Writerben", "Writerből", "Writertől", "Writerrel", "Writernek", "Writerhez", "Writerre", "Writeren", "Writerről", "Writerbe", "Writerig"]),
    ("GridCraft", "CraftOffice Sheets", ["Sheetset", "Sheetsben", "Sheetsből", "Sheetstől", "Sheetsszel", "Sheetsnek", "Sheetshez", "Sheetsre", "Sheetsen", "Sheetsről", "Sheetsbe", "Sheetsig"]),
    ("DeckCraft", "CraftOffice Slides", ["Slidesot", "Slidesban", "Slidesból", "Slidestól", "Slidesszal", "Slidesnak", "Slideshoz", "Slidesra", "Slideson", "Slidesról", "Slidesba", "Slidesig"]),
    ("DesignCraft", "CraftOffice Layout", ["Layoutot", "Layoutban", "Layoutból", "Layouttól", "Layouttal", "Layoutnak", "Layouthoz", "Layoutra", "Layouton", "Layoutról", "Layoutba", "Layoutig"]),
    ("PdfCraft", "CraftOffice PDF", ["PDF-et", "PDF-ben", "PDF-ből", "PDF-től", "PDF-fel", "PDF-nek", "PDF-hez", "PDF-re", "PDF-en", "PDF-ről", "PDF-be", "PDF-ig"]),
    ("PDFCraft", "CraftOffice PDF", ["PDF-et", "PDF-ben", "PDF-ből", "PDF-től", "PDF-fel", "PDF-nek", "PDF-hez", "PDF-re", "PDF-en", "PDF-ről", "PDF-be", "PDF-ig"]),
    ("PhotoCraft", "CraftOffice Photo", ["Photót", "Photóban", "Photóból", "Photótól", "Photóval", "Photónak", "Photóhoz", "Photóra", "Photón", "Photóról", "Photóba", "Photóig"]),
    ("VectorCraft", "CraftOffice Vector", ["Vectort", "Vectorban", "Vectorból", "Vectortól", "Vectorral", "Vectornak", "Vectorhoz", "Vectorra", "Vectoron", "Vectorról", "Vectorba", "Vectorig"]),
    ("LightCraft", "CraftOffice Darkroom", ["Darkroomot", "Darkroomban", "Darkroomból", "Darkroomtól", "Darkroommal", "Darkroomnak", "Darkroomhoz", "Darkroomra", "Darkroomon", "Darkroomról", "Darkroomba", "Darkroomig"]),
    ("FilmCraft", "CraftOffice Video", ["Videót", "Videóban", "Videóból", "Videótól", "Videóval", "Videónak", "Videóhoz", "Videóra", "Videón", "Videóról", "Videóba", "Videóig"]),
    ("EffectCraft", "CraftOffice Motion", ["Motiont", "Motionben", "Motionből", "Motiontől", "Motionnel", "Motionnek", "Motionhöz", "Motionre", "Motionön", "Motionről", "Motionbe", "Motionig"]),
    ("SoundCraft", "CraftOffice Audio", ["Audiót", "Audióban", "Audióból", "Audiótól", "Audióval", "Audiónak", "Audióhoz", "Audióra", "Audión", "Audióról", "Audióba", "Audióig"]),
    ("AniCraft", "CraftOffice Animation", ["Animationt", "Animationben", "Animationből", "Animationtől", "Animationnel", "Animationnek", "Animationhöz", "Animationre", "Animationön", "Animationről", "Animationbe", "Animationig"]),
    ("FigCraft", "CraftOffice Prototype", ["Prototype-ot", "Prototype-ban", "Prototype-ból", "Prototype-tól", "Prototype-pal", "Prototype-nak", "Prototype-hoz", "Prototype-ra", "Prototype-on", "Prototype-ról", "Prototype-ba", "Prototype-ig"]),
    ("CADCraft", "CraftOffice CAD", ["CAD-et", "CAD-ben", "CAD-ből", "CAD-től", "CAD-del", "CAD-nek", "CAD-hez", "CAD-re", "CAD-en", "CAD-ről", "CAD-be", "CAD-ig"]),
    ("CadCraft", "CraftOffice CAD", ["CAD-et", "CAD-ben", "CAD-ből", "CAD-től", "CAD-del", "CAD-nek", "CAD-hez", "CAD-re", "CAD-en", "CAD-ről", "CAD-be", "CAD-ig"]),
    ("MaterialCraft", "CraftOffice Material", ["Materialt", "Materialben", "Materialből", "Materialtől", "Materiallel", "Materialnek", "Materialhez", "Materialre", "Materialen", "Materialről", "Materialbe", "Materialig"]),
    ("TextureCraft", "CraftOffice Texture", ["Texture-t", "Texture-ben", "Texture-ből", "Texture-től", "Texture-rel", "Texture-nek", "Texture-höz", "Texture-re", "Texture-ön", "Texture-ről", "Texture-be", "Texture-ig"]),
    ("Craft Anything", "CraftOffice Workshop", ["Workshopot", "Workshopban", "Workshopból", "Workshoptól", "Workshoppal", "Workshopnak", "Workshophoz", "Workshopra", "Workshopon", "Workshopról", "Workshopba", "Workshopig"]),
];

const CRAFTOFFICE_SUFFIX: &[(&str, usize)] = &[
    ("ot", 0), ("et", 0), ("t", 0), ("ban", 1), ("ben", 1), ("ból", 2), ("ből", 2), ("tól", 3), ("től", 3),
    ("tal", 4), ("val", 4), ("vel", 4), ("nak", 5), ("nek", 5), ("hoz", 6), ("hez", 6), ("ra", 7), ("re", 7),
    ("on", 8), ("en", 8), ("ról", 9), ("ről", 9), ("ba", 10), ("be", 10), ("ig", 11),
];

fn craftoffice_brand_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut rest = s;
    'outer: while !rest.is_empty() {
        for (orig, new, forms) in CRAFTOFFICE_NAMES {
            if let Some(after) = rest.strip_prefix(orig) {
                let tail_len: usize =
                    after.char_indices().take_while(|(_, c)| c.is_alphabetic()).map(|(i, c)| i + c.len_utf8()).last().unwrap_or(0);
                let suffix = &after[..tail_len];
                if suffix.is_empty() {
                    out.push_str(new);
                } else if let Some((_, case)) = CRAFTOFFICE_SUFFIX.iter().find(|(k, _)| *k == suffix) {
                    if let Some((base, _)) = new.rsplit_once(' ') {
                        out.push_str(base);
                        out.push(' ');
                    }
                    out.push_str(forms.get(*case).copied().unwrap_or(new));
                } else {
                    out.push_str(new);
                    out.push('-');
                    out.push_str(suffix);
                }
                rest = &after[tail_len..];
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/// `s` with the CraftOffice names, or `None` when it names no app. Each changed text is made once
/// and kept (the interface's texts are few).
fn craftoffice_default_brand(s: &str) -> Option<&'static str> {
    if !(s.contains("Craft") || s.contains("Craft Anything")) {
        return None;
    }
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Option<&'static str>>>> = std::sync::Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let cache = guard.get_or_insert_with(std::collections::HashMap::new);
    if let Some(v) = cache.get(s) {
        return *v;
    }
    let branded = craftoffice_brand_string(s);
    let v: Option<&'static str> = (branded != s).then(|| &*Box::leak(branded.into_boxed_str()));
    if cache.len() < 20_000 {
        cache.insert(s.to_owned(), v);
    }
    v
}

/// CraftOffice: text painted straight onto the screen (`Painter::text`, `layout…`): the apps'
/// own names as their CraftOffice names (a start page's or title bar's name).
pub(crate) fn craftoffice_paint_text(s: String) -> String {
    match craftoffice_default_brand(&s) {
        Some(b) => b.to_owned(),
        None => s,
    }
}
'''

PAINTER_OLD = [
    "self.fonts_mut(|f| f.layout(text, font_id, color, wrap_width))",
    "self.fonts_mut(|f| f.layout(text, font_id, color, f32::INFINITY))",
]

for path in sys.argv[1:]:
    src = open(path, encoding="utf-8").read()
    if "craftoffice_default_brand" in src:
        print(f"{path}: already done")
        continue
    if OLD not in src:
        print(f"{path}: no CraftOffice hook here; left alone")
        continue
    src = src.replace(OLD, NEW, 1) + CODE
    open(path, "w", encoding="utf-8").write(src)
    print(f"{path}: CraftOffice names by default")
    painter = os.path.join(os.path.dirname(path), "painter.rs")
    if os.path.isfile(painter):
        p = open(painter, encoding="utf-8").read()
        n = 0
        for old in PAINTER_OLD:
            if old in p:
                p = p.replace(old, "{ let text = crate::widget_text::craftoffice_paint_text(text); " + old + " }")
                n += 1
        open(painter, "w", encoding="utf-8").write(p)
        print(f"{painter}: {n} painted-text hooks")
