import { Alert, Box, Button, Card, Group, SimpleGrid, Stack, Text, Title } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle, IconCheck, IconCloudUpload, IconLock, IconSettings } from '@tabler/icons-react'
import { useState } from 'react'

import { logger } from '../../../lib/logger'
import {
  useConfigContent,
  useStorageConfig,
  useStoragePaths,
  useTestStorageConnection,
} from '../../../queries/useStorageQueries'
import { useLockVault, useVaultStatus } from '../../../queries/useVaultQueries'
import { isTauri } from '../../../services'
import { lockMeansSomething } from '../../../utils/appLock'
import { isMobilePlatform } from '../../../utils/platform'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { SetupWizard } from '../../setup/SetupWizard'
import { AppLockCard } from '../AppLockCard'
import { ConfigEditorModal } from '../ConfigEditorModal'
import { StoragePathsDisplay } from '../StoragePathsDisplay'
import { STORAGE_BACKEND_LABELS } from '../storageLabels'

export const StorageSection = () => {
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
  const { refetch: loadConfigContent } = useConfigContent()
  const testConnectionMutation = useTestStorageConnection()

  const { data: vaultStatus } = useVaultStatus()
  const lockVaultMutation = useLockVault()
  const canLockVault = vaultStatus && lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked

  const handleLockVault = () => lockVaultMutation.mutate()

  const [setupWizardOpen, setSetupWizardOpen] = useState(false)
  const [configEditorOpen, setConfigEditorOpen] = useState(false)
  const [configContent, setConfigContent] = useState<string>('')
  const [connectionStatus, setConnectionStatus] = useState<'success' | 'error' | null>(null)
  const [connectionMessage, setConnectionMessage] = useState<string>('')

  const handleSetupComplete = () => {
    setSetupWizardOpen(false)
    refetchStoragePaths()
    refetchStorageConfig()
  }

  const handleSetupClose = () => {
    setSetupWizardOpen(false)
  }

  const openConfigEditor = async () => {
    try {
      const result = await loadConfigContent()

      if (result.data) {
        setConfigContent(result.data.content)
        setConfigEditorOpen(true)
      } else {
        throw result.error ?? new Error('The config file returned no content')
      }
    } catch (error) {
      logger.error('StorageSection', 'Failed to load config content', error)
      notifications.show({
        title: "Couldn't open config file",
        message: error instanceof Error ? error.message : 'Failed to load the config file',
        color: 'red',
      })
    }
  }

  const handleConfigSaved = () => {
    refetchStorageConfig()
    refetchStoragePaths()
  }

  const testConnection = async () => {
    if (!storageConfig) {
      return
    }

    setConnectionStatus(null)

    try {
      const result = await testConnectionMutation.mutateAsync(storageConfig.config)

      setConnectionStatus(result.success ? 'success' : 'error')
      setConnectionMessage(result.message || (result.success ? 'Connection successful' : 'Connection failed'))
    } catch (error) {
      setConnectionStatus('error')
      setConnectionMessage(error instanceof Error ? error.message : 'Failed to test connection')
    }
  }

  const backend = storageConfig?.config.storage.backend
  const isPostgres = backend === 'postgres'

  const renderContent = () => {
    if (isStorageError) {
      return (
        <ErrorState
          title="Couldn't load storage configuration"
          error={storageError}
          onRetry={() => refetchStorageConfig()}
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
          icon={<IconSettings size={24} />}
          action={<Button onClick={() => setSetupWizardOpen(true)}>Configure storage</Button>}
        />
      )
    }

    return (
      <Stack gap="md">
        <Card padding="md">
          <SimpleGrid cols={{ base: 1, sm: 3 }} spacing="lg">
            <Stack gap={4}>
              <Text size="xs" c="dimmed">
                Storage backend
              </Text>
              <Text size="sm" fw={500}>
                {STORAGE_BACKEND_LABELS[backend]}
              </Text>
            </Stack>

            <Stack gap={4}>
              <Text size="xs" c="dimmed">
                Credentials
              </Text>
              <Text size="sm" fw={500}>
                {vaultStatus?.password_source === 'keyring'
                  ? 'System keyring'
                  : `Encrypted in ${STORAGE_BACKEND_LABELS[backend]}`}
              </Text>
            </Stack>

            <Stack gap={4}>
              <Text size="xs" c="dimmed">
                Cache backend
              </Text>
              <Text size="sm" fw={500}>
                {isPostgres ? STORAGE_BACKEND_LABELS.postgres : 'Local filesystem'}
              </Text>
            </Stack>
          </SimpleGrid>
        </Card>

        {storagePathsError ? (
          <ErrorState
            title="Couldn't load storage paths"
            error={storagePathsError}
            onRetry={() => refetchStoragePaths()}
            retrying={isRefetchingPaths}
          />
        ) : storagePaths ? (
          <StoragePathsDisplay paths={storagePaths} config={storageConfig.config} onEditConfig={openConfigEditor} />
        ) : (
          <LoadingState message="Loading storage paths..." />
        )}

        <Card padding="md">
          <Stack gap="sm">
            {connectionStatus === 'success' && (
              <Alert icon={<IconCheck size={16} />} color="green" variant="light">
                {connectionMessage}
              </Alert>
            )}

            {connectionStatus === 'error' && (
              <Alert icon={<IconAlertCircle size={16} />} color="red" variant="light">
                {connectionMessage}
              </Alert>
            )}

            <Group grow>
              <Button
                variant="light"
                color="gray"
                leftSection={<IconCloudUpload size={16} />}
                onClick={testConnection}
                loading={testConnectionMutation.isPending}
              >
                Test connection
              </Button>
              {!isMobilePlatform() && (
                <Button
                  variant="light"
                  color="gray"
                  leftSection={<IconSettings size={16} />}
                  onClick={() => setSetupWizardOpen(true)}
                >
                  Configure storage
                </Button>
              )}
              {canLockVault && (
                <Button
                  variant="light"
                  color="gray"
                  leftSection={<IconLock size={16} />}
                  onClick={handleLockVault}
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
      <Title order={2} size="lg" fw={600} mb="lg">
        Storage
      </Title>

      {renderContent()}

      {isMobilePlatform() && (
        <Box mt="md">
          <AppLockCard />
        </Box>
      )}

      <SetupWizard opened={setupWizardOpen} onComplete={handleSetupComplete} onClose={handleSetupClose} />

      <ConfigEditorModal
        opened={configEditorOpen}
        onClose={() => setConfigEditorOpen(false)}
        initialContent={configContent}
        onSaved={handleConfigSaved}
      />
    </Box>
  )
}
