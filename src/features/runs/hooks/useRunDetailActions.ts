import { logger } from '../../../lib/logger'
import { useCancelRun, useRerunFailedJobs, useRerunRun } from '../../../queries/useRunHistoryQueries'
import { useModalStore } from '../../../stores/modalStore'
import type { Pipeline, PipelineRun } from '../../../types/pipelines'
import type { RerunMode } from '../lib/runActions'

interface UseRunDetailActionsOptions {
  pipelineId: string
  runNumber: number
  pipeline: Pipeline | undefined
  run: PipelineRun | undefined
  onClose: () => void
  onRerunSuccess?: ((pipelineId: string, newRunNumber: number) => void) | undefined
  onCancelSuccess?: (() => void) | undefined
  onCancelRequested: () => void
  forceConfirmation?: string | undefined
  cancellationRequested?: boolean | undefined
}

export interface UseRunDetailActionsResult {
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
  onClose,
  onRerunSuccess,
  onCancelSuccess,
  onCancelRequested,
  forceConfirmation,
  cancellationRequested = false,
}: UseRunDetailActionsOptions): UseRunDetailActionsResult {
  const cancelRun = useCancelRun()
  const openTriggerModal = useModalStore((state) => state.openTriggerModal)
  const rerunRunMutation = useRerunRun()
  const rerunFailedMutation = useRerunFailedJobs()

  const rerun = async (mode: RerunMode): Promise<void> => {
    if (!run) {
      return
    }

    if (mode !== 'failed' && pipeline?.actions.rerun_strategy !== 'in_place') {
      onClose()
      openTriggerModal(pipelineId, runNumber)

      return
    }

    try {
      await (mode === 'failed' ? rerunFailedMutation : rerunRunMutation).mutateAsync({ pipelineId, runNumber })
      onClose()
      onRerunSuccess?.(pipelineId, runNumber)
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
      const requested = await cancelRun.mutateAsync({
        pipelineId,
        runNumber,
        pipeline,
        run,
        forceConfirmation,
        preferForce: cancellationRequested,
      })

      if (requested) {
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
    rerunPending: rerunRunMutation.isPending || rerunFailedMutation.isPending,
    cancelPending: cancelRun.isPending,
  }
}
