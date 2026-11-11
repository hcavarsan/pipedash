import type { ReactNode } from 'react'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import type { Pipeline } from '../../../types/pipelines'
import { TriggerWorkflowForm } from './TriggerWorkflowForm'

interface TriggerWorkflowModalProps {
  opened: boolean
  onClose: () => void
  pipeline: Pipeline
  onSuccess?: ((pipelineId: string, runNumber: number) => void) | undefined
  rerunOf?: number | undefined
}

export const TriggerWorkflowModal = ({
  opened,
  onClose,
  pipeline,
  onSuccess,
  rerunOf,
}: TriggerWorkflowModalProps): ReactNode => {
  if (!opened) {
    return null
  }

  return (
    <ErrorBoundary scope="feature" name="Trigger workflow">
      <TriggerWorkflowForm
        key={pipeline.id}
        onClose={onClose}
        pipeline={pipeline}
        onSuccess={onSuccess}
        rerunOf={rerunOf}
      />
    </ErrorBoundary>
  )
}
