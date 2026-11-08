import { Alert, Button, Card, Group, Loader, SimpleGrid, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconCloudUpload, IconLock, IconSettings } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'

interface StorageOverviewProps {
  mobile: boolean
  backendLabel: string
  credentialsLabel: string
  cacheLabel: string
  pathsPanel: ReactNode
  connectionStatus: 'success' | 'error' | null
  connectionMessage: string
  testingConnection: boolean
  onTestConnection: () => void
  canLockVault: boolean
  lockPending: boolean
  onLockVault: () => void
  showConfigure: boolean
  onConfigure: () => void
  showMigrateHint: boolean
}

const StorageStat = ({ label, value }: { label: string; value: string }): ReactNode => (
  <Stack gap="3xs">
    <Text size="xs" c="dimmed">
      {label}
    </Text>
    <Text size="sm" fw={500}>
      {value}
    </Text>
  </Stack>
)

const MobileStorageOverview = (p: StorageOverviewProps): ReactNode => {
  const connectionFooter = p.connectionStatus && (
    <Text span inherit c={p.connectionStatus === 'success' ? 'green' : 'red'}>
      {p.connectionMessage}
    </Text>
  )

  return (
    <Stack gap="lg">
      <IosGroup>
        <IosRow title="Storage backend" value={p.backendLabel} />
        <IosRow title="Credentials" value={p.credentialsLabel} />
        <IosRow title="Cache backend" value={p.cacheLabel} />
      </IosGroup>

      {p.pathsPanel}

      <IosGroup footer={connectionFooter}>
        <IosRow
          title="Test connection"
          onClick={p.onTestConnection}
          disabled={p.testingConnection}
          accessory={p.testingConnection ? <Loader size={18} aria-hidden /> : undefined}
        />
        {p.canLockVault && (
          <IosRow
            title="Lock vault"
            onClick={p.onLockVault}
            disabled={p.lockPending}
            accessory={p.lockPending ? <Loader size={18} aria-hidden /> : undefined}
          />
        )}
      </IosGroup>
    </Stack>
  )
}

const DesktopStorageOverview = (p: StorageOverviewProps): ReactNode => (
  <Stack gap="md">
    <Card padding="md">
      <SimpleGrid cols={{ base: 1, sm: 3 }} spacing="lg">
        <StorageStat label="Storage backend" value={p.backendLabel} />
        <StorageStat label="Credentials" value={p.credentialsLabel} />
        <StorageStat label="Cache backend" value={p.cacheLabel} />
      </SimpleGrid>
    </Card>

    {p.pathsPanel}

    <Card padding="md">
      <Stack gap="sm">
        {p.connectionStatus === 'success' && (
          <Alert icon={<IconCheck size={16} aria-hidden />} color="green" variant="light">
            {p.connectionMessage}
          </Alert>
        )}

        {p.connectionStatus === 'error' && (
          <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="red" variant="light">
            {p.connectionMessage}
          </Alert>
        )}

        <Group grow>
          <Button
            variant="light"
            color="gray"
            leftSection={<IconCloudUpload size={16} aria-hidden />}
            onClick={p.onTestConnection}
            loading={p.testingConnection}
          >
            Test connection
          </Button>
          {p.showConfigure && (
            <Button
              variant="light"
              color="gray"
              leftSection={<IconSettings size={16} aria-hidden />}
              onClick={p.onConfigure}
            >
              Configure storage
            </Button>
          )}
          {p.canLockVault && (
            <Button
              variant="light"
              color="gray"
              leftSection={<IconLock size={16} aria-hidden />}
              onClick={p.onLockVault}
              loading={p.lockPending}
            >
              Lock vault
            </Button>
          )}
        </Group>

        {p.showMigrateHint && (
          <Text size="xs" c="dimmed" ta="center">
            Configure storage to migrate to an encrypted database or PostgreSQL
          </Text>
        )}
      </Stack>
    </Card>
  </Stack>
)

export const StorageOverview = (p: StorageOverviewProps): ReactNode =>
  p.mobile ? <MobileStorageOverview {...p} /> : <DesktopStorageOverview {...p} />
