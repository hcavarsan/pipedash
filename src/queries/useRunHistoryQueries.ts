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
import { confirmBody } from '../components/common/ActionTargetRows'
import { cancelTargetRows } from '../features/runs/lib/cancelTargetRows'
import { invalidateRunHistory } from '../features/runs/lib/runHistoryCache'
import {
  FIRST_RUN_HISTORY_PAGE,
  nextRunHistoryPage,
  type RunHistoryPageParam,
} from '../features/runs/lib/runHistoryPaging'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { notifications } from '../lib/notifications'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import type { Pipeline, PipelineRun, ReviewDecision, TriggerResult } from '../types/pipelines'
import type { RunHistoryPage, RunHistoryQuery } from '../types/run_history'
import { openConfirm } from '../utils/confirm'
import { errorMessage } from '../utils/errorDisplay'

function notifyCancellationRequested(message: string) {
  notifications.show({ title: 'Cancellation requested', message, color: 'gray' })
}

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
    enabled: !!pipelineId,
  })
}

interface CancelRunVariables {
  pipelineId: string
  runNumber: number
  pipeline?: Pipeline | undefined
  run?: Pick<PipelineRun, 'branch'> | undefined
  forceConfirmation?: string | undefined
}

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

type UseCancelRunResult = DistributiveOmit<
  UseMutationResult<void, Error, { pipelineId: string; runNumber: number }, unknown>,
  'mutateAsync'
> & { mutateAsync: (variables: CancelRunVariables) => Promise<boolean> }

function useRunCancellation(force: boolean): UseCancelRunResult {
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      force ? service.forceCancelPipelineRun(pipelineId, runNumber) : service.cancelPipelineRun(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      invalidateRunHistory(queryClient, pipelineId, runNumber)

      if (force) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs.active(pipelineId) })
        void queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) })
      }

      notifyCancellationRequested(`Run #${runNumber} ${force ? 'force cancellation' : 'cancellation'} requested`)
    },

    onError: (error: Error) => {
      notifications.show({
        title: force ? "Couldn't force cancel run" : "Couldn't cancel run",
        message: errorMessage(error, 'Check the run in your provider before trying again.'),
        color: 'red',
      })
    },
  })

  const cancel = async (variables: CancelRunVariables) => {
    const { promise, resolve } = Promise.withResolvers<boolean>()
    const targets = variables.pipeline ? cancelTargetRows(variables.pipeline, variables.run ? [variables.run] : []) : []

    openConfirm({
      title: force ? 'Force cancel run' : 'Cancel run',
      children: confirmBody(
        force
          ? `Force cancel run #${variables.runNumber}. ${variables.forceConfirmation ?? ''}`.trim()
          : `This cancels run #${variables.runNumber}. You can't undo this.`,
        targets
      ),
      labels: { confirm: force ? 'Force cancel' : 'Cancel run', cancel: 'Keep running' },
      confirmProps: { color: 'red' },
      onConfirm: () => resolve(true),
      onCancel: () => resolve(false),
      onClose: () => resolve(false),
    })
    const confirmed = await promise

    if (!confirmed) {
      return false
    }

    await mutation.mutateAsync({ pipelineId: variables.pipelineId, runNumber: variables.runNumber })

    return true
  }

  return { ...mutation, mutateAsync: cancel }
}

export function useCancelRun(): UseCancelRunResult {
  return useRunCancellation(false)
}

export function useForceCancelRun(): UseCancelRunResult {
  return useRunCancellation(true)
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

    onSuccess: (_data, { pipelineId, runNumber, decision }) => {
      invalidateRunHistory(queryClient, pipelineId, runNumber)

      notifications.show({
        title: decision === 'approve' ? 'Run approved' : 'Run rejected',
        message: `Deployment review for run #${runNumber} sent`,
        color: decision === 'approve' ? 'green' : 'gray',
      })
    },

    onError: (error: Error, { decision }) => {
      notifications.show({
        title: decision === 'approve' ? "Couldn't approve run" : "Couldn't reject run",
        message: errorMessage(error, 'Check the deployment review in your provider and try again.'),
        color: 'red',
      })
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

    onSuccess: (_data, { pipelineId, runNumber }) => {
      invalidateRunHistory(queryClient, pipelineId, runNumber)
      void queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) })

      notifications.show({
        title: 'Re-running failed jobs',
        message: `Run #${runNumber} is re-running its failed jobs`,
        color: 'gray',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't re-run failed jobs",
        message: errorMessage(error, 'Check the run in your provider and try rerunning failed jobs again.'),
        color: 'red',
      })
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

    onSuccess: (_data, { pipelineId, runNumber }) => {
      invalidateRunHistory(queryClient, pipelineId, runNumber)
      void queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) })

      notifications.show({
        title: 'Rerun started',
        message: `Run #${runNumber} is running again`,
        color: 'gray',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't rerun workflow",
        message: errorMessage(error, 'Check the run in your provider and try rerunning it again.'),
        color: 'red',
      })
    },
  })
}
