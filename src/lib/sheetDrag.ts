import { springFor } from '../components/layout/springValue'
import { nearestStop } from './gestures'
import { prefersReducedMotion } from './motion'
import { displayPosition, FLICK_SPEED, stopPositions } from './sheetDetents'
import {
  activate,
  begin,
  cleanup,
  commit,
  expandableCandidate,
  live,
  render,
  reveals,
  type SheetDrag,
  settling,
} from './sheetDragLayout'
import { animateSpring, SPRING_DEFAULT, SPRING_MOMENTUM, type SpringAnimation } from './spring'
import { trackTouches } from './touchTracker'

const DISMISS_CLEARANCE = 96
const FADE_MS = 150

function glide(drag: SheetDrag, to: number, velocity: number): SpringAnimation {
  if (prefersReducedMotion() && Math.abs(to - drag.current) > 1) {
    drag.sheet.animate({ opacity: [0.4, 1] }, { duration: FADE_MS, easing: 'ease-out' })
  }

  const spring = animateSpring({
    from: drag.current,
    to,
    velocity,
    config: Math.abs(velocity) > FLICK_SPEED ? SPRING_MOMENTUM : SPRING_DEFAULT,
    onUpdate: (position) => {
      drag.current = position
      render(drag, position)
    },
  })

  settling.set(drag.sheet, { spring, heights: drag.heights })

  return spring
}

async function settleTo(drag: SheetDrag, stop: number, velocity: number): Promise<boolean> {
  if (!(await glide(drag, stop, velocity).finished)) {
    return false
  }

  commit(drag, stop)

  return true
}

const restStop = (drag: SheetDrag): number => nearestStop(drag.current, 0, stopPositions(drag.heights, false))

async function dismiss(drag: SheetDrag, velocity: number): Promise<void> {
  const { sheet, handle } = drag

  if (prefersReducedMotion()) {
    const fade = sheet.animate({ opacity: [1, 0] }, { duration: FADE_MS, fill: 'forwards' })

    await fade.finished.catch(() => undefined)

    if (drag.disposed) {
      return
    }

    if (await handle.closeNow()) {
      cleanup(drag)
    } else {
      fade.cancel()
      await settleTo(drag, restStop(drag), 0)
    }

    return
  }

  if (!(await glide(drag, drag.top + DISMISS_CLEARANCE, velocity).finished)) {
    return
  }

  if (drag.disposed) {
    return
  }

  if (await handle.closeNow()) {
    cleanup(drag)
  } else {
    await settleTo(drag, restStop(drag), 0)
  }
}

async function release(drag: SheetDrag, velocity: number): Promise<void> {
  const target = nearestStop(drag.current, velocity, stopPositions(drag.heights, drag.dismissible))

  if (!drag.dismissible || target !== drag.top) {
    await settleTo(drag, target, velocity)

    return
  }

  if (!drag.sheet.hasAttribute('data-dirty')) {
    await dismiss(drag, velocity)

    return
  }

  if (await settleTo(drag, restStop(drag), velocity)) {
    await drag.handle.requestClose()
  }
}

export function startSheetDragTracking(target: Document): () => void {
  let disposed = false
  const stopTracking = trackTouches<SheetDrag>(target, {
    axis: 'y',
    retry: true,
    begin,
    accept: (_dx, dy) => dy > 0 || (expandableCandidate() && dy < 0),
    move: (drag, { dy }) => {
      let travel = dy

      if (drag.reveal) {
        drag.revealOrigin ??= drag.reveal.spring.grab()
        const shown = Math.min(Math.max(drag.revealOrigin + dy, 0), drag.reveal.full)

        drag.reveal.spring.set(shown)
        travel = drag.revealOrigin + dy - shown

        if (!drag.active && travel === 0) {
          return
        }
      }

      if (!drag.active) {
        activate(drag, travel)
      }

      drag.current = displayPosition(drag.base + travel - drag.anchor, drag.top, drag.floor)
      render(drag, drag.current)
    },
    end: (drag, _gesture, { vy }) => {
      if (drag.reveal && drag.revealOrigin !== null) {
        const shown = drag.reveal.spring.value
        const open = shown >= drag.reveal.full / 2 || (vy > FLICK_SPEED && shown > 0)

        void reveals.settle(drag.reveal, open, vy, springFor(vy))
      }

      if (drag.active) {
        void release(drag, vy)
      }
    },
    cancel: (drag) => {
      if (drag.reveal && drag.revealOrigin !== null) {
        void reveals.settle(drag.reveal, false, 0, SPRING_DEFAULT)
      }

      if (!disposed && drag.active) {
        void settleTo(drag, restStop(drag), 0)
      }
    },
  })

  return () => {
    disposed = true
    stopTracking()
    reveals.stop()

    for (const drag of live) {
      drag.disposed = true
      settling.get(drag.sheet)?.spring.stop()

      for (const animation of drag.sheet.getAnimations()) {
        animation.cancel()
      }

      cleanup(drag)
    }

    live.clear()
  }
}
