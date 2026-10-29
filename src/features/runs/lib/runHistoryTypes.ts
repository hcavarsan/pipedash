import type { PipelineRun } from '../../../types/domain'

export interface RunPageSelection {
  selectedRuns: PipelineRun[]
  selectRuns: (runs: PipelineRun[]) => void
  isRunCancellable: (run: PipelineRun) => boolean
  cancelSelected: () => void
  isCancelling: boolean
}
