import { type UseQueryResult, useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { platform } from '@tauri-apps/plugin-os'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { isTauri } from '../services'
import { mobileService } from '../services/mobile'
import type { BuildWatchStatus } from '../types'
import { displayErrorNotification } from '../utils/errorDisplay'

const mutationKey = [...queryKeys.buildWatch.all, 'change']

function isBuildWatchPlatform() {
  return isTauri() && platform() === 'ios'
}

type UseBuildWatchResult = UseQueryResult<BuildWatchStatus, Error> & {
  enabled: boolean
  busy: boolean
  start: (pipelineId: string, runNumber: number) => void
  stop: () => void
}

export function useBuildWatch(): UseBuildWatchResult {
  const queryClient = useQueryClient()
  const enabled = isBuildWatchPlatform()
  const query = useQuery({
    queryKey: queryKeys.buildWatch.status(),
    queryFn: () => mobileService.getBuildWatch(),
    enabled,
    staleTime: 0,
    retry: false,
    refetchInterval: (current) => (current.state.data?.watch?.monitoring === 'watching' ? 5000 : false),
    refetchIntervalInBackground: false,
  })
  const busy = useIsMutating({ mutationKey }) > 0
  const mutation = useMutation({
    mutationKey,
    mutationFn: (run: { pipelineId: string; runNumber: number } | null) =>
      run ? mobileService.startBuildWatch(run.pipelineId, run.runNumber) : mobileService.stopBuildWatch(),
    onMutate: () => queryClient.cancelQueries({ queryKey: queryKeys.buildWatch.status() }),
    onSuccess: (status) => queryClient.setQueryData(queryKeys.buildWatch.status(), status),
    onError: (error, run) => displayErrorNotification(error, run ? "Couldn't watch run" : "Couldn't stop watching"),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.buildWatch.status() }),
  })

  const start = (pipelineId: string, runNumber: number) => {
    const status = queryClient.getQueryData<BuildWatchStatus>(queryKeys.buildWatch.status())
    if (
      !enabled ||
      !status?.supported ||
      !status.available ||
      status.watch?.monitoring === 'watching' ||
      queryClient.isMutating({ mutationKey }) > 0 ||
      !pipelineId ||
      !Number.isSafeInteger(runNumber) ||
      runNumber <= 0
    ) {
      return
    }
    mutation.mutate({ pipelineId, runNumber })
  }
  const stop = () => {
    if (!enabled || queryClient.isMutating({ mutationKey }) > 0) {
      return
    }
    mutation.mutate(null)
  }

  return { ...query, enabled, busy, start, stop }
}

export function useBuildWatchLifecycle(): void {
  const queryClient = useQueryClient()

  useEffect(() => {
    if (!isBuildWatchPlatform()) {
      return
    }
    const refresh = () => {
      if (document.visibilityState === 'hidden') {
        return
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.buildWatch.status() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.all })
    }
    let active = true
    let remove: (() => void) | undefined
    listen('build-watch-changed', refresh)
      .then((unlisten) => {
        if (active) {
          remove = unlisten
        } else {
          unlisten()
        }
      })
      .catch((error: unknown) => logger.warn('BuildWatch', "Couldn't listen for native watch changes", error))
    document.addEventListener('visibilitychange', refresh)
    window.addEventListener('focus', refresh)

    return () => {
      active = false
      remove?.()
      document.removeEventListener('visibilitychange', refresh)
      window.removeEventListener('focus', refresh)
    }
  }, [queryClient])
}
