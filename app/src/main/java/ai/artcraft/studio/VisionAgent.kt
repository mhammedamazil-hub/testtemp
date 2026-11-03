package ai.artcraft.studio

import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.URL
import java.net.URLEncoder
import javax.net.ssl.HttpsURLConnection

/** Strict, bounded visual action protocol. It accepts UI gestures, never code or arbitrary URLs. */
internal object VisionAgent {
    private const val MAX_RESPONSE_BYTES = 1_000_000
    private const val MAX_STEPS = 12
    private const val MAX_TEXT_CHARS = 160

    data class Step(
        val type: String,
        val description: String,
        val x: Double = 0.5,
        val y: Double = 0.5,
        val x2: Double = 0.5,
        val y2: Double = 0.5,
        val dx: Double = 0.0,
        val dy: Double = 0.0,
        val text: String = "",
        val key: String = "",
        val ms: Int = 0,
    ) {
        fun approvalLabel(): String = when (type) {
            "type" -> "$description — type ‘$text’"
            "key" -> "$description — $key"
            else -> description
        }

        fun toJson(): JSONObject = JSONObject()
            .put("type", type)
            .put("description", description)
            .put("x", x)
            .put("y", y)
            .put("x2", x2)
            .put("y2", y2)
            .put("dx", dx)
            .put("dy", dy)
            .put("text", text)
            .put("key", key)
            .put("ms", ms)
    }

    data class Plan(val summary: String, val steps: List<Step>) {
        fun toJson(): JSONObject = JSONObject().put("summary", summary).put("steps", JSONArray().also { array -> steps.forEach { array.put(JSONObject().put("description", it.description).put("type", it.type)) } })
    }

    fun createPlan(provider: String, config: ProviderConfig, task: String, screenshotBase64: String, context: String = ""): Plan {
        require(provider in SecureProviderStore.PROVIDERS) { "Choose a supported AI provider in Settings." }
        require(config.apiKey.isNotBlank()) { "Add an API key in Settings before using the agent." }
        require(config.model.isNotBlank()) { "Choose a provider model in Settings before using the agent." }
        require(task.isNotBlank() && task.length <= 1_200) { "Describe a task of at most 1,200 characters." }
        require(screenshotBase64.isNotBlank() && screenshotBase64.length <= 1_500_000) { "The editor screenshot is unavailable or too large." }
        val prompt = buildPrompt(task, context)
        val body = requestBody(provider, config.model, prompt, screenshotBase64)
        val response = post(provider, config, body)
        return parsePlan(extractModelText(provider, response))
    }

    private fun buildPrompt(task: String, context: String): String = """
        You are the visual operator for ArtCraft Studio. The image shows the current editor screen. A human will review and approve the generated plan before any editor action. Return exactly one JSON object with keys "summary" and "steps". Do not use markdown.
        Task: ${task.replace("\u0000", " ").take(1_200)}
        Workflow context: ${context.replace("\u0000", " ").take(2_000).ifBlank { "Create the initial plan from the visible screen." }}
        Plan safe, visible actions in the open editor only. Do not ask for or produce code, scripts, shell commands, arbitrary file paths, network requests, or actions outside the editor. Do not claim a file was saved unless the screen confirms it. If you cannot identify the right editor control, return one "done" action explaining the limitation rather than guessing.
        Coordinates are normalized from 0.0 to 1.0 within the editor viewport only (top-left 0,0). Never target the ArtCraft Studio header or Android system UI. Maximum 12 steps. Allowed action types: click, drag, scroll, type, key, wait, done.
        Each step must include a short "description". click/scroll need x,y. drag needs x,y,x2,y2. scroll may include dx,dy normalized to -1..1. type needs short ordinary interface text only (not code, scripts, or multiline text). key must be one of Enter, Tab, Escape, Backspace, Delete, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Ctrl+Z, Ctrl+Y, Ctrl+S, Ctrl+A, Shift+Tab. wait may use ms from 100 to 3000. Put "done" last if finished. Example: {"summary":"Create a simple title card","steps":[{"type":"click","x":0.4,"y":0.2,"description":"Open the title tool"},{"type":"done","description":"Stop for review"}]}
    """.trimIndent()

