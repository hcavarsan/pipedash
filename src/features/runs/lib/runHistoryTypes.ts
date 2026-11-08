import type { PipelineRun } from '../../../types/pipelines'

export interface RunPageSelection {
  selectedRuns: PipelineRun[]
  selectRuns: (runs: PipelineRun[]) => void
  isRunCancellable: (run: PipelineRun) => boolean
  cancelSelected: () => void
  isCancelling: boolean
}
