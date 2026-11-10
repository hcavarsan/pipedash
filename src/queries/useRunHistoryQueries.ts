import {
  type InfiniteData,
  type UseInfiniteQueryResult,
  type UseMutationResult,
  type UseQueryResult,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { createElement } from 'react'
import { CancelRunConfirm } from '../features/runs/components/CancelRunConfirm'
import { cancelTargetRows } from '../features/runs/lib/cancelTargetRows'
import { invalidateRunHistory } from '../features/runs/lib/runHistoryCache'
import {
  FIRST_RUN_HISTORY_PAGE,
  nextRunHistoryPage,
  type RunHistoryPageParam,
} from '../features/runs/lib/runHistoryPaging'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import type { Pipeline, PipelineRun, ReviewDecision, TriggerResult } from '../types/pipelines'
import type { RunHistoryPage, RunHistoryQuery } from '../types/run_history'
import { openConfirm } from '../utils/confirm'
import { displayErrorNotification, displaySuccessNotification } from '../utils/errorDisplay'

export function useRunHistory(
  pipelineId: string,
  query: RunHistoryQuery,
  enabled = true
): UseQueryResult<RunHistoryPage, Error> {
  return useQuery<RunHistoryPage>({
    queryKey: queryKeys.runs.list(pipelineId, query),
    queryFn: () => service.fetchRunHistory(pipelineId, query),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[2] === pipelineId ? previous : undefined),
    enabled: enabled && !!pipelineId,
  })
}

export function useRunHistoryInfinite(
  pipelineId: string,
  query: Omit<RunHistoryQuery, 'page' | 'scan_depth'>
): UseInfiniteQueryResult<InfiniteData<RunHistoryPage, RunHistoryPageParam>, Error> {
  return useInfiniteQuery({
    queryKey: queryKeys.runs.infinite(pipelineId, query),
    queryFn: ({ pageParam }) =>
      service.fetchRunHistory(pipelineId, { ...query, page: pageParam.page, scan_depth: pageParam.scanDepth }),
    initialPageParam: FIRST_RUN_HISTORY_PAGE,
    getNextPageParam: (lastPage, _pages, lastPageParam) => nextRunHistoryPage(lastPage, lastPageParam),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[2] === pipelineId ? previous : undefined),
    enabled: !!pipelineId,
  })
}

interface CancelRunVariables {
  pipelineId: string
  runNumber: number
  pipeline?: Pipeline | undefined
  run?: Pick<PipelineRun, 'branch' | 'actions'> | undefined
  forceConfirmation?: string | undefined
  preferForce?: boolean | undefined
}

interface CancelRunRequest {
  pipelineId: string
  runNumber: number
  force: boolean
}

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

type UseCancelRunResult = DistributiveOmit<UseMutationResult<void, Error, CancelRunRequest, unknown>, 'mutateAsync'> & {
  mutateAsync: (variables: CancelRunVariables) => Promise<boolean>
}

export function useCancelRun(): UseCancelRunResult {
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: ({ pipelineId, runNumber, force }: CancelRunRequest) =>
      force ? service.forceCancelPipelineRun(pipelineId, runNumber) : service.cancelPipelineRun(pipelineId, runNumber),

    onSuccess: async (_data, { pipelineId, runNumber, force }) => {
      const invalidations = [invalidateRunHistory(queryClient, pipelineId, runNumber)]

      if (force) {
        invalidations.push(
          queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
          queryClient.invalidateQueries({ queryKey: queryKeys.runs.active(pipelineId) }),
          queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) })
        )
      }

      await Promise.all(invalidations)

      displaySuccessNotification(
        `Run #${runNumber} ${force ? 'force cancellation' : 'cancellation'} requested`,
        'Cancellation requested'
      )
    },

    onError: (error: Error, { force }) => {
      displayErrorNotification(error, force ? "Couldn't force cancel run" : "Couldn't cancel run")
    },
  })

  const cancel = async (variables: CancelRunVariables) => {
    const { promise, resolve } = Promise.withResolvers<boolean>()
    const targets = variables.pipeline ? cancelTargetRows(variables.pipeline, variables.run ? [variables.run] : []) : []
    const forceAllowed = variables.run?.actions.force_cancel.allowed === true
    const forceOnly = forceAllowed && variables.run?.actions.cancel.allowed === false
    let force = forceOnly || (forceAllowed && variables.preferForce === true)

    openConfirm({
      title: 'Cancel run',
      children: createElement(CancelRunConfirm, {
        message: `This cancels run #${variables.runNumber}. You can't undo this.`,
        targets,
        force: forceAllowed ? { initial: force, locked: forceOnly, help: variables.forceConfirmation } : null,
        onForceChange: (next) => {
          force = next
        },
      }),
      labels: { confirm: 'Cancel run', cancel: 'Keep running' },
      confirmProps: { color: 'red' },
      onConfirm: () => resolve(true),
      onCancel: () => resolve(false),
      onClose: () => resolve(false),
    })
    const confirmed = await promise

    if (!confirmed) {
      return false
    }

    await mutation.mutateAsync({ pipelineId: variables.pipelineId, runNumber: variables.runNumber, force })

    return true
  }

  return { ...mutation, mutateAsync: cancel }
}

export function useReviewRun(): UseMutationResult<
  void,
  Error,
  { pipelineId: string; runNumber: number; decision: ReviewDecision },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      pipelineId,
      runNumber,
      decision,
    }: {
      pipelineId: string
      runNumber: number
      decision: ReviewDecision
    }) => service.reviewPipelineRun(pipelineId, runNumber, decision),

    onSuccess: async (_data, { pipelineId, runNumber, decision }) => {
      await invalidateRunHistory(queryClient, pipelineId, runNumber)

      displaySuccessNotification(
        `Deployment review for run #${runNumber} sent`,
        decision === 'approve' ? 'Run approved' : 'Run rejected'
      )
    },

    onError: (error: Error, { decision }) => {
      displayErrorNotification(error, decision === 'approve' ? "Couldn't approve run" : "Couldn't reject run")
    },
  })
}

export function useRerunFailedJobs(): UseMutationResult<
  void,
  Error,
  { pipelineId: string; runNumber: number },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.rerunFailedJobs(pipelineId, runNumber),

    onSuccess: async (_data, { pipelineId, runNumber }) => {
      await Promise.all([
        invalidateRunHistory(queryClient, pipelineId, runNumber),
        queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) }),
      ])

      displaySuccessNotification(`Run #${runNumber} is re-running its failed jobs`, 'Re-running failed jobs')
    },

    onError: (error: Error) => {
      displayErrorNotification(error, "Couldn't re-run failed jobs")
    },
  })
}

export function useRerunRun(): UseMutationResult<
  TriggerResult,
  Error,
  { pipelineId: string; runNumber: number },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['runs', 'rerun'],
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.rerunRun(pipelineId, runNumber),

    onSuccess: async (result, { pipelineId, runNumber }) => {
      await Promise.all([
        invalidateRunHistory(queryClient, pipelineId, runNumber),
        queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) }),
      ])

      if (result.run_number === null) {
        displaySuccessNotification(`Rerun of run #${runNumber} requested`, 'Rerun requested')
      } else {
        displaySuccessNotification(`Run #${result.run_number} started from run #${runNumber}`, 'Run started')
      }
    },

    onError: (error: Error) => {
      displayErrorNotification(error, "Couldn't re-run workflow")
    },
  })
}
