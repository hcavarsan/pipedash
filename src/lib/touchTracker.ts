import { type Axis, lockAxis, releaseVelocity, type Sample } from './gestures'

export interface Drag {
  dx: number
  dy: number
  startX: number
  startY: number
}

export interface Release {
  vx: number
  vy: number
}

interface TrackerOptions<Context> {
  axis: Axis
  begin: (event: TouchEvent) => Context | null
  accept?: (dx: number, dy: number) => boolean
  move: (context: Context, drag: Drag) => void
  end: (context: Context, drag: Drag, release: Release) => void
  cancel?: (context: Context) => void
}

type Phase = 'pending' | 'active' | 'ignored'

interface Session<Context> {
  context: Context
  phase: Phase
  startX: number
  startY: number
  last: Drag
  samples: Sample[]
}

export function trackTouches<Context>(target: Document | HTMLElement, options: TrackerOptions<Context>): () => void {
  let session: Session<Context> | null = null

  const abort = () => {
    if (session?.phase === 'active') {
      options.cancel?.(session.context)
    }
    session = null
  }

  const onStart = (event: Event) => {
    const touchEvent = event as TouchEvent

    if (touchEvent.touches.length !== 1) {
      abort()

      return
    }

    const context = options.begin(touchEvent)

    if (context === null) {
      session = null

      return
    }

    const touch = touchEvent.touches[0]

    session = {
      context,
      phase: 'pending',
      startX: touch.clientX,
      startY: touch.clientY,
      last: { dx: 0, dy: 0, startX: touch.clientX, startY: touch.clientY },
      samples: [{ t: performance.now(), x: touch.clientX, y: touch.clientY }],
    }
  }

  const onMove = (event: Event) => {
    const touchEvent = event as TouchEvent

    if (!session || session.phase === 'ignored') {
      return
    }

    if (touchEvent.touches.length !== 1) {
      abort()

      return
    }

    const touch = touchEvent.touches[0]
    const drag = {
      dx: touch.clientX - session.startX,
      dy: touch.clientY - session.startY,
      startX: session.startX,
      startY: session.startY,
    }

    if (session.phase === 'pending') {
      const axis = lockAxis(drag.dx, drag.dy)

      if (axis === null) {
        return
      }

      if (axis !== options.axis || (options.accept && !options.accept(drag.dx, drag.dy))) {
        session.phase = 'ignored'

        return
      }

      session.phase = 'active'
    }

    if (touchEvent.cancelable) {
      touchEvent.preventDefault()
    }

    session.last = drag
    session.samples.push({ t: performance.now(), x: touch.clientX, y: touch.clientY })
    options.move(session.context, drag)
  }

  const onEnd = () => {
    const current = session

    session = null

    if (current?.phase === 'active') {
      options.end(current.context, current.last, releaseVelocity(current.samples))
    }
  }

  target.addEventListener('touchstart', onStart, { passive: true })
  target.addEventListener('touchmove', onMove, { passive: false })
  target.addEventListener('touchend', onEnd, { passive: true })
  target.addEventListener('touchcancel', abort, { passive: true })

  return () => {
    target.removeEventListener('touchstart', onStart)
    target.removeEventListener('touchmove', onMove)
    target.removeEventListener('touchend', onEnd)
    target.removeEventListener('touchcancel', abort)
  }
}
