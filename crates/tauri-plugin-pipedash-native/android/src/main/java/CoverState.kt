package com.henrique.pipedash.plugin

class CoverState {
    private var armed = false
    private var holdForFrontend = false
    private var awaitingFrontend = false
    private var foreground = true
    private var shown = false

    fun configure(armed: Boolean, holdForFrontend: Boolean): Boolean {
        this.armed = armed
        this.holdForFrontend = holdForFrontend
        return !armed && hide()
    }

    fun backgrounded(): Boolean {
        foreground = false
        if (!armed || shown) {
            return false
        }
        shown = true
        awaitingFrontend = holdForFrontend
        return true
    }

    fun resumed(): Boolean {
        foreground = true
        return !awaitingFrontend && hide()
    }

    fun released(): Boolean {
        awaitingFrontend = false
        return foreground && hide()
    }

    private fun hide(): Boolean {
        awaitingFrontend = false
        val wasShown = shown
        shown = false
        return wasShown
    }
}
