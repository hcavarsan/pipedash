import { lazy, type ReactNode, Suspense } from 'react'

import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'

import type { MetricsChartProps } from './MetricsChart'

const MetricsChart = lazy(() => import('./MetricsChart').then((m) => ({ default: m.MetricsChart })))

export function LazyMetricsChart(props: MetricsChartProps): ReactNode {
  return (
    <ErrorBoundary scope="feature" name="Chart" resetKeys={[props.data]}>
      <Suspense fallback={<LoadingState message="Loading chart..." />}>
        <MetricsChart {...props} />
      </Suspense>
    </ErrorBoundary>
  )
}
