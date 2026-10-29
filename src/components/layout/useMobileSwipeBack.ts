import { type RefObject, useEffect, useEffectEvent } from 'react'

import { decideSwipeBack, EDGE_ZONE } from '../../lib/gestures'
import { overlayOpen } from '../../lib/overlays'
import { trackTouches } from '../../lib/touchTracker'
import { type BarSnapshot, useMobileNavStore } from '../../stores/mobileNavStore'
import classes from './MobilePage.module.css'
import { mobileChrome, routeKey, splitLocation } from './mobileChrome'
import { edgeOffset } from './navigation'
import { mountSnapshot, snapshotCollapse } from './snapshots'
import { springFor } from './springValue'
import type { BackTarget } from './useMobileBack'
import type { MobilePageLayers, MobilePageMotion } from './useMobilePageMotion'

interface SwipeContext {
  target: BackTarget
  width: number
  origin: number
  activated: boolean
}

interface MobileSwipeBackProps {
  layers: MobilePageLayers
  motion: MobilePageMotion
  back: BackTarget | null
  goBack: (to: string) => void
  swiped: RefObject<boolean>
}

export function useMobileSwipeBack({ layers, motion, back, goBack, swiped }: MobileSwipeBackProps): void {
  const { underlay } = layers
  const { motion: spring, mode, width, engage, clearLayers } = motion
  const currentBack = useEffectEvent(() => back)
  const leave = useEffectEvent((to: string) => goBack(to))

  useEffect(() => {
    let grabbing = false
    let prepared = false

    const prepare = (target: BackTarget): BarSnapshot => {
      const parent = splitLocation(target.to)
      const parentChrome = mobileChrome(parent.pathname, parent.search)
      const parentKey = routeKey(parent.pathname, parent.search)

      if (underlay.current) {
        mountSnapshot(underlay.current, parentKey, parentChrome.title, classes.placeholder)
      }

      return {
        title: parentChrome.title,
        backLabel: parentChrome.back?.label ?? null,
        collapse: snapshotCollapse(parentKey),
      }
    }

    const abandon = () => {
      if (prepared) {
        prepared = false
        clearLayers()
      }
    }

    const settle = (target: number, velocity: number, onDone: () => void) => {
      void spring.to(target, velocity, springFor(velocity)).then((finished) => {
        if (finished) {
          onDone()
        }
      })
    }

    const stop = trackTouches<SwipeContext>(document, {
      axis: 'x',
      begin: ({ x }) => {
        const target = currentBack()
        const inFlight = mode.current === 'live' && spring.active

        if (!target || overlayOpen() || (x > EDGE_ZONE && !inFlight)) {
          return null
        }

        grabbing = inFlight

        if (!inFlight) {
          engage('live')
          useMobileNavStore.getState().setTransition({ kind: 'swipe', other: prepare(target) })
          prepared = true
        }

        return { target, width: width.current, origin: spring.value, activated: false }
      },
      accept: (dx) => grabbing || dx > 0,
      move: (context, { dx }) => {
        if (!context.activated) {
          context.activated = true
          prepared = false
          spring.grab()
        }

        spring.set(edgeOffset(context.origin + dx, context.width))
      },
      end: (context, _drag, { vx }) => {
        const complete = decideSwipeBack(spring.value, vx, context.width) === 'complete'

        settle(complete ? context.width : 0, vx, () => {
          if (complete) {
            swiped.current = true
            leave(context.target.to)
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
      spring.grab()
      for (const type of releases) {
        document.removeEventListener(type, abandon)
      }
    }
  }, [spring, mode, width, engage, clearLayers, underlay, swiped])
}
