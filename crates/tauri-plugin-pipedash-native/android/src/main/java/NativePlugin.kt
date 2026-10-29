package com.henrique.pipedash.plugin

import android.app.Activity
import android.app.UiModeManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.webkit.MimeTypeMap
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.view.WindowCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.IOException
import org.json.JSONObject

private const val PREFS = "pipedash.native"
private const val APPEARANCE_KEY = "appearance"
private const val BUILD_WATCH_UNSUPPORTED = "Build watching isn't available on Android"

fun isDarkTheme(context: Context): Boolean {
    val scheme = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(APPEARANCE_KEY, "system")
    val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
    return scheme == "dark" || (scheme != "light" && night == Configuration.UI_MODE_NIGHT_YES)
}

@InvokeArg
class HapticArgs {
    lateinit var kind: String
}

@InvokeArg
class AppearanceArgs {
    lateinit var scheme: String
}

@InvokeArg
class PrivacyCoverArgs {
    var armed: Boolean = false
    var holdForFrontend: Boolean = false
}

@InvokeArg
class ShareArgs {
    lateinit var path: String
}

@InvokeArg
class ShareRect {
    var x: Double = 0.0
    var y: Double = 0.0
    var width: Double = 0.0
    var height: Double = 0.0
}

@InvokeArg
class ShareLinkArgs {
    lateinit var url: String
    lateinit var sourceRect: ShareRect
}

@InvokeArg
class AuthSessionArgs {
    lateinit var url: String
}

@InvokeArg
class ConfigureNotificationsArgs {
    var enabled: Boolean = false
}

@TauriPlugin
class NativePlugin(private val host: Activity) : Plugin(host) {
    private var webView: WebView? = null
    private var safeArea: SafeAreaBridge? = null
    private var appFlowActive = false
    private var pausedByAppFlow = false
    private var stoppedSincePause = false
    private var authInvoke: Invoke? = null
    private var authLeftApp = false
    private var exportPath: String? = null
    private var pendingTap: JSObject? = null

    private val prefs get() = host.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    override fun load(webView: WebView) {
        this.webView = webView
        host.runOnUiThread {
            safeArea = SafeAreaBridge(webView)
            applyAppearance()
        }
        RefreshScheduler.reconcile(host)
        handleTap(host.intent)
    }

    override fun onNewIntent(intent: Intent) {
        handleTap(intent)
    }

    override fun onPause(activity: AppCompatActivity) {
        pausedByAppFlow = appFlowActive
        stoppedSincePause = false
        if (authInvoke != null) {
            authLeftApp = true
        }
    }

    override fun onStop(activity: AppCompatActivity) {
        stoppedSincePause = true
        if (!appFlowActive) {
            PrivacyCover.show(activity)
        }
    }

    override fun onResume(activity: AppCompatActivity) {
        PrivacyCover.resumed()
        webView?.evaluateJavascript("window.dispatchEvent(new Event('pipedash:accessibility-change'))", null)
        if (authInvoke != null && authLeftApp) {
            finishAuthSession()
        }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        applyAppearance()
    }

    private fun applyAppearance() {
        host.runOnUiThread {
            val dark = isDarkTheme(host)
            val controller = WindowCompat.getInsetsController(host.window, host.window.decorView)
            controller.isAppearanceLightStatusBars = !dark
            controller.isAppearanceLightNavigationBars = !dark
        }
    }

    private fun handleTap(intent: Intent?) {
        val raw = intent?.getStringExtra(BackgroundNotifier.EXTRA) ?: return
        intent.removeExtra(BackgroundNotifier.EXTRA)
        val source = try {
            JSONObject(raw)
        } catch (_: org.json.JSONException) {
            return
        }
        val payload = JSObject().put("actionId", "tap")
        for (name in listOf("key", "pipelineId", "providerId")) {
            if (!source.isNull(name)) {
                payload.put(name, source.getString(name))
            }
        }
        if (hasListener("notificationAction")) {
            trigger("notificationAction", payload as JSObject)
        } else {
            pendingTap = payload as JSObject
        }
    }

