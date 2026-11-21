//! CraftOffice on Android. The app is the CraftOffice program itself, built from its Rust source to
//! WebAssembly; this library is the small local web server the app's WebView loads it from, as in
//! Vormantis: Kotlin calls `Engine.start(port)` once, and the WebView opens `http://127.0.0.1:port/`.
//!
//! The site folder (the app's files, unpacked from the APK) comes in the environment variable
//! `CRAFTOFFICE_SITE`, set by the app with `Os.setenv` before the call, which keeps this boundary free
//! of JNI string handling. Only that folder is served, only to this device (127.0.0.1), and with the
//! headers that let the apps use threads and shared memory (cross-origin isolation).

use std::ffi::c_void;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

static START: Once = Once::new();

/// `app.craftoffice.Engine.start(port)`: start the server (once) and return the port it listens on.
#[no_mangle]
pub extern "system" fn Java_app_craftoffice_Engine_start(_env: *mut c_void, _class: *mut c_void, port: i32) -> i32 {
    START.call_once(|| {
        let root = std::env::var_os("CRAFTOFFICE_SITE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("site"));
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port as u16)) {
            let _ = std::thread::Builder::new().name("craftoffice-server".into()).spawn(move || serve(listener, root));
        }
    });
    port
}

/// Answer requests until the process ends, one thread per connection.
pub fn serve(listener: TcpListener, root: PathBuf) {
    for stream in listener.incoming().flatten() {
        let root = root.clone();
        let _ = std::thread::Builder::new().name("craftoffice-conn".into()).stack_size(256 << 10).spawn(move || {
            let _ = connection(stream, &root);
        });
    }
}

fn connection(stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    // Keep-alive: the WebView loads many files over few connections.
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
        let mut close = false;
        let mut body_len = 0usize;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 {
                return Ok(());
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            let lower = h.to_ascii_lowercase();
            if lower == "connection: close" {
                close = true;
            } else if let Some(v) = lower.strip_prefix("content-length:") {
                body_len = v.trim().parse().unwrap_or(0);
            }
        }
        if body_len > 0 {
            std::io::copy(&mut (&mut reader).take(body_len as u64), &mut std::io::sink())?;
        }
        respond(&mut out, root, method, target)?;
        if close {
            return Ok(());
        }
    }
}

fn respond(out: &mut TcpStream, root: &Path, method: &str, target: &str) -> std::io::Result<()> {
    if method != "GET" && method != "HEAD" {
        return send(out, "405 Method Not Allowed", "text/plain", b"method not allowed", method == "HEAD");
    }
    let Some(path) = resolve(root, target) else {
        return send(out, "404 Not Found", "text/plain", b"not found", method == "HEAD");
    };
    match std::fs::read(&path) {
        Ok(bytes) => send(out, "200 OK", content_type(&path), &bytes, method == "HEAD"),
        Err(_) => send(out, "404 Not Found", "text/plain", b"not found", method == "HEAD"),
    }
}

/// The file for a request target inside `root`: no `..`, a folder means its `index.html`.
pub fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let path = percent_decode(path)?;
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let mut full = root.join(rel);
    if path.ends_with('/') || full.is_dir() {
        full = full.join("index.html");
    }
    full.is_file().then_some(full)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" | "tsv" | "md" => "text/plain; charset=utf-8",
        "gz" => "application/gzip",
        _ => "application/octet-stream",
    }
}

fn send(out: &mut TcpStream, status: &str, ctype: &str, body: &[u8], head: bool) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: require-corp\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    out.write_all(header.as_bytes())?;
    if !head {
        out.write_all(body)?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_files_inside_the_site_only() {
        let dir = std::env::temp_dir().join(format!("co-site-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("a b")).unwrap();
        std::fs::write(dir.join("index.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("a b/x.wasm"), [0, 97, 115, 109]).unwrap();
        assert_eq!(resolve(&dir, "/").unwrap(), dir.join("index.html"));
        assert_eq!(resolve(&dir, "/a%20b/x.wasm?v=1").unwrap(), dir.join("a b/x.wasm"));
        assert!(resolve(&dir, "/../etc/passwd").is_none());
        assert!(resolve(&dir, "/missing.js").is_none());
        assert_eq!(content_type(Path::new("x.wasm")), "application/wasm");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let root = dir.clone();
        std::thread::spawn(move || serve(listener, root));
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /index.html HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        assert!(r.starts_with("HTTP/1.1 200 OK"), "{r}");
        assert!(r.contains("Cross-Origin-Embedder-Policy: require-corp"));
        assert!(r.ends_with("<p>hi</p>"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
