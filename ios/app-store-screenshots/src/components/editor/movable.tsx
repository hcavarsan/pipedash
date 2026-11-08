'use client'
import { RotateCw } from 'lucide-react'
import { type CSSProperties, type PointerEvent, type ReactNode, useEffect, useRef } from 'react'
import { Rnd } from 'react-rnd'
import type { ElementTransform } from '@/lib/types'
import type { Rect } from './canvas-types'

// Fraction of an element's width/height that must remain inside the canvas
// when overflow is allowed. Keeps a graspable handle visible so the user can
// always drag the element back onto the canvas.
const MIN_VISIBLE_FRAC = 0.1

function clampRect(r: Rect, boundsW: number, boundsH: number, allowOverflow = false): Rect {
  if (allowOverflow) {
    const { width, height } = r
    const minVisX = Math.max(8, width * MIN_VISIBLE_FRAC)
    const minVisY = Math.max(8, height * MIN_VISIBLE_FRAC)
    const x = Math.max(-(width - minVisX), Math.min(r.x, boundsW - minVisX))
    const y = Math.max(-(height - minVisY), Math.min(r.y, boundsH - minVisY))
    return { x, y, width, height }
  }
  const width = Math.min(r.width, boundsW)
  const height = Math.min(r.height, boundsH)
  const x = Math.max(0, Math.min(r.x, boundsW - width))
  const y = Math.max(0, Math.min(r.y, boundsH - height))
  return { x, y, width, height }
}

function pointerAngle(x: number, y: number, centerX: number, centerY: number): number {
  return (Math.atan2(y - centerY, x - centerX) * 180) / Math.PI
}

function normalizeRotation(degrees: number): number {
  let next = degrees
  while (next > 180) {
    next -= 360
  }
  while (next < -180) {
    next += 360
  }
  return Math.round(next)
}

// Follows the pointer around the element's centre until it is released.
function trackRotation(
  root: HTMLElement,
  start: { x: number; y: number; rotation: number },
  onRotate: (rotation: number) => void
): void {
  const box = root.getBoundingClientRect()
  const centerX = box.left + box.width / 2
  const centerY = box.top + box.height / 2
  const startAngle = pointerAngle(start.x, start.y, centerX, centerY)

  const handleMove = (event: globalThis.PointerEvent) => {
    event.preventDefault()
    onRotate(
      normalizeRotation(start.rotation + pointerAngle(event.clientX, event.clientY, centerX, centerY) - startAngle)
    )
  }
  const stopRotate = () => {
    window.removeEventListener('pointermove', handleMove)
    window.removeEventListener('pointerup', stopRotate)
    window.removeEventListener('pointercancel', stopRotate)
  }

  window.addEventListener('pointermove', handleMove, { passive: false })
  window.addEventListener('pointerup', stopRotate, { once: true })
  window.addEventListener('pointercancel', stopRotate, { once: true })
}

// Subtle resize handles (visible only on hover via globals.css).
const handleSize = 14
const handleStyle: Record<string, CSSProperties> = {
  top: { height: handleSize },
  right: { width: handleSize },
  bottom: { height: handleSize },
  left: { width: handleSize },
  topRight: { width: handleSize, height: handleSize },
  bottomRight: { width: handleSize, height: handleSize },
  bottomLeft: { width: handleSize, height: handleSize },
  topLeft: { width: handleSize, height: handleSize },
}

export function Movable({
  rect,
  boundsW,
  boundsH,
  editable,
  previewScale,
  onChange,
  children,
  lockAspectRatio = false,
  zIndex,
  rotation,
  allowOverflow = false,
  selected = false,
  onSelect,
}: {
  rect: Rect
  boundsW: number
  boundsH: number
  editable?: boolean | undefined
  previewScale: number
  onChange: (t: ElementTransform) => void
  children: ReactNode
  lockAspectRatio?: number | boolean | undefined
  zIndex: number
  rotation: number
  allowOverflow?: boolean | undefined
  selected?: boolean | undefined
  onSelect?: (() => void) | undefined
}): ReactNode {
  const rotationRef = useRef(rotation)
  useEffect(() => {
    rotationRef.current = rotation
  }, [rotation])

  // The editor shows the clamped rect, so export and thumbnails must place the
  // element there too — an out-of-bounds saved rect would otherwise export
  // somewhere the user never saw it.
  const display = clampRect(rect, boundsW, boundsH, allowOverflow)

  function startRotate(e: PointerEvent<HTMLButtonElement>) {
    e.preventDefault()
    e.stopPropagation()
    onSelect?.()

    const root = e.currentTarget.closest<HTMLElement>('.rnd-editable')
    if (!root) {
      return
    }
    trackRotation(root, { x: e.clientX, y: e.clientY, rotation: rotationRef.current }, (nextRotation) => {
      rotationRef.current = nextRotation
      onChange({ ...display, rotation: nextRotation, zIndex })
    })
  }

  // Rotation lives on the inner wrapper so the Rnd's axis-aligned rect remains
  // the authoritative bounding box for drag/resize math.
  const rotated = (
    <div
      style={{
        width: '100%',
        height: '100%',
        transform: rotation ? `rotate(${rotation}deg)` : undefined,
        transformOrigin: 'center center',
      }}
    >
      {children}
    </div>
  )

  // Non-editable (export/thumb) path: plain absolute-positioned div, no Rnd.
  if (!editable) {
    return (
      <div
        style={{
          position: 'absolute',
          left: display.x,
          top: display.y,
          width: display.width,
          height: display.height,
          zIndex,
        }}
      >
        {rotated}
      </div>
    )
  }

  const controlScale = Math.max(0.05, previewScale)
  const boundsProp: { bounds?: string } = allowOverflow ? {} : { bounds: 'parent' }

  return (
    <Rnd
      {...boundsProp}
      scale={previewScale}
      lockAspectRatio={lockAspectRatio}
      position={{ x: display.x, y: display.y }}
      size={{ width: display.width, height: display.height }}
      // A bare mousedown (no stopPropagation — that would prevent react-rnd from
      // starting a drag) marks the element as the current selection.
      onMouseDown={() => onSelect?.()}
      onDragStart={() => onSelect?.()}
      onResizeStart={() => onSelect?.()}
      onDragStop={(_e, d) => {
        const next = clampRect(
          { x: d.x, y: d.y, width: display.width, height: display.height },
          boundsW,
          boundsH,
          allowOverflow
        )
        onChange({ ...next, rotation, zIndex })
      }}
      onResizeStop={(_e, _dir, ref, _delta, position) => {
        const next = clampRect(
          {
            x: position.x,
            y: position.y,
            width: parseFloat(ref.style.width),
            height: parseFloat(ref.style.height),
          },
          boundsW,
          boundsH,
          allowOverflow
        )
        onChange({ ...next, rotation, zIndex })
      }}
      style={{ zIndex }}
      resizeHandleStyles={handleStyle}
      className={selected ? 'rnd-editable rnd-selected' : 'rnd-editable'}
    >
      {rotated}
      <button
        type="button"
        className="rnd-rotate-handle"
        style={{
          right: -14 / controlScale,
          top: -14 / controlScale,
          width: 28 / controlScale,
          height: 28 / controlScale,
        }}
        onPointerDown={startRotate}
        onFocus={() => onSelect?.()}
        title="Rotate"
        aria-label="Rotate element"
      >
        <RotateCw style={{ width: 14 / controlScale, height: 14 / controlScale }} />
      </button>
    </Rnd>
  )
}
