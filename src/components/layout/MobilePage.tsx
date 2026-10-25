import { type ReactNode, useEffect, useLayoutEffect, useRef } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { decidePull, decideSwipeBack, EDGE_ZONE, PULL_HOLD, PULL_LIMIT, rubberBand } from '../../lib/gestures'
import { overlayOpen } from '../../lib/overlays'
import { trackTouches } from '../../lib/touchTracker'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { usePullRefreshStore } from '../../stores/pullRefreshStore'
import { MobileLargeTitle } from './MobileLargeTitle'
import classes from './MobilePage.module.css'
import { mobileChrome } from './mobileChrome'
import { useMobileBack } from './useMobileBack'

const SPRING = 'cubic-bezier(0.32, 0.72, 0, 1)'
const PUSH_MS = 360
const SNAP_MS = 240
const PARALLAX = 0.3
const SPOKES = Array.from({ length: 12 }, (_, index) => index)

const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches

function animate(element: HTMLElement, keyframes: Keyframe[], duration = SNAP_MS) {
  return element.animate(keyframes, { duration: reducedMotion() ? 0 : duration, easing: SPRING })
}

const scrollerOf = (host: HTMLElement | null) => host?.closest<HTMLElement>('.mantine-AppShell-main') ?? null

interface Snapshot {
  node: Node
  scroll: number
}

