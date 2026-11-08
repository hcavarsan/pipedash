import { Alert, Badge, Button, Group, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { FeatureAvailability, PermissionStatus, PluginMetadata } from '../../../types/plugins'

import { PermissionCard } from './PermissionCard'
import { ShellPermissions } from './ShellPermissions'

function permissionStatusMessage(loading: boolean, error: string | null, status: PermissionStatus | null): string {
  if (loading) {
    return 'Checking permissions'
  }

  if (error) {
    return 'Permission check failed'
  }

  if (!status) {
    return ''
  }

  const grantedCount = status.permissions.filter((check) => check.granted).length

  return `${grantedCount} of ${status.permissions.length} permissions granted`
}

function PermissionsUnavailable({ onRetry }: { onRetry: () => void }): ReactNode {
  return (
    <Alert
      icon={<IconAlertCircle size={16} aria-hidden />}
      color="gray"
      variant="light"
      title="Permission details unavailable"
    >
      <Stack gap="sm" align="flex-start">
        <Text size="sm">Permission details could not be retrieved. These credentials have not been verified.</Text>
        <Button size="xs" variant="default" leftSection={<IconRefresh size={14} aria-hidden />} onClick={onRetry}>
          Retry
        </Button>
      </Stack>
    </Alert>
  )
}

interface PermissionCheckModalProps {
  opened: boolean
  onClose: () => void
  onRetry: () => void
  metadata: PluginMetadata | null
  status: PermissionStatus | null
  features: FeatureAvailability[]
  loading: boolean
  error: string | null
}

export const PermissionCheckModal = ({
  opened,
  onClose,
  onRetry,
  metadata,
  status,
  features,
  loading,
  error,
}: PermissionCheckModalProps): ReactNode => {
  const mobileShell = useMobileShell()
  const shellTitle = metadata?.presentation.wizard.permissions_title ?? ''
  const modalTitle = (
    <Group justify="space-between" w="100%">
      <Text fw={600}>{shellTitle}</Text>
      {status?.credential_label && (
        <Badge size="md" variant="light" color="gray">
          {status.credential_label}
        </Badge>
      )}
    </Group>
  )

  const renderBody = (): ReactNode => {
    if (loading) {
      return <LoadingState variant="section" message="Checking permissions..." />
    }

    if (error) {
      return <ErrorState title="Couldn't check permissions" error={error} onRetry={onRetry} />
    }

    if (!status) {
      return <PermissionsUnavailable onRetry={onRetry} />
    }

    if (mobileShell) {
      return <ShellPermissions status={status} features={features} />
    }

    return (
      <Stack gap="sm">
        {status.permissions.map((check) => (
          <PermissionCard key={check.permission.name} check={check} features={features} />
        ))}
      </Stack>
    )
  }

  return (
    <StandardModal opened={opened} onClose={onClose} title={mobileShell ? shellTitle : modalTitle} size="md">
      <LiveStatus message={permissionStatusMessage(loading, error, status)} />
      <ErrorBoundary scope="feature" name="Provider permissions">
        {renderBody()}
      </ErrorBoundary>
    </StandardModal>
  )
}
