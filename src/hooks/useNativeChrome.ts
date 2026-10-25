import { useEffect } from 'react'

const BODY_FONT = '-apple-system-body'
const BASE_BODY_PX = 17

const enableActiveStates = () => undefined

function watchTextScale(): () => void {
  if (!CSS.supports('font', BODY_FONT)) {
    return () => undefined
  }

  const root = document.documentElement
  const probe = document.createElement('div')

  probe.setAttribute('aria-hidden', 'true')
  probe.textContent = 'A'
  probe.style.cssText = `position:fixed;top:0;left:0;visibility:hidden;pointer-events:none;white-space:nowrap;font:${BODY_FONT}`
  document.body.appendChild(probe)

  const measure = () => {
    const px = parseFloat(getComputedStyle(probe).fontSize)

    if (px > 0) {
      root.style.setProperty('--ios-text-scale', (px / BASE_BODY_PX).toFixed(3))
    }
  }
  const onVisible = () => {
    if (document.visibilityState === 'visible') {
      measure()
    }
  }
  const observer = new ResizeObserver(measure)

  measure()
  observer.observe(probe)
  document.addEventListener('visibilitychange', onVisible)

  return () => {
    observer.disconnect()
    document.removeEventListener('visibilitychange', onVisible)
    probe.remove()
    root.style.removeProperty('--ios-text-scale')
  }
}

export function useNativeChrome(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    document.addEventListener('touchstart', enableActiveStates, { passive: true })
    const stopTextScale = watchTextScale()

    return () => {
      document.removeEventListener('touchstart', enableActiveStates)
      stopTextScale()
    }
  }, [active])
}
