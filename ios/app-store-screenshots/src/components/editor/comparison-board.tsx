'use client'
import type { ReactNode, Ref } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import type { ProjectState } from '@/lib/types'
import { DeckPreview } from './style-lab-preview'

const BOARD_ROW_H = 520
const BOARD_PAD = 72
const BOARD_MAX_W = 12000

export type BoardRow = { title: string; project: ProjectState }

// Offscreen board captured for "Export comparison". Lives at the page origin
// (behind the app) because html-to-image needs an untransformed node.
export function ComparisonBoard({
  appName,
  rows,
  ref,
}: {
  appName: string
  rows: BoardRow[]
  ref: Ref<HTMLDivElement>
}): ReactNode {
  const first = rows[0]?.project
  if (!first) {
    return null
  }
  const { cW, cH } = getCanvas(first.device, first.orientation)
  const count = (first.slidesByDevice[first.device] || []).length
  // Stay well under Safari's canvas limit for long or landscape decks.
  const rowH = Math.min(BOARD_ROW_H, Math.floor((BOARD_MAX_W - BOARD_PAD * 2) / Math.max(1, count) / (cW / cH)))
  const stripW = Math.ceil(count * cW * (rowH / cH))
  const width = Math.max(stripW, 960) + BOARD_PAD * 2
  return (
    <div
      ref={ref}
      aria-hidden
      style={{
        position: 'fixed',
        left: 0,
        top: 0,
        zIndex: -1,
        pointerEvents: 'none',
        width,
        padding: BOARD_PAD,
        background: '#F4F3EF',
        color: '#141413',
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      }}
    >
      <div style={{ fontSize: 22, letterSpacing: 2, textTransform: 'uppercase', opacity: 0.55, fontWeight: 600 }}>
        Style Lab
      </div>
      <div style={{ fontSize: 56, fontWeight: 700, marginTop: 6, marginBottom: 40 }}>{appName || 'My App'}</div>
      {rows.map((row) => (
        <div key={row.title} style={{ marginBottom: 44 }}>
          <div style={{ fontSize: 26, fontWeight: 600, marginBottom: 14 }}>{row.title}</div>
          <DeckPreview project={row.project} height={rowH} />
        </div>
      ))}
    </div>
  )
}
