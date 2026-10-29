import { type RefObject, useCallback, useEffect, useMemo, useRef } from 'react'

import { useMobileNavStore } from '../../stores/mobileNavStore'
import { swipeProgress } from './navigation'
import { createSpringValue, type SpringValue } from './springValue'

const PARALLAX = 0.3

export type ElementRef<T extends HTMLElement = HTMLDivElement> = RefObject<T | null>

export type MobilePageMode = 'idle' | 'live' | 'overlay'

export interface MobilePageLayers {
  host: ElementRef
  page: ElementRef
  underlay: ElementRef
  overlay: ElementRef
}

export interface MobilePageMotion {
  motion: SpringValue
  mode: RefObject<MobilePageMode>
  width: RefObject<number>
  engage: (mode: MobilePageMode) => void
  clearLayers: () => void
}

export function useMobilePageMotion({ host, page, underlay, overlay }: MobilePageLayers): MobilePageMotion {
  const mode = useRef<MobilePageMode>('idle')
  const width = useRef(0)

  const render = useCallback(
    (offset: number) => {
      const progress = swipeProgress(offset, width.current)
      const overlaid = mode.current === 'overlay'
      const front = overlaid ? overlay.current : page.current
      const back = overlaid ? page.current : underlay.current

      if (front) {
        front.style.transform = `translate3d(${offset}px, 0, 0)`
      }
      if (back) {
        back.style.transform = `translate3d(${-PARALLAX * width.current * (1 - progress)}px, 0, 0)`
      }
      host.current?.style.setProperty('--depth', String(1 - progress))
      document.documentElement.style.setProperty('--nav-progress', String(progress))
    },
    [host, page, underlay, overlay]
  )

  const motion = useMemo(() => createSpringValue(0, render), [render])

  const engage = useCallback(
    (next: MobilePageMode) => {
      mode.current = next
      width.current = window.innerWidth
      if (host.current) {
        host.current.dataset.mode = next
      }
    },
    [host]
  )

  const clearLayers = useCallback(() => {
    mode.current = 'idle'
    delete host.current?.dataset.mode
    host.current?.style.removeProperty('--depth')
    document.documentElement.style.removeProperty('--nav-progress')
    for (const layer of [page.current, underlay.current, overlay.current]) {
      if (layer) {
        layer.style.transform = ''
      }
    }
    underlay.current?.replaceChildren()
    overlay.current?.replaceChildren()
    useMobileNavStore.getState().setTransition(null)
  }, [host, page, underlay, overlay])

  useEffect(
    () => () => {
      motion.grab()
      clearLayers()
    },
    [motion, clearLayers]
  )

  return { motion, mode, width, engage, clearLayers }
}
