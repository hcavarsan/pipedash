import { useCallback, useEffect } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'

import { Center, Loader, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { useQuery, useQueryClient } from '@tanstack/react-query'

import { RequireAuth } from './components/auth'
import { FeatureErrorBoundary } from './components/ErrorBoundary/FeatureErrorBoundary'
import { RouteErrorBoundary } from './components/ErrorBoundary/RouteErrorBoundary'
import { AppLayout } from './components/layout/AppLayout'
import { TriggerWorkflowModal } from './components/pipeline/TriggerWorkflowModal'
import { WorkflowLogsModal } from './components/pipeline/WorkflowLogsModal'
import { SetupWizard } from './components/setup/SetupWizard'
import { useEventSync } from './lib/eventSync'
import { logger } from './lib/logger'
import { queryKeys } from './lib/queryKeys'
import { useGetPipelinesFromCache, usePipelines } from './queries/usePipelinesQueries'
import { useProviders } from './queries/useProvidersQueries'
import { useCancelRun } from './queries/useRunHistoryQueries'
import { useVaultStatus } from './queries/useVaultQueries'
import { useAuthStore } from './stores/authStore'
import { useFilterStore } from './stores/filterStore'
import { useModalStore } from './stores/modalStore'
import { displayErrorNotification } from './utils/errorDisplay'
import { PipelineDetailRoute, PipelinesRoute, SettingsRoute, UnlockRoute } from './routes'
import { service } from './services'
import type { Pipeline, PipelineRun } from './types'

function AppContent() {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()

  useEventSync()

  const { data: vaultStatus } = useVaultStatus()
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)

  const selectedProviderId = useFilterStore((s) => s.selectedProviderId)
  const selectedProviderName = useFilterStore((s) => s.selectedProviderName)
  const setSelectedProviderId = useFilterStore((s) => s.setSelectedProviderId)

  const triggerModal = useModalStore((s) => s.triggerModal)
  const logsModal = useModalStore((s) => s.logsModal)
  const openLogsModal = useModalStore((s) => s.openLogsModal)
  const closeTriggerModal = useModalStore((s) => s.closeTriggerModal)
  const closeLogsModal = useModalStore((s) => s.closeLogsModal)

  const isVaultUnlocked = vaultStatus?.is_unlocked ?? false

  const shouldEnableQueries =
    !vaultStatus?.requires_password || (isVaultUnlocked && isAuthenticated)

  const {
    data: providers = [],
    isLoading: providersLoading,
    isSuccess: providersSuccess,
  } = useProviders({
    enabled: shouldEnableQueries,
  })

  const { data: pipelines = [], isLoading: pipelinesLoading } = usePipelines(selectedProviderId, {
    enabled: shouldEnableQueries && providersSuccess,
    providers,
  })

  const { mutate: cancelRun } = useCancelRun()

  const getPipelinesFromCache = useGetPipelinesFromCache()

  useEffect(() => {
    if (selectedProviderId && providers.length > 0) {
      const exists = providers.some((p) => p.id === selectedProviderId)

      if (!exists) {
        const providerName = selectedProviderName || `ID ${selectedProviderId}`

        logger.info('App', 'Selected provider was removed, clearing filter', {
          providerId: selectedProviderId,
        })

        setSelectedProviderId(undefined)

        notifications.show({
          title: 'Filter Cleared',
          message: `Provider "${providerName}" was removed`,
          color: 'blue',
        })
      }
    }
  }, [providers, selectedProviderId, selectedProviderName, setSelectedProviderId])

  const handleRefreshAll = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
    ])
  }, [queryClient])

  const handleViewHistory = useCallback(
    (pipeline: { id: string }) => {
      navigate(`/pipelines/${pipeline.id}`)
    },
    [navigate]
  )

  const handleViewMetrics = useCallback(
    (pipeline: { id: string }) => {
      navigate(`/pipelines/${pipeline.id}?tab=metrics`)
    },
    [navigate]
  )

  const handleBackFromHistory = useCallback(() => {
    const path = selectedProviderId
      ? `/pipelines?provider=${selectedProviderId}`
      : '/pipelines'

    navigate(path)
  }, [navigate, selectedProviderId])

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
        displayErrorNotification(error, 'Failed to Rerun')
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

  const triggerPipeline = triggerModal.pipelineId
    ? getPipelinesFromCache()?.find((p) => p.id === triggerModal.pipelineId)
    : null

  return (
    <AppLayout
      onRefreshAll={handleRefreshAll}
      refreshing={loading}
      onOpenSettings={handleOpenSettings}
    >
      <Routes>
        <Route path="/" element={<Navigate to="/pipelines" replace />} />

        <Route
          path="/pipelines"
          element={
            <RouteErrorBoundary>
              <PipelinesRoute
                pipelines={pipelines}
                providers={providers}
                loading={loading}
                onViewHistory={handleViewHistory}
                onTrigger={(p) => useModalStore.getState().openTriggerModal(p.id)}
                onViewMetrics={handleViewMetrics}
              />
            </RouteErrorBoundary>
          }
        />

        <Route
          path="/pipelines/:pipelineId"
          element={
            <RouteErrorBoundary>
              <PipelineDetailRoute
                pipelines={pipelines}
                loading={loading}
                onBack={handleBackFromHistory}
                onViewRun={handleViewRun}
                onRerun={handleRerun}
                onCancel={handleCancel}
              />
            </RouteErrorBoundary>
          }
        />

        <Route path="/settings" element={<Navigate to="/settings/general" replace />} />

        <Route
          path="/settings/:section"
          element={
            <RouteErrorBoundary>
              <SettingsRoute onRefresh={handleRefreshAll} />
            </RouteErrorBoundary>
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
          providerId={pipelines.find((p) => p.id === logsModal.pipelineId)?.provider_id}
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

function App() {
  const queryClient = useQueryClient()

  const { data: setupStatus, isLoading: checkingSetup } = useQuery({
    queryKey: queryKeys.setup.status(),
    queryFn: () => service.checkSetupStatus(),
    retry: false,
  })

  const handleSetupComplete = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: queryKeys.setup.status() })
  }, [queryClient])

  // Show loading while checking setup status
  if (checkingSetup) {
    return (
      <Center h="100vh">
        <Stack align="center" gap="md">
          <Loader size="lg" />
          <Text size="sm" c="dimmed">
            Loading Pipedash...
          </Text>
        </Stack>
      </Center>
    )
  }

  // Show configuration error
  if (setupStatus?.config_exists && !setupStatus.config_valid) {
    const errors = setupStatus.validation_errors?.join(', ') || 'Unknown error'

    return (
      <Center h="100vh">
        <Stack align="center" gap="md" maw={500} p="xl">
          <Text size="xl" fw={600} c="red">
            Configuration Error
          </Text>
          <Text size="sm" c="dimmed" ta="center">
            Your configuration file exists but contains errors:
          </Text>
          <Text size="sm" c="red" ta="center" ff="monospace">
            {errors}
          </Text>
        </Stack>
      </Center>
    )
  }

  // Show setup wizard for first-time setup
  if (setupStatus?.needs_setup) {
    return (
      <FeatureErrorBoundary featureName="Setup Wizard">
        <SetupWizard opened onComplete={handleSetupComplete} />
      </FeatureErrorBoundary>
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
