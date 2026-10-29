package com.henrique.pipedash.plugin

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CoverStateTest {
    @Test
    fun resumingKeepsTheCoverUntilTheFrontendReleasesIt() {
        val state = CoverState()
        state.configure(armed = true, holdForFrontend = true)

        assertTrue(state.backgrounded())
        assertFalse(state.resumed())
        assertFalse(state.resumed())
        assertTrue(state.released())
        assertFalse(state.released())
    }

    @Test
    fun aReleaseInTheBackgroundHidesOnTheNextResume() {
        val state = CoverState()
        state.configure(armed = true, holdForFrontend = true)

        assertTrue(state.backgrounded())
        assertFalse(state.released())
        assertTrue(state.resumed())
    }

    @Test
    fun withoutFrontendHoldTheCoverLeavesOnResume() {
        val state = CoverState()
        state.configure(armed = true, holdForFrontend = false)

        assertTrue(state.backgrounded())
        assertTrue(state.resumed())
    }

    @Test
    fun anUnarmedCoverNeverShowsAndDisarmingRemovesIt() {
        val state = CoverState()

        assertFalse(state.backgrounded())
        state.configure(armed = true, holdForFrontend = true)
        assertTrue(state.backgrounded())
        assertTrue(state.configure(armed = false, holdForFrontend = true))
        assertFalse(state.resumed())
    }
}
