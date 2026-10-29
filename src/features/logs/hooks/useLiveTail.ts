import { useCallback, useEffect, useRef, useState } from 'react'

const FRESH_MS = 1500

interface TailState {
  revision: number
  lineCount: number
  seen: number
  atEnd: boolean
  freshFrom: number | null
}

interface LiveTail {
  behind: number
  freshFrom: number | null
  onViewport: (lastRow: number) => void
}

export function useLiveTail(revision: number, lineCount: number, rowCount: number): LiveTail {
  const [state, setState] = useState<TailState>({ revision, lineCount, seen: lineCount, atEnd: true, freshFrom: null })
  const latest = useRef({ lineCount, rowCount })

  latest.current = { lineCount, rowCount }

  if (state.revision !== revision) {
    setState({
      revision,
      lineCount,
      seen: state.atEnd ? lineCount : state.seen,
      atEnd: state.atEnd,
      freshFrom: lineCount > state.lineCount ? state.lineCount : state.freshFrom,
    })
  }

  useEffect(() => {
    if (state.freshFrom === null) {
      return undefined
    }

    const timer = setTimeout(() => setState((s) => ({ ...s, freshFrom: null })), FRESH_MS)

    return () => clearTimeout(timer)
  }, [state.freshFrom])

  const onViewport = useCallback((lastRow: number) => {
    const atEnd = lastRow >= latest.current.rowCount - 1

    setState((s) => {
      const seen = atEnd ? latest.current.lineCount : s.seen

      return s.atEnd === atEnd && s.seen === seen ? s : { ...s, atEnd, seen }
    })
  }, [])

  return { behind: Math.max(0, lineCount - state.seen), freshFrom: state.freshFrom, onViewport }
}
