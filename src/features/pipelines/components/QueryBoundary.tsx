import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'

interface QueryBoundaryProps<T> {
  query: UseQueryResult<T, Error>
  loading: string
  failure: string
  variant?: 'section' | 'compact'
  children: (data: T) => ReactNode
}

export function QueryBoundary<T>({
  query,
  loading,
  failure,
  variant = 'section',
  children,
}: QueryBoundaryProps<T>): ReactNode {
  const { data, error, isError, isFetching, isPending, fetchStatus, refetch } = query
  const alert = isError && (
    <ErrorState title={failure} error={error} onRetry={() => void refetch()} retrying={isFetching} />
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