    @Command
    override fun registerListener(invoke: Invoke) {
        super.registerListener(invoke)
        val tap = pendingTap
        if (tap != null && hasListener("notificationAction")) {
            pendingTap = null
            trigger("notificationAction", tap)
        }
    }

    @Command
    fun haptic(invoke: Invoke) {
        val args = invoke.parseArgs(HapticArgs::class.java)
        invoke.resolve()
        val modern = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        val feedback = when (args.kind) {
            "selection" -> HapticFeedbackConstants.CLOCK_TICK
            "light", "soft" -> HapticFeedbackConstants.VIRTUAL_KEY
            "medium" -> HapticFeedbackConstants.KEYBOARD_TAP
            "heavy", "warning" -> HapticFeedbackConstants.LONG_PRESS
            "rigid" -> HapticFeedbackConstants.CONTEXT_CLICK
            "success" -> if (modern) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
            "error" -> if (modern) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            else -> return
        }
        host.runOnUiThread { webView?.performHapticFeedback(feedback) }
    }

    @Command
    fun setAppearance(invoke: Invoke) {
        val args = invoke.parseArgs(AppearanceArgs::class.java)
        prefs.edit().putString(APPEARANCE_KEY, args.scheme).apply()
        applyAppearance()
        invoke.resolve()
    }

    @Command
    fun accessibilityState(invoke: Invoke) {
        val contrast = Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
            host.getSystemService(UiModeManager::class.java).contrast > 0f
        invoke.resolve(JSObject().put("reduceTransparency", false).put("increaseContrast", contrast) as JSObject)
    }

    private fun unsupportedBuildWatch(): JSObject =
        JSObject()
            .put("supported", false)
            .put("available", false)
            .put("reason", BUILD_WATCH_UNSUPPORTED)
            .put("watch", JSONObject.NULL) as JSObject

    @Command
    fun buildWatchStatus(invoke: Invoke) {
        invoke.resolve(unsupportedBuildWatch())
    }

    @Command
    fun startBuildWatch(invoke: Invoke) {
        invoke.reject(BUILD_WATCH_UNSUPPORTED)
    }

    @Command
    fun updateBuildWatch(invoke: Invoke) {
        invoke.resolve(unsupportedBuildWatch())
    }

    @Command
    fun stopBuildWatch(invoke: Invoke) {
        invoke.resolve(unsupportedBuildWatch())
    }

