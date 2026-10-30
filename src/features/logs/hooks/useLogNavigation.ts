import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'
import type { LogDocumentMeta } from '../../../types/logs'
import type { LogRulerHandle } from '../components/LogOverviewRuler'
import {
  filterAfterJump,
  filterAfterQuery,
  type LogFilter,
  lineOfItem,
  type RowModel,
  rowAt,
  rowNearLine,
  rowOfLine,
} from '../lib/logRows'
import type { LogBodyHandle } from '../lib/logWindowTypes'
import type { LogSearchState } from './useLogSearch'

export interface LogSelection {
  anchor: number
  from: number
  to: number
}

interface LogNavigationOptions {
  meta: LogDocumentMeta
  model: RowModel
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  search: LogSearchState
  reveal: (line: number) => void
  bodyRef: RefObject<LogBodyHandle | null>
  rulerRef: RefObject<LogRulerHandle | null>
}

export interface LogNavigationState {
  selection: LogSelection | null
  currentIdx: number | null
  clearSelection: () => void
  goToLine: (line: number, align?: 'center' | 'start') => void
  nextMatch: () => void
  prevMatch: () => void
  stepError: (direction: 1 | -1) => void
  changeText: (value: string) => void
  changeRegex: (value: boolean) => void
  changeCaseSensitive: (value: boolean) => void
  onSelect: (line: number, extend: boolean) => void
  onViewport: (firstRow: number, lastRow: number) => void
  initialFocus: () => void
}

export function useLogNavigation({
  meta,
  model,
  filter,
  onFilterChange,
  search,
  reveal,
  bodyRef,
  rulerRef,
}: LogNavigationOptions): LogNavigationState {
  const { matches, matchLines, result, settled, setQueryText, setRegex, setCaseSensitive } = search
  const lastError = useRef<number | null>(null)
  const pendingJump = useRef(false)
  const modelRef = useRef<RowModel>(model)
  const [currentIdx, setCurrentIdx] = useState<number | null>(null)
  const [selection, setSelection] = useState<LogSelection | null>(null)
  const [jump, setJump] = useState<{ line: number; align: 'center' | 'start' } | null>(null)

  modelRef.current = model

  const anchorLine = useCallback(() => {
    const range = bodyRef.current?.getVisibleRange()

    return range ? lineOfItem(rowAt(modelRef.current, range.first)) : 0
  }, [bodyRef])

  const goToLine = useCallback(
    (line: number, align: 'center' | 'start' = 'center') => {
      const nextFilter = filterAfterJump(filter, line, matchLines, meta.error_lines)

      reveal(line)

      if (nextFilter !== filter) {
        onFilterChange(nextFilter)
      }

      setJump({ line, align })
    },
    [filter, matchLines, meta.error_lines, reveal, onFilterChange]
  )

  useEffect(() => {
    if (!jump) {
      return
    }

    const row = rowOfLine(modelRef.current, jump.line) ?? rowNearLine(modelRef.current, jump.line)

    bodyRef.current?.scrollToRow(row, jump.align)
  }, [jump, bodyRef])

  const goToMatch = (index: number) => {
    const match = matches[index]

    if (match) {
      setCurrentIdx(index)
      setSelection({ anchor: match[0], from: match[0], to: match[0] })
      goToLine(match[0])
    }
  }

  const nextMatch = () => {
    if (matches.length === 0) {
      return
    }

    if (currentIdx !== null) {
      goToMatch((currentIdx + 1) % matches.length)

      return
    }

    const anchor = anchorLine()
    const found = matches.findIndex((match) => match[0] >= anchor)

    goToMatch(found === -1 ? 0 : found)
  }

  const prevMatch = () => {
    if (matches.length === 0) {
      return
    }

    if (currentIdx !== null) {
      goToMatch((currentIdx - 1 + matches.length) % matches.length)

      return
    }

    const anchor = anchorLine()
    const found = matches.findLastIndex((match) => match[0] < anchor)

    goToMatch(found === -1 ? matches.length - 1 : found)
  }

  const changeQuery = <T>(set: (value: T) => void, value: T) => {
    pendingJump.current = true
    setCurrentIdx(null)
    set(value)
  }

  const changeText = (value: string) => {
    onFilterChange(filterAfterQuery(filter, value))
    changeQuery(setQueryText, value)
  }

  const changeRegex = (value: boolean) => changeQuery(setRegex, value)

  const changeCaseSensitive = (value: boolean) => changeQuery(setCaseSensitive, value)

  useEffect(() => {
    if (!result || !settled) {
      return
    }

    if (pendingJump.current) {
      pendingJump.current = false

      if (result.matches.length > 0) {
        const anchor = anchorLine()
        const found = result.matches.findIndex((match) => match[0] >= anchor)
        const index = found === -1 ? 0 : found
        const target = result.matches[index]

        if (target) {
          setCurrentIdx(index)
          setSelection({ anchor: target[0], from: target[0], to: target[0] })
          goToLine(target[0])
        }
      }
    } else {
      setCurrentIdx((index) => (index !== null && index < result.matches.length ? index : null))
    }
  }, [result, settled, anchorLine, goToLine])

  const stepError = (direction: 1 | -1) => {
    const errors = meta.error_lines

    if (errors.length === 0) {
      return
    }

    const range = bodyRef.current?.getVisibleRange()
    const last = lastError.current
    const lastRow = last === null ? null : rowOfLine(modelRef.current, last)
    const visible = lastRow !== null && range !== undefined && lastRow >= range.first && lastRow <= range.last
    const reference = visible && last !== null ? last : anchorLine()
    const target =
      direction > 0
        ? (errors.find((line) => line > reference) ?? errors[0])
        : (errors.findLast((line) => line < reference) ?? errors.at(-1))

    if (target === undefined) {
      return
    }

    lastError.current = target
    setSelection({ anchor: target, from: target, to: target })
    goToLine(target)
  }

  const onSelect = useCallback(
    (line: number, extend: boolean) => {
      if (filter !== 'none') {
        onFilterChange('none')
        setSelection({ anchor: line, from: line, to: line })
        goToLine(line)

        return
      }

      setSelection((previous) => {
        if (extend && previous) {
          return {
            anchor: previous.anchor,
            from: Math.min(previous.anchor, line),
            to: Math.max(previous.anchor, line),
          }
        }

        return previous && previous.from === line && previous.to === line
          ? null
          : { anchor: line, from: line, to: line }
      })
    },
    [filter, goToLine, onFilterChange]
  )

  const onViewport = useCallback(
    (firstRow: number, lastRow: number) => {
      rulerRef.current?.setViewport(
        lineOfItem(rowAt(modelRef.current, firstRow)),
        lineOfItem(rowAt(modelRef.current, lastRow))
      )
    },
    [rulerRef]
  )

  const initialFocus = () => {
    if (meta.focus_line !== null) {
      lastError.current = meta.focus_line
      setSelection({ anchor: meta.focus_line, from: meta.focus_line, to: meta.focus_line })
      goToLine(meta.focus_line)
    } else if (!meta.complete) {
      bodyRef.current?.scrollToEnd()
    }
  }

  return {
    selection,
    currentIdx,
    clearSelection: () => setSelection(null),
    goToLine,
    nextMatch,
    prevMatch,
    stepError,
    changeText,
    changeRegex,
    changeCaseSensitive,
    onSelect,
    onViewport,
    initialFocus,
  }
}
