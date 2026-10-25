import { type CSSProperties, type ReactNode, useCallback, useEffect, useRef } from 'react'

import { decideRowSwipe, rowPosition } from '../../lib/gestures'
import { trackTouches } from '../../lib/touchTracker'

import classes from './SwipeActions.module.css'

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

const ACTION_WIDTH = 80
const SNAP_MS = 260

let closeOpenRow: (() => void) | null = null

export function SwipeActions({ actions, children }: SwipeActionsProps) {
  const slideRef = useRef<HTMLDivElement>(null)
  const underlayRef = useRef<HTMLDivElement>(null)
  const open = useRef(false)
  const actionsRef = useRef(actions)
  const maxWidth = actions.length * ACTION_WIDTH

  actionsRef.current = actions

  const place = useCallback((position: number, revealed: boolean) => {
    const slide = slideRef.current

    if (!slide) {
      return
    }

    slide.style.transform = position === 0 ? '' : `translateX(${-position}px)`
    underlayRef.current?.style.setProperty('--pos', `${position}px`)
    slide.toggleAttribute('data-revealed', revealed)
  }, [])

  const snapTo = useCallback(
    (from: number, to: number, onDone?: () => void) => {
      const animation = slideRef.current?.animate(
        [{ transform: `translateX(${-from}px)` }, { transform: `translateX(${-to}px)` }],
        { duration: SNAP_MS, easing: 'cubic-bezier(0.32, 0.72, 0, 1)' }
      )

      place(to, to > 0)

      if (animation) {
        animation.onfinish = () => onDone?.()
      }
    },
    [place]
  )

  const close = useCallback(() => {
    if (!open.current) {
      return
    }

    open.current = false
    if (closeOpenRow === close) {
      closeOpenRow = null
    }
    snapTo(actionsRef.current.length * ACTION_WIDTH, 0)
  }, [snapTo])

  useEffect(() => {
    const slide = slideRef.current

    if (!slide || actions.length === 0) {
      return
    }

    const onOutsideTouch = (event: Event) => {
      if (open.current && event.target instanceof Node && !slide.parentElement?.contains(event.target)) {
        close()
      }
    }

    document.addEventListener('touchstart', onOutsideTouch, { capture: true, passive: true })

    const stop = trackTouches(slide, {
      axis: 'x',
      begin: () => {
        if (closeOpenRow && closeOpenRow !== close) {
          closeOpenRow()
        }

        return { start: open.current ? maxWidth : 0, rowWidth: slide.offsetWidth }
      },
      accept: (dx) => open.current || dx < 0,
      move: ({ start, rowWidth }, { dx }) => place(rowPosition(start - dx, maxWidth, rowWidth), true),
      end: ({ start, rowWidth }, { dx }, { vx }) => {
        const position = rowPosition(start - dx, maxWidth, rowWidth)
        const primary = actionsRef.current.find((action) => action.primary)
        const decision = decideRowSwipe(position, -vx, maxWidth, rowWidth)

        if (decision === 'full' && primary) {
          open.current = false
          snapTo(position, rowWidth, () => {
            primary.onSelect()
            snapTo(rowWidth, 0)
          })

          return
        }

        if (decision === 'closed') {
          open.current = false
          snapTo(position, 0)

          return
        }

        open.current = true
        closeOpenRow = close
        snapTo(position, maxWidth)
      },
      cancel: () => place(open.current ? maxWidth : 0, open.current),
    })

    return () => {
      stop()
      document.removeEventListener('touchstart', onOutsideTouch, { capture: true })
      if (closeOpenRow === close) {
        closeOpenRow = null
      }
    }
  }, [actions.length, maxWidth, close, place, snapTo])

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
      <div
        ref={underlayRef}
        className={classes.underlay}
        style={{ '--actions': `${maxWidth}px` } as CSSProperties}
        aria-hidden
      >
        {actions.map((action) => (
          <button
            key={action.key}
            type="button"
            className={classes.action}
            data-tone={action.tone}
            data-primary={action.primary ? '' : undefined}
            tabIndex={-1}
            onClick={() => {
              close()
              action.onSelect()
            }}
          >
            {action.label}
          </button>
        ))}
      </div>
    </>
  )
}
