import { useEffect, useEffectEvent } from 'react'

import { PULL_HOLD, PULL_THRESHOLD, resist } from '../../lib/gestures'
import { haptic } from '../../lib/haptics'
import { overlayOpen } from '../../lib/overlays'
import { SPRING_DEFAULT } from '../../lib/spring'
import { trackTouches } from '../../lib/touchTracker'
import { createSearchReveal, type Reveal } from './searchReveal'
import { createSpringValue, FLICK_SPEED, springFor } from './springValue'
import type { ElementRef, MobilePageLayers } from './useMobilePageMotion'
import { scrollerOf } from './useMobilePageScroll'

const HYSTERESIS = 8
const REVEAL_SETTLE_MS = 150
const STATUS_CLEAR_MS = 2000

interface PullContext {
  origin: number
  height: number
  reveal: Reveal | null
  revealOrigin: number
}

interface MobilePullRefreshProps extends Pick<MobilePageLayers, 'host' | 'page'> {
  content: ElementRef
  spinner: ElementRef
  status: ElementRef
  refresh: (() => Promise<unknown>) | null
}

export function useMobilePullRefresh({ host, page, content, spinner, status, refresh }: MobilePullRefreshProps): void {
  const currentRefresh = useEffectEvent(() => refresh)

  useEffect(() => {
    const main = scrollerOf(host.current)
    const contentNode = content.current
    const spinnerNode = spinner.current

    if (!main || !contentNode || !spinnerNode) {
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

    const reveals = createSearchReveal(() => page.current)

    const hideRevealOutOfView = () => {
      const element = page.current?.querySelector<HTMLElement>('[data-search-reveal][data-open]')

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
      if (status.current) {
        status.current.textContent = text
      }
    }

    const pull = createSpringValue(0, (value) => {
      contentNode.style.transform = value === 0 ? '' : `translate3d(0, ${value}px, 0)`
      spinnerNode.style.setProperty('--pull', `${value}px`)
      spinnerNode.style.setProperty('--progress', String(Math.min(1, Math.max(0, value / PULL_HOLD))))
    })

    const arm = (next: boolean) => {
      if (armed === next) {
        return
      }

      armed = next
      spinnerNode.toggleAttribute('data-armed', next)
      if (next) {
        haptic('light')
      }
    }

    const rest = (velocity: number) => {
      void pull.to(inflight ? PULL_HOLD : 0, velocity, springFor(velocity))
    }

    const commit = (velocity: number) => {
      const refreshNow = currentRefresh()
      const mine = ++run

      if (!refreshNow) {
        rest(velocity)

        return
      }

      arm(false)
      inflight = true
      spinnerNode.setAttribute('data-spinning', '')
      main.setAttribute('aria-busy', 'true')
      window.clearTimeout(statusTimer)
      say('Refreshing…')
      rest(velocity)

      const complete = (text: string) => {
        if (mine !== run) {
          return
        }

        say(text)
        inflight = false
        spinnerNode.removeAttribute('data-spinning')
        main.removeAttribute('aria-busy')
        statusTimer = window.setTimeout(() => say(''), STATUS_CLEAR_MS)
        if (!dragging) {
          rest(0)
        }
      }

      void refreshNow().then(
        () => complete('Updated'),
        () => complete("Couldn't refresh")
      )
    }

    const stop = trackTouches<PullContext>(main, {
      axis: 'y',
      retry: true,
      begin: ({ target }) => {
        const closed = reveals.closed()

        if (
          (!currentRefresh() && !closed) ||
          overlayOpen() ||
          main.scrollTop > 0 ||
          target?.closest('[data-reorder-handle]')
        ) {
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

        if (!inflight && currentRefresh()) {
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

          void reveals.settle(context.reveal, open, vy, springFor(vy))
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
          void reveals.settle(context.reveal, false, 0, SPRING_DEFAULT)
        }
      },
    })

    return () => {
      run += 1
      stop()
      pull.grab()
      reveals.stop()
      window.clearTimeout(statusTimer)
      window.clearTimeout(settleTimer)
      main.removeEventListener('scroll', scheduleHide)
      main.removeEventListener('touchstart', onTouchStart)
      main.removeEventListener('touchend', onTouchEnd)
      main.removeEventListener('touchcancel', onTouchEnd)
      main.removeAttribute('aria-busy')
      spinnerNode.removeAttribute('data-spinning')
      contentNode.style.transform = ''
    }
  }, [host, page, content, spinner, status])
}
