import { Box, Button } from '@mantine/core'
import { IconServer2, IconSettings } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { STORAGE_BACKEND_LABELS } from '../../../constants/storageLabels'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import {
  useConfigContent,
  useStorageConfig,
  useStoragePaths,
  useTestStorageConnection,
} from '../../../queries/useStorageQueries'
import { useLockVault, useVaultStatus } from '../../../queries/useVaultQueries'
import { getConnection, hasLocalCore, isRemoteClient } from '../../../services/connection'
import { lockMeansSomething } from '../../../utils/appLock'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isMobilePlatform, secureStoreLabel } from '../../../utils/platform'
import { SetupWizard } from '../../setup/components/SetupWizard/SetupWizard'
import { AppLockCard } from '../../vault/components/AppLockCard'
import { ConfigEditorModal } from './ConfigEditorModal'
import { StorageOverview } from './StorageOverview'
import { StoragePathsDisplay } from './StoragePathsDisplay'

const LocalStorageSection = (): ReactNode => {
  const mobileShell = useMobileShell()
  const {
    data: storageConfig,
    isLoading: loadingStorage,
    isError: isStorageError,
    error: storageError,
    refetch: refetchStorageConfig,
    isRefetching: isRefetchingStorage,
  } = useStorageConfig()
  const {
    data: storagePaths,
    error: storagePathsError,
    refetch: refetchStoragePaths,
    isRefetching: isRefetchingPaths,
  } = useStoragePaths()
  const { data: configContent, refetch: loadConfigContent } = useConfigContent()
  const testConnectionMutation = useTestStorageConnection()

  const { data: vaultStatus } = useVaultStatus()
  const lockVaultMutation = useLockVault()
  const canLockVault = Boolean(vaultStatus && lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked)

  const [setupWizardOpen, setSetupWizardOpen] = useState(false)
  const [configEditorOpen, setConfigEditorOpen] = useState(false)

  const { data: connectionResult, error: connectionError, isPending: testingConnection } = testConnectionMutation
  let connectionStatus: 'success' | 'error' | null = null
  if (connectionResult) {
    connectionStatus = connectionResult.success ? 'success' : 'error'
  } else if (connectionError) {
    connectionStatus = 'error'
  }
  const connectionMessage = connectionResult
    ? connectionResult.message || (connectionResult.success ? 'Connection successful' : 'Connection failed')
    : (connectionError?.message ?? '')

  const handleSetupComplete = (): void => {
    setSetupWizardOpen(false)
    void refetchStoragePaths()
    void refetchStorageConfig()
  }

  const openConfigEditor = async (): Promise<void> => {
    try {
      const result = await loadConfigContent()

      if (result.data) {
        setConfigEditorOpen(true)
      } else {
        throw result.error ?? new Error('The config file returned no content')
      }
    } catch (error) {
      logger.error('StorageSection', 'Failed to load config content', error)
      displayErrorNotification(error, "Couldn't open config file")
    }
  }

  const handleConfigSaved = (): void => {
    void refetchStorageConfig()
    void refetchStoragePaths()
  }

  const testConnection = (): void => {
    if (storageConfig) {
      testConnectionMutation.mutate(storageConfig.config)
    }
  }

  const backend = storageConfig?.config.storage.backend
  const isPostgres = backend === 'postgres'

  const renderContent = (): ReactNode => {
    const storageAlert = isStorageError && (
      <ErrorState
        title="Couldn't load storage configuration"
        error={storageError}
        onRetry={() => void refetchStorageConfig()}
        retrying={isRefetchingStorage}
      />
    )

    if (!storageConfig && isStorageError) {
      return storageAlert
    }

    if (loadingStorage) {
      return <LoadingState message="Loading storage configuration…" />
    }

    if (!storageConfig || !backend) {
      return (
        <EmptyState
          title="No storage configuration found"
          description="Pipedash couldn't find a config file in its data directory. Configure storage to create one."
          icon={<IconSettings size={24} aria-hidden />}
          action={<Button onClick={() => setSetupWizardOpen(true)}>Configure storage</Button>}
        />
      )
    }

    const pathsAlert = storagePathsError && (
      <ErrorState
        title="Couldn't load storage paths"
        error={storagePathsError}
        onRetry={() => void refetchStoragePaths()}
        retrying={isRefetchingPaths}
      />
    )
    let pathsPanel: ReactNode
    if (storagePaths) {
      pathsPanel = (
        <>
          {pathsAlert}
          <StoragePathsDisplay paths={storagePaths} config={storageConfig.config} onEditConfig={openConfigEditor} />
        </>
      )
    } else if (pathsAlert) {
      pathsPanel = pathsAlert
    } else {
      pathsPanel = <LoadingState message="Loading storage paths…" />
    }

    let credentialsLabel = `Encrypted in ${STORAGE_BACKEND_LABELS[backend]}`
    if (vaultStatus?.password_source === 'keyring') {
      credentialsLabel = mobileShell ? secureStoreLabel() : 'System keyring'
    }

    return (
      <>
        {storageAlert}
        <StorageOverview
          mobile={mobileShell}
          backendLabel={STORAGE_BACKEND_LABELS[backend]}
          credentialsLabel={credentialsLabel}
          cacheLabel={isPostgres ? STORAGE_BACKEND_LABELS.postgres : 'Local filesystem'}
          pathsPanel={pathsPanel}
          connectionStatus={connectionStatus}
          connectionMessage={connectionMessage}
          testingConnection={testingConnection}
          onTestConnection={testConnection}
          canLockVault={canLockVault}
          lockPending={lockVaultMutation.isPending}
          onLockVault={() => lockVaultMutation.mutate()}
          showConfigure={!isMobilePlatform()}
          onConfigure={() => setSetupWizardOpen(true)}
          showMigrateHint={
            hasLocalCore() && !isMobilePlatform() && backend === 'sqlite' && !vaultStatus?.requires_password
          }
        />
      </>
    )
  }

  return (
    <Box>
      <LiveStatus message={testingConnection ? '' : connectionMessage} />

      {renderContent()}

      {isMobilePlatform() && hasLocalCore() && (
        <Box mt={mobileShell ? 'lg' : 'md'}>
          <AppLockCard />
        </Box>
      )}

      <ErrorBoundary scope="feature" name="Storage setup" resetKeys={[setupWizardOpen]}>
        <SetupWizard
          opened={setupWizardOpen}
          onComplete={handleSetupComplete}
          onClose={() => setSetupWizardOpen(false)}
        />
      </ErrorBoundary>

      {configEditorOpen && configContent && (
        <ConfigEditorModal
          key={configContent.content}
          onClose={() => setConfigEditorOpen(false)}
          initialContent={configContent.content}
          onSaved={handleConfigSaved}
        />
      )}
    </Box>
  )
}

export const StorageSection = (): ReactNode => {
  if (isRemoteClient()) {
    return (
      <EmptyState
        title={`Storage is on ${getConnection().host}`}
        description={`Change storage on ${getConnection().host}.`}
        icon={<IconServer2 size={24} aria-hidden />}
      />
    )
  }

  return <LocalStorageSection />
}
