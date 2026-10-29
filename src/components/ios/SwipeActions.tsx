import { type ReactNode, useCallback, useEffect, useRef } from 'react'

import { AXIS_SLOP, decideRowSwipe, rowPosition } from '../../lib/gestures'
import { haptic } from '../../lib/haptics'
import { animateSpring, type SpringAnimation } from '../../lib/spring'
import { trackTouches } from '../../lib/touchTracker'

import classes from './SwipeActions.module.css'
import { nextArmed, settleDecision } from './swipeArmed'

export interface SwipeAction {
  key: string
  label: string
  tone: 'blue' | 'red' | 'orange' | 'gray' | 'green'
  onSelect: () => void
  primary?: boolean
}

interface SwipeActionsProps {
  actions: SwipeAction[]
  children: ReactNode
}

interface Grab {
  start: number
  rowWidth: number
}

const ACTION_WIDTH = 80

let closeOpenRow: (() => void) | null = null

export function SwipeActions({ actions, children }: SwipeActionsProps): ReactNode {
  const slideRef = useRef<HTMLDivElement>(null)
  const primaryRef = useRef<HTMLButtonElement>(null)
  const open = useRef(false)
  const armed = useRef(false)
  const position = useRef(0)
  const progress = useRef(0)
  const motion = useRef<SpringAnimation | null>(null)
  const armMotion = useRef<SpringAnimation | null>(null)
  const actionsRef = useRef(actions)
  const maxWidth = actions.length * ACTION_WIDTH
  const primary = actions.find((action) => action.primary)
  const rest = actions.filter((action) => !action.primary)

  actionsRef.current = actions

  const draw = useCallback(() => {
    const slide = slideRef.current

    if (!slide) {
      return
    }

    const shown = Math.max(0, position.current)
    const visible = ACTION_WIDTH + Math.max(0, shown - ACTION_WIDTH) * progress.current

    slide.style.transform = shown === 0 ? '' : `translateX(${-shown}px)`
    slide.toggleAttribute('data-revealed', shown > 0)
    primaryRef.current?.style.setProperty('transform', `translateX(calc(100% - ${visible}px))`)
  }, [])

  const settle = useCallback(
    (to: number, velocity: number) => {
      motion.current?.stop()

      const animation = animateSpring({
        from: position.current,
        to,
        velocity,
        onUpdate: (value) => {
          position.current = value
          draw()
        },
      })

      motion.current = animation

      return animation.finished
    },
    [draw]
  )

  const setArmed = useCallback(
    (next: boolean) => {
      if (armed.current === next) {
        return
      }

      armed.current = next
      armMotion.current?.stop()
      if (next) {
        haptic('medium')
      }
      armMotion.current = animateSpring({
        from: progress.current,
        to: next ? 1 : 0,
        onUpdate: (value) => {
          progress.current = value
          draw()
        },
      })
    },
    [draw]
  )

  const close = useCallback(() => {
    if (!open.current) {
      return
    }

    open.current = false
    if (closeOpenRow === close) {
      closeOpenRow = null
    }
    setArmed(false)
    void settle(0, 0)
  }, [settle, setArmed])

  const reveal = useCallback(() => {
    if (open.current) {
      return
    }

    if (closeOpenRow && closeOpenRow !== close) {
      closeOpenRow()
    }
    open.current = true
    closeOpenRow = close
    void settle(maxWidth, 0)
  }, [close, settle, maxWidth])

  useEffect(() => {
    const slide = slideRef.current

    if (!slide || actions.length === 0) {
      return
    }

    let grab: Grab = { start: 0, rowWidth: 0 }
    let pan: { x: number; y: number } | null = null

    const onOutsideTouch = (event: Event) => {
      if (open.current && event.target instanceof Node && !slide.parentElement?.contains(event.target)) {
        close()
      }
    }

    const onScroll = (event: Event) => {
      const target = event.target

      if (open.current && (target === document || (target instanceof Node && target.contains(slide)))) {
        close()
      }
    }

    const onPanStart = (event: TouchEvent) => {
      const touch = event.touches[0]

      pan = touch ? { x: touch.clientX, y: touch.clientY } : null
    }

    const onPanMove = (event: TouchEvent) => {
      const touch = event.touches[0]

      if (!open.current || !pan || !touch) {
        return
      }

      const dy = Math.abs(touch.clientY - pan.y)

      if (dy > AXIS_SLOP && dy > Math.abs(touch.clientX - pan.x)) {
        pan = null
        close()
      }
    }

    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.target instanceof HTMLElement && event.target.closest('input, textarea, select')) {
        return
      }

      if (event.key === 'ArrowLeft') {
        reveal()
      } else if (event.key === 'ArrowRight' || event.key === 'Escape') {
        close()
      }
    }

    document.addEventListener('touchstart', onOutsideTouch, { capture: true, passive: true })
    document.addEventListener('scroll', onScroll, { capture: true, passive: true })
    slide.addEventListener('touchstart', onPanStart, { passive: true })
    slide.addEventListener('touchmove', onPanMove, { passive: true })
    slide.addEventListener('keydown', onKeyDown)

    const stop = trackTouches(slide, {
      axis: 'x',
      begin: () => {
        if (closeOpenRow && closeOpenRow !== close) {
          closeOpenRow()
        }
        grab = { start: position.current, rowWidth: slide.offsetWidth }

        return grab
      },
      accept: (dx) => {
        if (position.current <= 0 && dx >= 0) {
          return false
        }

        motion.current?.stop()
        grab.start = position.current

        return true
      },
      move: (context, { dx }) => {
        position.current = rowPosition(context.start - dx, maxWidth, context.rowWidth)
        if (actionsRef.current.some((action) => action.primary)) {
          setArmed(nextArmed(armed.current, position.current, context.rowWidth))
        }
        draw()
      },
      end: (context, _drag, { vx }) => {
        const leftward = -vx
        const target = actionsRef.current.find((action) => action.primary)
        const decision = settleDecision(
          decideRowSwipe(position.current, leftward, maxWidth, context.rowWidth),
          armed.current
        )

        if (decision === 'full' && target) {
          open.current = false
          if (closeOpenRow === close) {
            closeOpenRow = null
          }
          void settle(context.rowWidth, leftward).then((settled) => {
            if (!settled) {
              return
            }

            target.onSelect()
            setArmed(false)
            void settle(0, 0)
          })

          return
        }

        setArmed(false)
        if (decision === 'closed') {
          open.current = false
          if (closeOpenRow === close) {
            closeOpenRow = null
          }
          void settle(0, leftward)

          return
        }

        open.current = true
        closeOpenRow = close
        void settle(maxWidth, leftward)
      },
      cancel: () => {
        setArmed(false)
        void settle(open.current ? maxWidth : 0, 0)
      },
    })

    return () => {
      stop()
      motion.current?.stop()
      armMotion.current?.stop()
      document.removeEventListener('touchstart', onOutsideTouch, { capture: true })
      document.removeEventListener('scroll', onScroll, { capture: true })
      slide.removeEventListener('touchstart', onPanStart)
      slide.removeEventListener('touchmove', onPanMove)
      slide.removeEventListener('keydown', onKeyDown)
      if (closeOpenRow === close) {
        closeOpenRow = null
      }
    }
  }, [actions.length, maxWidth, close, reveal, draw, settle, setArmed])

  const renderAction = (action: SwipeAction) => (
    <button
      key={action.key}
      type="button"
      className={classes.action}
      data-tone={action.tone}
      onClick={() => {
        close()
        action.onSelect()
      }}
    >
      {action.label}
    </button>
  )

  return (
    <>
      <div
        ref={slideRef}
        className={classes.slide}
        onClickCapture={(event) => {
          if (open.current) {
            event.preventDefault()
            event.stopPropagation()
            close()
          }
        }}
      >
        {children}
      </div>
      <div className={classes.underlay} data-tone={(rest[0] ?? primary)?.tone}>
        <div className={classes.rest} style={{ right: primary ? ACTION_WIDTH : 0 }}>
          {rest.map(renderAction)}
        </div>
        {primary && (
          <button
            ref={primaryRef}
            type="button"
            className={classes.primary}
            data-tone={primary.tone}
            onClick={() => {
              close()
              primary.onSelect()
            }}
          >
            <span className={classes.label}>{primary.label}</span>
          </button>
        )}
      </div>
    </>
  )
}
