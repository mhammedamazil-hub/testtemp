package ai.artcraft.studio

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(28), dp(24), dp(24))
            setBackgroundColor(Color.rgb(15, 22, 35))
        }
        content.addView(TextView(this).apply {
            text = "ARTCRAFT STUDIO"
            textSize = 13f
            setTextColor(Color.rgb(126, 190, 255))
            letterSpacing = 0.12f
        })
        content.addView(TextView(this).apply {
            text = "Choose your workspace"
            textSize = 27f
            setTextColor(Color.WHITE)
            setPadding(0, dp(10), 0, dp(4))
        })
        content.addView(TextView(this).apply {
            text = "Seven native Rust editors. Android platform and file integrations are in progress."
            textSize = 14f
            setTextColor(Color.rgb(180, 193, 212))
            setPadding(0, 0, 0, dp(18))
        })

        val editors = listOf(
            "PhotoCraft" to PhotoCraftActivity::class.java,
            "VectorCraft" to VectorCraftActivity::class.java,
            "FilmCraft" to FilmCraftActivity::class.java,
            "LightCraft" to LightCraftActivity::class.java,
            "PdfCraft" to PdfCraftActivity::class.java,
            "EffectCraft" to EffectCraftActivity::class.java,
            "DesignCraft" to DesignCraftActivity::class.java,
        )
        for ((name, activity) in editors) {
            content.addView(Button(this).apply {
                text = "Open  $name"
                textSize = 16f
                gravity = Gravity.CENTER_VERTICAL
                setOnClickListener { startActivity(Intent(this@MainActivity, activity)) }
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(56)).apply {
                bottomMargin = dp(8)
            })
        }

        setContentView(ScrollView(this).apply { addView(content) })
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()
}
