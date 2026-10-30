import { type ReactNode, useRef } from 'react'
import { useLocation } from 'react-router-dom'

import { PULL_HOLD } from '../../lib/gestures'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { MobileLargeTitle } from './MobileLargeTitle'
import classes from './MobilePage.module.css'
import { fromOf, mobileChrome, routeKey } from './mobileChrome'
import { PullRefreshContext, usePullRefreshState } from './PullRefreshContext'
import { useGoBack } from './useMobileBack'
import { useMobilePageArrival } from './useMobilePageArrival'
import { useMobilePageMotion } from './useMobilePageMotion'
import { useMobilePageScroll } from './useMobilePageScroll'
import { useMobilePageSnapshots } from './useMobilePageSnapshots'
import { useMobilePullRefresh } from './useMobilePullRefresh'
import { useMobileRouteTransition } from './useMobileRouteTransition'
import { useMobileSwipeBack } from './useMobileSwipeBack'

const SPOKES = Array.from({ length: 12 }, (_, index) => index)

export function MobilePage({ children }: { children: ReactNode }): ReactNode {
  const { pathname, search, state } = useLocation()
  const from = fromOf(state)
  const chrome = mobileChrome(pathname, search, state)
  const key = routeKey(pathname, search, state)
  const goBack = useGoBack()
  const { registered, registry } = usePullRefreshState()
  const refreshAll = useRefreshAll()

  const host = useRef<HTMLDivElement>(null)
  const page = useRef<HTMLDivElement>(null)
  const content = useRef<HTMLDivElement>(null)
  const underlay = useRef<HTMLDivElement>(null)
  const overlay = useRef<HTMLDivElement>(null)
  const spinner = useRef<HTMLDivElement>(null)
  const announcement = useRef<HTMLDivElement>(null)
  const status = useRef<HTMLDivElement>(null)
  const swiped = useRef(false)

  const layers = { host, page, underlay, overlay }
  const rootTab = chrome.back === null && chrome.tab !== 'settings'
  const refresh = registered ?? (rootTab ? () => refreshAll.mutateAsync() : null)

  const motion = useMobilePageMotion(layers)
  const { scrollTop, collapse, measure } = useMobilePageScroll(layers)
  const arrive = useMobilePageArrival(page, announcement, chrome.title)

  useMobileRouteTransition({
    pathname,
    search,
    from,
    routeKey: key,
    layers,
    motion,
    scrollTop,
    collapse,
    swiped,
    measure,
    arrive,
  })
  useMobilePageSnapshots({ content, routeKey: key, scrollTop, collapse })
  useMobileSwipeBack({ layers, motion, back: chrome.back, goBack, swiped })
  useMobilePullRefresh({ host, content, spinner, status, refresh })

  return (
    <PullRefreshContext value={registry}>
      <div className={classes.spinner} ref={spinner} style={{ '--pull-hold': `${PULL_HOLD}px` }} aria-hidden>
        {SPOKES.map((index) => (
          <span key={index} className={classes.spoke} style={{ '--i': index }} />
        ))}
      </div>
      <div className={classes.live} role="status" aria-live="polite" ref={announcement} />
      <div className={classes.live} role="status" aria-live="polite" ref={status} />
      <div className={classes.host} ref={host}>
        <div className={classes.underlay} ref={underlay} aria-hidden inert />
        <div className={classes.page} ref={page}>
          <div ref={content}>
            <MobileLargeTitle />
            {children}
          </div>
        </div>
        <div className={classes.dim} aria-hidden />
        <div className={classes.overlay} ref={overlay} aria-hidden inert />
      </div>
    </PullRefreshContext>
  )
}
