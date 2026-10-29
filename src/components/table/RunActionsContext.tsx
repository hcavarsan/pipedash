import { useMutationState, useQueryClient } from '@tanstack/react-query'
import { createContext, type ReactNode, use, useState } from 'react'

import { queryKeys } from '../../lib/queryKeys'
import { useBuildWatch } from '../../queries/useBuildWatchQueries'
import { usePlugins } from '../../queries/usePluginsQueries'
import { useRefreshProvider } from '../../queries/useRefreshQueries'
import { useCancelRun, useRerunFailedJobs, useRerunRun, useReviewRun } from '../../queries/useRunHistoryQueries'
import { service } from '../../services'
import { useModalStore } from '../../stores/modalStore'
import type { Pipeline, PipelineRun, ReviewDecision } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { isRecord } from '../../utils/isRecord'
import {
  canRerunInPlace,
  getPipelineLabels,
  getReconcileBlockReason,
  getTriggerBlockReason,
} from '../../utils/pipelinePresentation'
import { getPluginByType } from '../../utils/pluginHelpers'
import type { RerunMode } from '../../utils/rerunSummary'

import type { RunActionsValue } from './runActions'

const RunActionsContext = createContext<RunActionsValue | null>(null)

interface RunActionsProviderProps {
  pipeline: Pipeline | null
  children: ReactNode
}

interface PendingRun {
  isPending: boolean
  variables: { pipelineId: string; runNumber: number } | undefined
}

export function RunActionsProvider({ pipeline, children }: RunActionsProviderProps): ReactNode {
  const queryClient = useQueryClient()
  const openLogsModal = useModalStore((state) => state.openLogsModal)
  const openTriggerModal = useModalStore((state) => state.openTriggerModal)
  const { data: plugins } = usePlugins()
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
    pipeline && mutation.isPending && mutation.variables?.pipelineId === pipeline.id
      ? mutation.variables.runNumber
      : null

  const rerunningRuns = new Set(
    rerunVariables.flatMap((variables) =>
      isRecord(variables) && variables.pipelineId === pipeline?.id && typeof variables.runNumber === 'number'
        ? [variables.runNumber]
        : []
    )
  )

  const rerun = async (run: PipelineRun): Promise<void> => {
    if (!pipeline) {
      return
    }

    if (pipeline.provider_type === 'fluxcd') {
      openTriggerModal(pipeline.id)

      return
    }

    try {
      if (canRerunInPlace(pipeline)) {
        await rerunRun
          .mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number })
          .then(() => refreshProvider(pipeline.provider_id))
          .catch(() => undefined)

        return
      }

      const runDetails = await queryClient.fetchQuery({
        queryKey: queryKeys.runs.detail(run.pipeline_id, run.run_number),
        queryFn: () => service.getWorkflowRunDetails(run.pipeline_id, run.run_number),
        staleTime: 30 * 1000,
      })
      const existingInputs = runDetails.inputs ?? {}
      const inputs =
        run.branch && !existingInputs.branch && !existingInputs.ref
          ? { ...existingInputs, ref: run.branch }
          : existingInputs

      openTriggerModal(pipeline.id, inputs)
    } catch (error) {
      displayErrorNotification(error, "Couldn't rerun")
    }
  }

  const rerunFailed = async (run: PipelineRun): Promise<void> => {
    if (!pipeline) {
      return
    }

    await rerunFailedJobs
      .mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number })
      .then(() => refreshProvider(pipeline.provider_id))
      .catch(() => undefined)
  }

  const cancel = async (run: PipelineRun): Promise<void> => {
    if (!pipeline) {
      return
    }

    const cancelled = await cancelRun
      .mutateAsync({ pipelineId: run.pipeline_id, runNumber: run.run_number, pipeline, run })
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

  const watching = buildWatch.data?.watch?.monitoring === 'watching'
  const value: RunActionsValue = {
    pipeline,
    labels: getPipelineLabels(pipeline?.provider_type),
    isFlux: pipeline?.provider_type === 'fluxcd',
    canRerun: !!pipeline && !getReconcileBlockReason(pipeline),
    canCancel: !!pipeline && !!getPluginByType(plugins, pipeline.provider_type)?.capabilities.cancel,
    cancellingRun: pendingNumber(cancelRun),
    reviewingRun: pendingNumber(reviewRun),
    rerunningFailedRun: pendingNumber(rerunFailedJobs),
    rerunningRuns,
    watch: {
      supported: !!buildWatch.data?.supported,
      available: !!buildWatch.data?.available,
      reason: buildWatch.data?.reason ?? undefined,
      busy: buildWatch.busy,
      disabled:
        buildWatch.busy || buildWatch.isFetching || buildWatch.isError || !buildWatch.data?.available || watching,
    },
    startWatch: (runNumber) => {
      if (pipeline) {
        buildWatch.start(pipeline.id, runNumber)
      }
    },
    openRun: (run) => {
      if (pipeline) {
        openLogsModal(pipeline.id, run.run_number)
      }
    },
    openTrigger: () => {
      if (pipeline && !getTriggerBlockReason(pipeline)) {
        openTriggerModal(pipeline.id)
      }
    },
    cancel,
    review,
    requestRerun: (run) => {
      if (pipeline && canRerunInPlace(pipeline)) {
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
    triggerBlockedReason: pipeline ? getTriggerBlockReason(pipeline) : undefined,
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
