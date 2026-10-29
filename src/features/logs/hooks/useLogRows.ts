import { useCallback, useMemo, useState } from 'react'
import type { LogDocumentMeta, LogSection } from '../../../types/logs'
import { styleClassNames } from '../components/LogRow'
import { buildRows, filterRows, type LogFilter, overridesFor, type RowModel, revealLine } from '../lib/logRows'

interface RowsState {
  meta: LogDocumentMeta
  overrides: Set<number>
}

interface LogRowsOptions {
  meta: LogDocumentMeta
  filter: LogFilter
  matchLines: readonly number[]
}

export interface LogRowsState {
  model: RowModel
  overrides: ReadonlySet<number>
  groups: ReadonlyMap<number, LogSection>
  styleClasses: readonly string[]
  reveal: (line: number) => void
  onToggle: (sectionId: number) => void
}

export function useLogRows({ meta, filter, matchLines }: LogRowsOptions): LogRowsState {
  const [rowsState, setRowsState] = useState<RowsState>(() => ({
    meta,
    overrides: meta.focus_line === null ? new Set() : revealLine(meta, new Set(), meta.focus_line),
  }))

  let current = rowsState

  if (rowsState.meta !== meta) {
    current = {
      meta,
      overrides:
        rowsState.meta.revision === meta.revision
          ? rowsState.overrides
          : overridesFor(meta, rowsState.meta, rowsState.overrides),
    }
    setRowsState(current)
  }

  const { overrides } = current

  const model = useMemo(() => {
    if (filter === 'matches') {
      return filterRows(matchLines)
    }

    return filter === 'errors' ? filterRows(meta.error_lines) : buildRows(meta, overrides)
  }, [filter, matchLines, meta, overrides])

  const groups = useMemo(() => {
    const map = new Map<number, LogSection>()

    for (const section of meta.sections) {
      if (section.kind === 'group' && section.header_line !== null) {
        map.set(section.header_line, section)
      }
    }

    return map
  }, [meta.sections])
  const styleClasses = useMemo(() => styleClassNames(meta.styles), [meta.styles])

  const reveal = useCallback((line: number) => {
    setRowsState((previous) => {
      const next = revealLine(previous.meta, previous.overrides, line)

      return next === previous.overrides ? previous : { meta: previous.meta, overrides: next }
    })
  }, [])

  const onToggle = useCallback((sectionId: number) => {
    setRowsState((previous) => {
      const next = new Set(previous.overrides)

      if (!next.delete(sectionId)) {
        next.add(sectionId)
      }

      return { ...previous, overrides: next }
    })
  }, [])

  return { model, overrides, groups, styleClasses, reveal, onToggle }
}
