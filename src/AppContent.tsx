import { useQueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { useLocation, useMatch, useNavigate, useSearchParams } from 'react-router-dom'

import { AppRoutes } from './AppRoutes'
import { DocumentTitle } from './components/common/DocumentTitle'
import { AppLayout } from './components/layout/AppLayout'
import { fromOf } from './components/layout/mobileChrome'
import { useGoBack, useRecordHistory } from './components/layout/useMobileBack'
import { useFailureNotifications } from './hooks/useFailureNotifications'
import { useProviderFilterReconciliation } from './hooks/useProviderFilterReconciliation'
import { useProviderHealthAlerts } from './hooks/useProviderHealthAlerts'
import { useEventSync } from './lib/eventSync'
import { queryKeys } from './lib/queryKeys'
import { useBuildWatchLifecycle } from './queries/useBuildWatchQueries'
import { useLocalNotificationsLifecycle } from './queries/useLocalNotificationQueries'
import { usePipelines } from './queries/usePipelinesQueries'
import { useLegacyPipelineMarksImport, usePipelineViews } from './queries/usePipelineViewQueries'
import { useProviders } from './queries/useProvidersQueries'
import { useRefreshProvider } from './queries/useRefreshQueries'
import { useCancelRun, useRerunFailedJobs, useRerunRun, useReviewRun } from './queries/useRunHistoryQueries'
import { useVaultStatus } from './queries/useVaultQueries'
import { isTauri, service } from './services'
import { useAuthStore } from './stores/authStore'
import { useModalStore } from './stores/modalStore'
import type { Pipeline, PipelineRun, ReviewDecision } from './types'
import { displayErrorNotification } from './utils/errorDisplay'
import { canRerunInPlace } from './utils/pipelinePresentation'
import { WorkflowModalHost } from './WorkflowModalHost'

export function AppContent(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()

  useEventSync()
  useBuildWatchLifecycle()
  useLocalNotificationsLifecycle()
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
    providers,
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
        : 'Page not found'

  const { mutateAsync: cancelRun } = useCancelRun()
  const { mutate: refreshProvider } = useRefreshProvider()
  const { mutateAsync: reviewRun } = useReviewRun()
  const { mutateAsync: rerunFailedJobs } = useRerunFailedJobs()
  const { mutateAsync: rerunRun } = useRerunRun()

  const invalidateAll = async (): Promise<void> => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }

  const listLocation = `${location.pathname}${location.search}`

  const handleViewHistory = (pipeline: Pipeline): void => {
    void navigate(`/pipelines/${encodeURIComponent(pipeline.id)}`, { state: { from: listLocation } })
  }

  const handleViewMetrics = (pipeline: Pipeline): void => {
    void navigate(`/pipelines/${encodeURIComponent(pipeline.id)}?tab=metrics`, { state: { from: listLocation } })
  }

  const backTarget = fromOf(location.state) ?? '/pipelines'

  const handleBackFromHistory = (): void => {
    goBack(backTarget)
  }

  const handleOpenSettings = (): void => {
    void navigate(location.pathname.startsWith('/settings') ? '/pipelines' : '/settings')
  }

  const handleRerun = async (pipeline: Pipeline, run: PipelineRun): Promise<void> => {
    if (pipeline.provider_type === 'fluxcd') {
      openTriggerModal(pipeline.id)

      return
    }

    try {
      if (canRerunInPlace(pipeline)) {
        await rerunRun({ pipelineId: run.pipeline_id, runNumber: run.run_number })
          .then(() => refreshProvider(pipeline.provider_id))
          .catch(() => undefined)

        return
      }

      const runDetails = await queryClient.fetchQuery({
        queryKey: queryKeys.runs.detail(run.pipeline_id, run.run_number),
        queryFn: () => service.getWorkflowRunDetails(run.pipeline_id, run.run_number),
        staleTime: 30 * 1000,
      })

      const existingInputs = runDetails.inputs ?? {}
      const inputs =
        run.branch && !existingInputs.branch && !existingInputs.ref
          ? { ...existingInputs, ref: run.branch }
          : existingInputs

      openTriggerModal(pipeline.id, inputs)
    } catch (error) {
      displayErrorNotification(error, "Couldn't rerun")
    }
  }

  const handleCancel = async (pipeline: Pipeline, run: PipelineRun): Promise<void> => {
    if (await cancelRun({ pipelineId: run.pipeline_id, runNumber: run.run_number, pipeline, run }).catch(() => false)) {
      refreshProvider(pipeline.provider_id)
    }
  }

  const handleReview = async (_pipeline: Pipeline, run: PipelineRun, decision: ReviewDecision): Promise<void> => {
    await reviewRun({ pipelineId: run.pipeline_id, runNumber: run.run_number, decision }).catch(() => undefined)
  }

  const handleRerunFailedJobs = async (pipeline: Pipeline, run: PipelineRun): Promise<void> => {
    await rerunFailedJobs({ pipelineId: run.pipeline_id, runNumber: run.run_number })
      .then(() => refreshProvider(pipeline.provider_id))
      .catch(() => undefined)
  }

  return (
    <AppLayout onOpenSettings={handleOpenSettings}>
      <DocumentTitle title={pageTitle} />
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
        onRerun={handleRerun}
        onCancel={handleCancel}
        onReview={handleReview}
        onRerunFailedJobs={handleRerunFailedJobs}
      />
      <WorkflowModalHost pipelines={pipelines} invalidateAll={invalidateAll} />
    </AppLayout>
  )
}
