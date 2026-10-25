import { type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef } from 'react'
import { useLocation } from 'react-router-dom'

import { decideSwipeBack, EDGE_ZONE, PULL_HOLD, PULL_THRESHOLD, resist } from '../../lib/gestures'
import { haptic } from '../../lib/haptics'
import { prefersReducedMotion } from '../../lib/motion'
import { overlayOpen } from '../../lib/overlays'
import { SPRING_DEFAULT, SPRING_MOMENTUM } from '../../lib/spring'
import { trackTouches } from '../../lib/touchTracker'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { type BarSnapshot, useMobileNavStore } from '../../stores/mobileNavStore'
import { usePullRefreshStore } from '../../stores/pullRefreshStore'
import { MobileLargeTitle } from './MobileLargeTitle'
import classes from './MobilePage.module.css'
import { fromOf, type MobileTab, mobileChrome, routeKey, splitLocation } from './mobileChrome'
import { edgeOffset, scrollEdge, swipeProgress, titleCollapse, transitionKind } from './navigation'
import { createSearchReveal, type Reveal } from './searchReveal'
import { captureSnapshot, mountSnapshot, snapshotCollapse, updateSnapshot } from './snapshots'
import { createSpringValue } from './springValue'
import { useGoBack } from './useMobileBack'

const PARALLAX = 0.3
const FADE_MS = 160
const HYSTERESIS = 8
const CAPTURE_DELAY_MS = 600
const FLICK_SPEED = 500
const SPOKES = Array.from({ length: 12 }, (_, index) => index)
const REVEAL_SETTLE_MS = 150

interface PullContext {
  origin: number
  height: number
  reveal: Reveal | null
  revealOrigin: number
}

type Mode = 'idle' | 'live' | 'overlay'

const scrollerOf = (host: HTMLElement | null) => host?.closest<HTMLElement>('.mantine-AppShell-main') ?? null
const springFor = (velocity: number) => (Math.abs(velocity) > FLICK_SPEED ? SPRING_MOMENTUM : SPRING_DEFAULT)

interface Screen {
  key: string
  tab: MobileTab
  depth: number
  title: string
  backLabel: string | null
}

