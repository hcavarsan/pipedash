import { useEffect, useState, useSyncExternalStore } from 'react'

import { Z } from '../theme/zIndex'

export interface SheetHandle {
  requestClose: () => Promise<boolean>
  closeNow: () => Promise<boolean>
}

let stack: readonly number[] = []
let nextId = 1
const listeners = new Set<() => void>()
const handles = new Map<number, SheetHandle>()

function publish(next: readonly number[]) {
  stack = next

  for (const listener of listeners) {
    listener()
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener)

  return () => listeners.delete(listener)
}

export function useSheetStack(active: boolean): { id: number; zIndex: number; covered: number } {
  const [id] = useState(() => nextId++)
  const snapshot = useSyncExternalStore(subscribe, () => stack)

  useEffect(() => {
    if (!active) {
      return
    }

    publish([...stack, id])

    return () => publish(stack.filter((other) => other !== id))
  }, [active, id])

  const index = snapshot.indexOf(id)

  const zIndex = Z.sheetBase + Math.max(index, 0) * Z.sheetStep

  return { id, zIndex, covered: index < 0 ? 0 : snapshot.length - 1 - index }
}

export function registerSheet(id: number, handle: SheetHandle): () => void {
  handles.set(id, handle)

  return () => {
    handles.delete(id)
  }
}

export const sheetHandle = (element: HTMLElement): SheetHandle | undefined =>
  handles.get(Number(element.dataset.sheetId))

export const sheetMark = (): number => nextId

export async function closeSheetsFrom(mark: number): Promise<boolean> {
  const open = [...handles.keys()].filter((id) => id >= mark).sort((a, b) => b - a)

  for (const id of open) {
    if ((await handles.get(id)?.requestClose()) === false) {
      return false
    }
  }

  return true
}
