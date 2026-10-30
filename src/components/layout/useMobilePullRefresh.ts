import { useEffect, useEffectEvent } from 'react'

import { PULL_HOLD, PULL_THRESHOLD, resist } from '../../lib/gestures'
import { haptic } from '../../lib/haptics'
import { overlayOpen } from '../../lib/overlays'
import { trackTouches } from '../../lib/touchTracker'
import { createSpringValue, springFor } from './springValue'
import type { ElementRef, MobilePageLayers } from './useMobilePageMotion'
import { scrollerOf } from './useMobilePageScroll'

const HYSTERESIS = 8
const STATUS_CLEAR_MS = 2000

interface PullContext {
  origin: number
  height: number
}

interface MobilePullRefreshProps extends Pick<MobilePageLayers, 'host'> {
  content: ElementRef
  spinner: ElementRef
  status: ElementRef
  refresh: (() => Promise<unknown>) | null
}

export function useMobilePullRefresh({ host, content, spinner, status, refresh }: MobilePullRefreshProps): void {
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
        if (!currentRefresh() || overlayOpen() || main.scrollTop > 0 || target?.closest('[data-reorder-handle]')) {
          return null
        }

        grabbing = pull.value > 0

        return { origin: pull.value, height: main.clientHeight }
      },
      accept: (_dx, dy) => grabbing || dy > 0,
      move: (context, { dy }) => {
        dragging = true

        const value = Math.max(0, context.origin + (dy > 0 ? resist(dy, context.height) : dy))

        pull.set(value)

        if (!inflight && currentRefresh()) {
          if (value >= PULL_THRESHOLD) {
            arm(true)
          } else if (value < PULL_THRESHOLD - HYSTERESIS) {
            arm(false)
          }
        }
      },
      end: (_context, _drag, { vy }) => {
        dragging = false

        if (armed && !inflight) {
          commit(vy)

          return
        }

        arm(false)
        rest(vy)
      },
      cancel: () => {
        dragging = false
        arm(false)
        rest(0)
      },
    })

    return () => {
      run += 1
      stop()
      pull.grab()
      window.clearTimeout(statusTimer)
      main.removeAttribute('aria-busy')
      spinnerNode.removeAttribute('data-spinning')
      contentNode.style.transform = ''
    }
  }, [host, content, spinner, status])
}
