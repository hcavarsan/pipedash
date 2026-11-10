import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { ErrorState } from './ErrorState'
import { LoadingState } from './LoadingState'

interface QueryBoundaryProps<T> {
  query: UseQueryResult<T, Error>
  loading: string
  failure: string
  hint?: ReactNode
  variant?: 'section' | 'compact'
  children: (data: T) => ReactNode
}

export function QueryBoundary<T>({
  query,
  loading,
  failure,
  hint,
  variant = 'section',
  children,
}: QueryBoundaryProps<T>): ReactNode {
  const { data, error, isError, isFetching, isPending, fetchStatus, refetch } = query
  const alert = isError && (
    <ErrorState title={failure} error={error} hint={hint} onRetry={() => void refetch()} retrying={isFetching} />
  )

  if (data === undefined) {
    if (isError) {
      return alert
    }

    return isPending && fetchStatus !== 'idle' ? <LoadingState message={loading} variant={variant} /> : null
  }

  return (
    <>
      {alert}
      {children(data)}
    </>
  )
}