    @Command
    fun openSettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.fromParts("package", host.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            host.startActivity(intent)
            invoke.resolve()
        } catch (_: ActivityNotFoundException) {
            invoke.reject("Couldn't open Settings")
        }
    }

    @Command
    fun setBadge(invoke: Invoke) {
        invoke.resolve()
    }

    @Command
    fun configurePrivacyCover(invoke: Invoke) {
        val args = invoke.parseArgs(PrivacyCoverArgs::class.java)
        host.runOnUiThread {
            PrivacyCover.configure(host, args.armed, args.holdForFrontend)
            invoke.resolve()
        }
    }

    @Command
    fun releasePrivacyCover(invoke: Invoke) {
        host.runOnUiThread {
            PrivacyCover.release()
            invoke.resolve()
        }
    }

    @Command
    fun shareFile(invoke: Invoke) {
        val args = invoke.parseArgs(ShareArgs::class.java)
        val file = File(args.path)
        if (!file.exists()) {
            invoke.reject("The file to share doesn't exist")
            return
        }
        if (exportPath != null) {
            invoke.reject("An export picker is already open")
            return
        }
        val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase())
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .setType(mime ?: "application/octet-stream")
            .putExtra(Intent.EXTRA_TITLE, file.name)
        exportPath = args.path
        appFlowActive = true
        try {
            startActivityForResult(invoke, intent, "exportResult")
        } catch (e: ActivityNotFoundException) {
            exportPath = null
            appFlowActive = false
            invoke.reject("Couldn't open the export picker")
        }
    }

    @ActivityCallback
    fun exportResult(invoke: Invoke, result: ActivityResult) {
        appFlowActive = false
        val path = exportPath
        exportPath = null
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null || path == null) {
            invoke.resolve(JSObject().put("completed", false) as JSObject)
            return
        }
        Thread {
            try {
                val output = host.contentResolver.openOutputStream(uri, "w") ?: throw IOException()
                output.use { out -> File(path).inputStream().use { it.copyTo(out) } }
                invoke.resolve(JSObject().put("completed", true) as JSObject)
            } catch (_: IOException) {
                invoke.reject("Couldn't save the file")
            }
        }.start()
    }

    @Command
    fun shareLink(invoke: Invoke) {
        val args = invoke.parseArgs(ShareLinkArgs::class.java)
        val scheme = Uri.parse(args.url).scheme?.lowercase()
        if (scheme != "http" && scheme != "https") {
            invoke.reject("The link must be an HTTP or HTTPS URL")
            return
        }
        val rect = args.sourceRect
        val finite = listOf(rect.x, rect.y, rect.width, rect.height).all { it.isFinite() }
        if (!finite || rect.width <= 0 || rect.height <= 0) {
            invoke.reject("The share button position isn't valid")
            return
        }
        val send = Intent(Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, args.url)
        appFlowActive = true
        try {
            startActivityForResult(invoke, Intent.createChooser(send, null), "shareResult")
        } catch (e: ActivityNotFoundException) {
            appFlowActive = false
            invoke.reject("Nothing is available to present the share sheet")
        }
    }

    @ActivityCallback
    fun shareResult(invoke: Invoke, result: ActivityResult) {
        appFlowActive = false
        invoke.resolve(JSObject().put("completed", true) as JSObject)
    }

    @Command
    fun openAuthSession(invoke: Invoke) {
        val args = invoke.parseArgs(AuthSessionArgs::class.java)
        val uri = Uri.parse(args.url)
        if (uri.scheme?.lowercase() != "https") {
            invoke.reject("The sign-in page must be an HTTPS URL")
            return
        }
        host.runOnUiThread {
            finishAuthSession()
            authInvoke = invoke
            authLeftApp = false
            appFlowActive = true
            val scheme = if (isDarkTheme(host)) {
                CustomTabsIntent.COLOR_SCHEME_DARK
            } else {
                CustomTabsIntent.COLOR_SCHEME_LIGHT
            }
            try {
                CustomTabsIntent.Builder().setColorScheme(scheme).build().launchUrl(host, uri)
            } catch (_: ActivityNotFoundException) {
                authInvoke = null
                appFlowActive = false
                invoke.reject("Couldn't open the sign-in page")
            }
        }
    }

    @Command
    fun closeAuthSession(invoke: Invoke) {
        host.runOnUiThread {
            finishAuthSession()
            host.startActivity(
                Intent(host, host.javaClass)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            )
            invoke.resolve()
        }
    }

    private fun finishAuthSession() {
        val invoke = authInvoke
        authInvoke = null
        appFlowActive = false
        invoke?.resolve(JSObject().put("cancelled", false) as JSObject)
    }

    @Command
    fun localNotificationSettings(invoke: Invoke) {
        invoke.resolve(RefreshScheduler.settings(host))
    }

    @Command
    fun configureLocalNotifications(invoke: Invoke) {
        val args = invoke.parseArgs(ConfigureNotificationsArgs::class.java)
        try {
            invoke.resolve(RefreshScheduler.configure(host, args.enabled))
        } catch (e: IllegalStateException) {
            invoke.reject(e.message)
        }
    }

    @Command
    fun notificationRefreshReady(invoke: Invoke) {
        invoke.resolve()
    }

    @Command
    fun finishNotificationRefresh(invoke: Invoke) {
        invoke.resolve()
    }

    @Command
    fun resumedInApp(invoke: Invoke) {
        invoke.resolve(JSObject().put("value", pausedByAppFlow || !stoppedSincePause) as JSObject)
    }
}
