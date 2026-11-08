import { logger } from '../../../lib/logger'
import { useCancelRun, useForceCancelRun, useRerunFailedJobs, useRerunRun } from '../../../queries/useRunHistoryQueries'
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
}

interface UseRunDetailActionsResult {
  rerun: (mode: RerunMode) => Promise<void>
  cancel: () => Promise<void>
  forceCancel: () => Promise<void>
  rerunPending: boolean
  cancelPending: boolean
  forceCancelPending: boolean
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
}: UseRunDetailActionsOptions): UseRunDetailActionsResult {
  const cancelRun = useCancelRun()
  const forceCancelRun = useForceCancelRun()
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
      if (await cancelRun.mutateAsync({ pipelineId, runNumber, pipeline, run, forceConfirmation })) {
        onCancelRequested()
        onCancelSuccess?.()
      }
    } catch (error: unknown) {
      logger.warn('WorkflowLogsModal', 'Cancellation request failed', error)
    }
  }

  const forceCancel = async (): Promise<void> => {
    if (!pipeline || !run?.actions.force_cancel.allowed) {
      return
    }

    const cancelled = await forceCancelRun
      .mutateAsync({ pipelineId, runNumber, pipeline, run, forceConfirmation })
      .catch(() => false)

    if (cancelled) {
      onCancelRequested()
      onCancelSuccess?.()
    }
  }

  return {
    rerun,
    cancel,
    forceCancel,
    rerunPending: rerunRunMutation.isPending || rerunFailedMutation.isPending,
    cancelPending: cancelRun.isPending,
    forceCancelPending: forceCancelRun.isPending,
  }
}
