package com.henrique.pipedash.plugin

import android.webkit.WebView
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature

class SafeAreaBridge(private val webView: WebView) {
    private var lastInsets: Insets? = null
    private var scriptHandler: ScriptHandler? = null

    init {
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            publish(insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()))
            ViewCompat.onApplyWindowInsets(view, insets)
        }
        ViewCompat.requestApplyInsets(webView)
    }

    private fun publish(insets: Insets) {
        if (insets == lastInsets) {
            return
        }
        lastInsets = insets
        val density = webView.resources.displayMetrics.density
        val js = """
            (() => {
              const apply = () => {
                const style = document.documentElement.style;
                style.setProperty('--native-safe-area-top', '${insets.top / density}px');
                style.setProperty('--native-safe-area-right', '${insets.right / density}px');
                style.setProperty('--native-safe-area-bottom', '${insets.bottom / density}px');
                style.setProperty('--native-safe-area-left', '${insets.left / density}px');
              };
              if (document.documentElement) apply();
              else document.addEventListener('DOMContentLoaded', apply, { once: true });
            })();
        """.trimIndent()
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            scriptHandler?.remove()
            scriptHandler = WebViewCompat.addDocumentStartJavaScript(webView, js, setOf("*"))
        }
        webView.evaluateJavascript(js, null)
    }
}
