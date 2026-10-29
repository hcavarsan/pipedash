import { type Axis, lockAxis, releaseVelocity, type Sample, VELOCITY_WINDOW_MS } from './gestures'

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

export interface GestureStart {
  target: Element | null
  x: number
  y: number
}

interface TrackerOptions<Context> {
  axis: Axis
  begin: (start: GestureStart) => Context | null
  retry?: boolean
  accept?: (dx: number, dy: number) => boolean
  move: (context: Context, drag: Drag) => void
  end: (context: Context, drag: Drag, release: Release) => void
  cancel?: (context: Context) => void
}

type TrackerTarget = Pick<HTMLElement, 'addEventListener' | 'removeEventListener'>

type Phase = 'pending' | 'active' | 'ignored'

interface Session<Context> {
  context: Context
  phase: Phase
  startX: number
  startY: number
  last: Drag
  samples: Sample[]
}

interface Point {
  x: number
  y: number
  t: number
}

export function trackTouches<Context>(target: TrackerTarget, options: TrackerOptions<Context>): () => void {
  let session: Session<Context> | null = null
  let waiting = false
  let pointerId: number | null = null

  const abort = () => {
    if (session?.phase === 'active') {
      options.cancel?.(session.context)
    }
    session = null
    waiting = false
    pointerId = null
  }

  const open = (element: Element | null, point: Point): boolean => {
    const context = options.begin({ target: element, x: point.x, y: point.y })

    if (context === null) {
      return false
    }

    session = {
      context,
      phase: 'pending',
      startX: point.x,
      startY: point.y,
      last: { dx: 0, dy: 0, startX: point.x, startY: point.y },
      samples: [point],
    }
    waiting = false

    return true
  }

  const start = (element: Element | null, point: Point) => {
    session = null
    waiting = !open(element, point) && options.retry === true
  }

  const advance = (element: Element | null, point: Point, prevent: () => void): void => {
    if (!session && waiting) {
      open(element, point)

      return
    }

    if (!session || session.phase === 'ignored') {
      return
    }

    const drag = {
      dx: point.x - session.startX,
      dy: point.y - session.startY,
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
        waiting = false

        return
      }

      session.phase = 'active'
    }

    prevent()
    session.last = drag
    session.samples.push(point)
    while (session.samples.length > 2) {
      const oldest = session.samples[0]

      if (!oldest || point.t - oldest.t <= VELOCITY_WINDOW_MS) {
        break
      }

      session.samples.shift()
    }
    options.move(session.context, drag)
  }

  const finish = (point: Point | null) => {
    const current = session

    session = null
    waiting = false
    pointerId = null

    if (current?.phase !== 'active') {
      return
    }

    const releaseTime = point?.t ?? performance.now()
    const samples = point ? [...current.samples, point] : current.samples

    options.end(current.context, current.last, releaseVelocity(samples, releaseTime))
  }

  const touchPoint = (touch: Touch): Point => ({ x: touch.clientX, y: touch.clientY, t: performance.now() })

  const onTouchStart = (event: TouchEvent): void => {
    const touch = event.touches[0]

    if (event.touches.length !== 1 || !touch) {
      abort()

      return
    }

    start(event.target instanceof Element ? event.target : null, touchPoint(touch))
  }

  const onTouchMove = (event: TouchEvent): void => {
    const touch = event.touches[0]

    if (event.touches.length !== 1 || !touch) {
      abort()

      return
    }

    advance(event.target instanceof Element ? event.target : null, touchPoint(touch), () => {
      if (event.cancelable) {
        event.preventDefault()
      }
    })
  }

  const onTouchEnd = (event: TouchEvent): void => {
    const touch = event.changedTouches[0]

    finish(touch ? touchPoint(touch) : null)
  }

  const pointerPoint = (event: PointerEvent): Point => ({ x: event.clientX, y: event.clientY, t: performance.now() })

  const onPointerDown = (event: PointerEvent): void => {
    if (event.pointerType === 'touch' || event.button !== 0) {
      return
    }

    pointerId = event.pointerId
    start(event.target instanceof Element ? event.target : null, pointerPoint(event))
  }

  const onPointerMove = (event: PointerEvent): void => {
    if (event.pointerId !== pointerId) {
      return
    }

    advance(event.target instanceof Element ? event.target : null, pointerPoint(event), () => {
      if (event.target instanceof Element && !event.target.hasPointerCapture(event.pointerId)) {
        event.target.setPointerCapture(event.pointerId)
      }
      event.preventDefault()
    })
  }

  const onPointerUp = (event: PointerEvent): void => {
    if (event.pointerId === pointerId) {
      finish(pointerPoint(event))
    }
  }

  const onPointerCancel = (event: PointerEvent): void => {
    if (event.pointerId === pointerId) {
      abort()
    }
  }

  target.addEventListener('touchstart', onTouchStart, { passive: true })
  target.addEventListener('touchmove', onTouchMove, { passive: false })
  target.addEventListener('touchend', onTouchEnd, { passive: true })
  target.addEventListener('touchcancel', abort, { passive: true })
  target.addEventListener('pointerdown', onPointerDown, { passive: true })
  target.addEventListener('pointermove', onPointerMove, { passive: false })
  target.addEventListener('pointerup', onPointerUp, { passive: true })
  target.addEventListener('pointercancel', onPointerCancel, { passive: true })
  target.addEventListener('lostpointercapture', onPointerCancel, { passive: true })

  return () => {
    target.removeEventListener('touchstart', onTouchStart)
    target.removeEventListener('touchmove', onTouchMove)
    target.removeEventListener('touchend', onTouchEnd)
    target.removeEventListener('touchcancel', abort)
    target.removeEventListener('pointerdown', onPointerDown)
    target.removeEventListener('pointermove', onPointerMove)
    target.removeEventListener('pointerup', onPointerUp)
    target.removeEventListener('pointercancel', onPointerCancel)
    target.removeEventListener('lostpointercapture', onPointerCancel)
  }
}
