import { Alert, Box, Button, Card, Group, Loader, SimpleGrid, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle, IconCheck, IconCloudUpload, IconLock, IconSettings } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
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
import { isTauri } from '../../../services/isTauri'
import { formatErrorMessage } from '../../../types/errors'
import { lockMeansSomething } from '../../../utils/appLock'
import { isMobilePlatform, secureStoreLabel } from '../../../utils/platform'
import { SetupWizard } from '../../setup/components/SetupWizard/SetupWizard'
import { AppLockCard } from '../../vault/components/AppLockCard'
import { ConfigEditorModal } from './ConfigEditorModal'
import { StoragePathsDisplay } from './StoragePathsDisplay'

export const StorageSection = (): ReactNode => {
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
  const canLockVault = vaultStatus && lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked

  const [setupWizardOpen, setSetupWizardOpen] = useState(false)
  const [configEditorOpen, setConfigEditorOpen] = useState(false)

  const { data: connectionResult, error: connectionError, isPending: testingConnection } = testConnectionMutation
  const connectionStatus = connectionResult
    ? connectionResult.success
      ? 'success'
      : 'error'
    : connectionError
      ? 'error'
      : null
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
      notifications.show({
        title: "Couldn't open config file",
        message: formatErrorMessage(error),
        color: 'red',
      })
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
    if (isStorageError) {
      return (
        <ErrorState
          title="Couldn't load storage configuration"
          error={storageError}
          onRetry={() => void refetchStorageConfig()}
          retrying={isRefetchingStorage}
        />
      )
    }

    if (loadingStorage) {
      return <LoadingState message="Loading storage configuration..." />
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

    const pathsPanel = storagePathsError ? (
      <ErrorState
        title="Couldn't load storage paths"
        error={storagePathsError}
        onRetry={() => void refetchStoragePaths()}
        retrying={isRefetchingPaths}
      />
    ) : storagePaths ? (
      <StoragePathsDisplay paths={storagePaths} config={storageConfig.config} onEditConfig={openConfigEditor} />
    ) : (
      <LoadingState message="Loading storage paths..." />
    )

    if (mobileShell) {
      return (
        <Stack gap="lg">
          <IosGroup>
            <IosRow title="Storage backend" value={STORAGE_BACKEND_LABELS[backend]} />
            <IosRow
              title="Credentials"
              value={
                vaultStatus?.password_source === 'keyring'
                  ? secureStoreLabel()
                  : `Encrypted in ${STORAGE_BACKEND_LABELS[backend]}`
              }
            />
            <IosRow title="Cache backend" value={isPostgres ? STORAGE_BACKEND_LABELS.postgres : 'Local filesystem'} />
          </IosGroup>

          {pathsPanel}

          <IosGroup
            footer={
              connectionStatus && (
                <Text span inherit c={connectionStatus === 'success' ? 'green' : 'red'}>
                  {connectionMessage}
                </Text>
              )
            }
          >
            <IosRow
              title="Test connection"
              onClick={testConnection}
              disabled={testingConnection}
              accessory={testingConnection ? <Loader size={18} aria-hidden /> : undefined}
            />
            {canLockVault && (
              <IosRow
                title="Lock vault"
                onClick={() => lockVaultMutation.mutate()}
                disabled={lockVaultMutation.isPending}
                accessory={lockVaultMutation.isPending ? <Loader size={18} aria-hidden /> : undefined}
              />
            )}
          </IosGroup>
        </Stack>
      )
    }

    return (
      <Stack gap="md">
        <Card padding="md">
          <SimpleGrid cols={{ base: 1, sm: 3 }} spacing="lg">
            <Stack gap="3xs">
              <Text size="xs" c="dimmed">
                Storage backend
              </Text>
              <Text size="sm" fw={500}>
                {STORAGE_BACKEND_LABELS[backend]}
              </Text>
            </Stack>

            <Stack gap="3xs">
              <Text size="xs" c="dimmed">
                Credentials
              </Text>
              <Text size="sm" fw={500}>
                {vaultStatus?.password_source === 'keyring'
                  ? 'System keyring'
                  : `Encrypted in ${STORAGE_BACKEND_LABELS[backend]}`}
              </Text>
            </Stack>

            <Stack gap="3xs">
              <Text size="xs" c="dimmed">
                Cache backend
              </Text>
              <Text size="sm" fw={500}>
                {isPostgres ? STORAGE_BACKEND_LABELS.postgres : 'Local filesystem'}
              </Text>
            </Stack>
          </SimpleGrid>
        </Card>

        {pathsPanel}

        <Card padding="md">
          <Stack gap="sm">
            {connectionStatus === 'success' && (
              <Alert icon={<IconCheck size={16} aria-hidden />} color="green" variant="light">
                {connectionMessage}
              </Alert>
            )}

            {connectionStatus === 'error' && (
              <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="red" variant="light">
                {connectionMessage}
              </Alert>
            )}

            <Group grow>
              <Button
                variant="light"
                color="gray"
                leftSection={<IconCloudUpload size={16} aria-hidden />}
                onClick={testConnection}
                loading={testingConnection}
              >
                Test connection
              </Button>
              {!isMobilePlatform() && (
                <Button
                  variant="light"
                  color="gray"
                  leftSection={<IconSettings size={16} aria-hidden />}
                  onClick={() => setSetupWizardOpen(true)}
                >
                  Configure storage
                </Button>
              )}
              {canLockVault && (
                <Button
                  variant="light"
                  color="gray"
                  leftSection={<IconLock size={16} aria-hidden />}
                  onClick={() => lockVaultMutation.mutate()}
                  loading={lockVaultMutation.isPending}
                >
                  Lock vault
                </Button>
              )}
            </Group>

            {isTauri() && !isMobilePlatform() && backend === 'sqlite' && !vaultStatus?.requires_password && (
              <Text size="xs" c="dimmed" ta="center">
                Configure storage to migrate to an encrypted database or PostgreSQL
              </Text>
            )}
          </Stack>
        </Card>
      </Stack>
    )
  }

  return (
    <Box>
      <LiveStatus message={testingConnection ? '' : connectionMessage} />

      {renderContent()}

      {isMobilePlatform() && (
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
