import { notifications } from '@mantine/notifications'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { ReviewDecision } from '../types'
import { openConfirm } from '../utils/confirm'
import { errorMessage } from '../utils/errorDisplay'
import { formatCount } from '../utils/formatCount'

function notifyCancellationRequested(message: string) {
  notifications.show({ title: 'Cancellation requested', message, color: 'gray' })
}

export function useRunHistory(pipelineId: string, page = 1, pageSize = 20) {
  return useQuery({
    queryKey: queryKeys.runs.list(pipelineId, page),
    queryFn: () => service.fetchRunHistory(pipelineId, page, pageSize),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
    placeholderData: keepPreviousData,
    enabled: !!pipelineId,
  })
}

export function useTablePreferences(providerId: number, tableId: string) {
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

export function useSaveTablePreferences() {
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
      }

      notifications.show({
        title: "Couldn't save preferences",
        message: errorMessage(error, 'Try saving the table preferences again.'),
        color: 'red',
      })
    },

    onSuccess: (_data, { providerId, tableId }) => {
      queryClient.invalidateQueries({
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

export function useClearRunHistoryCache() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipelineId: string) => service.clearRunHistoryCache(pipelineId),

    onSuccess: (_data, pipelineId) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.list(pipelineId),
      })

      notifications.show({
        title: 'Cache cleared',
        message: 'Run history cache cleared successfully',
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't clear cache",
        message: errorMessage(error, 'Try clearing the run history cache again.'),
        color: 'red',
      })
    },
  })
}

export function useCancelRun() {
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.cancelPipelineRun(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      queryClient.invalidateQueries({
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

  const cancel = async (variables: { pipelineId: string; runNumber: number }) => {
    const { promise, resolve } = Promise.withResolvers<boolean>()
    openConfirm({
      title: 'Cancel run',
      children: `This cancels run #${variables.runNumber}. You can't undo this.`,
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

    await mutation.mutateAsync(variables)

    return true
  }

  return { ...mutation, mutateAsync: cancel }
}

export function useReviewRun() {
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
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      queryClient.invalidateQueries({
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

export function useRerunFailedJobs() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ pipelineId, runNumber }: { pipelineId: string; runNumber: number }) =>
      service.rerunFailedJobs(pipelineId, runNumber),

    onSuccess: (_data, { pipelineId, runNumber }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.runs.detail(pipelineId, runNumber),
      })
      queryClient.invalidateQueries({
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

export function useCancelRuns() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ pipelineId, runNumbers }: { pipelineId: string; runNumbers: number[] }) => {
      const results = await Promise.allSettled(
        runNumbers.map((runNumber) => service.cancelPipelineRun(pipelineId, runNumber))
      )

      return {
        cancelled: runNumbers.filter((_, index) => results[index].status === 'fulfilled'),
        failed: runNumbers.filter((_, index) => results[index].status === 'rejected'),
      }
    },

    onSuccess: ({ cancelled, failed }, { pipelineId, runNumbers }) => {
      cancelled.forEach((runNumber) => {
        queryClient.invalidateQueries({
          queryKey: queryKeys.runs.detail(pipelineId, runNumber),
        })
      })
      queryClient.invalidateQueries({
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
