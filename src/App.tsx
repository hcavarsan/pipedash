import { Button, Center, Code, ThemeIcon } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle } from '@tabler/icons-react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback, useEffect, useState } from 'react'
import { Navigate, Route, Routes, useLocation, useMatch, useNavigate, useSearchParams } from 'react-router-dom'

import { RequireAuth } from './components/auth'
import { EmptyState } from './components/common/EmptyState'
import { ErrorState } from './components/common/ErrorState'
import { LoadingState } from './components/common/LoadingState'
import { PageContainer } from './components/common/PageContainer'
import { PageHeader } from './components/common/PageHeader'
import { ErrorBoundary } from './components/ErrorBoundary'
import { StatusScreen } from './components/ErrorBoundary/StatusScreen'
import { AppLayout } from './components/layout/AppLayout'
import { fromOf } from './components/layout/mobileChrome'
import { useGoBack, useRecordHistory } from './components/layout/useMobileBack'
import { NotificationsPage } from './components/notifications/NotificationCenter'
import { TriggerWorkflowModal } from './components/pipeline/TriggerWorkflowModal'
import { WorkflowLogsModal } from './components/pipeline/WorkflowLogsModal'
import { ProvidersPage } from './components/provider/ProvidersPage'
import { SetupWizard } from './components/setup/SetupWizard'
import { UnifiedPipelinesView } from './components/table/UnifiedPipelinesView'
import { usePlugins } from './contexts/PluginContext'
import { useDocumentTitle } from './hooks/useDocumentTitle'
import { useFailureNotifications } from './hooks/useFailureNotifications'
import { useMobileShell } from './hooks/useIsMobile'
import { useProviderHealthAlerts } from './hooks/useProviderHealthAlerts'
import { useEventSync } from './lib/eventSync'
import { logger } from './lib/logger'
import { queryKeys } from './lib/queryKeys'
import { useBuildWatchLifecycle } from './queries/useBuildWatchQueries'
import { useLocalNotificationsLifecycle } from './queries/useLocalNotificationQueries'
import { usePipelines } from './queries/usePipelinesQueries'
import { useLegacyPipelineMarksImport, usePipelineViews } from './queries/usePipelineViewQueries'
import { useProviders } from './queries/useProvidersQueries'
import { useRefreshProvider } from './queries/useRefreshQueries'
import { useCancelRun, useRerunFailedJobs, useRerunRun, useReviewRun } from './queries/useRunHistoryQueries'
import { useStoragePaths } from './queries/useStorageQueries'
import { useVaultStatus } from './queries/useVaultQueries'
import { PipelineDetailRoute, SettingsRoute, UnlockRoute } from './routes'
import { isTauri, service } from './services'
import { useAuthStore } from './stores/authStore'
import { useModalStore } from './stores/modalStore'
import type { Pipeline, PipelineRun, ReviewDecision } from './types'
import { displayErrorNotification } from './utils/errorDisplay'
import { canRerunInPlace, getReconcileBlockReason } from './utils/pipelinePresentation'

