#!/usr/bin/env python3
"""CraftOffice: one "About" for every app. Wherever an app shows its version ("Version 0.19.0",
in its About dialog or page), the CraftOffice version is shown beside it:
"Version 0.19.0 · CraftOffice 2.0.0". Done in egui's text hook, so it reaches every app alike.

Usage: egui_about.py <VERSION file> <widget_text.rs>...  (files that already have the CraftOffice hook)
"""
import sys

version = open(sys.argv[1], encoding="utf-8").read().strip()
assert version and all(c.isalnum() or c in ".-+" for c in version), version

SITES = [
    ("            text: translated_owned(text.into()),",
     "            text: craftoffice_version_note(translated_owned(text.into())),"),
    ("        Self::Text(translate_ui_text(text).unwrap_or(text).to_owned())",
     "        Self::Text(craftoffice_version_note(translate_ui_text(text).unwrap_or(text).to_owned()))"),
    ("        Self::Text(translate_ui_text(text).map_or_else(|| text.clone(), str::to_owned))",
     "        Self::Text(craftoffice_version_note(translate_ui_text(text).map_or_else(|| text.clone(), str::to_owned)))"),
    ("        Self::Text(translated_owned(text))",
     "        Self::Text(craftoffice_version_note(translated_owned(text)))"),
]

CODE = r'''

// ---------- CraftOffice: the suite's version beside each app's own ----------

/// The CraftOffice version these apps belong to.
pub const CRAFTOFFICE_VERSION: &str = "@VERSION@";

/// "Version 0.19.0" (also "Verzió", "Versiune", "Version: …") → "Version 0.19.0 · CraftOffice 2.0.0".
/// Any other text is returned as it is.
fn craftoffice_version_note(s: String) -> String {
    if s.len() > 48 || !s.contains('.') || s.contains("CraftOffice") {
        return s;
    }
    const WORDS: &[&str] = &["Version", "version", "Verzió", "verzió", "Versiune", "versiune", "Ver."];
    let Some(rest) = WORDS.iter().find_map(|w| s.strip_prefix(w)) else {
        return s;
    };
    let rest = rest.trim_start_matches([':', ' ']).trim_start_matches('v');
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let parts: Vec<&str> = num.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return s;
    }
    format!("{s} · CraftOffice {CRAFTOFFICE_VERSION}")
}
'''.replace("@VERSION@", version)

for path in sys.argv[2:]:
    src = open(path, encoding="utf-8").read()
    if "craftoffice_version_note" in src:
        print(f"{path}: already done")
        continue
    n = 0
    for old, new in SITES:
        if old in src:
            src = src.replace(old, new, 1)
            n += 1
    if n == 0:
        print(f"{path}: no CraftOffice hook here; left alone")
        continue
    open(path, "w", encoding="utf-8").write(src + CODE)
    print(f"{path}: CraftOffice {version} beside the apps' versions ({n} places)")
