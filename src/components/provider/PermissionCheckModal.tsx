import { Alert, Badge, Box, Button, Card, Code, Divider, Group, Stack, Text, ThemeIcon, Tooltip } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconRefresh, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { FeatureAvailability, PermissionStatus, PluginMetadata } from '../../types'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import { LiveStatus } from '../forms/LiveStatus'
import { IosGroup, IosRow } from '../ios/IosList'

import classes from './AddProviderModal/ShellForm.module.css'

const formatPermissionDescription = (description: string) => {
  const parts: ReactNode[] = []
  let lastIndex = 0

  for (const match of description.matchAll(/'([^']+)'/g)) {
    if (match.index > lastIndex) {
      parts.push(description.substring(lastIndex, match.index))
    }
    parts.push(
      <Code key={`code-${match.index}`} fz="xs">
        {match[1]}
      </Code>
    )
    lastIndex = match.index + match[0].length
  }

  if (lastIndex < description.length) {
    parts.push(description.substring(lastIndex))
  }

  return parts.length > 0 ? parts : [description]
}

function ShellPermissions({ status, features }: { status: PermissionStatus; features: FeatureAvailability[] }) {
  const tokenType = status.metadata?.token_type

  return (
    <Stack gap="lg">
      {tokenType && (
        <IosGroup>
          <IosRow title="Token type" value={tokenType === 'classic_pat' ? 'Classic PAT' : 'Fine-grained token'} />
        </IosGroup>
      )}
      <IosGroup>
        {status.permissions.map((check) => {
          const featureNames = features
            .filter((f) => f.feature.required_permissions.includes(check.permission.name))
            .map((f) => f.feature.name)

          return (
            <IosRow
              key={check.permission.name}
              title={<span className={classes.permissionName}>{check.permission.name}</span>}
              subtitle={
                <>
                  <span className={`${classes.line} ${check.granted ? classes.granted : classes.missing}`}>
                    {check.granted ? <IconCheck size={14} aria-hidden /> : <IconX size={14} aria-hidden />}
                    {check.granted ? 'Granted' : 'Missing'} · {check.permission.required ? 'Required' : 'Optional'}
                  </span>
                  <span className={classes.line}>{formatPermissionDescription(check.permission.description)}</span>
                  {featureNames.length > 0 && (
                    <span className={classes.line}>
                      {check.granted ? 'With this you can: ' : "Missing this means you can't: "}
                      {featureNames.join(', ')}
                    </span>
                  )}
                </>
              }
            />
          )
        })}
      </IosGroup>
    </Stack>
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
  const grantedCount = status?.permissions.filter((check) => check.granted).length ?? 0
  const statusMessage = loading
    ? 'Checking permissions'
    : error
      ? 'Permission check failed'
      : status
        ? `${grantedCount} of ${status.permissions.length} permissions granted`
        : ''
  const shellTitle = metadata?.provider_type === 'fluxcd' ? 'Kubernetes permissions' : 'Token permissions'
  const modalTitle = (
    <Group justify="space-between" w="100%">
      <Text fw={600}>{metadata?.provider_type === 'fluxcd' ? 'Kubernetes permissions' : 'Token permissions'}</Text>
      {status?.metadata?.token_type && (
        <Badge size="md" variant="light" color="gray">
          {status.metadata.token_type === 'classic_pat' ? 'Classic PAT' : 'Fine-grained token'}
        </Badge>
      )}
    </Group>
  )

  return (
    <StandardModal opened={opened} onClose={onClose} title={mobileShell ? shellTitle : modalTitle} size="md">
      <LiveStatus message={statusMessage} />
      <ErrorBoundary scope="feature" name="Provider permissions">
        {loading ? (
          <LoadingState variant="section" message="Checking permissions..." />
        ) : error ? (
          <ErrorState title="Couldn't check permissions" error={error} onRetry={onRetry} />
        ) : status && mobileShell ? (
          <ShellPermissions status={status} features={features} />
        ) : status ? (
          <Stack gap="sm">
            {status.permissions.map((check) => {
              const featuresForPermission = features.filter((f) =>
                f.feature.required_permissions.includes(check.permission.name)
              )

              return (
                <Card key={check.permission.name} p="md">
                  <Stack gap="sm">
                    <Group justify="space-between" align="flex-start">
                      <Group gap="xs">
                        <Tooltip label={check.granted ? 'Permission granted' : 'Permission not granted'}>
                          <ThemeIcon
                            size="sm"
                            variant="transparent"
                            color={check.granted ? 'green' : 'red'}
                            aria-label={check.granted ? 'Permission granted' : 'Permission not granted'}
                          >
                            {check.granted ? <IconCheck size={14} aria-hidden /> : <IconX size={14} aria-hidden />}
                          </ThemeIcon>
                        </Tooltip>
                        <Code fz="sm">{check.permission.name}</Code>
                      </Group>
                      <Badge size="xs" variant="light" color="gray">
                        {check.permission.required ? 'Required' : 'Optional'}
                      </Badge>
                    </Group>

                    <Text size="xs" c="dimmed">
                      {formatPermissionDescription(check.permission.description)}
                    </Text>

                    {featuresForPermission.length > 0 && <Divider />}

                    {featuresForPermission.length > 0 && (
                      <Box>
                        <Text size="xs" fw={500} mb="3xs" c="dimmed">
                          {check.granted ? 'With this you can:' : "Missing this means you can't:"}
                        </Text>
                        <Stack gap="3xs">
                          {featuresForPermission.map((f) => (
                            <Group key={f.feature.id} gap="2xs" wrap="nowrap">
                              <Text size="xs" c="dimmed">
                                •
                              </Text>
                              <Text size="xs" c="dimmed">
                                {f.feature.name}
                              </Text>
                            </Group>
                          ))}
                        </Stack>
                      </Box>
                    )}
                  </Stack>
                </Card>
              )
            })}
          </Stack>
        ) : (
          <Alert
            icon={<IconAlertCircle size={16} aria-hidden />}
            color="gray"
            variant="light"
            title="Permission details unavailable"
          >
            <Stack gap="sm" align="flex-start">
              <Text size="sm">
                Permission details could not be retrieved. These credentials have not been verified.
              </Text>
              <Button size="xs" variant="default" leftSection={<IconRefresh size={14} aria-hidden />} onClick={onRetry}>
                Retry
              </Button>
            </Stack>
          </Alert>
        )}
      </ErrorBoundary>
    </StandardModal>
  )
}
