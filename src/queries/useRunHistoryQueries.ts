import { notifications } from '@mantine/notifications'
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
import { useEffect, useState } from 'react'

import { confirmBody } from '../components/common/ActionTargetRows'
import { PAGE_SIZES } from '../constants/pagination'
import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { PaginatedRunHistory, Pipeline, PipelineRun, ReviewDecision } from '../types'
import { actionTarget, type TargetPipeline } from '../utils/actionTarget'
import { findUnfinishedRuns, type UnfinishedRuns } from '../utils/cancelActiveRuns'
import { openConfirm } from '../utils/confirm'
import { errorMessage } from '../utils/errorDisplay'
import { formatCount } from '../utils/formatCount'

function notifyCancellationRequested(message: string) {
  notifications.show({ title: 'Cancellation requested', message, color: 'gray' })
}

export function useRunHistory(
  pipelineId: string,
  page = 1,
  pageSize = 20,
  enabled = true
): UseQueryResult<PaginatedRunHistory, Error> {
  return useQuery<PaginatedRunHistory>({
    queryKey: queryKeys.runs.list(pipelineId, page, pageSize),
    queryFn: () => service.fetchRunHistory(pipelineId, page, pageSize),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[2] === pipelineId ? previous : undefined),
    enabled: enabled && !!pipelineId,
  })
}

export function useRunHistoryInfinite(
  pipelineId: string,
  pageSize: number = PAGE_SIZES.RUN_HISTORY
): UseInfiniteQueryResult<InfiniteData<PaginatedRunHistory, number>, Error> {
  return useInfiniteQuery({
    queryKey: queryKeys.runs.infinite(pipelineId, pageSize),
    queryFn: ({ pageParam }) => service.fetchRunHistory(pipelineId, pageParam, pageSize),
    initialPageParam: 1,
    getNextPageParam: (lastPage, _pages, lastPageParam) => (lastPage.has_more ? lastPageParam + 1 : undefined),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    enabled: !!pipelineId,
  })
}

type UseUnfinishedRunsResult = UseQueryResult<UnfinishedRuns, Error> & { progress: number }

export function useUnfinishedRuns(pipelines: Pipeline[], enabled: boolean): UseUnfinishedRunsResult {
  const queryClient = useQueryClient()
  const [progress, setProgress] = useState(0)
  const queryKey = queryKeys.runs.unfinished(pipelines.map((pipeline) => pipeline.id))
  const active = enabled && pipelines.length > 0
  const query = useQuery({
    queryKey,
    queryFn: ({ signal }) => {
      setProgress(0)

      return findUnfinishedRuns(
        pipelines,
        (pipelineId, page) =>
          queryClient.fetchQuery({
            queryKey: queryKeys.runs.list(pipelineId, page, PAGE_SIZES.RUN_HISTORY),
            queryFn: () => service.fetchRunHistory(pipelineId, page, PAGE_SIZES.RUN_HISTORY),
            staleTime: 0,
          }),
        undefined,
        setProgress,
        signal
      )
    },
    staleTime: 0,
    gcTime: 0,
    enabled: active,
  })
  const key = JSON.stringify(queryKey)

  useEffect(() => {
    if (!active) {
      return
    }

    return () => {
      void queryClient.cancelQueries({ queryKey: JSON.parse(key), exact: true })
    }
  }, [active, key, queryClient])

  return { ...query, progress }
}

export interface TablePreferences {
  columnOrder?: string[] | undefined
  columnVisibility?: Record<string, boolean> | undefined
}

export function useTablePreferences(
  providerId: number,
  tableId: string
): UseQueryResult<TablePreferences | null, Error> {
  return useQuery({
    queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
    queryFn: async () => {
      const prefsJson = await service.getTablePreferences(providerId, tableId)

      if (prefsJson) {
        try {
          return JSON.parse(prefsJson)
        } catch {
          return null
        }
      }

      const defaultPrefsJson = await service.getDefaultTablePreferences(providerId, tableId)

      if (defaultPrefsJson) {
        try {
          return JSON.parse(defaultPrefsJson)
        } catch {
          return null
        }
      }

      return null
    },
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.LONG,
    enabled: !!providerId && !!tableId,
  })
}

