import { useComputedColorScheme } from '@mantine/core'
import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react'

import classes from './LogViewer.module.css'

const TICK_PX = 2
const SMALL_STEP = 0.01
const LARGE_STEP = 0.1

export interface LogRulerHandle {
  setViewport: (firstLine: number, lastLine: number) => void
}

interface LogOverviewRulerProps {
  lineCount: number
  errorLines: readonly number[]
  warningLines: readonly number[]
  matchLines: readonly number[]
  onSeek: (line: number) => void
  ref?: Ref<LogRulerHandle>
}

export function LogOverviewRuler({
  ref,
  lineCount,
  errorLines,
  warningLines,
  matchLines,
  onSeek,
}: LogOverviewRulerProps): ReactNode {
  const rootRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const frameRef = useRef<number | null>(null)
  const pointerY = useRef(0)
  const dragging = useRef(false)
  const [band, setBand] = useState({ first: 0, last: 0 })
  const [size, setSize] = useState({ width: 0, height: 0 })
  const scheme = useComputedColorScheme('dark')

  useImperativeHandle(ref, () => ({ setViewport: (first, last) => setBand({ first, last }) }), [])

  useEffect(() => {
    const root = rootRef.current

    if (!root) {
      return
    }

    const observer = new ResizeObserver(() => setSize({ width: root.clientWidth, height: root.clientHeight }))

    observer.observe(root)
    setSize({ width: root.clientWidth, height: root.clientHeight })

    return () => observer.disconnect()
  }, [])

  useEffect(
    () => () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current)
      }
    },
    []
  )

  useEffect(() => {
    const canvas = canvasRef.current
    const context = canvas?.getContext('2d')

    if (!canvas || !context || size.height === 0 || scheme.length === 0) {
      return
    }

    const ratio = window.devicePixelRatio || 1
    const style = getComputedStyle(canvas)
    const layers: Array<[readonly number[], string]> = [
      [warningLines, '--mantine-color-orange-filled'],
      [matchLines, '--mantine-color-blue-filled'],
      [errorLines, '--mantine-color-red-filled'],
    ]

    canvas.width = Math.round(size.width * ratio)
    canvas.height = Math.round(size.height * ratio)
    context.setTransform(ratio, 0, 0, ratio, 0, 0)
    context.clearRect(0, 0, size.width, size.height)

    for (const [lines, token] of layers) {
      const drawn = new Uint8Array(size.height)

      context.fillStyle = style.getPropertyValue(token).trim()

      for (const line of lines) {
        const y = Math.min(size.height - 1, Math.floor((line / Math.max(lineCount, 1)) * size.height))

        if (!drawn[y]) {
          drawn[y] = 1
          context.fillRect(0, y, size.width, TICK_PX)
        }
      }
    }
  }, [lineCount, errorLines, warningLines, matchLines, size, scheme])

  const seekTo = (clientY: number) => {
    pointerY.current = clientY

    if (frameRef.current !== null) {
      return
    }

    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null

      const rect = rootRef.current?.getBoundingClientRect()

      if (rect && rect.height > 0 && lineCount > 0) {
        const fraction = Math.min(Math.max((pointerY.current - rect.top) / rect.height, 0), 1)

        onSeek(Math.min(lineCount - 1, Math.floor(fraction * lineCount)))
      }
    })
  }

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId)
    dragging.current = true
    seekTo(event.clientY)
  }

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragging.current) {
      seekTo(event.clientY)
    }
  }

  const onPointerEnd = (event: PointerEvent<HTMLDivElement>) => {
    dragging.current = false
    event.currentTarget.releasePointerCapture(event.pointerId)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const small = Math.max(1, Math.round(lineCount * SMALL_STEP))
    const large = Math.max(1, Math.round(lineCount * LARGE_STEP))
    const target: Record<string, number> = {
      ArrowUp: band.first - small,
      ArrowDown: band.first + small,
      PageUp: band.first - large,
      PageDown: band.first + large,
      Home: 0,
      End: lineCount - 1,
    }

    const seek = target[event.key]

    if (seek !== undefined && lineCount > 0) {
      event.preventDefault()
      onSeek(Math.min(Math.max(seek, 0), lineCount - 1))
    }
  }

  const total = Math.max(lineCount, 1)

  return (
    <div
      ref={rootRef}
      className={classes.ruler}
      role="slider"
      tabIndex={0}
      aria-label="Log position"
      aria-orientation="vertical"
      aria-valuemin={1}
      aria-valuemax={Math.max(lineCount, 1)}
      aria-valuenow={Math.min(band.first + 1, Math.max(lineCount, 1))}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onKeyDown={onKeyDown}
    >
      <canvas ref={canvasRef} className={classes.rulerCanvas} aria-hidden />
      <div
        className={classes.rulerBand}
        style={{
          top: `${(band.first / total) * 100}%`,
          height: `${((band.last - band.first + 1) / total) * 100}%`,
        }}
      />
    </div>
  )
}
