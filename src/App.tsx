import { Center } from '@mantine/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { lazy, type ReactNode, Suspense, useState } from 'react'
import { Route, Routes } from 'react-router-dom'

import { AppContent } from './AppContent'
import { ErrorState } from './components/common/ErrorState'
import { LazyRoute } from './components/common/LazyRoute'
import { LoadingState } from './components/common/LoadingState'
import { ErrorBoundary } from './components/ErrorBoundary/ErrorBoundary'
import { ConfigErrorScreen } from './features/setup/components/ConfigErrorScreen'
import { RequireAuth } from './features/vault/components/RequireAuth'
import { queryKeys } from './lib/queryKeys'
import { service } from './services/service'

const UnlockRoute = lazy(() =>
  import('./features/vault/routes/UnlockRoute').then((m) => ({
    default: m.UnlockRoute,
  }))
)

const SetupWizard = lazy(() =>
  import('./features/setup/components/SetupWizard/SetupWizard').then((m) => ({
    default: m.SetupWizard,
  }))
)

export function App(): ReactNode {
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

  const needsSetup = setupStatus?.needs_setup ?? false
  const [setupActive, setSetupActive] = useState(needsSetup)
  const [previousNeedsSetup, setPreviousNeedsSetup] = useState(needsSetup)

  if (previousNeedsSetup !== needsSetup) {
    setPreviousNeedsSetup(needsSetup)

    if (needsSetup) {
      setSetupActive(true)
    }
  }

  const handleSetupComplete = (): void => {
    setSetupActive(false)
    void queryClient.invalidateQueries({ queryKey: queryKeys.setup.status() })
  }

  const retrySetupStatus = (): void => {
    void refetchSetupStatus()
  }

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
            onRetry={retrySetupStatus}
            retrying={refetchingSetup}
          />
        </Center>
      </Center>
    )
  }

  if (setupStatus?.config_exists && !setupStatus.config_valid) {
    return (
      <ConfigErrorScreen errors={setupStatus.validation_errors} onRetry={retrySetupStatus} retrying={refetchingSetup} />
    )
  }

  if (needsSetup || setupActive) {
    return (
      <ErrorBoundary name="Setup wizard">
        <Suspense fallback={<LoadingState variant="page" message="Loading setup..." />}>
          <SetupWizard opened onComplete={handleSetupComplete} />
        </Suspense>
      </ErrorBoundary>
    )
  }

  return (
    <Routes>
      <Route
        path="/unlock"
        element={
          <LazyRoute>
            <UnlockRoute />
          </LazyRoute>
        }
      />

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
