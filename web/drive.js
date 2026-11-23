// CraftOffice web: "Save to Google Drive" beside the ordinary download, in every app.
// The apps save by handing the browser a file to download; this asks where it should go.
// Works only when the site has a Google OAuth client ID in /drive.json ({"clientId": "…"}),
// with https://<this site>/drive-auth.html as its redirect URI (see install.sh). Without one,
// nothing changes: files are downloaded as before.
//
// The pages are cross-origin isolated (COOP/COEP, for the apps' threads), so the sign-in window
// cannot talk to this page directly: drive-auth.html hands the token over on a BroadcastChannel.
(function () {
  "use strict";
  var ROOT = new URL("./", document.currentScript ? document.currentScript.src : location.href);
  var SCOPE = "https://www.googleapis.com/auth/drive.file";
  var lang = (navigator.language || "en").slice(0, 2);
  function tr(hu, ro, en) { return lang === "hu" ? hu : lang === "ro" ? ro : en; }
  var clientId = null;

  fetch(new URL("drive.json", ROOT), { cache: "no-store" })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (j) { if (j && j.clientId) { clientId = String(j.clientId); install(); } })
    .catch(function () {});

  // ---- sign-in: a token for drive.file, kept for this browser tab only ----
  function savedToken() {
    try {
      var t = JSON.parse(sessionStorage.getItem("craftoffice.drive") || "null");
      if (t && t.exp > Date.now() + 60000) return t.token;
    } catch (e) {}
    return null;
  }
  function signIn() {
    var t = savedToken();
    if (t) return Promise.resolve(t);
    return new Promise(function (resolve, reject) {
      var state = Math.random().toString(36).slice(2) + Date.now().toString(36);
      var ch = new BroadcastChannel("craftoffice-drive");
      var done = false;
      ch.onmessage = function (e) {
        var d = e.data || {};
        if (d.state !== state) return;
        done = true; ch.close();
        if (d.error || !d.token) { reject(new Error(d.error || "no token")); return; }
        try {
          sessionStorage.setItem("craftoffice.drive", JSON.stringify({ token: d.token, exp: Date.now() + (d.expires || 3600) * 1000 }));
        } catch (e2) {}
        resolve(d.token);
      };
      var url = "https://accounts.google.com/o/oauth2/v2/auth?" + new URLSearchParams({
        client_id: clientId,
        redirect_uri: new URL("drive-auth.html", ROOT).href,
        response_type: "token",
        scope: SCOPE,
        include_granted_scopes: "true",
        state: state,
      });
      var w = window.open(url, "craftoffice-drive", "width=520,height=640");
      if (!w) { ch.close(); reject(new Error(tr("A böngésző letiltotta a bejelentkező ablakot.", "Browserul a blocat fereastra de autentificare.", "The browser blocked the sign-in window."))); return; }
      // give up after five minutes (the window may have been closed without an answer)
      setTimeout(function () { if (!done) { ch.close(); reject(new Error(tr("Nem történt bejelentkezés.", "Autentificarea nu a avut loc.", "Not signed in."))); } }, 300000);
    });
  }

  // ---- Drive: the files go into a "CraftOffice" folder ----
  function api(token, url, opts) {
    opts = opts || {};
    opts.headers = Object.assign({ Authorization: "Bearer " + token }, opts.headers || {});
    return fetch(url, opts).then(function (r) {
      if (r.status === 401) { try { sessionStorage.removeItem("craftoffice.drive"); } catch (e) {} }
      if (!r.ok) return r.text().then(function (t) { throw new Error("Drive " + r.status + ": " + t.slice(0, 200)); });
      return r.json();
    });
  }
  function folder(token) {
    var q = "name='CraftOffice' and mimeType='application/vnd.google-apps.folder' and trashed=false";
    return api(token, "https://www.googleapis.com/drive/v3/files?fields=files(id)&q=" + encodeURIComponent(q)).then(function (j) {
      if (j.files && j.files.length) return j.files[0].id;
      return api(token, "https://www.googleapis.com/drive/v3/files?fields=id", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: "CraftOffice", mimeType: "application/vnd.google-apps.folder" }),
      }).then(function (f) { return f.id; });
    });
  }
  function upload(token, blob, name) {
    return folder(token).then(function (parent) {
      // a file of the same name in the folder is updated, not duplicated
      var q = "name='" + name.replace(/\\/g, "\\\\").replace(/'/g, "\\'") + "' and '" + parent + "' in parents and trashed=false";
      return api(token, "https://www.googleapis.com/drive/v3/files?fields=files(id)&q=" + encodeURIComponent(q)).then(function (j) {
        var existing = j.files && j.files.length ? j.files[0].id : null;
        var meta = existing ? {} : { name: name, parents: [parent] };
        var form = new FormData();
        form.append("metadata", new Blob([JSON.stringify(meta)], { type: "application/json" }));
        form.append("file", blob.type ? blob : new Blob([blob], { type: "application/octet-stream" }));
        var url = existing
          ? "https://www.googleapis.com/upload/drive/v3/files/" + existing + "?uploadType=multipart&fields=id,name,webViewLink"
          : "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,webViewLink";
        return api(token, url, { method: existing ? "PATCH" : "POST", body: form });
      });
    });
  }

  // ---- the question, and a short note afterwards ----
  var css = "position:fixed;z-index:2147483647;font:14px system-ui,sans-serif;color:#e8ebf2;background:#1d2a4a;" +
    "border-radius:10px;box-shadow:0 8px 30px rgba(0,0,0,.45);";
  function note(text, link) {
    var n = document.createElement("div");
    n.style.cssText = css + "left:50%;bottom:24px;transform:translateX(-50%);padding:10px 16px;max-width:80vw";
    n.textContent = text;
    if (link) {
      var a = document.createElement("a");
      a.href = link; a.target = "_blank"; a.rel = "noopener";
      a.textContent = " " + tr("Megnyitás", "Deschide", "Open");
      a.style.color = "#8fb4ff";
      n.appendChild(a);
    }
    document.body.appendChild(n);
    setTimeout(function () { n.remove(); }, 6000);
  }
  function ask(name) {
    return new Promise(function (resolve) {
      var bg = document.createElement("div");
      bg.style.cssText = "position:fixed;inset:0;z-index:2147483646;background:rgba(0,0,0,.35)";
      var box = document.createElement("div");
      box.style.cssText = css + "left:50%;top:50%;transform:translate(-50%,-50%);padding:18px 20px;min-width:280px";
      var t = document.createElement("div");
      t.textContent = name;
      t.style.cssText = "font-weight:600;margin-bottom:14px;word-break:break-all";
      box.appendChild(t);
      function button(label, value, main) {
        var b = document.createElement("button");
        b.textContent = label;
        b.style.cssText = "display:block;width:100%;margin-top:8px;padding:9px 12px;border:0;border-radius:7px;cursor:pointer;font:inherit;" +
          (main ? "background:#3d6fe0;color:#fff" : "background:#2b3a60;color:#e8ebf2");
        b.onclick = function () { bg.remove(); box.remove(); resolve(value); };
        box.appendChild(b);
      }
      button(tr("Letöltés", "Descărcare", "Download"), "download", false);
      button(tr("Mentés a Google Drive-ra", "Salvare în Google Drive", "Save to Google Drive"), "drive", true);
      button(tr("Mégse", "Anulare", "Cancel"), null, false);
      bg.onclick = function () { bg.remove(); box.remove(); resolve(null); };
      document.body.appendChild(bg);
      document.body.appendChild(box);
    });
  }

  function handle(a, click) {
    var href = a.href, name = a.getAttribute("download") || "file";
    ask(name).then(function (choice) {
      if (choice === "download") { a.setAttribute("data-craftoffice-pass", "1"); click.call(a); a.removeAttribute("data-craftoffice-pass"); return; }
      if (choice !== "drive") return;
      // the blob first: the app may revoke its address soon after handing it over
      var blob = fetch(href).then(function (r) { return r.blob(); });
      signIn()
        .then(function (token) {
          note(tr("Mentés a Google Drive-ra…", "Se salvează în Google Drive…", "Saving to Google Drive…"));
          return blob.then(function (b) { return upload(token, b, name); });
        })
        .then(function (f) { note("✓ Google Drive: CraftOffice/" + (f.name || name), f.webViewLink); })
        .catch(function (e) { note("✗ " + (e && e.message ? e.message : e)); });
    });
  }

  function install() {
    var click = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () {
      if (this.hasAttribute("download") && this.href && !this.hasAttribute("data-craftoffice-pass")) { handle(this, click); return; }
      return click.call(this);
    };
    document.addEventListener("click", function (e) {
      var a = e.target && e.target.closest ? e.target.closest("a[download]") : null;
      if (a && a.href && !a.hasAttribute("data-craftoffice-pass")) { e.preventDefault(); e.stopImmediatePropagation(); handle(a, click); }
    }, true);
  }
})();
