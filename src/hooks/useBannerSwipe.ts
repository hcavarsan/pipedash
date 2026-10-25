import { notifications } from '@mantine/notifications'
import { useEffect } from 'react'

import { nearestStop } from '../lib/gestures'
import { prefersReducedMotion } from '../lib/motion'
import { FLICK_SPEED } from '../lib/sheetDetents'
import { animateSpring, rubberband, SPRING_DEFAULT, SPRING_MOMENTUM, type SpringAnimation } from '../lib/spring'
import { type GestureStart, trackTouches } from '../lib/touchTracker'
import { iosOverlayClasses } from '../theme/iosOverlays'

const FADE_MS = 150
const EXIT_CLEARANCE = 24

interface Banner {
  element: HTMLElement
  base: number
  anchor: number | null
  position: number
}

const springs = new WeakMap<HTMLElement, SpringAnimation>()

function begin({ target }: GestureStart): Banner | null {
  const element = target?.closest<HTMLElement>(`.${iosOverlayClasses.toast}`)

  return element?.id ? { element, base: 0, anchor: null, position: 0 } : null
}

function settle(banner: Banner, to: number, velocity: number): Promise<boolean> {
  const { element } = banner
  const spring = animateSpring({
    from: banner.position,
    to,
    velocity,
    config: Math.abs(velocity) > FLICK_SPEED ? SPRING_MOMENTUM : SPRING_DEFAULT,
    onUpdate: (position) => {
      banner.position = position
      element.style.translate = `0 ${position}px`
    },
  })

  springs.set(element, spring)

  return spring.finished
}

async function release(banner: Banner, velocity: number) {
  const { element } = banner
  const exit = -(element.getBoundingClientRect().bottom - banner.position + EXIT_CLEARANCE)

  if (nearestStop(banner.position, velocity, [0, exit]) === 0) {
    if (await settle(banner, 0, velocity)) {
      element.style.removeProperty('translate')
    }

    return
  }

  if (prefersReducedMotion()) {
    await element.animate({ opacity: [1, 0] }, { duration: FADE_MS, fill: 'forwards' }).finished
  } else if (!(await settle(banner, exit, velocity))) {
    return
  }

  notifications.hide(element.id)
}

export function useBannerSwipe(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    return trackTouches<Banner>(document, {
      axis: 'y',
      begin,
      accept: (_dx, dy) => dy < 0,
      move: (banner, { dy }) => {
        const { element } = banner

        if (banner.anchor === null) {
          const spring = springs.get(element)

          banner.base = spring?.value ?? 0
          spring?.stop()
          banner.anchor = dy
        }

        const raw = banner.base + dy - banner.anchor

        banner.position = raw > 0 ? rubberband(raw, element.offsetHeight) : raw
        element.style.translate = `0 ${banner.position}px`
      },
      end: (banner, _drag, { vy }) => {
        void release(banner, vy)
      },
      cancel: (banner) => {
        void settle(banner, 0, 0)
      },
    })
  }, [active])
}
