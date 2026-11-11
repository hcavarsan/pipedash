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

export interface PrerequisiteState {
  loading: boolean
  error: unknown
  retrying: boolean
}

interface AppRoutesProps {
  providers: ProviderSummary[]
  pipelines: Pipeline[]
  listPrerequisites: PrerequisiteState
  detailPrerequisites: PrerequisiteState
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
  listPrerequisites,
  detailPrerequisites,
  onRetry,
  onViewHistory,
  onViewMetrics,
  onViewRun,
  onTrigger,
  onBack,
}: AppRoutesProps): ReactNode {
  const mobileShell = useMobileShell()
  const home = mobileShell ? '/browse' : '/pipelines'
  const pipelinesPage = (
    <LazyRoute>
      <UnifiedPipelinesView
        providers={providers}
        loading={listPrerequisites.loading}
        error={listPrerequisites.error}
        retrying={listPrerequisites.retrying}
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
      <Route path="/" element={<Navigate to={home} replace />} />

      <Route path="/pipelines" element={pipelinesPage} />

      <Route path="/pipelines/groups/:group" element={pipelinesPage} />

      <Route
        path="/pipelines/:pipelineId"
        element={
          <LazyRoute>
            <PipelineDetailRoute
              pipelines={pipelines}
              loading={detailPrerequisites.loading}
              error={detailPrerequisites.error}
              retrying={detailPrerequisites.retrying}
              onRetry={() => void onRetry()}
              onBack={onBack}
            />
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

      <Route path="/providers" element={<Navigate to={home} replace />} />

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
