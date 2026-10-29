package com.henrique.pipedash.plugin

import android.app.Activity
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager

object PrivacyCover {
    private var armed = false
    private var holdForFrontend = false
    private var awaitingFrontend = false
    private var foreground = true
    private var cover: View? = null

    fun configure(activity: Activity, armed: Boolean, holdForFrontend: Boolean) {
        this.armed = armed
        this.holdForFrontend = holdForFrontend
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            activity.setRecentsScreenshotEnabled(!armed)
        } else if (armed) {
            activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        } else {
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        if (!armed) {
            hide()
        }
    }

    fun show(activity: Activity) {
        foreground = false
        if (!armed || cover != null) {
            return
        }
        val view = View(activity)
        view.setBackgroundColor(if (isDarkTheme(activity)) 0xFF242424.toInt() else 0xFFFFFFFF.toInt())
        view.isClickable = true
        view.isFocusable = true
        (activity.window.decorView as ViewGroup).addView(
            view,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
        cover = view
        awaitingFrontend = holdForFrontend
    }

    fun resumed() {
        foreground = true
        if (!awaitingFrontend) {
            hide()
        }
    }

    fun release() {
        awaitingFrontend = false
        if (foreground) {
            hide()
        }
    }

    private fun hide() {
        awaitingFrontend = false
        val view = cover ?: return
        cover = null
        view.animate().alpha(0f).setDuration(150).withEndAction {
            (view.parent as? ViewGroup)?.removeView(view)
        }
    }
}
