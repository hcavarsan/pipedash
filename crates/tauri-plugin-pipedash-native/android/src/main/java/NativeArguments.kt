package com.henrique.pipedash.plugin

import app.tauri.annotation.InvokeArg

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
