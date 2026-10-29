import { type ReactNode, Suspense } from 'react'
import { useLocation } from 'react-router-dom'

import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'

import { LoadingState } from './LoadingState'

export function LazyRoute({ children }: { children: ReactNode }): ReactNode {
  const location = useLocation()

  return (
    <ErrorBoundary scope="route" resetKeys={[location.pathname, location.search]}>
      <Suspense fallback={<LoadingState variant="page" message="Loading page..." />}>{children}</Suspense>
    </ErrorBoundary>
  )
}
