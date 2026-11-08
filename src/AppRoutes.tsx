import { lazy, type ReactNode } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'

import { LazyRoute } from './components/common/LazyRoute'
import { NotFoundPage } from './components/common/NotFoundPage'
import { useMobileShell } from './hooks/useIsMobile'
import type { Pipeline } from './types/pipelines'
import type { ProviderSummary } from './types/providers'

const UnifiedPipelinesView = lazy(() =>
  import('./features/pipelines/components/UnifiedPipelinesView').then((m) => ({
    default: m.UnifiedPipelinesView,
  }))
)

const PipelineDetailRoute = lazy(() =>
  import('./features/runs/routes/PipelineDetailRoute').then((m) => ({
    default: m.PipelineDetailRoute,
  }))
)

const SettingsRoute = lazy(() =>
  import('./features/settings/routes/SettingsRoute').then((m) => ({
    default: m.SettingsRoute,
  }))
)

const BrowsePage = lazy(() =>
  import('./features/providers/components/BrowsePage').then((m) => ({
    default: m.BrowsePage,
  }))
)

const NotificationsPage = lazy(() =>
  import('./features/notifications/components/NotificationsPage').then((m) => ({
    default: m.NotificationsPage,
  }))
)

interface AppRoutesProps {
  providers: ProviderSummary[]
  pipelines: Pipeline[]
  loading: boolean
  error: unknown
  onRetry: () => Promise<void>
  onBack: () => void
  onViewHistory: (pipeline: Pipeline) => void
  onViewMetrics: (pipeline: Pipeline) => void
  onViewRun: (pipelineId: string, runNumber: number) => void
  onTrigger: (pipeline: Pipeline) => void
}

export function AppRoutes({
  providers,
  pipelines,
  loading,
  error,
  onRetry,
  onViewHistory,
  onViewMetrics,
  onViewRun,
  onTrigger,
  onBack,
}: AppRoutesProps): ReactNode {
  const mobileShell = useMobileShell()
  const pipelinesPage = (
    <LazyRoute>
      <UnifiedPipelinesView
        providers={providers}
        loading={loading}
        error={error}
        onRetry={() => {
          void onRetry()
        }}
        onViewHistory={onViewHistory}
        onViewRun={onViewRun}
        onTrigger={onTrigger}
        onViewMetrics={onViewMetrics}
      />
    </LazyRoute>
  )

  return (
    <Routes>
      <Route path="/" element={<Navigate to="/pipelines" replace />} />

      <Route path="/pipelines" element={pipelinesPage} />

      <Route path="/pipelines/groups/:group" element={pipelinesPage} />

      <Route
        path="/pipelines/:pipelineId"
        element={
          <LazyRoute>
            <PipelineDetailRoute pipelines={pipelines} loading={loading} onBack={onBack} />
          </LazyRoute>
        }
      />

      <Route
        path="/settings"
        element={
          mobileShell ? (
            <LazyRoute>
              <SettingsRoute onRefresh={onRetry} />
            </LazyRoute>
          ) : (
            <Navigate to="/settings/general" replace />
          )
        }
      />

      <Route
        path="/browse"
        element={
          mobileShell ? (
            <LazyRoute>
              <BrowsePage />
            </LazyRoute>
          ) : (
            <Navigate to="/pipelines" replace />
          )
        }
      />

      <Route path="/providers" element={<Navigate to={mobileShell ? '/browse' : '/pipelines'} replace />} />

      <Route
        path="/notifications"
        element={
          <LazyRoute>
            <NotificationsPage />
          </LazyRoute>
        }
      />

      <Route
        path="/settings/:section"
        element={
          <LazyRoute>
            <SettingsRoute onRefresh={onRetry} />
          </LazyRoute>
        }
      />

      <Route path="*" element={mobileShell ? <Navigate to="/pipelines" replace /> : <NotFoundPage />} />
    </Routes>
  )
}