function AppContent() {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()
  const { getPluginByType } = usePlugins()
  const mobileShell = useMobileShell()

  useEventSync()
  useBuildWatchLifecycle()
  useLocalNotificationsLifecycle()
  useRecordHistory()
  const goBack = useGoBack()
  useFailureNotifications()
  useProviderHealthAlerts()

  const { data: vaultStatus } = useVaultStatus()
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)

  const [searchParams, setSearchParams] = useSearchParams()
  const providerParam = searchParams.get('provider')

  const triggerModal = useModalStore((s) => s.triggerModal)
  const logsModal = useModalStore((s) => s.logsModal)
  const openLogsModal = useModalStore((s) => s.openLogsModal)
  const closeTriggerModal = useModalStore((s) => s.closeTriggerModal)
  const closeLogsModal = useModalStore((s) => s.closeLogsModal)

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
  useDocumentTitle(
    location.pathname.startsWith('/settings')
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
  )

  const { mutateAsync: cancelRun } = useCancelRun()
  const { mutate: refreshProvider } = useRefreshProvider()
  const { mutateAsync: reviewRun } = useReviewRun()
  const { mutateAsync: rerunFailedJobs } = useRerunFailedJobs()
  const { mutateAsync: rerunRun } = useRerunRun()

  useEffect(() => {
    if (providerParam && providersSuccess && selectedProviderId === undefined) {
      logger.info('App', 'Selected provider no longer exists, clearing filter', { providerParam })

      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev)

          next.delete('provider')

          return next
        },
        { replace: true }
      )

      notifications.show({
        title: 'Filter cleared',
        message: 'The selected provider no longer exists.',
        color: 'gray',
      })
    }
  }, [providerParam, providersSuccess, selectedProviderId, setSearchParams])

  const invalidateAll = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }, [queryClient])

  const syncAfterAction = useCallback(
    async (providerId?: number) => {
      if (providerId !== undefined) {
        refreshProvider(providerId)
      }
      await invalidateAll()
    },
    [refreshProvider, invalidateAll]
  )

  const listLocation = `${location.pathname}${location.search}`

  const handleViewHistory = useCallback(
    (pipeline: { id: string }) => {
      navigate(`/pipelines/${encodeURIComponent(pipeline.id)}`, { state: { from: listLocation } })
    },
    [navigate, listLocation]
  )

  const handleViewMetrics = useCallback(
    (pipeline: { id: string }) => {
      navigate(`/pipelines/${encodeURIComponent(pipeline.id)}?tab=metrics`, { state: { from: listLocation } })
    },
    [navigate, listLocation]
  )

  const backTarget = fromOf(location.state) ?? '/pipelines'

  const handleBackFromHistory = useCallback(() => {
    goBack(backTarget)
  }, [goBack, backTarget])

  const handleOpenSettings = useCallback(() => {
    const isOnSettings = location.pathname.startsWith('/settings')

    navigate(isOnSettings ? '/pipelines' : '/settings')
  }, [navigate, location.pathname])

  const handleViewRun = useCallback(
    (pipelineId: string, runNumber: number) => {
      openLogsModal(pipelineId, runNumber)
    },
    [openLogsModal]
  )

  const handleRerun = useCallback(
    async (pipeline: Pipeline, run: PipelineRun) => {
      if (pipeline.provider_type === 'fluxcd') {
        useModalStore.getState().openTriggerModal(pipeline.id)
        return
      }
      useModalStore.getState().setRerunLoading(pipeline.id, run.run_number)
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

        const inputs = runDetails.inputs || {}

        if (run.branch && !inputs.branch && !inputs.ref) {
          inputs.ref = run.branch
        }

        useModalStore.getState().openTriggerModal(pipeline.id, inputs)
      } catch (error) {
        displayErrorNotification(error, "Couldn't rerun")
      } finally {
        useModalStore.getState().clearRerunLoading()
      }
    },
    [queryClient, rerunRun, refreshProvider]
  )

  const handleCancel = useCallback(
    async (pipeline: Pipeline, run: PipelineRun) => {
      if (
        await cancelRun({ pipelineId: run.pipeline_id, runNumber: run.run_number, pipeline, run }).catch(() => false)
      ) {
        refreshProvider(pipeline.provider_id)
      }
    },
    [cancelRun, refreshProvider]
  )

  const handleReview = useCallback(
    async (_pipeline: Pipeline, run: PipelineRun, decision: ReviewDecision) => {
      await reviewRun({ pipelineId: run.pipeline_id, runNumber: run.run_number, decision }).catch(() => undefined)
    },
    [reviewRun]
  )

  const handleRerunFailedJobs = useCallback(
    async (pipeline: Pipeline, run: PipelineRun) => {
      await rerunFailedJobs({ pipelineId: run.pipeline_id, runNumber: run.run_number })
        .then(() => refreshProvider(pipeline.provider_id))
        .catch(() => undefined)
    },
    [rerunFailedJobs, refreshProvider]
  )

  const loading = providersLoading || pipelinesLoading

  const triggerPipeline = pipelines.find((p) => p.id === triggerModal.pipelineId) ?? null
  const logsPipeline = pipelines.find((p) => p.id === logsModal.pipelineId)

  return (
    <AppLayout onOpenSettings={handleOpenSettings}>
      <Routes>
        <Route path="/" element={<Navigate to="/pipelines" replace />} />

        <Route
          path="/pipelines"
          element={
            <ErrorBoundary scope="route">
              <UnifiedPipelinesView
                providers={providers}
                loading={loading}
                error={providersError ?? pipelinesError}
                onRetry={invalidateAll}
                onViewHistory={handleViewHistory}
                onViewRun={handleViewRun}
                onTrigger={(p) => useModalStore.getState().openTriggerModal(p.id)}
                onViewMetrics={handleViewMetrics}
              />
            </ErrorBoundary>
          }
        />

        <Route
          path="/pipelines/:pipelineId"
          element={
            <ErrorBoundary scope="route">
              <PipelineDetailRoute
                pipelines={pipelines}
                loading={loading}
                onBack={handleBackFromHistory}
                onViewRun={handleViewRun}
                onRerun={handleRerun}
                onCancel={handleCancel}
                onReview={handleReview}
                onRerunFailedJobs={handleRerunFailedJobs}
              />
            </ErrorBoundary>
          }
        />

        <Route
          path="/settings"
          element={
            mobileShell ? (
              <ErrorBoundary scope="route">
                <SettingsRoute onRefresh={invalidateAll} />
              </ErrorBoundary>
            ) : (
              <Navigate to="/settings/general" replace />
            )
          }
        />

        <Route
          path="/providers"
          element={
            mobileShell ? (
              <ErrorBoundary scope="route">
                <ProvidersPage />
              </ErrorBoundary>
            ) : (
              <Navigate to="/pipelines" replace />
            )
          }
        />

        <Route
          path="/notifications"
          element={
            mobileShell ? (
              <ErrorBoundary scope="route">
                <NotificationsPage />
              </ErrorBoundary>
            ) : (
              <Navigate to="/pipelines" replace />
            )
          }
        />

        <Route
          path="/settings/:section"
          element={
            <ErrorBoundary scope="route">
              <SettingsRoute onRefresh={invalidateAll} />
            </ErrorBoundary>
          }
        />

        <Route
          path="*"
          element={
            mobileShell ? (
              <Navigate to="/pipelines" replace />
            ) : (
              <PageContainer>
                <PageHeader title="Page not found" />
                <EmptyState
                  title="This page doesn't exist"
                  description="Check the address or return to your pipelines."
                  action={
                    <Button variant="default" onClick={() => navigate('/pipelines')}>
                      Pipelines
                    </Button>
                  }
                />
              </PageContainer>
            )
          }
        />
      </Routes>

      {triggerPipeline && (
        <TriggerWorkflowModal
          opened={triggerModal.open}
          onClose={closeTriggerModal}
          pipeline={triggerPipeline}
          initialInputs={triggerModal.initialInputs}
          onSuccess={async (pipelineId, runNumber) => {
            await syncAfterAction(triggerPipeline.provider_id)
            openLogsModal(pipelineId, runNumber)
          }}
        />
      )}

      {logsModal.open && logsModal.pipelineId && logsModal.runNumber && (
        <WorkflowLogsModal
          opened={logsModal.open}
          onClose={closeLogsModal}
          pipelineId={logsModal.pipelineId}
          runNumber={logsModal.runNumber}
          providerId={logsPipeline?.provider_id}
          providerType={logsPipeline?.provider_type}
          pipeline={logsPipeline}
          canViewLogs={!!logsPipeline && !!getPluginByType(logsPipeline.provider_type)?.capabilities.logs}
          canTrigger={!!logsPipeline && !getReconcileBlockReason(logsPipeline)}
          triggerBlockedReason={logsPipeline ? getReconcileBlockReason(logsPipeline) : undefined}
          rerunInPlace={!!logsPipeline && canRerunInPlace(logsPipeline)}
          canCancel={!!logsPipeline && !!getPluginByType(logsPipeline.provider_type)?.capabilities.cancel}
          onRerunSuccess={async (pipelineId, newRunNumber) => {
            await syncAfterAction(logsPipeline?.provider_id)
            openLogsModal(pipelineId, newRunNumber)
          }}
          onCancelSuccess={() => syncAfterAction(logsPipeline?.provider_id)}
        />
      )}
    </AppLayout>
  )
}

