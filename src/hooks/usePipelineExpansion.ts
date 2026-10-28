import { useCallback, useState } from 'react'

import { usePipelineExpansionStore } from '../stores/pipelineExpansionStore'

const EMPTY: Record<string, boolean> = {}

export function usePipelineExpansion(view: string, filterKey: string, filtersActive: boolean) {
  const saved = usePipelineExpansionStore((s) => s.expanded[view] ?? EMPTY)
  const setExpanded = usePipelineExpansionStore((s) => s.setExpanded)
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