export function MobilePage({ children }: { children: ReactNode }) {
  const { pathname, search, state } = useLocation()
  const from = fromOf(state)
  const chrome = mobileChrome(pathname, search, state)
  const key = routeKey(pathname, search, state)
  const backTarget = chrome.back
  const goBack = useGoBack()
  const registered = usePullRefreshStore((s) => s.handler)
  const refreshAll = useRefreshAll()

  const hostRef = useRef<HTMLDivElement>(null)
  const pageRef = useRef<HTMLDivElement>(null)
  const pullRef = useRef<HTMLDivElement>(null)
  const underlayRef = useRef<HTMLDivElement>(null)
  const overlayRef = useRef<HTMLDivElement>(null)
  const spinnerRef = useRef<HTMLDivElement>(null)
  const announceRef = useRef<HTMLDivElement>(null)
  const statusRef = useRef<HTMLDivElement>(null)

  const modeRef = useRef<Mode>('idle')
  const widthRef = useRef(0)
  const lastRef = useRef<Screen | null>(null)
  const scrollTopRef = useRef(0)
  const collapseRef = useRef(0)
  const swiped = useRef(false)
  const transitionToken = useRef(0)
  const titleRef = useRef(chrome.title)
  const goBackRef = useRef(goBack)
  const backRef = useRef(backTarget)
  const refreshAllRef = useRef(refreshAll.mutateAsync)

  const rootTab = backTarget === null && chrome.tab !== 'settings'
  const refresh = registered ?? (rootTab ? () => refreshAllRef.current() : null)
  const refreshRef = useRef(refresh)

  titleRef.current = chrome.title
  goBackRef.current = goBack
  backRef.current = backTarget
  refreshAllRef.current = refreshAll.mutateAsync
  refreshRef.current = refresh

  const render = useCallback((offset: number) => {
    const width = widthRef.current
    const progress = swipeProgress(offset, width)
    const overlay = modeRef.current === 'overlay'
    const front = overlay ? overlayRef.current : pageRef.current
    const back = overlay ? pageRef.current : underlayRef.current

    if (front) {
      front.style.transform = `translate3d(${offset}px, 0, 0)`
    }
    if (back) {
      back.style.transform = `translate3d(${-PARALLAX * width * (1 - progress)}px, 0, 0)`
    }
    hostRef.current?.style.setProperty('--depth', String(1 - progress))
    document.documentElement.style.setProperty('--nav-progress', String(progress))
  }, [])

  const motion = useMemo(() => createSpringValue(0, render), [render])

  const engage = useCallback((mode: Mode) => {
    modeRef.current = mode
    widthRef.current = window.innerWidth
    if (hostRef.current) {
      hostRef.current.dataset.mode = mode
    }
  }, [])

  const clearLayers = useCallback(() => {
    modeRef.current = 'idle'
    delete hostRef.current?.dataset.mode
    hostRef.current?.style.removeProperty('--depth')
    document.documentElement.style.removeProperty('--nav-progress')
    for (const layer of [pageRef.current, underlayRef.current, overlayRef.current]) {
      if (layer) {
        layer.style.transform = ''
      }
    }
    underlayRef.current?.replaceChildren()
    overlayRef.current?.replaceChildren()
    useMobileNavStore.getState().setTransition(null)
  }, [])

  const measure = useCallback(() => {
    const root = document.documentElement
    const title = pageRef.current?.querySelector('[data-large-title]')
    const barBottom = document.querySelector('.mantine-AppShell-header')?.getBoundingClientRect().bottom ?? 0
    let collapse = 1

    if (title) {
      const rect = title.getBoundingClientRect()

      collapse = titleCollapse(rect.bottom, barBottom, rect.height)
    }

    collapseRef.current = collapse
    root.style.setProperty('--title-collapse', String(collapse))
    root.style.setProperty('--scroll-edge', String(scrollEdge(scrollTopRef.current)))
  }, [])

  const arrive = useCallback(() => {
    const heading =
      pageRef.current?.querySelector<HTMLElement>('[data-page-heading]') ??
      document.querySelector<HTMLElement>('[data-bar-live] [data-page-heading]')

    heading?.focus({ preventScroll: true })

    const region = announceRef.current

    if (region) {
      region.textContent = ''
      requestAnimationFrame(() => {
        region.textContent = useMobileNavStore.getState().title ?? titleRef.current
      })
    }
  }, [])

  useLayoutEffect(() => {
    const main = scrollerOf(hostRef.current)
    const origin = from ? { from } : undefined
    const current = mobileChrome(pathname, search, origin)
    const next: Screen = {
      key,
      tab: current.tab,
      depth: current.back ? 1 : 0,
      title: current.title,
      backLabel: current.back?.label ?? null,
    }
    const previous = lastRef.current
    const nav = useMobileNavStore.getState()

    lastRef.current = next
    nav.rememberRoute(next.tab, { path: pathname + search, from })

    if (!main || !previous || previous.key === key) {
      return
    }

    const token = ++transitionToken.current
    const outgoing: BarSnapshot = {
      title: nav.title ?? previous.title,
      backLabel: previous.backLabel,
      collapse: previous.depth > 0 ? 1 : collapseRef.current,
    }
    const kind = swiped.current ? 'none' : transitionKind(previous, next)

    swiped.current = false
    motion.grab()
    clearLayers()
    nav.rememberScroll(previous.key, scrollTopRef.current)
    updateSnapshot(previous.key, scrollTopRef.current, collapseRef.current)

    main.scrollTop = kind === 'push' ? 0 : (useMobileNavStore.getState().scrolls[key] ?? 0)
    scrollTopRef.current = main.scrollTop
    measure()

    const page = pageRef.current
    const underlay = underlayRef.current
    const overlay = overlayRef.current

    if (kind === 'none' || !page || !underlay || !overlay) {
      arrive()

      return
    }

    const settled = (finished: boolean) => finished && token === transitionToken.current

    if (prefersReducedMotion()) {
      if (kind === 'push') {
        page.animate([{ opacity: 0 }, { opacity: 1 }], { duration: FADE_MS })
        arrive()

        return
      }

      engage('overlay')
      mountSnapshot(overlay, previous.key, outgoing.title, classes.placeholder)
      overlay
        .animate([{ opacity: 1 }, { opacity: 0 }], { duration: FADE_MS })
        .finished.then(() => settled(true) && clearLayers())
        .catch(() => undefined)
      arrive()

      return
    }

    if (kind === 'push') {
      engage('live')
      mountSnapshot(underlay, previous.key, outgoing.title, classes.placeholder)
      useMobileNavStore.getState().setTransition({ kind: 'push', other: outgoing })
      motion.set(widthRef.current)
      motion.to(0).then((finished) => {
        if (settled(finished)) {
          clearLayers()
          arrive()
        }
      })

      return
    }

    engage('overlay')
    mountSnapshot(overlay, previous.key, outgoing.title, classes.placeholder)
    useMobileNavStore.getState().setTransition({ kind: 'pop', other: outgoing })
    motion.set(0)
    motion.to(widthRef.current).then((finished) => {
      if (settled(finished)) {
        clearLayers()
        arrive()
      }
    })
  }, [pathname, search, from, key, motion, clearLayers, measure, arrive, engage])

  useEffect(() => {
    const main = scrollerOf(hostRef.current)
    let frame = 0

    const onScroll = () => {
      scrollTopRef.current = main?.scrollTop ?? 0
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0
          measure()
        })
      }
    }

    measure()
    main?.addEventListener('scroll', onScroll, { passive: true })
    window.addEventListener('resize', measure)

    return () => {
      cancelAnimationFrame(frame)
      main?.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', measure)
    }
  }, [measure])

  useEffect(() => {
    const content = pullRef.current

    if (!content) {
      return
    }

    let timer = 0
    let dirty = false

    const flush = () => {
      window.clearTimeout(timer)
      if (dirty) {
        dirty = false
        captureSnapshot(key, content, scrollTopRef.current, collapseRef.current)
      }
    }
    const schedule = () => {
      dirty = true
      window.clearTimeout(timer)
      timer = window.setTimeout(flush, CAPTURE_DELAY_MS)
    }
    const observer = new MutationObserver((records) => {
      if (records.some((record) => record.target !== content)) {
        schedule()
      }
    })

    schedule()
    observer.observe(content, { subtree: true, childList: true, characterData: true, attributes: true })
    document.addEventListener('click', flush, { capture: true, passive: true })

    return () => {
      window.clearTimeout(timer)
      observer.disconnect()
      document.removeEventListener('click', flush, { capture: true })
    }
  }, [key])

  useEffect(() => {
    interface SwipeContext {
      target: { to: string; label: string }
      width: number
      origin: number
      activated: boolean
    }

    let grabbing = false
    let prepared = false

    const prepare = (target: { to: string }) => {
      const parent = splitLocation(target.to)
      const parentChrome = mobileChrome(parent.pathname, parent.search)
      const parentKey = routeKey(parent.pathname, parent.search)

      if (underlayRef.current) {
        mountSnapshot(underlayRef.current, parentKey, parentChrome.title, classes.placeholder)
      }

      return {
        title: parentChrome.title,
        backLabel: parentChrome.back?.label ?? null,
        collapse: snapshotCollapse(parentKey),
      } satisfies BarSnapshot
    }

    const abandon = () => {
      if (prepared) {
        prepared = false
        clearLayers()
      }
    }

    const settle = (target: number, velocity: number, onDone: () => void) => {
      motion.to(target, velocity, springFor(velocity)).then((finished) => {
        if (finished) {
          onDone()
        }
      })
    }

    const stop = trackTouches<SwipeContext>(document, {
      axis: 'x',
      begin: ({ x }) => {
        const target = backRef.current
        const inFlight = modeRef.current === 'live' && motion.active

        if (!target || overlayOpen() || (x > EDGE_ZONE && !inFlight)) {
          return null
        }

        grabbing = inFlight

        if (!inFlight) {
          engage('live')
          useMobileNavStore.getState().setTransition({ kind: 'swipe', other: prepare(target) })
          prepared = true
        }

        return { target, width: widthRef.current, origin: motion.value, activated: false }
      },
      accept: (dx) => grabbing || dx > 0,
      move: (context, { dx }) => {
        if (!context.activated) {
          context.activated = true
          prepared = false
          motion.grab()
        }

        motion.set(edgeOffset(context.origin + dx, context.width))
      },
      end: (context, _drag, { vx }) => {
        const complete = decideSwipeBack(motion.value, vx, context.width) === 'complete'

        settle(complete ? context.width : 0, vx, () => {
          if (complete) {
            swiped.current = true
            goBackRef.current(context.target.to)
          } else {
            clearLayers()
          }
        })
      },
      cancel: () => settle(0, 0, clearLayers),
    })

    const releases = ['touchend', 'touchcancel', 'pointerup', 'pointercancel']

    for (const type of releases) {
      document.addEventListener(type, abandon, { passive: true })
    }

    return () => {
      stop()
      for (const type of releases) {
        document.removeEventListener(type, abandon)
      }
    }
  }, [motion, clearLayers, engage])

  useEffect(() => {
    const main = scrollerOf(hostRef.current)
    const content = pullRef.current
    const spinner = spinnerRef.current

    if (!main || !content || !spinner) {
      return
    }

    let armed = false
    let inflight = false
    let dragging = false
    let grabbing = false
    let run = 0
    let statusTimer = 0
    let touching = false
    let settleTimer = 0

    const reveals = createSearchReveal(() => pageRef.current)

    const hideRevealOutOfView = () => {
      const element = pageRef.current?.querySelector<HTMLElement>('[data-search-reveal][data-open]')

      if (touching || !element || element.hasAttribute('data-pinned') || element.matches(':focus-within')) {
        return
      }

      const barBottom = document.querySelector('.mantine-AppShell-header')?.getBoundingClientRect().bottom ?? 0
      const rect = element.getBoundingClientRect()

      if (rect.bottom <= barBottom) {
        element.removeAttribute('data-open')
        main.scrollTop -= rect.height
      }
    }

    const scheduleHide = () => {
      window.clearTimeout(settleTimer)
      settleTimer = window.setTimeout(hideRevealOutOfView, REVEAL_SETTLE_MS)
    }

    const onTouchStart = () => {
      touching = true
      window.clearTimeout(settleTimer)
    }

    const onTouchEnd = () => {
      touching = false
      scheduleHide()
    }

    main.addEventListener('scroll', scheduleHide, { passive: true })
    main.addEventListener('touchstart', onTouchStart, { passive: true })
    main.addEventListener('touchend', onTouchEnd, { passive: true })
    main.addEventListener('touchcancel', onTouchEnd, { passive: true })

    const say = (text: string) => {
      if (statusRef.current) {
        statusRef.current.textContent = text
      }
    }

    const pull = createSpringValue(0, (value) => {
      content.style.transform = value === 0 ? '' : `translate3d(0, ${value}px, 0)`
      spinner.style.setProperty('--pull', `${value}px`)
      spinner.style.setProperty('--progress', String(Math.min(1, Math.max(0, value / PULL_HOLD))))
    })

    const arm = (next: boolean) => {
      if (armed === next) {
        return
      }

      armed = next
      spinner.toggleAttribute('data-armed', next)
      if (next) {
        haptic('light')
      }
    }

    const rest = (velocity: number) => {
      pull.to(inflight ? PULL_HOLD : 0, velocity, springFor(velocity))
    }

    const commit = (velocity: number) => {
      const refreshNow = refreshRef.current
      const mine = ++run

      if (!refreshNow) {
        rest(velocity)

        return
      }

      arm(false)
      inflight = true
      spinner.setAttribute('data-spinning', '')
      main.setAttribute('aria-busy', 'true')
      window.clearTimeout(statusTimer)
      say('Refreshing…')
      rest(velocity)

      refreshNow()
        .catch(() => undefined)
        .finally(() => {
          inflight = false
          spinner.removeAttribute('data-spinning')
          main.removeAttribute('aria-busy')
          say('Updated')
          statusTimer = window.setTimeout(() => say(''), 2000)
          if (mine === run && !dragging) {
            rest(0)
          }
        })
    }

    const stop = trackTouches<PullContext>(main, {
      axis: 'y',
      retry: true,
      begin: () => {
        const closed = reveals.closed()

        if ((!refreshRef.current && !closed) || overlayOpen() || main.scrollTop > 0) {
          return null
        }

        grabbing = pull.value > 0

        return {
          origin: pull.value,
          height: main.clientHeight,
          reveal: closed,
          revealOrigin: closed ? closed.spring.grab() : 0,
        }
      },
      accept: (_dx, dy) => grabbing || dy > 0,
      move: (context, { dy }) => {
        dragging = true

        let travel = dy

        if (context.reveal) {
          const shown = Math.min(Math.max(context.revealOrigin + dy, 0), context.reveal.full)

          context.reveal.spring.set(shown)
          travel = context.revealOrigin + dy - shown
        }

        const value = Math.max(0, context.origin + (travel > 0 ? resist(travel, context.height) : travel))

        pull.set(value)

        if (!inflight && refreshRef.current) {
          if (value >= PULL_THRESHOLD) {
            arm(true)
          } else if (value < PULL_THRESHOLD - HYSTERESIS) {
            arm(false)
          }
        }
      },
      end: (context, _drag, { vy }) => {
        dragging = false

        if (context.reveal) {
          const shown = context.reveal.spring.value
          const open = armed || shown >= context.reveal.full / 2 || (vy > FLICK_SPEED && shown > 0)

          reveals.settle(context.reveal, open, vy, springFor(vy))
        }

        if (armed && !inflight) {
          commit(vy)

          return
        }

        arm(false)
        rest(vy)
      },
      cancel: (context) => {
        dragging = false
        arm(false)
        rest(0)
        if (context.reveal) {
          reveals.settle(context.reveal, false, 0, SPRING_DEFAULT)
        }
      },
    })

    return () => {
      stop()
      pull.grab()
      reveals.stop()
      run += 1
      window.clearTimeout(statusTimer)
      window.clearTimeout(settleTimer)
      main.removeEventListener('scroll', scheduleHide)
      main.removeEventListener('touchstart', onTouchStart)
      main.removeEventListener('touchend', onTouchEnd)
      main.removeEventListener('touchcancel', onTouchEnd)
      main.removeAttribute('aria-busy')
      content.style.transform = ''
    }
  }, [])

  useEffect(
    () => () => {
      motion.grab()
      clearLayers()
    },
    [motion, clearLayers]
  )

  return (
    <>
      <div className={classes.spinner} ref={spinnerRef} aria-hidden>
        {SPOKES.map((index) => (
          <span key={index} className={classes.spoke} style={{ '--i': index } as React.CSSProperties} />
        ))}
      </div>
      <div className={classes.live} role="status" aria-live="polite" ref={announceRef} />
      <div className={classes.live} role="status" aria-live="polite" ref={statusRef} />
      <div className={classes.host} ref={hostRef}>
        <div className={classes.underlay} ref={underlayRef} aria-hidden inert />
        <div className={classes.page} ref={pageRef}>
          <div ref={pullRef}>
            <MobileLargeTitle />
            {children}
          </div>
        </div>
        <div className={classes.dim} aria-hidden />
        <div className={classes.overlay} ref={overlayRef} aria-hidden inert />
      </div>
    </>
  )
}
