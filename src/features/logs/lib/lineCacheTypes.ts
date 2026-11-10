import type { LogLine } from '../../../types/logs'

export interface DocRef {
  pipelineId: string
  runNumber: number
  nodeId: string
  revision: number
  lineCount: number
  previousRevision: number | null
  stableLines: number
}

export type LineState = LogLine | 'loading' | 'error' | undefined

export interface LineRequest {
  doc: DocRef
  start: number
  count: number
  retried?: boolean
}
