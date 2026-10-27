import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useMemo } from 'react'

import { useDebounce } from '../hooks/useDebounce'
import { GC_TIMES } from '../lib/cacheConfig'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { events, service } from '../services'
import type { EventPayloadMap } from '../types/events'
import type { LogSearchQuery, RunLogDocument } from '../types/logs'

const TREE_REFETCH_MS = 10_000
const TREE_COMPLETE_STALE_MS = 60_000
const DOC_LOADING_REFETCH_MS = 3000
const DOC_LIVE_REFETCH_MS = 10_000
const SEARCH_DEBOUNCE_MS = 250

type RunLogEvent = 'run-log-progress' | 'run-log-ready' | 'run-log-failed'

export interface RunLogTarget {
  pipelineId: string
  runNumber: number
  nodeId: string
}

export function useRunLogTree(pipelineId: string, runNumber: number, enabled: boolean = true) {
  return useQuery({
    queryKey: queryKeys.runLogs.tree(pipelineId, runNumber),
    queryFn: () => service.getRunLogTree(pipelineId, runNumber),
    enabled: enabled && !!pipelineId && runNumber > 0,
    staleTime: (query) => (query.state.data?.complete ? TREE_COMPLETE_STALE_MS : 0),
    gcTime: GC_TIMES.SHORT,
    refetchInterval: (query) => (query.state.data && !query.state.data.complete ? TREE_REFETCH_MS : false),
    refetchIntervalInBackground: false,
  })
}

export function useRunLogDocument(pipelineId: string, runNumber: number, nodeId: string, enabled: boolean = true) {
  const queryClient = useQueryClient()
  const active = enabled && !!pipelineId && !!nodeId && runNumber > 0

  useEffect(() => {
    if (!active) {
      return
    }

    let mounted = true
    const cleanups: Array<() => void> = []
    const queryKey = queryKeys.runLogs.doc(pipelineId, runNumber, nodeId)

    async function register<E extends RunLogEvent>(name: E, handler: (payload: EventPayloadMap[E]) => void) {
      try {
        const unlisten = await events.listen<EventPayloadMap[E]>(name, (payload) => {
          if (
            mounted &&
            payload?.pipelineId === pipelineId &&
            payload.runNumber === runNumber &&
            payload.nodeId === nodeId
          ) {
            handler(payload)
          }
        })

        if (mounted) {
          cleanups.push(unlisten)
        } else {
          unlisten()
        }
      } catch (error) {
        logger.error('RunLogs', `Failed to listen to ${name}`, error)
      }
    }

    const invalidate = () => queryClient.invalidateQueries({ queryKey, exact: true })

    register('run-log-progress', ({ receivedBytes }) => {
      queryClient.setQueryData<RunLogDocument>(queryKey, (current) =>
        current?.state === 'loading' ? { state: 'loading', received_bytes: receivedBytes } : current
      )
    })
    register('run-log-ready', invalidate)
    register('run-log-failed', invalidate)

    return () => {
      mounted = false
      for (const unlisten of cleanups) {
        unlisten()
      }
    }
  }, [active, queryClient, pipelineId, runNumber, nodeId])

  return useQuery({
    queryKey: queryKeys.runLogs.doc(pipelineId, runNumber, nodeId),
    queryFn: () => service.openRunLog(pipelineId, runNumber, nodeId, false),
    enabled: active,
    retry: 1,
    gcTime: GC_TIMES.SHORT,
    refetchInterval: (query) => {
      const doc = query.state.data

      if (doc?.state === 'loading') {
        return DOC_LOADING_REFETCH_MS
      }

      return doc?.state === 'pending' || (doc?.state === 'ready' && !doc.complete) ? DOC_LIVE_REFETCH_MS : false
    },
    refetchIntervalInBackground: false,
  })
}

export function useRetryRunLog() {
  const queryClient = useQueryClient()

  return useMutation({
    retry: false,
    mutationFn: ({ pipelineId, runNumber, nodeId }: RunLogTarget) =>
      service.openRunLog(pipelineId, runNumber, nodeId, true),
    onSuccess: (doc, { pipelineId, runNumber, nodeId }) => {
      queryClient.setQueryData(queryKeys.runLogs.doc(pipelineId, runNumber, nodeId), doc)
    },
  })
}

export function useRunLogSearch(
  pipelineId: string,
  runNumber: number,
  nodeId: string,
  revision: number | undefined,
  sessionId: string,
  query: LogSearchQuery
) {
  const text = useDebounce(query.text, SEARCH_DEBOUNCE_MS)
  const effective = useMemo<LogSearchQuery>(
    () => ({ text, regex: query.regex, case_sensitive: query.case_sensitive }),
    [text, query.regex, query.case_sensitive]
  )
  const hasText = effective.text.length > 0

  return useQuery({
    queryKey: queryKeys.runLogs.search(pipelineId, runNumber, nodeId, revision ?? 0, effective),
    queryFn: ({ signal }) =>
      service.searchRunLog(pipelineId, runNumber, nodeId, revision ?? 0, sessionId, effective, signal),
    enabled: hasText && revision !== undefined && !!pipelineId && !!nodeId,
    retry: false,
    staleTime: Infinity,
    gcTime: GC_TIMES.SHORT,
    placeholderData: hasText ? keepPreviousData : undefined,
  })
}
