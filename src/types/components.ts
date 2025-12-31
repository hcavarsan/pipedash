
import type { Pipeline, PipelineRun } from '.'

export interface ModalBaseProps {
  opened: boolean
  onClose: () => void
}

export interface PipelineComponentProps {
  pipeline: Pipeline | null
  loading?: boolean
  onBack?: () => void
  onViewRun?: (pipelineId: string, runNumber: number) => void
  onRerun?: (pipeline: Pipeline, run: PipelineRun) => void
  onCancel?: (pipeline: Pipeline, run: PipelineRun) => void
}