interface ConfigErrorScreenProps {
  errors: string[]
  onRetry: () => void
  retrying: boolean
}

function ConfigErrorScreen({ errors, onRetry, retrying }: ConfigErrorScreenProps) {
  const { data: storagePaths } = useStoragePaths()
  const mobileShell = useMobileShell()
  const hint = storagePaths ? (
    <>
      Fix the errors in <Code>{storagePaths.config_file}</Code>, then retry.
    </>
  ) : (
    'Fix the errors in your Pipedash configuration file, then retry.'
  )

  if (mobileShell) {
    return (
      <StatusScreen
        icon={
          <ThemeIcon variant="transparent" color="red" size={48}>
            <IconAlertCircle size={40} aria-hidden />
          </ThemeIcon>
        }
        title="Configuration file has errors"
        message={errors.join(', ') || 'Unknown error'}
        hint={hint}
        minHeight="100dvh"
        actions={
          <Button size="md" fullWidth loading={retrying} onClick={onRetry}>
            Retry
          </Button>
        }
      />
    )
  }

  return (
    <Center h="100%" p="xl">
      <Center maw={560} w="100%">
        <ErrorState
          title="Configuration file has errors"
          error={errors.join(', ') || 'Unknown error'}
          hint={hint}
          onRetry={onRetry}
          retrying={retrying}
        />
      </Center>
    </Center>
  )
}

