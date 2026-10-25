import { useEffect } from 'react'

export function keyboardInset(layoutHeight: number, viewportHeight: number, viewportOffsetTop: number): number {
  return Math.max(0, Math.round(layoutHeight - viewportHeight - viewportOffsetTop))
}

export function useKeyboardInset(active: boolean): void {
  useEffect(() => {
    const viewport = window.visualViewport

    if (!active || !viewport) {
      return
    }

    const root = document.documentElement
    const update = () =>
      root.style.setProperty(
        '--keyboard-inset',
        `${keyboardInset(window.innerHeight, viewport.height, viewport.offsetTop)}px`
      )

    update()
    viewport.addEventListener('resize', update)
    viewport.addEventListener('scroll', update)

    return () => {
      viewport.removeEventListener('resize', update)
      viewport.removeEventListener('scroll', update)
      root.style.removeProperty('--keyboard-inset')
    }
  }, [active])
}
