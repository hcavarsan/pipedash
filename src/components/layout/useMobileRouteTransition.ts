import { type RefObject, useEffect, useLayoutEffect, useRef } from 'react'

import { prefersReducedMotion } from '../../lib/motion'
import { type BarSnapshot, useMobileNavStore } from '../../stores/mobileNavStore'
import classes from './MobilePage.module.css'
import { type MobileTab, mobileChrome } from './mobileChrome'
import { transitionKind } from './navigation'
import { mountSnapshot, updateSnapshot } from './snapshots'
import type { MobilePageLayers, MobilePageMotion } from './useMobilePageMotion'
import { scrollerOf } from './useMobilePageScroll'

const FADE_MS = 160

interface Screen {
  key: string
  tab: MobileTab
  depth: number
  title: string
  backLabel: string | null
}

interface MobileRouteTransitionProps {
  pathname: string
  search: string
  from: string | null
  routeKey: string
  layers: MobilePageLayers
  motion: MobilePageMotion
  scrollTop: RefObject<number>
  collapse: RefObject<number>
  swiped: RefObject<boolean>
  measure: () => void
  arrive: () => void
}

export function useMobileRouteTransition({
  pathname,
  search,
  from,
  routeKey,
  layers,
  motion,
  scrollTop,
  collapse,
  swiped,
  measure,
  arrive,
}: MobileRouteTransitionProps): void {
  const { host, page, underlay, overlay } = layers
  const { motion: spring, width, engage, clearLayers } = motion
  const last = useRef<Screen | null>(null)
  const token = useRef(0)
  const animation = useRef<Animation | null>(null)

  useLayoutEffect(() => {
    const main = scrollerOf(host.current)
    const origin = from ? { from } : undefined
    const current = mobileChrome(pathname, search, origin)
    const next: Screen = {
      key: routeKey,
      tab: current.tab,
      depth: current.back ? 1 : 0,
      title: current.title,
      backLabel: current.back?.label ?? null,
    }
    const previous = last.current
    const nav = useMobileNavStore.getState()

    last.current = next
    nav.rememberRoute(next.tab, { path: pathname + search, from })

    if (!main || !previous || previous.key === routeKey) {
      return
    }

    const mine = ++token.current
    const outgoing: BarSnapshot = {
      title: nav.title ?? previous.title,
      backLabel: previous.backLabel,
      collapse: previous.depth > 0 ? 1 : collapse.current,
    }
    const kind = swiped.current ? 'none' : transitionKind(previous, next)

    swiped.current = false
    animation.current?.cancel()
    animation.current = null
    spring.grab()
    clearLayers()
    nav.rememberScroll(previous.key, scrollTop.current)
    updateSnapshot(previous.key, scrollTop.current, collapse.current)

    main.scrollTop = kind === 'push' ? 0 : (useMobileNavStore.getState().scrolls[routeKey] ?? 0)
    scrollTop.current = main.scrollTop
    measure()

    const pageNode = page.current
    const underlayNode = underlay.current
    const overlayNode = overlay.current

    if (kind === 'none' || !pageNode || !underlayNode || !overlayNode) {
      arrive()

      return
    }

    const settled = (finished: boolean) => finished && mine === token.current

    if (prefersReducedMotion()) {
      if (kind === 'push') {
        animation.current = pageNode.animate([{ opacity: 0 }, { opacity: 1 }], { duration: FADE_MS })
        arrive()

        return
      }

      engage('overlay')
      mountSnapshot(overlayNode, previous.key, outgoing.title, classes.placeholder)

      const fade = overlayNode.animate([{ opacity: 1 }, { opacity: 0 }], { duration: FADE_MS })

      animation.current = fade
      fade.finished.then(() => settled(true) && clearLayers()).catch(() => undefined)
      arrive()

      return
    }

    if (kind === 'push') {
      engage('live')
      mountSnapshot(underlayNode, previous.key, outgoing.title, classes.placeholder)
      nav.setTransition({ kind: 'push', other: outgoing })
      spring.set(width.current)
      void spring.to(0).then((finished) => {
        if (settled(finished)) {
          clearLayers()
          arrive()
        }
      })

      return
    }

    engage('overlay')
    mountSnapshot(overlayNode, previous.key, outgoing.title, classes.placeholder)
    nav.setTransition({ kind: 'pop', other: outgoing })
    spring.set(0)
    void spring.to(width.current).then((finished) => {
      if (settled(finished)) {
        clearLayers()
        arrive()
      }
    })
  }, [
    pathname,
    search,
    from,
    routeKey,
    host,
    page,
    underlay,
    overlay,
    spring,
    width,
    engage,
    clearLayers,
    scrollTop,
    collapse,
    swiped,
    measure,
    arrive,
  ])

  useEffect(
    () => () => {
      token.current += 1
      animation.current?.cancel()
      animation.current = null
    },
    []
  )
}
