import { useQueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { useLocation, useMatch, useNavigate, useSearchParams } from 'react-router-dom'

import { AppRoutes } from './AppRoutes'
import { DocumentTitle } from './components/common/DocumentTitle'
import { AppLayout } from './components/layout/AppLayout'
import { fromOf, pushState } from './components/layout/mobileChrome'
import { useGoBack, useRecordHistory } from './components/layout/useMobileBack'
import { NativeIntentBridge } from './components/NativeIntentBridge'
import { useDesktopNotifications } from './features/notifications/queries/useDesktopNotifications'
import { useLocalNotificationsLifecycle } from './features/notifications/queries/useLocalNotificationQueries'
import { useLegacyPipelineMarksImport, usePipelineViews } from './features/pipelines/queries/usePipelineViewQueries'
import { useDesktopLifecycle } from './features/settings/queries/useDesktopQueries'
import { useEventSync } from './hooks/useEventSync'
import { useFailureNotifications } from './hooks/useFailureNotifications'
import { useProviderFilterReconciliation } from './hooks/useProviderFilterReconciliation'
import { useProviderHealthAlerts } from './hooks/useProviderHealthAlerts'
import { queryKeys } from './lib/queryKeys'
import { useBuildWatchLifecycle } from './queries/useBuildWatchQueries'
import { usePipelines } from './queries/usePipelinesQueries'
import { useProviders } from './queries/useProvidersQueries'
import { useVaultStatus } from './queries/useVaultQueries'
import { isTauri } from './services/isTauri'
import { useAuthStore } from './stores/authStore'
import { useModalStore } from './stores/modalStore'
import type { Pipeline } from './types/domain'
import { WorkflowModalHost } from './WorkflowModalHost'

export function AppContent(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()

  useEventSync()
  useBuildWatchLifecycle()
  useLocalNotificationsLifecycle()
  useDesktopLifecycle()
  useDesktopNotifications()
  useRecordHistory()
  const goBack = useGoBack()
  useFailureNotifications()
  useProviderHealthAlerts()
  useProviderFilterReconciliation()

  const { data: vaultStatus } = useVaultStatus()
  const isAuthenticated = useAuthStore((s) => s.token !== null)

  const [searchParams] = useSearchParams()
  const providerParam = searchParams.get('provider')

  const openLogsModal = useModalStore((s) => s.openLogsModal)
  const openTriggerModal = useModalStore((s) => s.openTriggerModal)

  const isVaultUnlocked = vaultStatus?.is_unlocked ?? false

  const shouldEnableQueries = !vaultStatus?.requires_password || (isVaultUnlocked && (isTauri() || isAuthenticated))

  const {
    data: providers = [],
    isLoading: providersLoading,
    isSuccess: providersSuccess,
    error: providersError,
  } = useProviders({
    enabled: shouldEnableQueries,
  })
  useLegacyPipelineMarksImport(shouldEnableQueries && providersSuccess && providers.length > 0)
  const { data: pipelineViews } = usePipelineViews(shouldEnableQueries)
  const viewParam = searchParams.get('view')

  const selectedProviderId = providers.find((p) => String(p.id) === providerParam)?.id

  const {
    data: pipelines = [],
    isLoading: pipelinesLoading,
    error: pipelinesError,
  } = usePipelines(selectedProviderId, {
    enabled: shouldEnableQueries && providersSuccess,
  })
  const detailMatch = useMatch('/pipelines/:pipelineId')
  const detailPipeline = pipelines.find((pipeline) => pipeline.id === detailMatch?.params.pipelineId)
  const providerName = providers.find((provider) => provider.id === selectedProviderId)?.name
  const section = location.pathname.split('/')[2] ?? 'general'
  const sectionTitle = section.charAt(0).toUpperCase() + section.slice(1)
  const pageTitle = location.pathname.startsWith('/settings')
    ? `${sectionTitle} Settings`
    : detailMatch
      ? (detailPipeline?.name ?? 'Pipeline unavailable')
      : location.pathname === '/pipelines'
        ? viewParam
          ? `${pipelineViews?.find((view) => view.id === viewParam)?.name ?? 'View'} Pipelines`
          : providerName
            ? `${providerName} Pipelines`
            : 'Pipelines'
        : location.pathname === '/notifications'
          ? 'Notifications'
          : 'Page not found'

  const invalidateAll = async (): Promise<void> => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }

  const listState = pushState(location.pathname, location.search, location.state)

  const handleViewHistory = (pipeline: Pipeline): void => {
    void navigate(`/pipelines/${encodeURIComponent(pipeline.id)}`, { state: listState })
  }

  const handleViewMetrics = (pipeline: Pipeline): void => {
    void navigate(`/pipelines/${encodeURIComponent(pipeline.id)}?tab=metrics`, { state: listState })
  }

  const backTarget = fromOf(location.state) ?? '/pipelines'

  const handleBackFromHistory = (): void => {
    goBack(backTarget)
  }

  const handleOpenSettings = (): void => {
    void navigate(location.pathname.startsWith('/settings') ? '/pipelines' : '/settings')
  }
  return (
    <AppLayout onOpenSettings={handleOpenSettings}>
      <DocumentTitle title={pageTitle} />
      <NativeIntentBridge />
      <AppRoutes
        providers={providers}
        pipelines={pipelines}
        loading={providersLoading || pipelinesLoading}
        error={providersError ?? pipelinesError}
        onRetry={invalidateAll}
        onViewHistory={handleViewHistory}
        onViewMetrics={handleViewMetrics}
        onViewRun={openLogsModal}
        onTrigger={(pipeline) => openTriggerModal(pipeline.id)}
        onBack={handleBackFromHistory}
      />
      <WorkflowModalHost pipelines={pipelines} invalidateAll={invalidateAll} />
    </AppLayout>
  )
}