export function useSaveTablePreferences(): UseMutationResult<
  void,
  Error,
  { providerId: number; tableId: string; preferences: unknown },
  { previousPreferences: unknown; providerId: number; tableId: string }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ providerId, tableId, preferences }: { providerId: number; tableId: string; preferences: unknown }) =>
      service.saveTablePreferences(providerId, tableId, JSON.stringify(preferences)),

    onMutate: async ({ providerId, tableId, preferences }) => {
      await queryClient.cancelQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      const previousPreferences = queryClient.getQueryData(queryKeys.tablePreferences.detail(providerId, tableId))

      queryClient.setQueryData(queryKeys.tablePreferences.detail(providerId, tableId), preferences)

      return { previousPreferences, providerId, tableId }
    },

    onError: (error: Error, _variables, context) => {
      if (context?.previousPreferences !== undefined) {
        queryClient.setQueryData(
          queryKeys.tablePreferences.detail(context.providerId, context.tableId),
          context.previousPreferences
        )
      } else if (context) {
        queryClient.removeQueries({
          queryKey: queryKeys.tablePreferences.detail(context.providerId, context.tableId),
          exact: true,
        })
      }

      notifications.show({
        title: "Couldn't save preferences",
        message: errorMessage(error, 'Try saving the table preferences again.'),
        color: 'red',
      })
    },

    onSuccess: (_data, { providerId, tableId }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.tablePreferences.detail(providerId, tableId),
      })

      notifications.show({
        title: 'Preferences saved',
        message: 'Table preferences updated successfully',
        color: 'green',
        autoClose: 2000,
      })
    },
  })
}

interface CancelRunVariables {
  pipelineId: string
  runNumber: number
  pipeline?: TargetPipeline | undefined
  run?: Pick<PipelineRun, 'branch' | 'inputs'> | undefined
}

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

type UseCancelRunResult = DistributiveOmit<
  UseMutationResult<void, Error, { pipelineId: string; runNumber: number }, unknown>,
  'mutateAsync'
> & { mutateAsync: (variables: CancelRunVariables) => Promise<boolean> }

export function useCancelRun(): UseCancelRunResult {
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.cancelPipelineRun(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      notifyCancellationRequested(`Run #${runNumber} cancellation requested`)
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't cancel run",
        message: errorMessage(error, 'Check the run in your provider before trying again.'),
        color: 'red',
      })
    },
  })

  const cancel = async (variables: CancelRunVariables) => {
    const { promise, resolve } = Promise.withResolvers<boolean>()
    openConfirm({
      title: 'Cancel run',
      children: confirmBody(
        `This cancels run #${variables.runNumber}. You can't undo this.`,
        variables.pipeline ? actionTarget(variables.pipeline, variables.run ? [variables.run] : []) : []
      ),
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

    await mutation.mutateAsync({ pipelineId: variables.pipelineId, runNumber: variables.runNumber })

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

    onSuccess: (_data, { pipelineId, runNumber, decision }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

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
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runLogs.tree(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

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

export function useRerunRun(): UseMutationResult<void, Error, { pipelineId: string; runNumber: number }, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['runs', 'rerun'],
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.rerunRun(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runLogs.tree(pipelineId, runNumber),
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

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

export function useCancelRuns(): UseMutationResult<
  { cancelled: number[]; failed: number[] },
  Error,
  { pipelineId: string; runNumbers: number[] },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ pipelineId, runNumbers }: { pipelineId: string; runNumbers: number[] }) => {
      const results = await Promise.allSettled(
        runNumbers.map((runNumber) => service.cancelPipelineRun(pipelineId, runNumber))
      )

      return {
        cancelled: runNumbers.filter((_, index) => results[index]?.status === 'fulfilled'),
        failed: runNumbers.filter((_, index) => results[index]?.status === 'rejected'),
      }
    },

    onSuccess: ({ cancelled, failed }, { pipelineId, runNumbers }) => {
      cancelled.forEach((runNumber) => {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.runs.detail(pipelineId, runNumber),
        })
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      if (failed.length === 0) {
        notifyCancellationRequested(`Cancellation requested for ${formatCount(cancelled.length, 'run')}`)

        return
      }

      notifications.show({
        title: "Couldn't request cancellation for every run",
        message: `Cancellation requested for ${cancelled.length} of ${formatCount(runNumbers.length, 'run')}. Failed: ${failed.map((runNumber) => `#${runNumber}`).join(', ')}`,
        color: 'red',
      })
    },
  })
}
