package com.henrique.pipedash.plugin

import android.app.Activity
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager

object PrivacyCover {
    private val state = CoverState()
    private var cover: View? = null

    fun configure(activity: Activity, armed: Boolean, holdForFrontend: Boolean) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            activity.setRecentsScreenshotEnabled(!armed)
        } else if (armed) {
            activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        } else {
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        if (state.configure(armed, holdForFrontend)) {
            hide()
        }
    }

    fun show(activity: Activity) {
        if (!state.backgrounded()) {
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
    }

    fun resumed() {
        if (state.resumed()) {
            hide()
        }
    }

    fun release() {
        if (state.released()) {
            hide()
        }
    }

    private fun hide() {
        val view = cover ?: return
        cover = null
        view.animate().alpha(0f).setDuration(150).withEndAction {
            (view.parent as? ViewGroup)?.removeView(view)
        }
    }
}
