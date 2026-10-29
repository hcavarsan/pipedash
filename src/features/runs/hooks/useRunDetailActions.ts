import { logger } from '../../../lib/logger'
import { useCancelRun, useRerunFailedJobs, useRerunRun } from '../../../queries/useRunHistoryQueries'
import type { Pipeline, PipelineRun } from '../../../types/domain'
import type { RerunMode } from '../../../utils/rerunSummary'
import { useRerunWorkflow } from '../queries/useRunDetailsQuery'

interface UseRunDetailActionsOptions {
  pipelineId: string
  runNumber: number
  pipeline: Pipeline | undefined
  run: PipelineRun | undefined
  rerunInPlace: boolean
  onClose: () => void
  onRerunSuccess?: ((pipelineId: string, newRunNumber: number) => void) | undefined
  onCancelSuccess?: (() => void) | undefined
  onCancelRequested: () => void
}

interface UseRunDetailActionsResult {
  rerun: (mode: RerunMode) => Promise<void>
  cancel: () => Promise<void>
  rerunPending: boolean
  cancelPending: boolean
}

export function useRunDetailActions({
  pipelineId,
  runNumber,
  pipeline,
  run,
  rerunInPlace,
  onClose,
  onRerunSuccess,
  onCancelSuccess,
  onCancelRequested,
}: UseRunDetailActionsOptions): UseRunDetailActionsResult {
  const cancelRun = useCancelRun()
  const rerunMutation = useRerunWorkflow()
  const rerunRunMutation = useRerunRun()
  const rerunFailedMutation = useRerunFailedJobs()

  const rerun = async (mode: RerunMode): Promise<void> => {
    if (!run) {
      return
    }

    try {
      if (mode === 'failed' || rerunInPlace) {
        await (mode === 'failed' ? rerunFailedMutation : rerunRunMutation).mutateAsync({ pipelineId, runNumber })
        onClose()
        onRerunSuccess?.(pipelineId, runNumber)

        return
      }

      const result = await rerunMutation.mutateAsync({ pipelineId, inputs: run.inputs })

      onClose()

      if (onRerunSuccess && result.newRunNumber > 0) {
        onRerunSuccess(pipelineId, result.newRunNumber)
      }
    } catch (error: unknown) {
      logger.error('WorkflowLogsModal', 'Failed to re-run workflow', error)
      throw error
    }
  }

  const cancel = async (): Promise<void> => {
    if (!run) {
      return
    }

    try {
      if (await cancelRun.mutateAsync({ pipelineId, runNumber, pipeline, run })) {
        onCancelRequested()
        onCancelSuccess?.()
      }
    } catch (error: unknown) {
      logger.warn('WorkflowLogsModal', 'Cancellation request failed', error)
    }
  }

  return {
    rerun,
    cancel,
    rerunPending: rerunMutation.isPending || rerunRunMutation.isPending || rerunFailedMutation.isPending,
    cancelPending: cancelRun.isPending,
  }
}
