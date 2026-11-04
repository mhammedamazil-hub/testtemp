package ai.artcraft.studio

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.view.inputmethod.InputMethodManager
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.util.Base64
import android.util.Log
import android.view.ViewGroup
import android.view.PixelCopy
import android.webkit.ConsoleMessage
import android.webkit.JavascriptInterface
import android.webkit.JsResult
import android.webkit.MimeTypeMap
import android.webkit.RenderProcessGoneDetail
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.Toast
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.webkit.WebViewAssetLoader
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.net.URI
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

class MainActivity : Activity() {
    private lateinit var webView: WebView
    private lateinit var assetLoader: WebViewAssetLoader
    private val fileChooserCallback = AtomicReference<ValueCallback<Array<Uri>>?>(null)
    private var pendingSave: File? = null
    private val saveMimes = ConcurrentHashMap<String, String>()
    private val saveOutputs = ConcurrentHashMap<String, FileOutputStream>()
    private val saveFiles = ConcurrentHashMap<String, File>()
    private val io = Executors.newSingleThreadExecutor()
    private val agentIo = Executors.newSingleThreadExecutor()
    private val agentPlans = ConcurrentHashMap<String, AgentTask>()
    private val agentRuns = ConcurrentHashMap<String, AgentRun>()
    private val cancelledPlanRequests = ConcurrentHashMap.newKeySet<String>()
    private val providerStore by lazy { SecureProviderStore(this) }
    private lateinit var pixelCopyThread: HandlerThread
    private lateinit var pixelCopyHandler: Handler
    @Volatile private var activeEditorId: String? = null
    @Volatile private var shellLoaded = false
    @Volatile internal var smokeSurfaceEditor: String? = null
        private set
    @Volatile internal var smokeFatalEditor: String? = null
        private set
    @Volatile private var smokeLastDiagnostic: String? = null

    internal fun smokeStartupEvidence(editor: String): String {
        val fatal = if (smokeFatalEditor == editor) "fatal startup marker observed" else "no fatal startup marker"
        val diagnostic = smokeLastDiagnostic?.takeIf { it.startsWith("$editor ") } ?: "no local WebView diagnostic captured"
        return "$fatal; $diagnostic"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pixelCopyThread = HandlerThread("ArtCraftScreenshot").apply { start() }
        pixelCopyHandler = Handler(pixelCopyThread.looper)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.rgb(16, 19, 24)
        window.navigationBarColor = Color.rgb(16, 19, 24)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }

