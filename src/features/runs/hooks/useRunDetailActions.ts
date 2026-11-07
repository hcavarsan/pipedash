import { logger } from '../../../lib/logger'
import { useCancelRun, useForceCancelRun, useRerunFailedJobs, useRerunRun } from '../../../queries/useRunHistoryQueries'
import type { Pipeline, PipelineRun } from '../../../types/domain'
import type { RerunMode } from '../../../utils/rerunSummary'
import { useRerunWorkflow } from '../queries/useRunDetailsQuery'

const branchInputs: Partial<Record<string, string>> = {
  gitlab: 'ref',
  bitbucket: 'ref',
  gitea: 'ref',
  buildkite: 'branch',
  'azure-devops': 'source-branch',
  woodpecker: 'branch',
}

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
  rerunInPlace,
  onClose,
  onRerunSuccess,
  onCancelSuccess,
  onCancelRequested,
}: UseRunDetailActionsOptions): UseRunDetailActionsResult {
  const cancelRun = useCancelRun()
  const forceCancelRun = useForceCancelRun()
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

      const existingInputs = run.inputs ?? {}
      const branchInput = branchInputs[pipeline?.provider_type ?? '']
      const inputs =
        run.branch && branchInput && !existingInputs[branchInput] && !existingInputs.branch && !existingInputs.ref
          ? { ...existingInputs, [branchInput]: run.branch }
          : existingInputs
      const result = await rerunMutation.mutateAsync({ pipelineId, inputs })

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

  const forceCancel = async (): Promise<void> => {
    if (!pipeline || !run || run.status !== 'running') {
      return
    }

    const cancelled = await forceCancelRun.mutateAsync({ pipelineId, runNumber, pipeline, run }).catch(() => false)

    if (cancelled) {
      onCancelRequested()
      onCancelSuccess?.()
    }
  }

  return {
    rerun,
    cancel,
    forceCancel,
    rerunPending: rerunMutation.isPending || rerunRunMutation.isPending || rerunFailedMutation.isPending,
    cancelPending: cancelRun.isPending,
    forceCancelPending: forceCancelRun.isPending,
  }
}
