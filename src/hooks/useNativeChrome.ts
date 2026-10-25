import { useEffect } from 'react'

const LOCKED_VIEWPORT = 'width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover'

const preventGesture = (event: Event) => event.preventDefault()

const enableActiveStates = () => undefined

export function useNativeChrome(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    const viewport = document.querySelector<HTMLMetaElement>('meta[name="viewport"]')
    const original = viewport?.getAttribute('content') ?? null

    viewport?.setAttribute('content', LOCKED_VIEWPORT)
    document.addEventListener('touchstart', enableActiveStates, { passive: true })
    document.addEventListener('gesturestart', preventGesture)
    document.addEventListener('gesturechange', preventGesture)

    return () => {
      if (viewport && original !== null) {
        viewport.setAttribute('content', original)
      }
      document.removeEventListener('touchstart', enableActiveStates)
      document.removeEventListener('gesturestart', preventGesture)
      document.removeEventListener('gesturechange', preventGesture)
    }
  }, [active])
}
