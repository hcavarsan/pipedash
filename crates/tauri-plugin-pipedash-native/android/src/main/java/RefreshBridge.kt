package com.henrique.pipedash.plugin

import android.content.Context

object RefreshBridge {
    init {
        System.loadLibrary("pipedash_desktop_lib")
    }

    @JvmStatic
    external fun runRefresh(context: Context, dataDir: String): String

    @JvmStatic
    external fun cancelRefresh()
}
