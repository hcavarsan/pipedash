import { useState } from 'react'

import {
  Alert,
  Box,
  Button,
  Card,
  Group,
  Loader,
  SimpleGrid,
  Stack,
  Text,
} from '@mantine/core'
import {
  IconAlertCircle,
  IconCheck,
  IconCloudUpload,
  IconLock,
  IconSettings,
} from '@tabler/icons-react'

import {
  useConfigContent,
  useStorageConfig,
  useStoragePaths,
  useTestStorageConnection,
} from '../../../queries/useStorageQueries'
import { useLockVault, useVaultStatus } from '../../../queries/useVaultQueries'
import { isTauri } from '../../../services'
import { clearApiToken } from '../../../services/auth'
import { ErrorFallback } from '../../ErrorBoundary/ErrorFallback'
import { SetupWizard } from '../../setup/SetupWizard'
import { ConfigEditorModal } from '../ConfigEditorModal'
import { StoragePathsDisplay } from '../StoragePathsDisplay'

export const StorageSection = () => {
  const {
    data: storageConfig,
    isLoading: loadingStorage,
    isError: isStorageError,
    error: storageError,
    refetch: refetchStorageConfig,
  } = useStorageConfig()
  const { data: storagePaths, refetch: refetchStoragePaths } = useStoragePaths()
  const { refetch: loadConfigContent } = useConfigContent()
  const testConnectionMutation = useTestStorageConnection()

  const { data: vaultStatus } = useVaultStatus()
  const lockVaultMutation = useLockVault()
  const canLockVault = vaultStatus?.requires_password && vaultStatus?.is_unlocked

  const handleLockVault = async () => {
    try {
      await lockVaultMutation.mutateAsync()
      clearApiToken()
      window.location.reload()
    } catch (error) {
      console.error('Failed to lock vault:', error)
    }
  }

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
      }
    } catch (error) {
      console.error('Failed to load config content:', error)
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
      setConnectionMessage(
        result.message || (result.success ? 'Connection successful' : 'Connection failed')
      )
    } catch (error) {
      setConnectionStatus('error')
      setConnectionMessage(error instanceof Error ? error.message : 'Failed to test connection')
    }
  }

  const getBackendLabel = (type: string): string => {
    const labels: Record<string, string> = {
      Sqlite: 'SQLite Encrypted',
      keyring: 'System Keyring',
      env: 'Environment Variables',
      memory: 'Memory (Testing)',
      Postgres: 'PostgreSQL',
      sqlite: 'SQLite',
      postgres: 'PostgreSQL',
      local: 'Local Filesystem',
      hybrid: 'Hybrid (Local + Remote)',
    }

    return labels[type] || type
  }

  if (isStorageError) {
    return (
      <Box>
        <Text size="lg" fw={600} mb="lg">
          Storage Configuration
        </Text>
        <ErrorFallback
          error={storageError as Error}
          resetError={() => refetchStorageConfig()}
          title="Failed to load storage configuration"
        />
      </Box>
    )
  }

  if (loadingStorage) {
    return (
      <Box>
        <Text size="lg" fw={600} mb="lg">
          Storage Configuration
        </Text>
        <Stack align="center" py="xl">
          <Loader size="sm" />
        </Stack>
      </Box>
    )
  }

  if (!storageConfig) {
    return (
      <Box>
        <Text size="lg" fw={600} mb="lg">
          Storage Configuration
        </Text>
        <Alert icon={<IconAlertCircle size={16} />} color="red">
          Unable to load storage configuration
        </Alert>
      </Box>
    )
  }

  return (
    <Box>
      <Text size="lg" fw={600} mb="lg">
        Storage Configuration
      </Text>

      <Stack gap="md">
        <Card withBorder padding="md" radius="md">
          <SimpleGrid cols={{ base: 1, sm: 3 }} spacing="lg">
            <Stack gap={4}>
              <Text size="xs" c="dimmed">Storage Backend</Text>
              <Text size="sm" fw={500}>
                {getBackendLabel(storageConfig.config.storage.backend)}
              </Text>
            </Stack>

            <Stack gap={4}>
              <Text size="xs" c="dimmed">Credentials</Text>
              <Text size="sm" fw={500}>
                {storageConfig.config.storage.backend === 'postgres'
                  ? 'Encrypted (Postgres)'
                  : vaultStatus?.password_source === 'keyring'
                    ? 'System Keyring'
                    : 'Encrypted (SQLite)'}
              </Text>
            </Stack>

            <Stack gap={4}>
              <Text size="xs" c="dimmed">Cache Backend</Text>
              <Text size="sm" fw={500}>
                {storageConfig.config.storage.backend === 'postgres'
                  ? 'PostgreSQL'
                  : 'Local Filesystem'}
              </Text>
            </Stack>
          </SimpleGrid>
        </Card>

        {storagePaths && (
          <StoragePathsDisplay
            paths={storagePaths}
            config={storageConfig.config}
            onEditConfig={openConfigEditor}
          />
        )}

        <Card withBorder padding="md" radius="md">
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
                Test Connection
              </Button>
              <Button
                variant="light"
                color="blue"
                leftSection={<IconSettings size={16} />}
                onClick={() => setSetupWizardOpen(true)}
              >
                Configure Storage
              </Button>
              {canLockVault && (
                <Button
                  variant="light"
                  color="orange"
                  leftSection={<IconLock size={16} />}
                  onClick={handleLockVault}
                  loading={lockVaultMutation.isPending}
                >
                  Lock Vault
                </Button>
              )}
            </Group>

            {isTauri() && storageConfig.config.storage.backend === 'sqlite' && !vaultStatus?.requires_password && (
              <Text size="xs" c="dimmed" ta="center">
                Configure Storage to migrate to encrypted database or PostgreSQL
              </Text>
            )}
          </Stack>
        </Card>
      </Stack>

      <SetupWizard
        opened={setupWizardOpen}
        onComplete={handleSetupComplete}
        onClose={handleSetupClose}
      />

      <ConfigEditorModal
        opened={configEditorOpen}
        onClose={() => setConfigEditorOpen(false)}
        initialContent={configContent}
        onSaved={handleConfigSaved}
      />
    </Box>
  )
}
