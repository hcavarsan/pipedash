import { lazy, type ReactNode, Suspense } from 'react'

import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import type { FunnelStep } from '../../../types/applications'

const FunnelPlot = lazy(() => import('./FunnelPlot').then((m) => ({ default: m.FunnelPlot })))

export function LazyFunnelPlot({ steps }: { steps: FunnelStep[] }): ReactNode {
  return (
    <ErrorBoundary scope="feature" name="Funnel" resetKeys={[steps]}>
      <Suspense fallback={<LoadingState message="Loading chart…" />}>
        <FunnelPlot steps={steps} />
      </Suspense>
    </ErrorBoundary>
  )
}
