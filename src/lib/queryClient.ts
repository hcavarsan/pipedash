import { QueryCache, QueryClient } from '@tanstack/react-query'

import { logger } from './logger'

const CRITICAL_QUERIES = new Set(['providers', 'setup'])

export const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error, query) => {
      logger.error('QueryClient', 'Query error', {
        queryKey: query.queryKey,
        error: error instanceof Error ? error.message : error,
      })
    },
  }),
  defaultOptions: {
    queries: {
      staleTime: 30 * 1000,
      gcTime: 5 * 60 * 1000,

      refetchOnWindowFocus: true,
      refetchOnReconnect: true,

      retry: 3,
      retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 10000),

      throwOnError: (_error, query) => {
        const scope = query.queryKey[0] as string



return CRITICAL_QUERIES.has(scope)
      },
    },
    mutations: {
      retry: 1,
      retryDelay: 1000,
    },
  },
})
