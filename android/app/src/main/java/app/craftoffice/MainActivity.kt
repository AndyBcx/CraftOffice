package app.craftoffice

import android.app.Activity
import android.app.AlertDialog
import android.content.ContentValues
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.content.res.Configuration
import android.graphics.drawable.Icon
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
import org.json.JSONArray
import org.json.JSONObject

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
                val multi = params.mode == FileChooserParams.MODE_OPEN_MULTIPLE
                // the recent files first, if there are any; otherwise straight to the system picker
                val recent = recents().filter { (u, _) -> stillReadable(Uri.parse(u)) }
                if (recent.isEmpty()) return browse(multi)
                val labels = recent.map { it.second }.toTypedArray()
                var picked = false
                AlertDialog.Builder(this@MainActivity)
                    .setTitle(tr("Legutóbbi fájlok", "Fișiere recente", "Recent files"))
                    .setItems(labels) { _, i ->
                        picked = true
                        val u = Uri.parse(recent[i].first)
                        addRecent(u, recent[i].second)
                        chooser?.onReceiveValue(arrayOf(u))
                        chooser = null
                    }
                    .setPositiveButton(tr("Tallózás…", "Răsfoire…", "Browse…")) { _, _ ->
                        picked = true
                        if (!browse(multi)) { chooser?.onReceiveValue(null); chooser = null }
                    }
                    .setNegativeButton(tr("Mégse", "Anulare", "Cancel"), null)
                    .setOnDismissListener {
                        if (!picked) { chooser?.onReceiveValue(null); chooser = null }
                    }
                    .show()
                return true
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
                    if (Build.VERSION.SDK_INT < 29) {
                        saveAs()
                        return@runOnUiThread
                    }
                    // the phone's Downloads folder in one tap, or anywhere else through the system
                    AlertDialog.Builder(this@MainActivity)
                        .setTitle(pendingName)
                        .setItems(
                            arrayOf(
                                tr("Mentés a Letöltések mappába", "Salvare în Descărcări", "Save to Downloads") + " (Download/CraftOffice)",
                                tr("Mentés máshová…", "Salvare în altă parte…", "Save elsewhere…"),
                            ),
                        ) { _, i -> if (i == 0) saveToDownloads() else saveAs() }
                        .setNegativeButton(tr("Mégse", "Anulare", "Cancel")) { _, _ -> pendingBytes = null }
                        .show()
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
                if (keep(uri, write = false)) addRecent(uri, name)
                val path = "/_open/" + stamp + "/" + Uri.encode(safe)
                val js = OPEN_FILE.replace("%PATH%", jsString(path)).replace("%NAME%", jsString(name)).replace("%MIME%", jsString(mime))
                runOnUiThread { web.evaluateJavascript(js, null) }
            } catch (e: Exception) {
                removeRecent(uri)
                runOnUiThread { Toast.makeText(this, "✗ ${e.message}", Toast.LENGTH_LONG).show() }
            }
        }.start()
    }

    private fun tr(hu: String, ro: String, en: String) = when (java.util.Locale.getDefault().language) {
        "hu" -> hu
        "ro" -> ro
        else -> en
    }

    private fun browse(multi: Boolean): Boolean {
        // OPEN_DOCUMENT (not GET_CONTENT): the file can be kept among the recent ones
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            if (multi) putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        }
        return try {
            startActivityForResult(intent, REQ_OPEN)
            true
        } catch (e: Exception) {
            chooser = null
            false
        }
    }

    private fun stillReadable(uri: Uri): Boolean =
        contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission }

    /** The system's save dialog, starting where the last file went. */
    private fun saveAs() {
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = pendingMime
            putExtra(Intent.EXTRA_TITLE, pendingName)
            prefs().getString(KEY_LAST_SAVE, null)?.let { putExtra(android.provider.DocumentsContract.EXTRA_INITIAL_URI, Uri.parse(it)) }
        }
        try {
            startActivityForResult(intent, REQ_SAVE)
        } catch (e: Exception) {
            Toast.makeText(this, "✗ ${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    /** Straight into Download/CraftOffice (Android 10+, no permission needed). */
    private fun saveToDownloads() {
        val bytes = pendingBytes ?: return
        val name = pendingName
        val mime = pendingMime
        pendingBytes = null
        Thread {
            try {
                val values = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                    put(MediaStore.MediaColumns.MIME_TYPE, mime)
                    put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/CraftOffice")
                    put(MediaStore.MediaColumns.IS_PENDING, 1)
                }
                val uri = contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                    ?: throw IllegalStateException("MediaStore")
                contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                values.clear()
                values.put(MediaStore.MediaColumns.IS_PENDING, 0)
                contentResolver.update(uri, values, null, null)
                // the system may have renamed it ("name (1).ext") if one existed already
                val saved = displayName(uri)
                runOnUiThread {
                    Toast.makeText(this, "✓ Download/CraftOffice/$saved", Toast.LENGTH_LONG).show()
                }
            } catch (e: Exception) {
                runOnUiThread { Toast.makeText(this, "✗ ${e.message}", Toast.LENGTH_LONG).show() }
            }
        }.start()
    }

    // ---- Recent files: kept with lasting access, shown when the app's icon is held down ----

    private fun prefs() = getSharedPreferences("craftoffice", MODE_PRIVATE)

    /** Keep lasting access to a file; whether the app has it now. */
    private fun keep(uri: Uri, write: Boolean): Boolean {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or (if (write) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
        try {
            contentResolver.takePersistableUriPermission(uri, flags)
        } catch (e: Exception) {
            // a file shared without lasting access
        }
        return contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission }
    }

    private fun displayName(uri: Uri): String {
        try {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst() && !c.isNull(0)) return c.getString(0)
            }
        } catch (e: Exception) {
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "file"
    }

    private fun recents(): MutableList<Pair<String, String>> {
        val out = mutableListOf<Pair<String, String>>()
        try {
            val a = JSONArray(prefs().getString(KEY_RECENT, "[]"))
            for (i in 0 until a.length()) {
                val o = a.getJSONObject(i)
                out.add(o.getString("uri") to o.getString("name"))
            }
        } catch (e: Exception) {
        }
        return out
    }

    private fun saveRecents(list: List<Pair<String, String>>) {
        val a = JSONArray()
        for ((u, n) in list.take(MAX_RECENT)) a.put(JSONObject().put("uri", u).put("name", n))
        prefs().edit().putString(KEY_RECENT, a.toString()).apply()
        updateShortcuts(list)
    }

    private fun addRecent(uri: Uri, name: String) {
        val list = recents()
        list.removeAll { it.first == uri.toString() }
        list.add(0, uri.toString() to name)
        saveRecents(list)
    }

    private fun removeRecent(uri: Uri) {
        val list = recents()
        if (list.removeAll { it.first == uri.toString() }) saveRecents(list)
    }

    /** The last files as shortcuts of the app's icon (hold the icon down to see them). */
    private fun updateShortcuts(list: List<Pair<String, String>>) {
        try {
            val sm = getSystemService(ShortcutManager::class.java) ?: return
            val max = minOf(sm.maxShortcutCountPerActivity, 4)
            val shortcuts = list.take(max).mapIndexed { i, (u, n) ->
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse(u), this, MainActivity::class.java)
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                ShortcutInfo.Builder(this, "recent$i")
                    .setShortLabel(n.take(24).ifBlank { "file" })
                    .setLongLabel(n.take(60).ifBlank { "file" })
                    .setIcon(Icon.createWithResource(this, R.mipmap.ic_launcher))
                    .setIntent(intent)
                    .setRank(i)
                    .build()
            }
            sm.dynamicShortcuts = shortcuts
        } catch (e: Exception) {
            // shortcuts are a convenience only
        }
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
                // files opened from the picker become recent ones
                if (resultCode == RESULT_OK && result != null) {
                    Thread {
                        for (u in result) if (keep(u, write = false)) addRecent(u, displayName(u))
                    }.start()
                }
            }
            REQ_SAVE -> {
                val uri = data?.data
                val bytes = pendingBytes
                pendingBytes = null
                if (resultCode == RESULT_OK && uri != null && bytes != null) {
                    try {
                        contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                        Toast.makeText(this, "✓ $pendingName", Toast.LENGTH_SHORT).show()
                        prefs().edit().putString(KEY_LAST_SAVE, uri.toString()).apply()
                        if (keep(uri, write = true)) addRecent(uri, displayName(uri))
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
        private const val KEY_RECENT = "recent"
        private const val KEY_LAST_SAVE = "lastSave"
        private const val MAX_RECENT = 12

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