export function MobilePage({ children }: { children: ReactNode }) {
  const { pathname, search } = useLocation()
  const navigate = useNavigate()
  const { tab, back: pushed } = mobileChrome(pathname, search)
  const isPushed = pushed !== null
  const backTarget = useMobileBack()
  const registered = usePullRefreshStore((s) => s.handler)
  const refreshAll = useRefreshAll()

  const hostRef = useRef<HTMLDivElement>(null)
  const pageRef = useRef<HTMLDivElement>(null)
  const pullRef = useRef<HTMLDivElement>(null)
  const underlayRef = useRef<HTMLDivElement>(null)
  const spinnerRef = useRef<HTMLDivElement>(null)

  const snapshot = useRef<Snapshot | null>(null)
  const rootScroll = useRef(0)
  const wasPushed = useRef(isPushed)
  const swiped = useRef(false)
  const refreshing = useRef(false)

  const refreshAllRef = useRef(refreshAll.mutateAsync)
  refreshAllRef.current = refreshAll.mutateAsync

  const rootTab = !isPushed && tab !== 'settings'
  const refresh = registered ?? (rootTab ? () => refreshAllRef.current() : null)
  const refreshRef = useRef(refresh)
  const backRef = useRef(backTarget)
  refreshRef.current = refresh
  backRef.current = backTarget

  useEffect(() => {
    const main = scrollerOf(hostRef.current)
    const onScroll = () => {
      if (!backRef.current && main) {
        rootScroll.current = main.scrollTop
      }
    }
    const onClick = () => {
      if (!backRef.current && pullRef.current && main) {
        snapshot.current = { node: pullRef.current.cloneNode(true), scroll: main.scrollTop }
      }
    }

    main?.addEventListener('scroll', onScroll, { passive: true })
    document.addEventListener('click', onClick, { capture: true, passive: true })

    return () => {
      main?.removeEventListener('scroll', onScroll)
      document.removeEventListener('click', onClick, { capture: true })
    }
  }, [])

  useLayoutEffect(() => {
    const page = pageRef.current
    const main = scrollerOf(hostRef.current)

    if (!page || !main) {
      return
    }

    page.style.transform = ''
    delete page.dataset.dragging

    if (isPushed && !wasPushed.current) {
      main.scrollTop = 0
      animate(page, [{ transform: 'translateX(100%)' }, { transform: 'translateX(0)' }], PUSH_MS)
    } else if (!isPushed && wasPushed.current) {
      main.scrollTop = snapshot.current?.scroll ?? rootScroll.current

      if (!swiped.current) {
        animate(
          page,
          [
            { transform: `translateX(${-PARALLAX * 100}%)`, opacity: 0.6 },
            { transform: 'translateX(0)', opacity: 1 },
          ],
          PUSH_MS
        )
      }
    }

    swiped.current = false
    wasPushed.current = isPushed
    underlayRef.current?.replaceChildren()
  }, [isPushed])

  useEffect(
    () =>
      trackTouches(document, {
        axis: 'x',
        begin: (event) => {
          const page = pageRef.current
          const target = backRef.current

          if (!page || !target || overlayOpen() || event.touches[0].clientX > EDGE_ZONE) {
            return null
          }

          return { page, target, width: window.innerWidth }
        },
        accept: (dx) => dx > 0,
        move: ({ page, width }, { dx }) => {
          const underlay = underlayRef.current
          const progress = Math.min(1, Math.max(0, dx / width))

          if (underlay && snapshot.current && !underlay.firstChild) {
            const copy = snapshot.current.node.cloneNode(true) as HTMLElement

            copy.style.transform = `translateY(${-snapshot.current.scroll}px)`
            underlay.replaceChildren(copy)
          }

          page.dataset.dragging = ''
          page.style.transform = `translateX(${Math.max(0, dx)}px)`
          hostRef.current?.style.setProperty('--swipe', String(progress))
          if (underlay) {
            underlay.style.transform = `translateX(${-PARALLAX * 100 * (1 - progress)}%)`
          }
        },
        end: ({ page, target, width }, { dx }, { vx }) => {
          if (decideSwipeBack(dx, vx, width) === 'complete') {
            swiped.current = true
            animate(page, [{ transform: `translateX(${dx}px)` }, { transform: `translateX(${width}px)` }]).onfinish =
              () => {
                page.style.transform = `translateX(${width}px)`
                navigate(target.to)
              }

            return
          }

          animate(page, [{ transform: `translateX(${dx}px)` }, { transform: 'translateX(0)' }]).onfinish = () => {
            page.style.transform = ''
            delete page.dataset.dragging
            underlayRef.current?.replaceChildren()
          }
        },
        cancel: ({ page }) => {
          page.style.transform = ''
          delete page.dataset.dragging
        },
      }),
    [navigate]
  )

  useEffect(() => {
    const main = scrollerOf(hostRef.current)

    if (!main) {
      return
    }

    const setPull = (pull: number) => {
      const spinner = spinnerRef.current

      if (pullRef.current) {
        pullRef.current.style.transform = pull > 0 ? `translateY(${pull}px)` : ''
      }
      spinner?.style.setProperty('--pull', `${pull}px`)
      spinner?.style.setProperty('--progress', String(Math.min(1, pull / PULL_HOLD)))
    }

    const settle = (from: number, spinning: boolean) => {
      const content = pullRef.current

      if (!content) {
        return
      }

      const to = spinning ? PULL_HOLD : 0

      animate(content, [{ transform: `translateY(${from}px)` }, { transform: `translateY(${to}px)` }]).onfinish =
        () => {
          setPull(to)
          if (spinning) {
            spinnerRef.current?.setAttribute('data-spinning', '')
          }
        }
    }

    return trackTouches(main, {
      axis: 'y',
      begin: () => (refreshRef.current && !refreshing.current && !overlayOpen() && main.scrollTop <= 0 ? {} : null),
      accept: (_dx, dy) => dy > 0,
      move: (_context, { dy }) => setPull(rubberBand(dy, PULL_LIMIT)),
      end: (_context, { dy }) => {
        const pull = rubberBand(dy, PULL_LIMIT)

        if (decidePull(pull) === 'cancel') {
          settle(pull, false)

          return
        }

        refreshing.current = true
        settle(pull, true)
        refreshRef
          .current?.()
          .catch(() => undefined)
          .finally(() => {
            refreshing.current = false
            spinnerRef.current?.removeAttribute('data-spinning')
            settle(PULL_HOLD, false)
          })
      },
      cancel: () => setPull(0),
    })
  }, [])

  return (
    <>
      <div className={classes.spinner} ref={spinnerRef} aria-hidden>
        {SPOKES.map((index) => (
          <span key={index} className={classes.spoke} style={{ '--i': index } as React.CSSProperties} />
        ))}
      </div>
      <div className={classes.host} ref={hostRef}>
        <div className={classes.underlay} ref={underlayRef} aria-hidden inert />
        <div className={classes.page} ref={pageRef}>
          <div ref={pullRef}>
            <MobileLargeTitle />
            {children}
          </div>
        </div>
      </div>
    </>
  )
}
