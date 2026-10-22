import { Center, Code } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback, useEffect, useState } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { RequireAuth } from './components/auth'
import { ErrorState } from './components/common/ErrorState'
import { LoadingState } from './components/common/LoadingState'
import { ErrorBoundary } from './components/ErrorBoundary'
import { AppLayout } from './components/layout/AppLayout'
import { TriggerWorkflowModal } from './components/pipeline/TriggerWorkflowModal'
import { WorkflowLogsModal } from './components/pipeline/WorkflowLogsModal'
import { SetupWizard } from './components/setup/SetupWizard'
import { UnifiedPipelinesView } from './components/table/UnifiedPipelinesView'
import { usePlugins } from './contexts/PluginContext'
import { useFailureNotifications } from './hooks/useFailureNotifications'
import { useEventSync } from './lib/eventSync'
import { logger } from './lib/logger'
import { queryKeys } from './lib/queryKeys'
import { usePipelines } from './queries/usePipelinesQueries'
import { useProviders } from './queries/useProvidersQueries'
import { useCancelRun } from './queries/useRunHistoryQueries'
import { useStoragePaths } from './queries/useStorageQueries'
import { useVaultStatus } from './queries/useVaultQueries'
import { PipelineDetailRoute, SettingsRoute, UnlockRoute } from './routes'
import { service } from './services'
import { useAuthStore } from './stores/authStore'
import { useModalStore } from './stores/modalStore'
import type { Pipeline, PipelineRun } from './types'
import { displayErrorNotification } from './utils/errorDisplay'

function AppContent() {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()
  const { getPluginByType } = usePlugins()

  useEventSync()
  useFailureNotifications()

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

  const shouldEnableQueries = !vaultStatus?.requires_password || (isVaultUnlocked && isAuthenticated)

  const {
    data: providers = [],
    isLoading: providersLoading,
    isSuccess: providersSuccess,
    error: providersError,
  } = useProviders({
    enabled: shouldEnableQueries,
  })

  const selectedProviderId = providers.find((p) => String(p.id) === providerParam)?.id

  const {
    data: pipelines = [],
    isLoading: pipelinesLoading,
    error: pipelinesError,
  } = usePipelines(selectedProviderId, {
    enabled: shouldEnableQueries && providersSuccess,
    providers,
  })

  const { mutate: cancelRun } = useCancelRun()

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

  const handleRefreshAll = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }, [queryClient])

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

  const backTarget =
    typeof location.state === 'object' &&
    location.state !== null &&
    'from' in location.state &&
    typeof location.state.from === 'string'
      ? location.state.from
      : '/pipelines'

  const handleBackFromHistory = useCallback(() => {
    navigate(backTarget)
  }, [navigate, backTarget])

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
    [queryClient]
  )

  const handleCancel = useCallback(
    (_pipeline: Pipeline, run: PipelineRun) => {
      cancelRun({ pipelineId: run.pipeline_id, runNumber: run.run_number })
    },
    [cancelRun]
  )

  const loading = providersLoading || pipelinesLoading

  const triggerPipeline = pipelines.find((p) => p.id === triggerModal.pipelineId) ?? null
  const logsPipeline = pipelines.find((p) => p.id === logsModal.pipelineId)

  return (
    <AppLayout onRefreshAll={handleRefreshAll} refreshing={loading} onOpenSettings={handleOpenSettings}>
      <Routes>
        <Route path="/" element={<Navigate to="/pipelines" replace />} />

        <Route
          path="/pipelines"
          element={
            <ErrorBoundary scope="route">
              <UnifiedPipelinesView
                pipelines={pipelines}
                providers={providers}
                loading={loading}
                error={providersError ?? pipelinesError}
                onRetry={handleRefreshAll}
                onViewHistory={handleViewHistory}
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
              />
            </ErrorBoundary>
          }
        />

        <Route path="/settings" element={<Navigate to="/settings/general" replace />} />

        <Route
          path="/settings/:section"
          element={
            <ErrorBoundary scope="route">
              <SettingsRoute onRefresh={handleRefreshAll} />
            </ErrorBoundary>
          }
        />

        <Route path="*" element={<Navigate to="/pipelines" replace />} />
      </Routes>

      {triggerPipeline && (
        <TriggerWorkflowModal
          opened={triggerModal.open}
          onClose={closeTriggerModal}
          pipeline={triggerPipeline}
          initialInputs={triggerModal.initialInputs}
          onSuccess={async (pipelineId, runNumber) => {
            await handleRefreshAll()
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
          canTrigger={!!logsPipeline && logsPipeline.metadata?.suspended !== true}
          canCancel={!!logsPipeline && !!getPluginByType(logsPipeline.provider_type)?.capabilities.cancel}
          onRerunSuccess={async (pipelineId, newRunNumber) => {
            await handleRefreshAll()
            openLogsModal(pipelineId, newRunNumber)
          }}
          onCancelSuccess={handleRefreshAll}
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

  return (
    <Center h="100%" p="xl">
      <Center maw={560} w="100%">
        <ErrorState
          title="Configuration file has errors"
          error={errors.join(', ') || 'Unknown error'}
          hint={
            storagePaths ? (
              <>
                Fix the errors in <Code>{storagePaths.config_file}</Code>, then retry.
              </>
            ) : (
              'Fix the errors in your Pipedash configuration file, then retry.'
            )
          }
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
