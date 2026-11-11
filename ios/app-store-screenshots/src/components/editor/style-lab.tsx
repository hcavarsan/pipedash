'use client'
import { FlaskConical } from 'lucide-react'
import { type ReactNode, useCallback, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { toast } from 'sonner'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { getCanvas } from '@/lib/canvas-device'
import { DEVICE_LABEL } from '@/lib/device-specs'
import {
  applyLook,
  canRearrange,
  currentLook,
  generateLook,
  LOCK_KEYS,
  type Locks,
  NO_LOCKS,
  pickDirections,
  savedLookId,
  withLocks,
} from '@/lib/style-lab'
import type { Look, ProjectState } from '@/lib/types'
import { ComparisonBoard } from './comparison-board'
import { GeneratedLookActions, SavedLookActions, StyleLabControls } from './style-lab-actions'
import { LookRow } from './style-lab-preview'
import { useComparisonExport } from './use-comparison-export'

type Slot = { direction: string; seed: number }
type Row = { key: string; title: string; look: Look; current: boolean }

const PREVIEW_MIN_H = 160
const PREVIEW_MAX_H = 440
// Card padding + border inside the scrolling list.
const ROW_CHROME_W = 28

function slotsFor(round: number): Slot[] {
  return pickDirections(round).map((direction, i) => ({ direction, seed: round * 16 + i + 1 }))
}

export function StyleLab({
  open,
  onOpenChange,
  state,
  onApply,
  onSavedLooksChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  state: ProjectState
  onApply: (next: ProjectState, look: Look) => void
  onSavedLooksChange: (looks: Look[]) => void
}): ReactNode {
  const [locks, setLocks] = useState<Locks>(NO_LOCKS)
  const [round, setRound] = useState(0)
  const [slots, setSlots] = useState<Slot[]>(() => slotsFor(0))
  const { exporting, boardRef, exportComparison } = useComparisonExport(state.appName)
  const [listWidth, setListWidth] = useState(0)
  const listObserver = useRef<ResizeObserver | null>(null)
  // Callback ref: the list only exists while the dialog is open.
  const listRef = useCallback((node: HTMLDivElement | null) => {
    listObserver.current?.disconnect()
    if (!node) {
      return
    }
    const observer = new ResizeObserver(([entry]) => {
      if (entry) {
        setListWidth(entry.contentRect.width)
      }
    })
    observer.observe(node)
    listObserver.current = observer
  }, [])

  const slides = state.slidesByDevice[state.device] || []
  const rearrange = canRearrange(state.device, state.orientation)
  const saved = state.savedLooks || []
  const base = useMemo(() => currentLook(state), [state])
  const looks = useMemo(
    () => slots.map((slot) => withLocks(generateLook(slot.direction, slot.seed), base, locks)),
    [slots, base, locks]
  )
  // Size every deck preview so the whole strip fits the row without scrolling.
  const { cW, cH } = getCanvas(state.device, state.orientation)
  const previewH =
    Math.round(
      Math.max(
        PREVIEW_MIN_H,
        Math.min(PREVIEW_MAX_H, (listWidth - ROW_CHROME_W) / (Math.max(1, slides.length) * (cW / cH)))
      )
    ) || PREVIEW_MIN_H
  const allLocked = LOCK_KEYS.every((key) => locks[key] || (key === 'layout' && !rearrange))

  function shuffle() {
    const next = round + 1
    setRound(next)
    setSlots(slotsFor(next))
  }

  function remix(index: number) {
    setSlots((prev) => prev.map((slot, i) => (i === index ? { ...slot, seed: slot.seed + 1009 } : slot)))
  }

  function apply(look: Look) {
    onApply(applyLook(state, look, locks), look)
    onOpenChange(false)
  }

  function removeSaved(look: Look) {
    onSavedLooksChange(saved.filter((s) => s.id !== look.id))
  }

  function toggleSaved(look: Look) {
    if (saved.some((s) => s.id === look.id)) {
      removeSaved(look)
      return
    }
    onSavedLooksChange([...saved, { ...look, id: savedLookId(look, saved) }])
    toast.success('Look saved', { description: 'Find it under Saved looks in Style Lab.' })
  }

  const rows: Row[] = [
    { key: 'current', title: 'Current', look: base, current: true },
    ...looks.map((look, i) => ({ key: `slot-${i}`, title: look.name, look, current: false })),
  ]

  function projectFor(row: Row): ProjectState {
    return row.current ? state : applyLook(state, row.look, locks)
  }

  const lookRows = rows.map((row, i) => (
    <LookRow
      key={row.key}
      title={row.title}
      look={row.look}
      project={projectFor(row)}
      height={previewH}
      current={row.current}
      actions={
        row.current ? (
          <span className="text-[11px] text-muted-foreground">Your deck now</span>
        ) : (
          <GeneratedLookActions
            look={row.look}
            title={row.title}
            saved={saved.some((s) => s.id === row.look.id)}
            onRemix={() => remix(i - 1)}
            onToggleSaved={toggleSaved}
            onApply={apply}
          />
        )
      }
    />
  ))

  const savedRows = saved.map((look) => (
    <LookRow
      key={`saved-${look.id}`}
      title={look.name}
      look={look}
      project={applyLook(state, look, locks)}
      height={previewH}
      actions={<SavedLookActions look={look} onRemove={removeSaved} onApply={apply} />}
    />
  ))

  const savedSection =
    saved.length > 0 ? (
      <div className="space-y-3 pt-3">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Saved looks</h3>
        {savedRows}
      </div>
    ) : null

  return (
    <Dialog open={open} onOpenChange={(next) => !exporting && onOpenChange(next)}>
      <DialogContent className="flex h-[92vh] max-w-[min(1240px,96vw)] flex-col gap-0 p-0">
        <DialogHeader className="space-y-1 border-b px-5 pb-3 pt-4 text-left">
          <DialogTitle className="flex items-center gap-2">
            <FlaskConical className="h-4 w-4" /> Style Lab
          </DialogTitle>
          <DialogDescription>
            Four complete looks for your {DEVICE_LABEL[state.device]} deck. Lock what you like, shuffle the rest, apply
            one. Your screenshots and copy stay as they are.
          </DialogDescription>
        </DialogHeader>

        <StyleLabControls
          locks={locks}
          rearrange={rearrange}
          allLocked={allLocked}
          exporting={exporting}
          canExport={slides.length > 0}
          onToggleLock={(key) => setLocks((prev) => ({ ...prev, [key]: !prev[key] }))}
          onExport={() => void exportComparison()}
          onShuffle={shuffle}
        />

        <div
          ref={listRef}
          className="min-h-0 flex-1 space-y-3 overflow-y-auto bg-muted/30 px-5 py-4"
          data-testid="style-lab-looks"
        >
          {slides.length === 0 ? (
            <p className="py-16 text-center text-sm text-muted-foreground">Add a screen to try looks on it.</p>
          ) : (
            <>
              {lookRows}
              {savedSection}
            </>
          )}
        </div>
      </DialogContent>
      {exporting &&
        createPortal(
          <ComparisonBoard
            ref={boardRef}
            appName={state.appName}
            rows={rows.map((row) => ({ title: row.title, project: projectFor(row) }))}
          />,
          document.body
        )}
    </Dialog>
  )
}
