package ai.artcraft.studio

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.concurrent.Executors

class SettingsActivity : Activity() {
    private val store by lazy { SecureProviderStore(this) }
    private val worker = Executors.newSingleThreadExecutor()
    private lateinit var provider: Spinner
    private lateinit var endpoint: EditText
    private lateinit var model: EditText
    private lateinit var apiKey: EditText
    private lateinit var status: TextView
    private var loading = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = Color.rgb(18, 20, 25)
        window.navigationBarColor = Color.rgb(18, 20, 25)
        val page = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(18), dp(12), dp(18), dp(18))
            setBackgroundColor(Color.rgb(18, 20, 25))
        }
        val heading = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        heading.addView(TextView(this).apply {
            text = "AI PROVIDERS"
            textSize = 18f
            setTextColor(Color.WHITE)
            typeface = android.graphics.Typeface.DEFAULT_BOLD
        }, LinearLayout.LayoutParams(0, dp(56), 1f))
        heading.addView(Button(this).apply { text = "DONE"; setOnClickListener { finish() } })
        page.addView(heading)

        val scroll = ScrollView(this)
        val fields = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(0, dp(8), 0, dp(16)) }
        fields.addView(label("Provider"))
        provider = Spinner(this).apply {
            adapter = ArrayAdapter(this@SettingsActivity, android.R.layout.simple_spinner_dropdown_item, listOf("Gemini", "Claude / Anthropic", "OpenAI-compatible"))
        }
        fields.addView(provider, matchWrap())
        fields.addView(label("API endpoint (HTTPS)"))
        endpoint = field("https://…", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI)
        fields.addView(endpoint, matchWrap())
        fields.addView(label("Model"))
        model = field("Provider model ID", InputType.TYPE_CLASS_TEXT)
        fields.addView(model, matchWrap())
        fields.addView(label("API key"))
        apiKey = field("Stored encrypted; leave blank to keep current key", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD)
        fields.addView(apiKey, matchWrap())
        fields.addView(TextView(this).apply {
            text = "Keys are encrypted with an Android Keystore-backed AES-GCM key and are not logged. The connection test sends only a short generic prompt to the selected provider. Project content is not sent by this screen."
            textSize = 13f
            setTextColor(Color.rgb(178, 185, 198))
            setPadding(0, dp(12), 0, dp(12))
        })
        val buttons = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        buttons.addView(Button(this).apply { text = "SAVE"; setOnClickListener { saveConfig() } }, LinearLayout.LayoutParams(0, dp(52), 1f))
        buttons.addView(Button(this).apply { text = "TEST"; setOnClickListener { testConnection() } }, LinearLayout.LayoutParams(0, dp(52), 1f))
        fields.addView(buttons)
        fields.addView(Button(this).apply {
            text = "VIEW LICENSE NOTICES"
            setOnClickListener { startActivity(android.content.Intent(this@SettingsActivity, LicenseActivity::class.java)) }
        }, matchWrap())
        fields.addView(Button(this).apply {
            text = "DELETE SAVED KEY"
            setOnClickListener {
                store.clearKey(providerId())
                apiKey.setText("")
                setStatus("Saved API key deleted from this device.")
            }
        }, matchWrap())
        fields.addView(TextView(this).apply {
            text = "Provider keys are local to this installation. Visual control is experimental: each task that shares a screenshot requires separate consent, and the agent shows a plan for your approval before any editor action. Use a vision-capable model."
            textSize = 12f
            setTextColor(Color.rgb(139, 148, 162))
            setPadding(0, dp(12), 0, dp(12))
        })
        status = TextView(this).apply { textSize = 14f; setTextColor(Color.rgb(136, 208, 177)); setPadding(0, dp(8), 0, dp(8)) }
        fields.addView(status)
        scroll.addView(fields)
        page.addView(scroll, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        setContentView(page)

        provider.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                if (!loading) {
                    store.setActiveProvider(providerId())
                    loadConfig()
                }
            }
        }
        provider.setSelection(SecureProviderStore.PROVIDERS.indexOf(store.activeProvider()).coerceAtLeast(0))
        loadConfig()
    }

    private fun loadConfig() {
        loading = true
        val config = store.get(providerId())
        endpoint.setText(config.endpoint)
        model.setText(config.model)
        apiKey.setText("")
        apiKey.hint = if (config.apiKey.isBlank()) "Enter API key" else "Key saved (hidden); enter replacement or leave blank"
        loading = false
    }

    private fun saveConfig(): Boolean {
        return try {
            store.setActiveProvider(providerId())
            store.save(providerId(), endpoint.text.toString(), model.text.toString(), apiKey.text.toString())
            apiKey.setText("")
            apiKey.hint = "Key saved (hidden); enter replacement or leave blank"
            setStatus("Configuration saved securely.")
            true
        } catch (e: IllegalArgumentException) {
            setStatus(e.message ?: "Check the endpoint, model and key.", error = true)
            false
        } catch (_: Exception) {
            setStatus("Could not save provider configuration.", error = true)
            false
        }
    }

    private fun testConnection() {
        if (!saveConfig()) return
        val config = store.get(providerId())
        if (config.apiKey.isBlank()) {
            setStatus("Add an API key before testing.", error = true)
            return
        }
        val selected = providerId()
        setStatus("Testing connection…")
        worker.execute {
            val result = runCatching { performConnectionTest(selected, config) }
            runOnUiThread {
                result.onSuccess { setStatus("Connection test succeeded (HTTP $it).") }
                    .onFailure { setStatus("Connection test failed. Check provider, endpoint, model, key, and network; response details are omitted to protect secrets.", error = true) }
            }
        }
    }

    private fun performConnectionTest(provider: String, config: ProviderConfig): Int {
        val modelName = config.model
        val body = when (provider) {
            "gemini" -> """{"contents":[{"parts":[{"text":"Reply with OK."}]}],"generationConfig":{"maxOutputTokens":4}}"""
            "anthropic" -> """{"model":${JSONObject.quote(modelName)},"max_tokens":4,"messages":[{"role":"user","content":"Reply with OK."}]}"""
            else -> """{"model":${JSONObject.quote(modelName)},"messages":[{"role":"user","content":"Reply with OK."}],"max_tokens":4}"""
        }
        val target = config.endpoint.replace("{model}", URLEncoder.encode(modelName, "UTF-8"))
        val connection = (URL(target).openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 12_000
            readTimeout = 12_000
            instanceFollowRedirects = false
            doOutput = true
            setRequestProperty("Content-Type", "application/json")
            when (provider) {
                "gemini" -> setRequestProperty("x-goog-api-key", config.apiKey)
                "anthropic" -> {
                    setRequestProperty("x-api-key", config.apiKey)
                    setRequestProperty("anthropic-version", "2023-06-01")
                }
                "openai" -> setRequestProperty("Authorization", "Bearer ${config.apiKey}")
            }
        }
        try {
            connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            val statusCode = connection.responseCode
            if (statusCode !in 200..299) throw IllegalStateException("Provider did not accept the connection test")
            return statusCode
        } finally {
            connection.disconnect()
        }
    }

    private fun providerId() = when (provider.selectedItemPosition) {
        0 -> "gemini"
        1 -> "anthropic"
        else -> "openai"
    }

    private fun setStatus(message: String, error: Boolean = false) {
        status.text = message
        status.setTextColor(if (error) Color.rgb(255, 143, 143) else Color.rgb(136, 208, 177))
    }

    private fun label(text: String) = TextView(this).apply {
        this.text = text
        textSize = 14f
        setTextColor(Color.rgb(213, 218, 229))
        setPadding(0, dp(16), 0, dp(4))
    }

    private fun field(hintText: String, type: Int) = EditText(this).apply {
        hint = hintText
        inputType = type
        setSingleLine(true)
        setTextColor(Color.WHITE)
        setHintTextColor(Color.rgb(137, 145, 160))
        setPadding(dp(12), dp(10), dp(12), dp(10))
        setBackgroundColor(Color.rgb(34, 38, 46))
    }

    private fun matchWrap() = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(6) }
    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    override fun onDestroy() {
        worker.shutdownNow()
        super.onDestroy()
    }
}
