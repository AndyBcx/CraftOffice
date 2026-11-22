//! The browser (and Android) build: start the app in the page's canvas, and save by download.

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Start the app in the page (called from `main`).
pub fn start() {
    let _ = eframe::WebLogger::init(log::LevelFilter::Warn);
    wasm_bindgen_futures::spawn_local(async {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
        let Some(canvas) = doc.get_element_by_id("sketch_canvas").and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else { return };
        let result = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(crate::SketchApp::new(cc, None)))))
            .await;
        if let Some(loading) = doc.get_element_by_id("loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_inner_html(&format!("CraftOffice Sketch could not start: {e:?}")),
            }
        }
    });
}

/// Save `bytes` as `name` through a download link (on Android the app hands it to "Save to").
pub fn download(name: &str, bytes: &[u8], mime: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr);
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Ok(a) = doc.create_element("a").and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().map_err(|e| e.into())) {
        a.set_href(&url);
        a.set_download(name);
        if let Some(body) = doc.body() {
            let _ = body.append_child(&a);
            a.click();
            let _ = body.remove_child(&a);
        }
    }
    let url2 = url.clone();
    let cb = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url2);
    });
    if let Some(w) = web_sys::window() {
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), 60_000);
    }
}
