import type { ReactNode } from 'react'

import type { Pipeline } from '../../types'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import { TriggerWorkflowForm } from './TriggerWorkflowForm'

interface TriggerWorkflowModalProps {
  opened: boolean
  onClose: () => void
  pipeline: Pipeline
  onSuccess?: ((pipelineId: string, runNumber: number) => void) | undefined
  initialInputs?: Record<string, unknown> | undefined
}

export const TriggerWorkflowModal = ({
  opened,
  onClose,
  pipeline,
  onSuccess,
  initialInputs,
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
        initialInputs={initialInputs}
      />
    </ErrorBoundary>
  )
}
