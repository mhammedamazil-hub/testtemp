package ai.artcraft.studio

import android.content.Intent
import android.os.SystemClock
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith

/** Launch each bundled upstream editor in the real Android WebView and await its visible canvas. */
@RunWith(AndroidJUnit4::class)
class EditorStartupSmokeTest {
    @Test
    fun allSevenEditorsReachCanvasSurface() {
        EDITORS.forEach { editor ->
            val context = ApplicationProvider.getApplicationContext<android.content.Context>()
            val intent = Intent(context, MainActivity::class.java)
                .putExtra(MainActivity.EXTRA_SMOKE_EDITOR, editor)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ActivityScenario.launch<MainActivity>(intent).use { scenario ->
                val deadline = SystemClock.uptimeMillis() + EDITOR_START_TIMEOUT_MS
                var observed: String? = null
                var evidence = "no startup evidence available"
                while (SystemClock.uptimeMillis() < deadline && observed != editor) {
                    scenario.onActivity { activity ->
                        observed = activity.smokeSurfaceEditor
                        evidence = activity.smokeStartupEvidence(editor)
                    }
                    if (observed != editor) SystemClock.sleep(500)
                }
                assertNotNull("$editor did not render a visible canvas within ${EDITOR_START_TIMEOUT_MS / 1_000}s; $evidence", observed)
                assertEquals("Wrong editor surface observed; $evidence", editor, observed)
                SystemClock.sleep(4_000)
                scenario.onActivity { activity ->
                    assertEquals("$editor reported a fatal startup error", null, activity.smokeFatalEditor)
                }
            }
            // Give WebView time to release the previous editor's renderer before the next launch.
            SystemClock.sleep(1_000)
        }
    }

    companion object {
        private val EDITORS = listOf("photocraft", "vectorcraft", "filmcraft", "lightcraft", "pdfcraft", "effectcraft", "designcraft")
        private const val EDITOR_START_TIMEOUT_MS = 120_000L
    }
}