    private fun requestBody(provider: String, model: String, prompt: String, image: String): JSONObject {
        val imagePart = JSONObject().put("mime_type", "image/jpeg").put("data", image)
        return when (provider) {
            "gemini" -> JSONObject()
                .put("contents", JSONArray().put(JSONObject().put("role", "user").put("parts", JSONArray()
                    .put(JSONObject().put("text", prompt))
                    .put(JSONObject().put("inline_data", imagePart)))))
                .put("generationConfig", JSONObject().put("maxOutputTokens", 1_600).put("responseMimeType", "application/json").put("temperature", 0.1))
            "anthropic" -> JSONObject()
                .put("model", model)
                .put("max_tokens", 1_600)
                .put("temperature", 0.1)
                .put("system", "Return strict JSON for the approved ArtCraft visual-action protocol. Never output code or arbitrary commands.")
                .put("messages", JSONArray().put(JSONObject().put("role", "user").put("content", JSONArray()
                    .put(JSONObject().put("type", "text").put("text", prompt))
                    .put(JSONObject().put("type", "image").put("source", JSONObject().put("type", "base64").put("media_type", "image/jpeg").put("data", image))))))
            else -> {
                val openAiImage = JSONObject().put("url", "data:image/jpeg;base64,$image").put("detail", "low")
                JSONObject()
                    .put("model", model)
                    .put("max_tokens", 1_600)
                    .put("temperature", 0.1)
                    .put("response_format", JSONObject().put("type", "json_object"))
                    .put("messages", JSONArray().put(JSONObject().put("role", "user").put("content", JSONArray()
                        .put(JSONObject().put("type", "text").put("text", prompt))
                        .put(JSONObject().put("type", "image_url").put("image_url", openAiImage)))))
            }
        }
    }

    private fun post(provider: String, config: ProviderConfig, body: JSONObject): String {
        val rawEndpoint = config.endpoint.trim().replace("{model}", URLEncoder.encode(config.model, Charsets.UTF_8.name()))
        val endpoint = URL(rawEndpoint)
        require(endpoint.protocol == "https" && endpoint.host.isNotBlank() && endpoint.userInfo == null) { "Provider endpoint must be a valid HTTPS URL." }
        val sensitiveQuery = endpoint.query.orEmpty().split('&').map { it.substringBefore('=').lowercase() }
        require(sensitiveQuery.none { it in setOf("key", "api_key", "api-key", "token", "access_token", "authorization") }) { "Keep credentials in the encrypted API key field, not in the endpoint URL." }
        val parsed = URL(rawEndpoint)
        val connection = (parsed.openConnection() as? HttpsURLConnection) ?: throw IllegalArgumentException("Provider endpoint must use HTTPS.")
        try {
            connection.requestMethod = "POST"
            connection.connectTimeout = 20_000
            connection.readTimeout = 35_000
            connection.instanceFollowRedirects = false
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
            connection.setRequestProperty("Accept", "application/json")
            when (provider) {
                "gemini" -> connection.setRequestProperty("x-goog-api-key", config.apiKey)
                "anthropic" -> {
                    connection.setRequestProperty("x-api-key", config.apiKey)
                    connection.setRequestProperty("anthropic-version", "2023-06-01")
                }
                "openai" -> connection.setRequestProperty("Authorization", "Bearer ${config.apiKey}")
            }
            connection.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            val status = connection.responseCode
            if (status !in 200..299) throw IllegalStateException("The selected provider returned HTTP $status. Check its model, key, and endpoint in Settings.")
            val response = connection.inputStream.use { readBounded(it, MAX_RESPONSE_BYTES) }
            return response.toString(Charsets.UTF_8)
        } finally {
            connection.disconnect()
        }
    }

