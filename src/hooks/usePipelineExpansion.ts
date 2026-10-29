import { useCallback, useState } from 'react'

import { isRecord } from '../utils/isRecord'

const STORAGE_KEY = 'pipedash-pipeline-expansion'
const EMPTY: Record<string, boolean> = {}

export type ExpansionMap = Record<string, Record<string, boolean>>

export function readExpansion(): ExpansionMap {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')
    const expanded = isRecord(parsed) && isRecord(parsed.state) ? parsed.state.expanded : null

    if (!isRecord(expanded)) {
      return {}
    }

    const result: ExpansionMap = {}

    for (const [view, flags] of Object.entries(expanded)) {
      if (isRecord(flags)) {
        const entries: Record<string, boolean> = {}

        for (const [id, open] of Object.entries(flags)) {
          if (typeof open === 'boolean') {
            entries[id] = open
          }
        }

        result[view] = entries
      }
    }

    return result
  } catch {
    return {}
  }
}

export function writeExpansion(expanded: ExpansionMap): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify({ state: { expanded }, version: 0 }))
}

interface PipelineExpansionResult {
  isOpen: (id: string, fallback: boolean) => boolean
  toggle: (id: string, fallback: boolean) => void
  setAll: (ids: string[], open: boolean) => void
}

export function usePipelineExpansion(
  view: string,
  expanded: ExpansionMap,
  setExpanded: (view: string, changes: Record<string, boolean>) => void,
  filterKey: string,
  filtersActive: boolean
): PipelineExpansionResult {
  const saved = expanded[view] ?? EMPTY
  const [session, setSession] = useState({ key: filterKey, overrides: EMPTY })

  if (session.key !== filterKey) {
    setSession({ key: filterKey, overrides: EMPTY })
  }

  const overrides = filtersActive ? session.overrides : saved

  const isOpen = useCallback(
    (id: string, fallback: boolean) => overrides[id] ?? (filtersActive || fallback),
    [overrides, filtersActive]
  )

  const setAll = useCallback(
    (ids: string[], open: boolean) => {
      const changes = Object.fromEntries(ids.map((id) => [id, open]))

      if (filtersActive) {
        setSession((s) => ({ ...s, overrides: { ...s.overrides, ...changes } }))
      } else {
        setExpanded(view, changes)
      }
    },
    [filtersActive, setExpanded, view]
  )

  const toggle = useCallback((id: string, fallback: boolean) => setAll([id], !isOpen(id, fallback)), [isOpen, setAll])

  return { isOpen, toggle, setAll }
}