function App() {
  const queryClient = useQueryClient()

  const {
    data: setupStatus,
    isLoading: checkingSetup,
    isError: setupFailed,
    error: setupError,
    refetch: refetchSetupStatus,
    isRefetching: refetchingSetup,
  } = useQuery({
    queryKey: queryKeys.setup.status(),
    queryFn: () => service.checkSetupStatus(),
    retry: false,
  })

  const [setupActive, setSetupActive] = useState(false)
  const needsSetup = setupStatus?.needs_setup ?? false

  useEffect(() => {
    if (needsSetup) {
      setSetupActive(true)
    }
  }, [needsSetup])

  const handleSetupComplete = useCallback(() => {
    setSetupActive(false)
    queryClient.invalidateQueries({ queryKey: queryKeys.setup.status() })
  }, [queryClient])

  if (checkingSetup) {
    return <LoadingState variant="page" message="Loading Pipedash" />
  }

  if (setupFailed) {
    return (
      <Center h="100%" p="xl">
        <Center maw={560} w="100%">
          <ErrorState
            title="Couldn't check setup status"
            error={setupError}
            onRetry={() => refetchSetupStatus()}
            retrying={refetchingSetup}
          />
        </Center>
      </Center>
    )
  }

  if (setupStatus?.config_exists && !setupStatus.config_valid) {
    return (
      <ConfigErrorScreen
        errors={setupStatus.validation_errors}
        onRetry={() => refetchSetupStatus()}
        retrying={refetchingSetup}
      />
    )
  }

  if (needsSetup || setupActive) {
    return (
      <ErrorBoundary name="Setup wizard">
        <SetupWizard opened onComplete={handleSetupComplete} />
      </ErrorBoundary>
    )
  }

  // Main app with route-based auth
  return (
    <Routes>
      {/* Unlock route - always accessible */}
      <Route path="/unlock" element={<UnlockRoute />} />

      {/* All other routes require auth */}
      <Route
        path="/*"
        element={
          <RequireAuth>
            <AppContent />
          </RequireAuth>
        }
      />
    </Routes>
  )
}

export default App
