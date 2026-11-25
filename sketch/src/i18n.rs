//! The interface languages: English, Hungarian, Romanian. The launcher passes `CRAFTOFFICE_LANG`;
//! otherwise the system (or browser) language decides, and View › Language changes it.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En = 0,
    Hu = 1,
    Ro = 2,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::Hu,
        2 => Lang::Ro,
        _ => Lang::En,
    }
}

/// The language for a locale tag such as `hu-HU` or `ro`.
pub fn from_tag(tag: &str) -> Lang {
    let t = tag.trim().to_ascii_lowercase();
    if t.starts_with("hu") {
        Lang::Hu
    } else if t.starts_with("ro") {
        Lang::Ro
    } else {
        Lang::En
    }
}

/// The starting language: `CRAFTOFFICE_LANG`, then the system's.
pub fn detect() -> Lang {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(l) = std::env::var("CRAFTOFFICE_LANG") {
            return from_tag(&l);
        }
        sys_locale::get_locale().map_or(Lang::En, |l| from_tag(&l))
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().and_then(|w| w.navigator().language()).map_or(Lang::En, |l| from_tag(&l))
    }
}

/// `s` in the interface language (English when there is no translation).
pub fn t(s: &'static str) -> &'static str {
    let lang = current();
    if lang == Lang::En {
        return s;
    }
    for (en, hu, ro) in TABLE {
        if *en == s {
            return if lang == Lang::Hu { hu } else { ro };
        }
    }
    s
}

/// English, Hungarian, Romanian.
const TABLE: &[(&str, &str, &str)] = &[
    // menus
    ("File", "Fájl", "Fișier"),
    ("Edit", "Szerkesztés", "Editare"),
    ("Image", "Kép", "Imagine"),
    ("Layer", "Réteg", "Strat"),
    ("View", "Nézet", "Vizualizare"),
    ("Help", "Súgó", "Ajutor"),
    ("New…", "Új…", "Nou…"),
    ("Open…", "Megnyitás…", "Deschidere…"),
    ("Save", "Mentés", "Salvare"),
    ("Save As…", "Mentés másként…", "Salvare ca…"),
    ("Import as Layer…", "Importálás rétegként…", "Importare ca strat…"),
    ("Export PNG…", "Exportálás PNG-be…", "Exportare PNG…"),
    ("Export JPEG…", "Exportálás JPEG-be…", "Exportare JPEG…"),
    ("Quit", "Kilépés", "Ieșire"),
    ("Undo", "Visszavonás", "Anulare"),
    ("Redo", "Mégis", "Refacere"),
    ("Clear Layer", "Réteg törlése", "Golire strat"),
    ("Fill Layer with Colour", "Réteg kitöltése színnel", "Umplere strat cu culoare"),
    ("Flip Horizontal", "Vízszintes tükrözés", "Oglindire orizontală"),
    ("Flip Vertical", "Függőleges tükrözés", "Oglindire verticală"),
    ("Rotate 90° Clockwise", "Forgatás 90°-kal jobbra", "Rotire 90° în sens orar"),
    ("Rotate 90° Counter-clockwise", "Forgatás 90°-kal balra", "Rotire 90° în sens antiorar"),
    ("Canvas Size…", "Vászonméret…", "Dimensiune pânză…"),
    ("New Layer", "Új réteg", "Strat nou"),
    ("Duplicate Layer", "Réteg kettőzése", "Duplicare strat"),
    ("Delete Layer", "Réteg törlése a listából", "Ștergere strat"),
    ("Merge Down", "Egyesítés az alatta lévővel", "Îmbinare în jos"),
    ("Add Layer Mask", "Rétegmaszk hozzáadása", "Adăugare mască de strat"),
    ("Layer Mask from Selection", "Rétegmaszk a kijelölésből", "Mască de strat din selecție"),
    ("Edit Layer Mask", "Rétegmaszk szerkesztése", "Editare mască de strat"),
    ("Edit Layer", "Réteg szerkesztése", "Editare strat"),
    ("Apply Layer Mask", "Rétegmaszk alkalmazása", "Aplicare mască de strat"),
    ("Delete Layer Mask", "Rétegmaszk törlése", "Ștergere mască de strat"),
    ("Painting the layer mask: black hides, white shows.", "A rétegmaszkot festesz: a fekete elrejt, a fehér megmutat.", "Pictezi masca stratului: negrul ascunde, albul arată."),
    ("Move Up", "Feljebb", "Mutare în sus"),
    ("Move Down", "Lejjebb", "Mutare în jos"),
    ("Flatten Image", "Kép összevonása", "Aplatizare imagine"),
    ("Zoom In", "Nagyítás", "Mărire"),
    ("Zoom Out", "Kicsinyítés", "Micșorare"),
    ("Fit to Window", "Ablakhoz igazítás", "Potrivire în fereastră"),
    ("Actual Size", "Valódi méret", "Dimensiune reală"),
    ("Panels", "Panelek", "Panouri"),
    ("Language", "Nyelv", "Limbă"),
    ("About CraftOffice Sketch", "A CraftOffice Sketch névjegye", "Despre CraftOffice Sketch"),
    ("Keyboard Shortcuts", "Billentyűparancsok", "Comenzi rapide"),
    // tools
    ("Pencil", "Ceruza", "Creion"),
    ("Pen", "Toll", "Stilou"),
    ("Brush", "Ecset", "Pensulă"),
    ("Marker", "Filctoll", "Marker"),
    ("Airbrush", "Festékszóró", "Aerograf"),
    ("Eraser", "Radír", "Radieră"),
    ("Fill", "Kitöltés", "Umplere"),
    ("Eyedropper", "Pipetta", "Pipetă"),
    ("Line", "Vonal", "Linie"),
    ("Rectangle", "Téglalap", "Dreptunghi"),
    ("Ellipse", "Ellipszis", "Elipsă"),
    ("Hand", "Kéz (mozgatás)", "Mână (deplasare)"),
    // panels
    ("Colour", "Szín", "Culoare"),
    ("Recent colours", "Legutóbbi színek", "Culori recente"),
    ("Tool", "Eszköz", "Instrument"),
    ("Size", "Méret", "Dimensiune"),
    ("Opacity", "Átlátszatlanság", "Opacitate"),
    ("Hardness", "Keménység", "Duritate"),
    ("Smoothing", "Simítás", "Netezire"),
    ("Pressure changes size", "A nyomás a méretet változtatja", "Presiunea schimbă dimensiunea"),
    ("Pressure changes opacity", "A nyomás az átlátszatlanságot változtatja", "Presiunea schimbă opacitatea"),
    ("Tolerance", "Tűrés", "Toleranță"),
    ("Filled shape", "Kitöltött alakzat", "Formă plină"),
    ("Layers", "Rétegek", "Straturi"),
    ("Background", "Háttér", "Fundal"),
    ("Visible", "Látható", "Vizibil"),
    ("Rename", "Átnevezés", "Redenumire"),
    // dialogs
    ("New Drawing", "Új rajz", "Desen nou"),
    ("Width", "Szélesség", "Lățime"),
    ("Height", "Magasság", "Înălțime"),
    ("White background", "Fehér háttér", "Fundal alb"),
    ("Transparent background", "Átlátszó háttér", "Fundal transparent"),
    ("Create", "Létrehozás", "Creare"),
    ("Cancel", "Mégse", "Anulare"),
    ("Apply", "Alkalmaz", "Aplicare"),
    ("Close", "Bezárás", "Închidere"),
    ("pixels", "képpont", "pixeli"),
    ("Unsaved changes", "Nem mentett változások", "Modificări nesalvate"),
    ("Save the drawing before closing?", "Mentsük a rajzot bezárás előtt?", "Salvați desenul înainte de închidere?"),
    ("Don't Save", "Nincs mentés", "Nu salva"),
    ("Untitled", "Névtelen", "Fără titlu"),
    ("Saved", "Mentve", "Salvat"),
    ("Exported", "Exportálva", "Exportat"),
    ("Could not open the file", "A fájl nem nyitható meg", "Fișierul nu poate fi deschis"),
    ("Could not save the file", "A fájl nem menthető", "Fișierul nu poate fi salvat"),
    ("Drawing and sketching with layers, brushes and shapes.", "Rajzolás és vázlatkészítés rétegekkel, ecsetekkel és alakzatokkal.", "Desen și schițe cu straturi, pensule și forme."),
    ("CraftOffice edition by AndyBcx: Hungarian and Romanian interface.", "CraftOffice-kiadás – AndyBcx: magyar és román felület.", "Ediția CraftOffice de AndyBcx: interfață în maghiară și română."),
    ("Version", "Verzió", "Versiune"),
    ("Anchor", "Igazítás", "Ancorare"),
    ("Drop to open", "Engedd el a megnyitáshoz", "Eliberați pentru a deschide"),
    ("Layer", "Réteg", "Strat"),
    ("Shortcuts", "Billentyűk", "Taste"),
    ("Brush size", "Ecsetméret", "Dimensiune pensulă"),
    ("Pan", "Mozgatás", "Deplasare"),
    ("Zoom", "Nagyítás", "Zoom"),
    ("Swap colours", "Színcsere", "Schimbare culori"),
    ("Secondary colour", "Másodlagos szín", "Culoare secundară"),
    ("Lasso", "Lasszó", "Lasou"),
    ("Magic Wand", "Varázspálca", "Baghetă magică"),
    ("Contiguous", "Összefüggő", "Contiguu"),
    ("Shift: add to the selection · Alt: take away from it", "Shift: hozzáadás a kijelöléshez · Alt: elvétel belőle", "Shift: adaugă la selecție · Alt: scade din ea"),
    ("Invert Selection", "Kijelölés megfordítása", "Inversare selecție"),
    ("Export PSD (layers)…", "Exportálás PSD-be (rétegekkel)…", "Export PSD (cu straturi)…"),
    ("Filters", "Szűrők", "Filtre"),
    ("Blur", "Elmosás", "Estompare"),
    ("Sharpen", "Élesítés", "Clarificare"),
    ("Brightness / Contrast", "Fényerő / kontraszt", "Luminozitate / contrast"),
    ("Hue / Saturation", "Színárnyalat / telítettség", "Nuanță / saturație"),
    ("Invert Colours", "Színek megfordítása", "Inversare culori"),
    ("Black and White", "Fekete-fehér", "Alb-negru"),
    ("Radius", "Sugár", "Rază"),
    ("Amount", "Mérték", "Intensitate"),
    ("Brightness", "Fényerő", "Luminozitate"),
    ("Contrast", "Kontraszt", "Contrast"),
    ("Hue", "Színárnyalat", "Nuanță"),
    ("Saturation", "Telítettség", "Saturație"),
    ("Lightness", "Világosság", "Luminozitate"),
    ("On the active layer, inside the selection.", "Az aktív rétegen, a kijelölésen belül.", "Pe stratul activ, în interiorul selecției."),
    ("On the active layer.", "Az aktív rétegen.", "Pe stratul activ."),
    ("Select", "Kijelölés", "Selecție"),
    ("Move", "Áthelyezés", "Mutare"),
    ("Gradient", "Színátmenet", "Degrade"),
    ("Text", "Szöveg", "Text"),
    ("Brush tip", "Ecsethegy", "Vârf pensulă"),
    ("Round", "Kerek", "Rotund"),
    ("Square", "Szögletes", "Pătrat"),
    ("Calligraphy", "Kalligráfia", "Caligrafie"),
    ("Chalk", "Kréta", "Cretă"),
    ("Spray", "Szórófej", "Spray"),
    ("Normal", "Normál", "Normal"),
    ("Multiply", "Szorzás", "Multiplicare"),
    ("Screen", "Képernyő", "Ecran"),
    ("Overlay", "Átfedés", "Suprapunere"),
    ("Darken", "Sötétítés", "Întunecare"),
    ("Lighten", "Világosítás", "Luminare"),
    ("Color Dodge", "Színfakítás", "Estompare culoare"),
    ("Color Burn", "Színégetés", "Ardere culoare"),
    ("Hard Light", "Kemény fény", "Lumină puternică"),
    ("Soft Light", "Lágy fény", "Lumină difuză"),
    ("Difference", "Különbség", "Diferență"),
    ("Add", "Összeadás", "Adunare"),
    ("Linear", "Lineáris", "Liniar"),
    ("Radial", "Sugaras", "Radial"),
    ("Colour to secondary colour", "Színből a másodlagos színbe", "Din culoare în culoarea secundară"),
    ("Colour to transparent", "Színből átlátszóba", "Din culoare în transparent"),
    ("Drag on the canvas to lay the gradient.", "Húzd végig a vásznon a színátmenethez.", "Trageți pe pânză pentru a aplica degradeul."),
    ("Drag to select a rectangle; painting stays inside it. Click to deselect.", "Húzással téglalapot jelölhetsz ki; a festés a kijelölésen belül marad. Kattintással megszünteted.", "Trageți pentru a selecta un dreptunghi; pictura rămâne în interior. Clic pentru deselectare."),
    ("Drag to move the selection, or the whole layer when nothing is selected.", "Húzással mozgatod a kijelölést, vagy ha nincs kijelölés, az egész réteget.", "Trageți pentru a muta selecția sau întregul strat, dacă nu există selecție."),
    ("Click on the canvas where the text should go.", "Kattints a vásznon oda, ahová a szöveget szeretnéd.", "Faceți clic pe pânză unde doriți textul."),
    ("Click or drag on the canvas to move it. The colour is the current colour.", "Kattintással vagy húzással a vásznon áthelyezheted. A szín az aktuális szín.", "Faceți clic sau trageți pe pânză pentru a-l muta. Culoarea este culoarea curentă."),
    ("Type the text here", "Ide írd a szöveget", "Scrieți textul aici"),
    ("Font", "Betűtípus", "Font"),
    ("Load Font…", "Betűtípus betöltése…", "Încărcare font…"),
    ("Left", "Balra", "Stânga"),
    ("Centre", "Középre", "Centru"),
    ("Right", "Jobbra", "Dreapta"),
    ("Place", "Elhelyezés", "Plasare"),
    ("Cut", "Kivágás", "Decupare"),
    ("Copy", "Másolás", "Copiere"),
    ("Paste as New Layer", "Beillesztés új rétegként", "Lipire ca strat nou"),
    ("Delete Selection", "Kijelölés törlése", "Ștergere selecție"),
    ("Select All", "Összes kijelölése", "Selectare tot"),
    ("Deselect", "Kijelölés megszüntetése", "Deselectare"),
    ("Crop to Selection", "Körbevágás a kijelölésre", "Decupare la selecție"),
    ("Pasted", "Beillesztett", "Lipit"),
    ("2H pencil", "2H ceruza", "Creion 2H"),
    ("HB pencil", "HB ceruza", "Creion HB"),
    ("2B pencil", "2B ceruza", "Creion 2B"),
    ("6B pencil", "6B ceruza", "Creion 6B"),
    ("Mechanical pencil 0.5", "Töltőceruza 0,5", "Creion mecanic 0,5"),
    ("Coloured pencil", "Színes ceruza", "Creion colorat"),
    ("Charcoal", "Szén", "Cărbune"),
    ("Pixel pencil", "Pixelceruza", "Creion pixel"),
    ("Fineliner 0.3", "Tűfilc 0,3", "Liner 0,3"),
    ("Technical pen", "Rajztoll", "Rapidograf"),
    ("Ballpoint", "Golyóstoll", "Pix"),
    ("Gel pen", "Zselés toll", "Pix cu gel"),
    ("Ink pen", "Tustoll", "Stilou cu tuș"),
    ("Brush pen", "Ecsetfilc", "Pensulă-marker"),
    ("Calligraphy nib", "Kalligráfiai hegy", "Peniță caligrafică"),
    ("Felt-tip pen", "Filctoll", "Carioca"),
    ("Graphite", "Grafit", "Grafit"),
    ("Pencils", "Ceruzák", "Creioane"),
    ("Pens", "Tollak", "Stilouri"),
    ("Taper", "Elvékonyodó kezdet", "Subțiere la început"),
    ("Thinner when fast", "Gyorsan vékonyabb", "Mai subțire când e rapid"),
    ("Predictive stroke", "Prediktív vonás", "Trasare predictivă"),
    ("Straight lines, circles, ellipses, triangles and rectangles drawn by hand come out clean.", "A kézzel húzott egyenesek, körök, ellipszisek, háromszögek és téglalapok szabályosak lesznek.", "Liniile drepte, cercurile, elipsele, triunghiurile și dreptunghiurile desenate de mână devin precise."),
    ("Snap lines to 0°, 45° and 90°", "Egyenesek igazítása 0°, 45° és 90°-ra", "Aliniere linii la 0°, 45° și 90°"),
    ("Guides", "Segédvonalak", "Ghidaje"),
    ("Symmetry", "Szimmetria", "Simetrie"),
    ("Off", "Ki", "Oprit"),
    ("Left–right", "Bal–jobb", "Stânga–dreapta"),
    ("Top–bottom", "Fent–lent", "Sus–jos"),
    ("Four ways", "Négyirányú", "În patru"),
    ("Free angle", "Szabad szög", "Unghi liber"),
    ("Kaleidoscope", "Kaleidoszkóp", "Caleidoscop"),
    ("Sectors", "Szeletek", "Sectoare"),
    ("Angle", "Szög", "Unghi"),
    ("Centre on the canvas", "Középre a vásznon", "Centrare pe pânză"),
    ("Perspective", "Perspektíva", "Perspectivă"),
    ("1-point", "1 pontos", "Cu 1 punct"),
    ("2-point", "2 pontos", "Cu 2 puncte"),
    ("3-point", "3 pontos", "Cu 3 puncte"),
    ("Snap strokes to the perspective", "Vonások igazítása a perspektívához", "Aliniere trasări la perspectivă"),
    ("Reset vanishing points", "Iránypontok visszaállítása", "Resetare puncte de fugă"),
    ("Ruler", "Vonalzó", "Riglă"),
    ("Strokes begun near the ruler follow its edge.", "A vonalzó közelében kezdett vonás a vonalzó élét követi.", "Trasările începute lângă riglă îi urmează marginea."),
    ("Ellipse guide", "Ellipszis-sablon", "Șablon elipsă"),
    ("Strokes begun near the ellipse follow it.", "Az ellipszis közelében kezdett vonás az ellipszist követi.", "Trasările începute lângă elipsă o urmează."),
    ("Drag the round handles to move the guides.", "A segédvonalakat a kerek fogantyúkkal mozgathatod.", "Mutați ghidajele trăgând de mânerele rotunde."),
    ("Transform", "Átalakítás", "Transformare"),
    ("Drag inside to move, a corner to resize, outside to turn. Ctrl+corner: distort. Shift: from the centre / in 15° steps.", "Belül húzva mozgat, a sarkot húzva átméretez, kívül húzva forgat. Ctrl+sarok: torzítás. Shift: középről / 15°-os lépésekben.", "Trageți în interior pentru mutare, de un colț pentru redimensionare, în exterior pentru rotire. Ctrl+colț: deformare. Shift: din centru / în pași de 15°."),
    ("Lasso Fill", "Lasszós kitöltés", "Umplere lasou"),
    ("Draw around an area to fill it with the colour.", "Rajzold körbe a területet, és kitölti a színnel.", "Desenați în jurul unei zone pentru a o umple cu culoarea."),
    ("Draw Outline of Selection", "Kijelölés körvonalának megrajzolása", "Desenare contur selecție"),
    ("Clipping Mask", "Vágómaszk", "Mască de decupare"),
    ("Release Clipping Mask", "Vágómaszk megszüntetése", "Eliberare mască de decupare"),
    ("Random colour", "Véletlen szín", "Culoare aleatorie"),
    ("Colour library", "Színkönyvtár", "Bibliotecă de culori"),
    ("Cool greys", "Hideg szürkék", "Griuri reci"),
    ("Warm greys", "Meleg szürkék", "Griuri calde"),
    ("Neutral greys", "Semleges szürkék", "Griuri neutre"),
    ("Skin tones", "Bőrtónusok", "Tonuri de piele"),
    ("Reds", "Vörösek", "Roșii"),
    ("Oranges", "Narancsok", "Portocalii"),
    ("Yellows", "Sárgák", "Galbene"),
    ("Yellow greens", "Sárgászöldek", "Verde-gălbui"),
    ("Greens", "Zöldek", "Verzi"),
    ("Blue greens", "Kékeszöldek", "Verde-albăstrui"),
    ("Blues", "Kékek", "Albastre"),
    ("Blue violets", "Kékeslilák", "Albastru-violet"),
    ("Violets", "Lilák", "Violete"),
    ("Pinks", "Rózsaszínek", "Roz"),
    ("Earth tones", "Földszínek", "Tonuri de pământ"),
    ("Rotate View Left", "Nézet forgatása balra", "Rotire vizualizare la stânga"),
    ("Rotate View Right", "Nézet forgatása jobbra", "Rotire vizualizare la dreapta"),
    ("Reset View Rotation", "Nézet forgatásának visszaállítása", "Resetare rotire vizualizare"),
    ("Flipbook (animation)", "Lapozó (animáció)", "Flipbook (animație)"),
    ("Flipbook", "Lapozó", "Flipbook"),
    ("Frame", "Képkocka", "Cadru"),
    ("Previous frame", "Előző képkocka", "Cadrul anterior"),
    ("Next frame", "Következő képkocka", "Cadrul următor"),
    ("Play", "Lejátszás", "Redare"),
    ("Stop", "Leállítás", "Oprire"),
    ("Frames per second", "Képkocka/másodperc", "Cadre pe secundă"),
    ("Onion skin", "Hagymahéj", "Piele de ceapă"),
    ("Bottom layer is the background of every frame", "Az alsó réteg minden képkocka háttere", "Stratul de jos e fundalul fiecărui cadru"),
    ("New Frame", "Új képkocka", "Cadru nou"),
    ("Duplicate Frame", "Képkocka kettőzése", "Duplicare cadru"),
    ("Each layer is a frame; File › Export Animated GIF saves the animation.", "Minden réteg egy képkocka; a Fájl › Animált GIF exportálása elmenti az animációt.", "Fiecare strat este un cadru; Fișier › Exportare GIF animat salvează animația."),
    ("Export Animated GIF…", "Animált GIF exportálása…", "Exportare GIF animat…"),
    ("The flipbook's frames (the layers) as an animation.", "A lapozó képkockái (a rétegek) animációként.", "Cadrele flipbook-ului (straturile) ca animație."),
    ("Import Pictures as Frames…", "Képek importálása képkockaként…", "Importare imagini ca cadre…"),
    ("A picture sequence becomes the flipbook's frames.", "Egy képsorozatból a lapozó képkockái lesznek.", "O secvență de imagini devine cadrele flipbook-ului."),
    ("Record Time-lapse", "Time-lapse felvétel", "Înregistrare time-lapse"),
    ("Every stroke is recorded; export the drawing's making as an animation.", "Minden vonást rögzít; a rajz elkészülését animációként exportálhatod.", "Fiecare trasare este înregistrată; exportați realizarea desenului ca animație."),
    ("Export Time-lapse (GIF)…", "Time-lapse exportálása (GIF)…", "Exportare time-lapse (GIF)…"),
    ("Curve ruler", "Görbevonalzó", "Riglă curbă"),
    ("A French curve: strokes begun near it follow the curve.", "Francia görbe: a közelében kezdett vonás a görbét követi.", "Florar: trasările începute lângă el urmează curba."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_and_lookup() {
        assert_eq!(from_tag("hu-HU"), Lang::Hu);
        assert_eq!(from_tag("ro_RO"), Lang::Ro);
        assert_eq!(from_tag("de"), Lang::En);
        set(Lang::Hu);
        assert_eq!(t("File"), "Fájl");
        assert_eq!(t("not translated"), "not translated");
        set(Lang::Ro);
        assert_eq!(t("Eraser"), "Radieră");
        set(Lang::En);
        assert_eq!(t("Eraser"), "Eraser");
    }
}
