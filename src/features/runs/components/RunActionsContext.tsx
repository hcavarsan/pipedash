import { useMutationState } from '@tanstack/react-query'
import { createContext, type ReactNode, use, useState } from 'react'

import { LoadingState } from '../../../components/common/LoadingState'
import { useBuildWatch } from '../../../queries/useBuildWatchQueries'
import { usePluginPresentation } from '../../../queries/usePluginsQueries'
import { useRefreshProvider } from '../../../queries/useRefreshQueries'
import { useCancelRun, useRerunFailedJobs, useRerunRun, useReviewRun } from '../../../queries/useRunHistoryQueries'
import { useModalStore } from '../../../stores/modalStore'
import type { Pipeline, PipelineRun, ReviewDecision } from '../../../types/pipelines'
import { isRecord } from '../../../utils/isRecord'
import { type RerunMode, type RunActionsValue, watchStateOf } from '../lib/runActions'

const RunActionsContext = createContext<RunActionsValue | null>(null)

interface RunActionsProviderProps {
  pipeline: Pipeline
  children: ReactNode
}

interface PendingRun {
  isPending: boolean
  variables: { pipelineId: string; runNumber: number } | undefined
}

export function RunActionsProvider({ pipeline, children }: RunActionsProviderProps): ReactNode {
  const openLogsModal = useModalStore((state) => state.openLogsModal)
  const openTriggerModal = useModalStore((state) => state.openTriggerModal)
  const presentation = usePluginPresentation(pipeline.provider_type)
  const buildWatch = useBuildWatch()
  const cancelRun = useCancelRun()
  const reviewRun = useReviewRun()
  const rerunFailedJobs = useRerunFailedJobs()
  const rerunRun = useRerunRun()
  const { mutate: refreshProvider } = useRefreshProvider()
  const [rerunTarget, setRerunTarget] = useState<PipelineRun | null>(null)
  const rerunVariables = useMutationState({
    filters: { mutationKey: ['runs', 'rerun'], status: 'pending' },
    select: (mutation) => mutation.state.variables,
  })

  const pendingNumber = (mutation: PendingRun): number | null =>
    mutation.isPending && mutation.variables?.pipelineId === pipeline.id ? mutation.variables.runNumber : null

  const rerunningRuns = new Set(
    rerunVariables.flatMap((variables) =>
      isRecord(variables) && variables.pipelineId === pipeline.id && typeof variables.runNumber === 'number'
        ? [variables.runNumber]
        : []
    )
  )

  const rerun = async (run: PipelineRun): Promise<void> => {
    if (pipeline.actions.rerun_strategy !== 'in_place') {
      openTriggerModal(pipeline.id, run.run_number)

      return
    }

    await rerunRun.mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number })
    refreshProvider(pipeline.provider_id)
  }

  const rerunFailed = async (run: PipelineRun): Promise<void> => {
    await rerunFailedJobs.mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number })
    refreshProvider(pipeline.provider_id)
  }

  const cancel = async (run: PipelineRun): Promise<void> => {
    const cancelled = await cancelRun
      .mutateAsync({
        pipelineId: run.pipeline_id,
        runNumber: run.run_number,
        pipeline,
        run,
        forceConfirmation: presentation?.force_cancel_confirmation,
      })
      .catch(() => false)

    if (cancelled) {
      refreshProvider(pipeline.provider_id)
    }
  }

  const review = async (run: PipelineRun, decision: ReviewDecision): Promise<void> => {
    await reviewRun
      .mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number, decision })
      .catch(() => undefined)
  }

  if (!presentation) {
    return <LoadingState message="Loading run actions…" />
  }

  const value: RunActionsValue = {
    pipeline,
    presentation,
    labels: presentation.terminology,
    canCancel: pipeline.actions.cancel.allowed,
    cancellingRun: pendingNumber(cancelRun),
    reviewingRun: pendingNumber(reviewRun),
    rerunningFailedRun: pendingNumber(rerunFailedJobs),
    rerunningRuns,
    watch: watchStateOf(buildWatch),
    startWatch: (runNumber) => buildWatch.start(pipeline.id, runNumber),
    openRun: (run) => openLogsModal(pipeline.id, run.run_number),
    openTrigger: () => {
      if (pipeline.actions.trigger.allowed) {
        openTriggerModal(pipeline.id)
      }
    },
    cancel,
    review,
    requestRerun: (run) => {
      if (pipeline.actions.rerun_strategy === 'in_place') {
        setRerunTarget(run)
      } else {
        void rerun(run)
      }
    },
    rerunTarget,
    closeRerun: () => setRerunTarget(null),
    confirmRerun: async (mode: RerunMode) => {
      if (rerunTarget) {
        await (mode === 'failed' ? rerunFailed(rerunTarget) : rerun(rerunTarget))
      }
    },
    triggerBlockedReason: pipeline.actions.trigger.reason ?? undefined,
  }

  return <RunActionsContext value={value}>{children}</RunActionsContext>
}

export function useRunActions(): RunActionsValue {
  const value = use(RunActionsContext)

  if (!value) {
    throw new Error('useRunActions must be used inside RunActionsProvider')
  }

  return value
}
