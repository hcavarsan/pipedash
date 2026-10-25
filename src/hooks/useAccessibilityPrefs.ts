import { useEffect } from 'react'

import { mobileService, type NativeAccessibility } from '../services/mobile'
import { isMobilePlatform } from '../utils/platform'
import { isMobileShell } from './useIsMobile'

const REDUCE_TRANSPARENCY = '(prefers-reduced-transparency: reduce)'
const MORE_CONTRAST = '(prefers-contrast: more)'
const NAVIGATION_KEYS: Record<string, true> = {
  Tab: true,
  ArrowUp: true,
  ArrowDown: true,
  ArrowLeft: true,
  ArrowRight: true,
}

export function useAccessibilityPrefs(): void {
  useEffect(() => {
    const root = document.documentElement
    const transparency = window.matchMedia(REDUCE_TRANSPARENCY)
    const contrast = window.matchMedia(MORE_CONTRAST)
    let native: NativeAccessibility = { reduceTransparency: false, increaseContrast: false }
    let cancelled = false

    const apply = () => {
      root.toggleAttribute('data-reduce-transparency', transparency.matches || native.reduceTransparency)
      root.toggleAttribute('data-increase-contrast', contrast.matches || native.increaseContrast)
    }
    const refreshNative = () => {
      if (!isMobileShell() || !isMobilePlatform()) {
        return
      }

      mobileService
        .accessibility()
        .then((next) => {
          if (!cancelled) {
            native = next
            apply()
          }
        })
        .catch(() => undefined)
    }
    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        refreshNative()
      }
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (Object.hasOwn(NAVIGATION_KEYS, event.key)) {
        root.setAttribute('data-keyboard-focus', '')
      }
    }
    const onPointer = () => root.removeAttribute('data-keyboard-focus')

    apply()
    refreshNative()
    transparency.addEventListener('change', apply)
    contrast.addEventListener('change', apply)
    document.addEventListener('visibilitychange', onVisible)
    document.addEventListener('keydown', onKeyDown, true)
    document.addEventListener('pointerdown', onPointer, true)
    document.addEventListener('touchstart', onPointer, { capture: true, passive: true })

    return () => {
      cancelled = true
      transparency.removeEventListener('change', apply)
      contrast.removeEventListener('change', apply)
      document.removeEventListener('visibilitychange', onVisible)
      document.removeEventListener('keydown', onKeyDown, true)
      document.removeEventListener('pointerdown', onPointer, true)
      document.removeEventListener('touchstart', onPointer, true)
      root.removeAttribute('data-reduce-transparency')
      root.removeAttribute('data-increase-contrast')
      root.removeAttribute('data-keyboard-focus')
    }
  }, [])
}
