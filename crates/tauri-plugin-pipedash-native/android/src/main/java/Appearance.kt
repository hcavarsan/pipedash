package com.henrique.pipedash.plugin

import android.content.Context
import android.content.res.Configuration

internal const val PREFS = "pipedash.native"
internal const val APPEARANCE_KEY = "appearance"

fun isDarkTheme(context: Context): Boolean {
    val scheme = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(APPEARANCE_KEY, "system")
    val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
    return scheme == "dark" || (scheme != "light" && night == Configuration.UI_MODE_NIGHT_YES)
}
