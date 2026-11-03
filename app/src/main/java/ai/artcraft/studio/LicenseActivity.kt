package ai.artcraft.studio

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

class LicenseActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = Color.rgb(18, 20, 25)
        window.navigationBarColor = Color.rgb(18, 20, 25)
        val text = TextView(this).apply {
            text = readNotices()
            textSize = 13f
            setTextColor(Color.rgb(225, 229, 237))
            setPadding(dp(18), dp(18), dp(18), dp(24))
        }
        val scroll = ScrollView(this).apply {
            setBackgroundColor(Color.rgb(18, 20, 25))
            addView(text, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        setContentView(scroll)
    }

    private fun readNotices(): String {
        val sections = mutableListOf<String>()
        fun walk(path: String) {
            val entries = runCatching { assets.list(path).orEmpty() }.getOrDefault(emptyArray())
            if (entries.isNotEmpty()) {
                entries.sorted().forEach { name -> walk(if (path.isBlank()) name else "$path/$name") }
                return
            }
            val leaf = path.substringAfterLast('/').lowercase()
            if (!(leaf.contains("license") || leaf.contains("attribution") || leaf.contains("assets") || leaf.contains("notice"))) return
            val content = runCatching { assets.open(path).bufferedReader().use { it.readText() } }.getOrNull() ?: return
            sections += "\n\n===== ${path.removePrefix("licenses/")} =====\n$content"
        }
        runCatching { assets.list("licenses").orEmpty().forEach { walk("licenses/$it") } }
        return if (sections.isEmpty()) {
            "License files will be included by the Android APK workflow. If this screen is empty, install a package built by the Android APK workflow."
        } else {
            "ARTCRAFT STUDIO — OPEN-SOURCE NOTICES\n\nThe editor source projects have their own licenses. Bundled assets and third-party dependencies may have separate terms.\n" + sections.sorted().joinToString("\n")
        }
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()
}
