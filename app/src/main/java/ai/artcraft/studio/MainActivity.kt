package ai.artcraft.studio

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.util.Base64
import android.view.Gravity
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.JsResult
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.webkit.MimeTypeMap
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.webkit.WebViewAssetLoader
import java.io.File
import java.io.FileOutputStream
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

class MainActivity : Activity() {
    private lateinit var webView: WebView
    private lateinit var assetLoader: WebViewAssetLoader
    private var fileChooserCallback: ValueCallback<Array<Uri>>? = null
    private var pendingSave: File? = null
    private val saveMimes = ConcurrentHashMap<String, String>()
    private var pendingSaveMime: String = "application/octet-stream"
    private val saveOutputs = ConcurrentHashMap<String, FileOutputStream>()
    private val saveFiles = ConcurrentHashMap<String, File>()
    private val io = Executors.newSingleThreadExecutor()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = Color.rgb(18, 20, 25)
        window.navigationBarColor = Color.rgb(18, 20, 25)
        window.decorView.systemUiVisibility = 0

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.rgb(18, 20, 25))
        }
        val toolbar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(16), dp(8), dp(12), dp(8))
            setBackgroundColor(Color.rgb(27, 30, 37))
        }
        toolbar.addView(TextView(this).apply {
            text = "ARTCRAFT STUDIO"
            textSize = 17f
            setTextColor(Color.WHITE)
            typeface = android.graphics.Typeface.DEFAULT_BOLD
        }, LinearLayout.LayoutParams(0, dp(48), 1f))
        toolbar.addView(Button(this).apply {
            text = "AI PROVIDERS"
            setTextColor(Color.WHITE)
            setOnClickListener { startActivity(Intent(this@MainActivity, SettingsActivity::class.java)) }
        })
        root.addView(toolbar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(64)))

        assetLoader = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", localAssetHandler())
            .build()
        webView = WebView(this)
        webView.setBackgroundColor(Color.rgb(18, 20, 25))
        webView.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            mediaPlaybackRequiresUserGesture = true
            cacheMode = WebSettings.LOAD_DEFAULT
        }
        webView.addJavascriptInterface(AndroidFilesBridge(), "ArtCraftFiles")
        webView.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                val uri = request.url
                if (uri.scheme == "about" || uri.scheme == "blob" || uri.scheme == "data") return null
                if (uri.scheme == "https" && uri.host == "appassets.androidplatform.net") {
                    return assetLoader.shouldInterceptRequest(uri)
                }
                // The editor bundle is local. Network access is reserved for native, user-approved
                // AI requests; remote scripts and frames are not allowed into this WebView.
                return blockedResponse()
            }

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val uri = request.url
                if (uri.scheme == "about" || uri.scheme == "blob" || uri.scheme == "data") return false
                return uri.scheme != "https" || uri.host != "appassets.androidplatform.net"
            }
        }
        webView.webChromeClient = object : WebChromeClient() {
            override fun onJsConfirm(view: WebView?, url: String?, message: String?, result: JsResult?): Boolean {
                AlertDialog.Builder(this@MainActivity)
                    .setMessage(message ?: "Leave the editor? Save your work first; unsaved changes may be lost.")
                    .setPositiveButton("Continue") { _, _ -> result?.confirm() }
                    .setNegativeButton("Stay") { _, _ -> result?.cancel() }
                    .setOnCancelListener { result?.cancel() }
                    .show()
                return true
            }

            override fun onShowFileChooser(
                webView: WebView?,
                filePathCallback: ValueCallback<Array<Uri>>?,
                fileChooserParams: FileChooserParams?
            ): Boolean {
                fileChooserCallback?.onReceiveValue(null)
                fileChooserCallback = filePathCallback
                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "*/*"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                return try {
                    startActivityForResult(intent, REQUEST_OPEN)
                    true
                } catch (_: Exception) {
                    fileChooserCallback?.onReceiveValue(null)
                    fileChooserCallback = null
                    false
                }
            }
        }
        root.addView(webView, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        setContentView(root)

        val restored = savedInstanceState?.let { webView.restoreState(it) != null } ?: false
        if (!restored) webView.loadUrl("https://appassets.androidplatform.net/assets/studio/index.html")
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_OPEN) {
            val result = if (resultCode == RESULT_OK && data?.data != null) arrayOf(data.data!!) else null
            fileChooserCallback?.onReceiveValue(result)
            fileChooserCallback = null
            return
        }
        if (requestCode == REQUEST_SAVE) {
            val file = pendingSave
            pendingSave = null
            if (resultCode == RESULT_OK && data?.data != null && file != null) {
                val uri = data.data!!
                io.execute {
                    runCatching {
                        contentResolver.openOutputStream(uri, "w")!!.use { output -> file.inputStream().use { it.copyTo(output) } }
                    }.onSuccess {
                        runOnUiThread { Toast.makeText(this, "Export saved", Toast.LENGTH_SHORT).show() }
                    }.onFailure {
                        runOnUiThread { Toast.makeText(this, "Export failed. Check storage access and try again.", Toast.LENGTH_LONG).show() }
                    }
                    file.delete()
                }
            } else {
                file?.delete()
            }
        }
    }

    override fun onPause() {
        webView.onPause()
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        webView.onResume()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        webView.saveState(outState)
        super.onSaveInstanceState(outState)
    }

    override fun onBackPressed() {
        webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.goHome && window.ArtCraftStudio.goHome()") { result ->
            if (result == "false" || result == "null") finish()
        }
    }

    override fun onDestroy() {
        fileChooserCallback?.onReceiveValue(null)
        saveOutputs.values.forEach { runCatching { it.close() } }
        saveFiles.values.forEach { it.delete() }
        io.shutdown()
        webView.destroy()
        super.onDestroy()
    }

    private fun localAssetHandler(): WebViewAssetLoader.PathHandler = object : WebViewAssetLoader.PathHandler {
        override fun handle(path: String): WebResourceResponse? {
            val relative = path.removePrefix("/")
            if (relative.isBlank() || relative.split('/').any { it == ".." || it.isBlank() }) {
                return WebResourceResponse("text/plain", "UTF-8", 404, "Not Found", emptyMap(), "Not found".byteInputStream())
            }
            val extension = relative.substringAfterLast('.', "").lowercase()
            val mime = when (extension) {
                "wasm" -> "application/wasm"
                "js", "mjs" -> "text/javascript"
                "html" -> "text/html"
                else -> MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
            }
            return runCatching { assets.open(relative) }.getOrNull()?.let { WebResourceResponse(mime, null, it) }
                ?: WebResourceResponse("text/plain", "UTF-8", 404, "Not Found", emptyMap(), "Not found".byteInputStream())
        }
    }

    private fun blockedResponse() = WebResourceResponse(
        "text/plain", "utf-8", 403, "Blocked by local-only editor policy", emptyMap(),
        "Network access is disabled in the editor WebView".byteInputStream()
    )

    private inner class AndroidFilesBridge {
        @JavascriptInterface
        fun beginSave(name: String, mime: String?): String {
            val id = UUID.randomUUID().toString()
            val safeName = name.substringAfterLast('/').substringAfterLast('\\').replace(Regex("[^A-Za-z0-9._ -]"), "_").take(120).ifBlank { "artcraft-export" }
            val file = File(cacheDir, "export-$id-$safeName")
            saveFiles[id] = file
            saveOutputs[id] = FileOutputStream(file)
            saveMimes[id] = mime?.takeIf { it.length <= 128 && it.contains('/') } ?: "application/octet-stream"
            return id
        }

        @JavascriptInterface
        fun writeChunk(id: String, base64: String): Boolean {
            val output = saveOutputs[id] ?: return false
            val bytes = runCatching { Base64.decode(base64, Base64.DEFAULT) }.getOrNull() ?: return false
            return runCatching { synchronized(output) { output.write(bytes) } }.isSuccess
        }

        @JavascriptInterface
        fun finishSave(id: String) {
            val output = saveOutputs.remove(id) ?: return
            runCatching { synchronized(output) { output.flush(); output.close() } }.onFailure {
                saveFiles.remove(id)?.delete()
                return
            }
            val file = saveFiles.remove(id) ?: return
            pendingSave = file
            pendingSaveMime = saveMimes.remove(id) ?: "application/octet-stream"
            runOnUiThread {
                val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = pendingSaveMime
                    putExtra(Intent.EXTRA_TITLE, file.name.substringAfter("export-$id-"))
                }
                try {
                    startActivityForResult(intent, REQUEST_SAVE)
                } catch (_: Exception) {
                    pendingSave?.delete()
                    pendingSave = null
                    Toast.makeText(this@MainActivity, "Could not open the Android save picker", Toast.LENGTH_LONG).show()
                }
            }
        }

        @JavascriptInterface
        fun cancelSave(id: String) {
            saveOutputs.remove(id)?.let { runCatching { it.close() } }
            saveFiles.remove(id)?.delete()
        }
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    companion object {
        private const val REQUEST_OPEN = 7101
        private const val REQUEST_SAVE = 7102
    }
}
