package app.craftoffice

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.provider.OpenableColumns
import android.os.Bundle
import android.system.Os
import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** The engine library (Rust): a local web server for this app's files. */
object Engine {
    init {
        System.loadLibrary("craftoffice_android")
    }

    @JvmStatic
    external fun start(port: Int): Int
}

/**
 * A CraftOffice app on Android, as Vormantis: the program (Rust, built to WebAssembly) runs in a
 * WebView, served by the engine library from the app's own files, so it works offline. Opening
 * files uses the system file picker; saving hands the file to the system "Save to" screen.
 */
class MainActivity : Activity() {
    private lateinit var web: WebView
    private var chooser: ValueCallback<Array<Uri>>? = null
    private var pendingName = ""
    private var pendingMime = "application/octet-stream"
    private var pendingBytes: ByteArray? = null
    private var site: File? = null
    private var pageReady = false
    private var openUri: Uri? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        web = WebView(this)
        setContentView(web)
        web.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            allowFileAccess = false
            mediaPlaybackRequiresUserGesture = false
            cacheMode = WebSettings.LOAD_DEFAULT
            // The page is laid out at the screen's own width, never zoomed out to an "overview":
            // turning the phone then lays it out again at the new width (#rotation).
            useWideViewPort = false
            loadWithOverviewMode = false
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
        }
        web.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView, url: String) {
                view.evaluateJavascript(SAVE_BRIDGE, null)
                if (url.startsWith("http://127.0.0.1")) {
                    pageReady = true
                    deliverOpen()
                }
            }
        }
        web.webChromeClient = object : WebChromeClient() {
            // File › Open, Import, Place: the system file picker.
            override fun onShowFileChooser(
                view: WebView,
                callback: ValueCallback<Array<Uri>>,
                params: FileChooserParams,
            ): Boolean {
                chooser?.onReceiveValue(null)
                chooser = callback
                val intent = Intent(Intent.ACTION_GET_CONTENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "*/*"
                    if (params.mode == FileChooserParams.MODE_OPEN_MULTIPLE) putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
                }
                return try {
                    startActivityForResult(intent, REQ_OPEN)
                    true
                } catch (e: Exception) {
                    chooser = null
                    false
                }
            }
        }
        // Saving: the page hands the finished file over; the system asks where it should go.
        web.addJavascriptInterface(object {
            @JavascriptInterface
            fun saveFile(name: String, mime: String, base64: String) {
                pendingName = name
                pendingMime = if (mime.isBlank()) "application/octet-stream" else mime
                pendingBytes = Base64.decode(base64, Base64.DEFAULT)
                runOnUiThread {
                    val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = pendingMime
                        putExtra(Intent.EXTRA_TITLE, pendingName)
                    }
                    startActivityForResult(intent, REQ_SAVE)
                }
            }
        }, "CraftOfficeAndroid")

        web.loadData(
            "<body style='background:#14213d;color:#e8ebf2;font:16px sans-serif;padding:32px'>" + getString(R.string.app_name) + "…</body>",
            "text/html",
            "utf-8",
        )
        takeOpenIntent(intent)
        Thread {
            val site = unpackSite()
            this.site = site
            Os.setenv("CRAFTOFFICE_SITE", site.absolutePath, true)
            val port = Engine.start(BuildConfig.APP_PORT)
            val url = "http://127.0.0.1:$port/"
            var up = false
            for (i in 0 until 100) {
                try {
                    val c = URL(url).openConnection() as HttpURLConnection
                    c.connectTimeout = 300
                    c.readTimeout = 1000
                    if (c.responseCode == 200) {
                        up = true
                        break
                    }
                } catch (e: Exception) {
                    // not up yet
                }
                Thread.sleep(150)
            }
            runOnUiThread {
                if (up) web.loadUrl(url)
                else Toast.makeText(this, "The engine did not start.", Toast.LENGTH_LONG).show()
            }
        }.start()
    }

    /** Opened again with a file (the app was already running): open that file too. */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        takeOpenIntent(intent)
        deliverOpen()
    }

    /** A file from another app: "Open with…" (VIEW) or shared to this app (SEND). */
    private fun takeOpenIntent(intent: Intent?) {
        val uri: Uri? = when (intent?.action) {
            Intent.ACTION_VIEW -> intent.data
            Intent.ACTION_SEND -> {
                @Suppress("DEPRECATION")
                intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
            }
            else -> null
        }
        if (uri != null) openUri = uri
    }

    /**
     * The file waiting to be opened, once the page is up: copied beside the page (the engine
     * serves it) and dropped onto the app's window, as if dragged there, so the app opens it.
     */
    private fun deliverOpen() {
        val uri = openUri ?: return
        val site = site ?: return
        if (!pageReady) return
        openUri = null
        Thread {
            try {
                var name = "file"
                contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                    if (c.moveToFirst() && !c.isNull(0)) name = c.getString(0)
                }
                if (name == "file") uri.lastPathSegment?.substringAfterLast('/')?.let { if (it.isNotBlank()) name = it }
                val safe = name.replace(Regex("[\\\\/:*?\"<>|\\u0000-\\u001f]"), "_").take(150).ifBlank { "file" }
                val dir = File(site, "_open")
                dir.deleteRecursively()
                val stamp = System.currentTimeMillis().toString()
                val out = File(File(dir, stamp), safe)
                out.parentFile?.mkdirs()
                contentResolver.openInputStream(uri)?.use { input -> out.outputStream().use { input.copyTo(it, 1 shl 16) } }
                    ?: throw IllegalStateException("cannot read")
                val mime = contentResolver.getType(uri) ?: ""
                val path = "/_open/" + stamp + "/" + Uri.encode(safe)
                val js = OPEN_FILE.replace("%PATH%", jsString(path)).replace("%NAME%", jsString(name)).replace("%MIME%", jsString(mime))
                runOnUiThread { web.evaluateJavascript(js, null) }
            } catch (e: Exception) {
                runOnUiThread { Toast.makeText(this, "✗ ${e.message}", Toast.LENGTH_LONG).show() }
            }
        }.start()
    }

    private fun jsString(s: String): String =
        "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\r", "") + "\""

    /** The app's files (assets/site) unpacked once per version, so the engine can serve them. */
    private fun unpackSite(): File {
        val site = File(filesDir, "site")
        val stamp = File(filesDir, "site.version")
        val version = packageManager.getPackageInfo(packageName, 0).let { "${it.versionName}-${it.lastUpdateTime}" }
        if (site.isDirectory && stamp.isFile && stamp.readText() == version) return site
        site.deleteRecursively()
        copyAssets("site", site)
        stamp.writeText(version)
        return site
    }

    private fun copyAssets(path: String, to: File) {
        val children = assets.list(path) ?: emptyArray()
        if (children.isEmpty()) {
            to.parentFile?.mkdirs()
            assets.open(path).use { input -> to.outputStream().use { input.copyTo(it, 1 shl 16) } }
            return
        }
        to.mkdirs()
        for (c in children) copyAssets("$path/$c", File(to, c))
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        when (requestCode) {
            REQ_OPEN -> {
                val many = data?.clipData
                val result = if (resultCode == RESULT_OK && many != null) {
                    Array(many.itemCount) { many.getItemAt(it).uri }
                } else {
                    WebChromeClient.FileChooserParams.parseResult(resultCode, data)
                }
                chooser?.onReceiveValue(result)
                chooser = null
            }
            REQ_SAVE -> {
                val uri = data?.data
                val bytes = pendingBytes
                pendingBytes = null
                if (resultCode == RESULT_OK && uri != null && bytes != null) {
                    try {
                        contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                        Toast.makeText(this, "✓ $pendingName", Toast.LENGTH_SHORT).show()
                    } catch (e: Exception) {
                        Toast.makeText(this, "✗ $pendingName: ${e.message}", Toast.LENGTH_LONG).show()
                    }
                }
            }
        }
    }

    /** Turning the phone: the app keeps running (no restart); the page is told its new size. */
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        web.requestLayout()
        for (delay in longArrayOf(50L, 300L, 800L)) {
            web.postDelayed({ web.evaluateJavascript(RESIZE, null) }, delay)
        }
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        if (web.canGoBack()) web.goBack() else super.onBackPressed()
    }

    companion object {
        private const val REQ_OPEN = 1
        private const val REQ_SAVE = 2

        /** The page's new size after the screen turned: the canvas follows the window. */
        private const val RESIZE = """
(function () {
  var w = window.innerWidth, h = window.innerHeight;
  document.querySelectorAll("canvas").forEach(function (c) {
    if (c.style.position === "absolute" || getComputedStyle(c).position === "absolute") {
      c.style.width = w + "px"; c.style.height = h + "px";
      requestAnimationFrame(function () { c.style.width = "100%"; c.style.height = "100%"; });
    }
  });
  window.dispatchEvent(new Event("resize"));
})();
"""

        /**
         * Opens a file handed over by another app: once the app's window is drawn, the file is
         * dropped onto it (the apps open dropped files, as on a desktop).
         */
        private const val OPEN_FILE = """
(function () {
  var path = %PATH%, name = %NAME%, mime = %MIME%, t0 = Date.now();
  function visible(el) { return el && el.offsetParent !== null && getComputedStyle(el).display !== "none"; }
  function ready() {
    var c = document.querySelector("canvas");
    if (!c || c.width === 0) return null;
    var l = document.getElementById("loading") || document.getElementById("loader") || document.querySelector(".loading");
    return visible(l) ? null : c;
  }
  function drop(c) {
    fetch(path).then(function (r) { return r.blob(); }).then(function (b) {
      var f = new File([b], name, { type: mime || b.type || "" });
      var dt = new DataTransfer();
      dt.items.add(f);
      ["dragenter", "dragover", "drop"].forEach(function (t) {
        var e = new DragEvent(t, { dataTransfer: dt, bubbles: true, cancelable: true });
        c.dispatchEvent(e);
      });
    });
  }
  (function wait() {
    var c = ready();
    if (c) { setTimeout(function () { drop(c); }, 1500); return; }
    if (Date.now() - t0 < 120000) setTimeout(wait, 400);
  })();
})();
"""

        /** Downloads made by the page (a link with `download`) go to the system "Save to" screen. */
        private const val SAVE_BRIDGE = """
(function () {
  if (window.__craftofficeSave) return;
  window.__craftofficeSave = true;
  function save(href, name) {
    fetch(href).then(function (r) { return r.blob(); }).then(function (b) {
      var fr = new FileReader();
      fr.onload = function () {
        var s = String(fr.result);
        CraftOfficeAndroid.saveFile(name || "file", b.type || "", s.substring(s.indexOf(",") + 1));
      };
      fr.readAsDataURL(b);
    });
  }
  document.addEventListener("click", function (e) {
    var a = e.target && e.target.closest ? e.target.closest("a[download]") : null;
    if (a && a.href) { e.preventDefault(); e.stopImmediatePropagation(); save(a.href, a.getAttribute("download")); }
  }, true);
  var click = HTMLAnchorElement.prototype.click;
  HTMLAnchorElement.prototype.click = function () {
    if (this.hasAttribute("download") && this.href) { save(this.href, this.getAttribute("download")); return; }
    return click.call(this);
  };
})();
"""
    }
}