    private fun readBounded(input: InputStream, maxBytes: Int): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8_192)
        var total = 0
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            total += count
            require(total <= maxBytes) { "Provider response exceeded the 1 MB safety limit." }
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private fun extractModelText(provider: String, response: String): String {
        val root = JSONObject(response)
        return when (provider) {
            "gemini" -> root.optJSONArray("candidates")?.optJSONObject(0)?.optJSONObject("content")
                ?.optJSONArray("parts")?.let { parts -> (0 until parts.length()).joinToString("\n") { parts.optJSONObject(it)?.optString("text").orEmpty() } }
            "anthropic" -> root.optJSONArray("content")?.let { parts -> (0 until parts.length()).joinToString("\n") { parts.optJSONObject(it)?.optString("text").orEmpty() } }
            else -> {
                val content = root.optJSONArray("choices")?.optJSONObject(0)?.optJSONObject("message")?.opt("content")
                when (content) {
                    is String -> content
                    is JSONArray -> (0 until content.length()).joinToString("\n") { content.optJSONObject(it)?.optString("text").orEmpty() }
                    else -> null
                }
            }
        }?.takeIf { it.isNotBlank() } ?: throw IllegalArgumentException("The provider returned no plan text. Check that the selected model supports vision and structured output.")
    }

    private fun parsePlan(raw: String): Plan {
        val jsonText = raw.trim().removePrefix("```json").removePrefix("```").removeSuffix("```").trim()
        val root = JSONObject(jsonText)
        val summary = cleanText(root.optString("summary"), 300).ifBlank { "Proposed visual workflow" }
        val source = root.optJSONArray("steps") ?: throw IllegalArgumentException("The provider did not return an action plan.")
        require(source.length() in 1..MAX_STEPS) { "The provider must return between 1 and $MAX_STEPS steps." }
        val steps = ArrayList<Step>(source.length())
        for (index in 0 until source.length()) {
            val item = source.optJSONObject(index) ?: throw IllegalArgumentException("The provider returned an invalid step.")
            val type = item.optString("type").lowercase()
            val description = cleanText(item.optString("description"), 180).ifBlank { "Editor action ${index + 1}" }
            val step = when (type) {
                "click" -> Step(type, description, coordinate(item, "x"), coordinate(item, "y"))
                "drag" -> Step(type, description, coordinate(item, "x"), coordinate(item, "y"), coordinate(item, "x2"), coordinate(item, "y2"))
                "scroll" -> Step(type, description, coordinate(item, "x"), coordinate(item, "y"), dx = boundedDelta(item, "dx"), dy = boundedDelta(item, "dy"))
                "type" -> {
                    val rawText = item.optString("text")
                    require(!rawText.contains('\n') && !rawText.contains('\r')) { "Multiline typing is not permitted by visual-agent actions." }
                    val text = cleanText(rawText, MAX_TEXT_CHARS)
                    require(text.isNotBlank()) { "The provider proposed empty typing; that action was rejected." }
                    require(text.all { it.isLetterOrDigit() || it.isWhitespace() || it in ".,!?'-_()&" }) {
                        "Only short plain-language text and simple names are permitted in visual-agent input."
                    }
                    require(!looksLikeCode(text)) { "Code-like text is not permitted in visual-agent input." }
                    Step(type, description, text = text)
                }
                "key" -> {
                    val key = cleanText(item.optString("key"), 24)
                    require(key in ALLOWED_KEYS) { "The provider proposed an unsupported keyboard action." }
                    Step(type, description, key = key)
                }
                "wait" -> Step(type, description, ms = item.optInt("ms", 400).coerceIn(100, 3_000))
                "done" -> Step(type, description)
                else -> throw IllegalArgumentException("The provider proposed an unsupported action type.")
            }
            steps += step
        }
        val doneIndex = steps.indexOfFirst { it.type == "done" }
        val boundedSteps = if (doneIndex >= 0) steps.take(doneIndex + 1).toMutableList() else steps
        if (boundedSteps.last().type != "done") boundedSteps.add(Step("done", "Pause for your review"))
        return Plan(summary, boundedSteps)
    }

    private fun coordinate(item: JSONObject, name: String): Double {
        val value = item.optDouble(name, Double.NaN)
        require(value.isFinite() && value in 0.0..1.0) { "The provider proposed an out-of-bounds cursor position." }
        return value
    }

    private fun boundedDelta(item: JSONObject, name: String): Double {
        val value = item.optDouble(name, 0.0)
        require(value.isFinite() && value in -1.0..1.0) { "The provider proposed an out-of-bounds scroll." }
        return value
    }

    private fun cleanText(value: String, max: Int): String = value.filter { it == '\t' || it.code >= 0x20 }.take(max)

    private fun looksLikeCode(value: String): Boolean {
        val lower = value.lowercase()
        val patterns = listOf("<script", "javascript:", "eval(", "function ", "import ", "export ", "npm ", "bash ", "powershell", "`", "=>", "</")
        return patterns.any(lower::contains)
    }

    private val ALLOWED_KEYS = setOf("Enter", "Tab", "Escape", "Backspace", "Delete", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Ctrl+Z", "Ctrl+Y", "Ctrl+S", "Ctrl+A", "Shift+Tab")
}
