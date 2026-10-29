export const LONG_PRESS_MS = 500
export const LONG_PRESS_SLOP = 10

type Handle = ReturnType<typeof setTimeout>

interface LongPressOptions {
  delay?: number
  slop?: number
  schedule?: (callback: () => void, ms: number) => Handle
  unschedule?: (handle: Handle) => void
}

export interface LongPress {
  start: (x: number, y: number) => void
  move: (x: number, y: number) => boolean
  cancel: () => void
  end: () => boolean
}

export function createLongPress(
  onFire: () => void,
  {
    delay = LONG_PRESS_MS,
    slop = LONG_PRESS_SLOP,
    schedule = (callback, ms) => setTimeout(callback, ms),
    unschedule = (handle) => clearTimeout(handle),
  }: LongPressOptions = {}
): LongPress {
  let timer: Handle | null = null
  let origin: { x: number; y: number } | null = null
  let fired = false

  const cancel = () => {
    if (timer !== null) {
      unschedule(timer)
      timer = null
    }
    origin = null
  }

  return {
    start(x, y) {
      cancel()
      fired = false
      origin = { x, y }
      timer = schedule(() => {
        timer = null
        origin = null
        fired = true
        onFire()
      }, delay)
    },
    move(x, y) {
      if (origin && Math.hypot(x - origin.x, y - origin.y) > slop) {
        cancel()
      }

      return timer !== null
    },
    cancel,
    end() {
      cancel()

      const didFire = fired

      fired = false

      return didFire
    },
  }
}

export function guardLongPressClick(target: EventTarget, pointerId: number | null): () => void {
  const block = (event: Event) => {
    if ('pointerId' in event && event.pointerId !== pointerId) {
      remove()
      return
    }
    if (event.type === 'pointerdown') {
      return
    }

    event.preventDefault()
    event.stopImmediatePropagation()
    if (event.type === 'click') {
      remove()
    }
  }
  const remove = () => {
    clearTimeout(timer)
    target.removeEventListener('click', block, { capture: true })
    target.removeEventListener('mousedown', block, { capture: true })
    target.removeEventListener('pointerdown', block, { capture: true })
  }
  const timer = setTimeout(remove, 600)

  target.addEventListener('click', block, { capture: true })
  target.addEventListener('mousedown', block, { capture: true })
  target.addEventListener('pointerdown', block, { capture: true })

  return remove
}
