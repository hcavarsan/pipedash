import { QueryCache, QueryClient } from '@tanstack/react-query'

import { isTransientError } from '../types/errors'

import { logger } from './logger'

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

      retry: (failureCount, error) => failureCount < 1 && isTransientError(error),
      retryDelay: 1000,
    },
    mutations: {
      retry: false,
    },
  },
})