        val root = FrameLayout(this).apply { setBackgroundColor(Color.rgb(16, 19, 24)) }
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            val safe = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(safe.left, safe.top, safe.right, safe.bottom)
            insets
        }
        assetLoader = WebViewAssetLoader.Builder().addPathHandler("/assets/", localAssetHandler()).build()
        webView = WebView(this).apply {
            setBackgroundColor(Color.rgb(16, 19, 24))
            isVerticalScrollBarEnabled = false
            isHorizontalScrollBarEnabled = false
            settings.apply {
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
                if (android.os.Build.VERSION.SDK_INT >= 26) safeBrowsingEnabled = true
            }
        }
        webView.addJavascriptInterface(AndroidFilesBridge(), "ArtCraftFiles")
        webView.addJavascriptInterface(AgentBridge(), "ArtCraftAgent")
        webView.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                val uri = request.url
                if (uri.scheme == "about" || uri.scheme == "blob" || uri.scheme == "data") return null
                if (uri.scheme == "https" && uri.host == "appassets.androidplatform.net") return assetLoader.shouldInterceptRequest(uri)
                // Editor bundles are local. Provider traffic is performed natively only after consent.
                return blockedResponse()
            }

            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val uri = request.url
                if (uri.scheme == "about" || uri.scheme == "blob" || uri.scheme == "data") return false
                return uri.scheme != "https" || uri.host != "appassets.androidplatform.net"
            }

            override fun onPageFinished(view: WebView, url: String) {
                super.onPageFinished(view, url)
                if (!shellLoaded && runCatching { URI(url).path == "/assets/studio/index.html" }.getOrDefault(false)) {
                    shellLoaded = true
                    if (isDebuggable()) Log.i(SMOKE_TAG, "STUDIO_SHELL_READY")
                    launchSmokeEditorIfRequested()
                }
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                super.onReceivedError(view, request, error)
                val uri = request.url
                if (uri.host == "appassets.androidplatform.net") {
                    recordDiagnostic(
                        if (request.isForMainFrame) "error" else "warning",
                        editorFromSource(uri.toString()) ?: activeEditorId ?: "studio",
                        safeSource(uri.toString()), 0,
                        "WebView resource load failed with code ${error.errorCode}.", request.isForMainFrame
                    )
                }
            }

            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                val message = if (detail.didCrash()) "The Android WebView renderer crashed. Restart the studio; unsaved in-memory work may be lost."
                    else "Android stopped the WebView renderer to recover memory. Restart the studio; unsaved in-memory work may be lost."
                recordDiagnostic("error", activeEditorId ?: "studio", "WebView renderer", 0, message, true)
                if (!isFinishing) {
                    AlertDialog.Builder(this@MainActivity)
                        .setTitle("Editor renderer stopped")
                        .setMessage(message)
                        .setPositiveButton("Restart studio") { _, _ ->
                            startActivity(Intent(this@MainActivity, MainActivity::class.java))
                            finish()
                        }
                        .setNegativeButton("Close") { _, _ -> finish() }
                        .setOnCancelListener { finish() }
                        .show()
                }
                return true
            }
        }
        webView.webChromeClient = object : WebChromeClient() {
            override fun onConsoleMessage(message: ConsoleMessage): Boolean {
                val source = message.sourceId().orEmpty()
                val editor = editorFromSource(source) ?: activeEditorId ?: "studio"
                when (message.messageLevel()) {
                    ConsoleMessage.MessageLevel.ERROR -> {
                        val text = message.message().orEmpty()
                        recordDiagnostic("error", editor, safeSource(source), message.lineNumber(), text, isFatalMessage(text))
                    }
                    ConsoleMessage.MessageLevel.WARNING -> if (isDebuggable() && editor != "studio") {
                        recordDiagnostic("warning", editor, safeSource(source), message.lineNumber(), message.message().orEmpty(), false)
                    }
                    ConsoleMessage.MessageLevel.LOG, ConsoleMessage.MessageLevel.DEBUG -> {
                        val text = message.message().orEmpty()
                        val surfaceId = text.takeIf { it.startsWith("ARTCRAFT_EDITOR_SURFACE_PRESENT ") }?.substringAfterLast(' ')
                        val iframeId = text.takeIf { it.startsWith("ARTCRAFT_IFRAME_LOADED ") }?.substringAfterLast(' ')
                        val safeSmokeMarker = text == "ARTCRAFT_STUDIO_SHELL_READY" || text == "ARTCRAFT_EDITOR_READY pdfcraft" || (surfaceId != null && surfaceId in EDITORS) || (iframeId != null && iframeId in EDITORS)
                        if (isDebuggable() && safeSmokeMarker) {
                            if (surfaceId != null && surfaceId in EDITORS) smokeSurfaceEditor = surfaceId
                            Log.i(SMOKE_TAG, text)
                        }
                    }
                    else -> Unit
                }
                return true
            }

            override fun onJsConfirm(view: WebView?, url: String?, message: String?, result: JsResult?): Boolean {
                AlertDialog.Builder(this@MainActivity)
                    .setMessage(message ?: "Leave the editor? Save your work first; unsaved changes may be lost.")
                    .setPositiveButton("Continue") { _, _ -> result?.confirm() }
                    .setNegativeButton("Stay") { _, _ -> result?.cancel() }
                    .setOnCancelListener { result?.cancel() }
                    .show()
                return true
            }

            override fun onShowFileChooser(view: WebView?, callback: ValueCallback<Array<Uri>>?, params: FileChooserParams?): Boolean {
                fileChooserCallback.getAndSet(callback)?.onReceiveValue(null)
                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "*/*"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                return try {
                    startActivityForResult(intent, REQUEST_OPEN)
                    true
                } catch (_: Exception) {
                    fileChooserCallback.getAndSet(null)?.onReceiveValue(null)
                    false
                }
            }
        }
        root.addView(webView, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        setContentView(root)
        ViewCompat.requestApplyInsets(root)
        webView.loadUrl("https://appassets.androidplatform.net/assets/studio/index.html")
    }

    private fun launchSmokeEditorIfRequested() {
        if (!isDebuggable()) return
        val id = intent.getStringExtra(EXTRA_SMOKE_EDITOR) ?: return
        if (id !in EDITORS) return
        smokeFatalEditor = null
        smokeLastDiagnostic = null
        webView.postDelayed({ webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.openEditor(${JSONObject.quote(id)})") { } }, 250)
    }

    private fun isDebuggable() = applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0

    private fun recordDiagnostic(level: String, editor: String, source: String, line: Int, message: String, fatal: Boolean) {
        val item = JSONObject().put("level", level).put("editor", editor).put("source", source.take(180))
            .put("line", line).put("message", safeDiagnosticMessage(message)).put("fatal", fatal)
        if (fatal && isDebuggable() && editor in EDITORS) smokeFatalEditor = editor
        if (isDebuggable() && editor in EDITORS) smokeLastDiagnostic = "$editor ${item.optString("source")}: ${item.optString("message")}".take(320)
        if (isDebuggable()) Log.println(if (level == "error") Log.ERROR else Log.WARN, "ArtCraftWebView", item.toString())
        if (::webView.isInitialized) webView.post {
            webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.recordDiagnostic($item)") { }
        }
    }

    private fun editorFromSource(source: String) = EDITORS.firstOrNull { source.contains("/editors/$it/") }
    private fun safeSource(source: String): String = runCatching {
        val path = Uri.parse(source).path.orEmpty()
        when {
            path.startsWith("/assets/editors/") && !path.split('/').any { it == ".." } -> path.take(180)
            path.startsWith("/assets/studio/") -> "/assets/studio/index.html"
            else -> "WebView"
        }
    }.getOrDefault("WebView")

    /** Diagnostics intentionally retain error categories, not arbitrary JS text that may contain artwork or keys. */
    private fun safeDiagnosticMessage(raw: String): String {
        val message = raw.lowercase()
        val resourceCode = Regex("webview resource load failed with code ([0-9-]+)").find(message)?.groupValues?.getOrNull(1)
        return when {
            resourceCode != null -> "Bundled WebView resource failed to load (code $resourceCode)."
            "out of memory" in message || "memory allocation" in message -> "The editor reported a memory allocation failure."
            "webassembly.instantiate" in message || "failed to compile" in message || "wasm" in message -> "WebAssembly load or compilation failed."
            "runtimeerror" in message || "unreachable" in message || "panic" in message -> "The editor engine stopped with a runtime error."
            "failed to start" in message || "startup watchdog" in message || "canvas became visible" in message -> "The editor did not complete startup."
            "renderer" in message -> "The Android WebView renderer crashed or was reclaimed."
            "webgl" in message || "webgpu" in message || "gpu" in message -> "The editor reported a graphics initialization or runtime error."
            "export" in message -> "The editor export transfer failed."
            else -> "The editor reported a JavaScript error; details are omitted to protect project content."
        }
    }

    private fun isFatalMessage(message: String): Boolean {
        val lower = message.lowercase()
        return listOf("failed to start", "failed to load", "runtimeerror", "unreachable", "out of memory", "memory allocation", "webassembly.instantiate", "failed to compile", "panic", "webgl", "webgpu", "failed to create wgpu surface").any(lower::contains)
    }

    private fun localAssetHandler() = object : WebViewAssetLoader.PathHandler {
        override fun handle(path: String): WebResourceResponse? {
            val relative = path.removePrefix("/")
            if (relative.isBlank() || relative.split('/').any { it == ".." || it.isBlank() }) return notFound()
            val extension = relative.substringAfterLast('.', "").lowercase()
            val mime = when (extension) {
                "wasm" -> "application/wasm"
                "js", "mjs" -> "text/javascript"
                "html" -> "text/html"
                "json", "webmanifest" -> "application/json"
                else -> MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
            }
            return runCatching { assets.open(relative) }.getOrNull()?.let { WebResourceResponse(mime, null, it) } ?: notFound()
        }
    }

    private fun notFound() = WebResourceResponse("text/plain", "UTF-8", 404, "Not Found", emptyMap(), "Not found".byteInputStream())
    private fun blockedResponse() = WebResourceResponse("text/plain", "utf-8", 403, "Blocked by local-only editor policy", emptyMap(), "Network access is disabled in the editor WebView".byteInputStream())

    private inner class AndroidFilesBridge {
        @JavascriptInterface fun openSettings() { runOnUiThread { startActivity(Intent(this@MainActivity, SettingsActivity::class.java)) } }
        @JavascriptInterface fun setActiveEditor(id: String) { activeEditorId = if (id in EDITORS) id else null }

        @JavascriptInterface fun beginSave(name: String, mime: String?): String {
            val id = UUID.randomUUID().toString()
            val safeName = name.substringAfterLast('/').substringAfterLast('\\').replace(Regex("[^A-Za-z0-9._ -]"), "_").take(120).ifBlank { "artcraft-export" }
            val file = File(cacheDir, "export-$id-$safeName")
            saveFiles[id] = file
            saveOutputs[id] = FileOutputStream(file)
            saveMimes[id] = mime?.takeIf { it.length <= 128 && it.contains('/') } ?: "application/octet-stream"
            return id
        }

        @JavascriptInterface fun writeChunk(id: String, base64: String): Boolean {
            val output = saveOutputs[id] ?: return false
            val bytes = runCatching { Base64.decode(base64, Base64.DEFAULT) }.getOrNull() ?: return false
            return runCatching { synchronized(output) { output.write(bytes) } }.isSuccess
        }

        @JavascriptInterface fun finishSave(id: String) {
            val output = saveOutputs.remove(id) ?: return
            runCatching { synchronized(output) { output.flush(); output.close() } }.onFailure { saveFiles.remove(id)?.delete(); return }
            val file = saveFiles.remove(id) ?: return
            val mime = saveMimes.remove(id) ?: "application/octet-stream"
            runOnUiThread {
                pendingSave = file
                val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = mime
                    putExtra(Intent.EXTRA_TITLE, file.name.substringAfter("export-$id-"))
                }
                try { startActivityForResult(intent, REQUEST_SAVE) }
                catch (_: Exception) {
                    pendingSave?.delete(); pendingSave = null
                    Toast.makeText(this@MainActivity, "Could not open the Android save picker", Toast.LENGTH_LONG).show()
                }
            }
        }

        @JavascriptInterface fun cancelSave(id: String) {
            saveOutputs.remove(id)?.let { runCatching { it.close() } }
            saveFiles.remove(id)?.delete()
            saveMimes.remove(id)
        }
    }

    private inner class AgentBridge {
        @JavascriptInterface fun requestPlan(task: String, requestId: String) {
            if (requestId.length !in 8..80 || task.length > 1_200) {
                postAgentEvent(requestId.take(80), JSONObject().put("type", "error").put("message", "The task or request ID is invalid."))
                return
            }
            cancelledPlanRequests.remove(requestId)
            runOnUiThread { requestPlanWithConsent(task.trim(), requestId) }
        }

        @JavascriptInterface fun startPlan(requestId: String) {
            val task = agentPlans[requestId] ?: run {
                postAgentEvent(requestId, JSONObject().put("type", "error").put("message", "This plan expired. Ask for a new plan."))
                return
            }
            val run = AgentRun(AtomicBoolean(false))
            if (agentRuns.putIfAbsent(requestId, run) != null) return
            agentIo.execute { executeAgentPlan(requestId, task, run) }
        }

        @JavascriptInterface fun cancel(requestId: String?) {
            if (requestId.isNullOrBlank()) {
                agentRuns.values.forEach { it.cancelled.set(true) }
                agentPlans.keys.toList().forEach { id ->
                    if (agentPlans.remove(id) != null) postAgentEvent(id, JSONObject().put("type", "cancelled"))
                }
            } else {
                val run = agentRuns[requestId]
                if (run != null) run.cancelled.set(true)
                else if (agentPlans.remove(requestId) != null) postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
                else cancelledPlanRequests.add(requestId)
            }
        }

        @JavascriptInterface fun actionResult(requestId: String, index: Int, success: Boolean, message: String?) {
            val run = agentRuns[requestId] ?: return
            if (run.waitingIndex != index) return
            run.actionSuccess = success
            run.actionError = message.orEmpty().take(180)
            run.actionLatch.countDown()
        }
    }

    private data class AgentTask(val task: String, val provider: String, val config: ProviderConfig, val plan: VisionAgent.Plan)
    private class AgentRun(val cancelled: AtomicBoolean) {
        @Volatile var waitingIndex = -1
        @Volatile var actionSuccess = false
        @Volatile var actionError = ""
        @Volatile var actionLatch = CountDownLatch(0)
    }

    private fun requestPlanWithConsent(task: String, requestId: String) {
        if (task.isBlank()) { postAgentEvent(requestId, JSONObject().put("type", "error").put("message", "Describe the task first.")); return }
        val editor = activeEditorId
        if (editor == null || editor !in EDITORS) { postAgentEvent(requestId, JSONObject().put("type", "error").put("message", "Open one of the six included editors before asking the agent to work.")); return }
        val provider = providerStore.activeProvider()
        val config = providerStore.get(provider)
        if (config.apiKey.isBlank() || config.model.isBlank()) {
            AlertDialog.Builder(this).setTitle("Set up an AI provider")
                .setMessage("Add a vision-capable model and API key in Settings before using visual control.")
                .setPositiveButton("Settings") { _, _ -> startActivity(Intent(this, SettingsActivity::class.java)) }
                .setNegativeButton("Cancel", null).show()
            postAgentEvent(requestId, JSONObject().put("type", "error").put("message", "Configure a vision-capable model and API key in Settings."))
            return
        }
        val providerName = when (provider) { "gemini" -> "Gemini"; "anthropic" -> "Claude"; else -> "OpenAI-compatible" }
        AlertDialog.Builder(this).setTitle("Share this screen for this task?")
            .setMessage("The task text and a screenshot of the open $editor editor will be sent to $providerName. If you approve the plan, updated screenshots will also be sent while it runs. Screenshots are not saved by ArtCraft Studio. The provider may process or retain data under its own policy. Do not include private content unless you agree to share it.")
            .setPositiveButton("Share & make plan") { _, _ -> captureForAgent(requestId, task, provider, config) }
            .setNegativeButton("Cancel") { _, _ ->
                postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
            }
            .setOnCancelListener {
                postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
            }
            .show()
    }

    private fun dismissKeyboard() {
        val manager = getSystemService(INPUT_METHOD_SERVICE) as? InputMethodManager ?: return
        currentFocus?.windowToken?.let { manager.hideSoftInputFromWindow(it, 0) }
    }

    private fun captureForAgent(requestId: String, task: String, provider: String, config: ProviderConfig) {
        if (cancelledPlanRequests.remove(requestId)) {
            postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
            return
        }
        dismissKeyboard()
        webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.prepareAgentCapture && window.ArtCraftStudio.prepareAgentCapture()") { }
        webView.postDelayed({
            captureWindowBase64 { screenshot ->
                webView.post { webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.restoreAgentPanel && window.ArtCraftStudio.restoreAgentPanel()") { } }
                if (cancelledPlanRequests.remove(requestId)) {
                    postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
                    return@captureWindowBase64
                }
                if (screenshot == null) {
                    postAgentEvent(requestId, JSONObject().put("type", "error").put("message", "Could not capture the current editor screen."))
                    return@captureWindowBase64
                }
                postAgentEvent(requestId, JSONObject().put("type", "progress").put("index", 0).put("total", 1).put("description", "Preparing a plan from the approved screenshot"))
                agentIo.execute {
                    runCatching { VisionAgent.createPlan(provider, config, task, screenshot) }
                        .onSuccess { plan ->
                            if (cancelledPlanRequests.remove(requestId)) {
                                postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
                            } else {
                                agentPlans[requestId] = AgentTask(task, provider, config, plan)
                                val payload = JSONObject().put("summary", plan.summary).put("steps", JSONArray().also { array ->
                                    plan.steps.forEach { array.put(JSONObject().put("type", it.type).put("description", it.approvalLabel())) }
                                })
                                webView.post { webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.onAgentPlan(${JSONObject.quote(requestId)},$payload)") { } }
                            }
                        }
                        .onFailure { error ->
                            if (cancelledPlanRequests.remove(requestId)) postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
                            else postAgentEvent(requestId, JSONObject().put("type", "error").put("message", error.message ?: "The provider could not make a plan. Check its vision support and Settings."))
                        }
                }
            }
        }, 180)
    }

    private fun executeAgentPlan(requestId: String, task: AgentTask, run: AgentRun) {
        try {
            val approvedSteps = task.plan.steps.filterNot { it.type == "done" }
            val completed = ArrayList<String>()
            for (index in approvedSteps.indices) {
                if (run.cancelled.get()) { postAgentEvent(requestId, JSONObject().put("type", "cancelled")); return }
                val approved = approvedSteps[index]
                val step = if (index == 0) approved else {
                    postAgentEvent(requestId, JSONObject().put("type", "progress").put("index", index).put("total", approvedSteps.size).put("description", "Checking the current screen"))
                    val screenshot = captureWebViewBase64Blocking() ?: throw IllegalStateException("Could not capture the updated editor screen.")
                    val context = "Approved summary: ${task.plan.summary}. Approved steps: ${task.plan.steps.joinToString("; ") { it.approvalLabel() }}. Completed actions: ${completed.joinToString("; ")}. The next approved step is: ${approved.approvalLabel()}. Return exactly one action for that next step, or done if the step is already complete. Use the same action type; do not broaden the task."
                    val candidate = VisionAgent.createPlan(task.provider, task.config, task.task, screenshot, context).steps.firstOrNull()
                        ?: throw IllegalStateException("The provider did not return a next action.")
                    if (candidate.type == "done") break
                    if (candidate.type != approved.type) throw IllegalStateException("The refreshed action did not match the approved step type; stopped safely.")
                    when (approved.type) {
                        "type" -> {
                            if (candidate.text != approved.text) throw IllegalStateException("The refreshed text differed from the approved text; stopped safely.")
                            approved
                        }
                        "key", "wait" -> {
                            if (approved.type == "key" && candidate.key != approved.key) throw IllegalStateException("The refreshed key differed from the approved key; stopped safely.")
                            approved
                        }
                        else -> candidate.copy(description = approved.description)
                    }
                }
                if (step.type == "done") break
                if (run.cancelled.get()) { postAgentEvent(requestId, JSONObject().put("type", "cancelled")); return }
                postAgentEvent(requestId, JSONObject().put("type", "progress").put("index", index).put("total", approvedSteps.size).put("description", step.description))
                val latch = CountDownLatch(1)
                run.waitingIndex = index; run.actionSuccess = false; run.actionError = ""; run.actionLatch = latch
                val js = "window.ArtCraftStudio && window.ArtCraftStudio.runAgentAction(${JSONObject.quote(requestId)},$index,${step.toJson()})"
                webView.post { webView.evaluateJavascript(js) { } }
                if (!latch.await(20, TimeUnit.SECONDS)) throw IllegalStateException("The editor did not finish the visual action in time.")
                if (!run.actionSuccess) throw IllegalStateException(run.actionError.ifBlank { "The editor rejected the visual action." })
                completed += step.description
            }
            postAgentEvent(requestId, JSONObject().put("type", "complete"))
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            postAgentEvent(requestId, JSONObject().put("type", "cancelled"))
        } catch (error: Exception) {
            postAgentEvent(requestId, JSONObject().put("type", "error").put("message", error.message ?: "The visual workflow stopped."))
        } finally {
            agentRuns.remove(requestId)
            agentPlans.remove(requestId)
            cancelledPlanRequests.remove(requestId)
        }
    }

    private fun captureWebViewBase64Blocking(): String? {
        val latch = CountDownLatch(1)
        var result: String? = null
        runOnUiThread {
            webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.prepareAgentCapture && window.ArtCraftStudio.prepareAgentCapture()") { }
            webView.postDelayed({
                captureWindowBase64 { bitmapData ->
                    webView.post {
                        result = bitmapData
                        webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.restoreAgentPanel && window.ArtCraftStudio.restoreAgentPanel()") { }
                        latch.countDown()
                    }
                }
            }, 140)
        }
        return if (latch.await(8, TimeUnit.SECONDS)) result else null
    }

    /** PixelCopy captures the actual WebView/window surface, including WebGL/WebGPU canvas output. */
    private fun captureWindowBase64(callback: (String?) -> Unit) {
        if (!::webView.isInitialized || !::pixelCopyHandler.isInitialized) { callback(null); return }
        val decor = window.decorView
        val width = decor.width
        val height = decor.height
        if (width <= 0 || height <= 0) { callback(null); return }
        val bitmap = runCatching { Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888) }.getOrNull()
        if (bitmap == null) { callback(null); return }
        PixelCopy.request(window, bitmap, { result ->
            val encoded = if (result == PixelCopy.SUCCESS) runCatching { compressScreenshot(bitmap) }.getOrNull() else null
            bitmap.recycle()
            callback(encoded)
        }, pixelCopyHandler)
    }

    private fun compressScreenshot(source: Bitmap): String? {
        val scale = minOf(1f, 1024f / maxOf(source.width, source.height).toFloat())
        val scaled = if (scale < 1f) Bitmap.createScaledBitmap(source, (source.width * scale).toInt().coerceAtLeast(1), (source.height * scale).toInt().coerceAtLeast(1), true) else source
        return try {
            val bytes = java.io.ByteArrayOutputStream().use { output ->
                if (!scaled.compress(Bitmap.CompressFormat.JPEG, 68, output)) return null
                output.toByteArray()
            }
            Base64.encodeToString(bytes, Base64.NO_WRAP)
        } catch (_: Exception) {
            null
        } finally {
            if (scaled !== source) scaled.recycle()
        }
    }

    private fun postAgentEvent(requestId: String, event: JSONObject) {
        if (!::webView.isInitialized) return
        val id = JSONObject.quote(requestId)
        webView.post { webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.onAgentEvent($id,$event)") { } }
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_OPEN) {
            val result = if (resultCode == RESULT_OK && data?.data != null) arrayOf(data.data!!) else null
            fileChooserCallback.getAndSet(null)?.onReceiveValue(result)
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
            } else file?.delete()
        }
    }

    override fun onPause() { if (::webView.isInitialized) webView.onPause(); super.onPause() }
    override fun onResume() { super.onResume(); if (::webView.isInitialized) webView.onResume() }
    override fun onSaveInstanceState(outState: Bundle) { if (::webView.isInitialized) webView.saveState(outState); super.onSaveInstanceState(outState) }

    override fun onBackPressed() {
        if (!::webView.isInitialized) { super.onBackPressed(); return }
        webView.evaluateJavascript("window.ArtCraftStudio && window.ArtCraftStudio.goHome && window.ArtCraftStudio.goHome()") { result ->
            if (result == "false" || result == "null") finish()
        }
    }

    override fun onDestroy() {
        agentRuns.values.forEach { it.cancelled.set(true) }
        agentIo.shutdownNow(); io.shutdown()
        fileChooserCallback.getAndSet(null)?.onReceiveValue(null)
        saveOutputs.values.forEach { runCatching { it.close() } }
        saveFiles.values.forEach { it.delete() }
        if (::webView.isInitialized) webView.destroy()
        if (::pixelCopyThread.isInitialized) pixelCopyThread.quitSafely()
        super.onDestroy()
    }

    companion object {
        private val EDITORS = setOf("vectorcraft", "filmcraft", "lightcraft", "pdfcraft", "effectcraft", "designcraft")
        private const val REQUEST_OPEN = 7101
        private const val REQUEST_SAVE = 7102
        private const val SMOKE_TAG = "ArtCraftSmoke"
        const val EXTRA_SMOKE_EDITOR = "ai.artcraft.studio.SMOKE_EDITOR"
    }
}
