# WebView JavaScript bridge methods are accessed by name from JavaScript.
-keepclassmembers class ai.artcraft.studio.** {
    @android.webkit.JavascriptInterface <methods>;
}
