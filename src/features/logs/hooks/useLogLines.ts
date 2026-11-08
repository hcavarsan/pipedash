import { useEffect, useLayoutEffect, useState, useSyncExternalStore } from 'react'

import { logLineCache } from '../lib/lineCache'
import type { DocRef, LineState } from '../lib/lineCacheTypes'

export function useLogLines(doc: DocRef | null, lines: readonly number[]): (line: number) => LineState {
  const [owner] = useState(() => Symbol('log-lines'))

  useSyncExternalStore(logLineCache.subscribe, logLineCache.version)

  const pipelineId = doc?.pipelineId
  const runNumber = doc?.runNumber
  const nodeId = doc?.nodeId
  const revision = doc?.revision
  const lineCount = doc?.lineCount
  const previousRevision = doc?.previousRevision
  const stableLines = doc?.stableLines

  useLayoutEffect(() => {
    if (
      pipelineId === undefined ||
      runNumber === undefined ||
      nodeId === undefined ||
      revision === undefined ||
      lineCount === undefined ||
      previousRevision === undefined ||
      stableLines === undefined
    ) {
      logLineCache.release(owner)

      return
    }

    logLineCache.need(
      owner,
      { pipelineId, runNumber, nodeId, revision, lineCount, previousRevision, stableLines },
      lines
    )
  }, [owner, pipelineId, runNumber, nodeId, revision, lineCount, previousRevision, stableLines, lines])

  useEffect(() => () => logLineCache.release(owner), [owner])

  return (line) => (doc ? logLineCache.line(doc, line) : undefined)
}
