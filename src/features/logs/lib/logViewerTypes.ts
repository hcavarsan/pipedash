import type { Ref } from 'react'
import type { RunLogNode } from '../../../types/logs'

export interface LogViewerHandle {
  focusSearch: () => void
  nextMatch: () => void
  prevMatch: () => void
  nextError: () => void
  prevError: () => void
  scrollToStart: () => void
  scrollToEnd: () => void
  focusBody: () => void
}

export interface LogViewerProps {
  pipelineId: string
  pipelineName: string
  runNumber: number
  node: RunLogNode
  steps: RunLogNode[]
  variant: 'desktop' | 'narrow' | 'phone'
  providerName: string
  ref?: Ref<LogViewerHandle> | undefined
}
