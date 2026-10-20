import { Alert, Badge, Box, Button, Card, Code, Divider, Group, Stack, Text, ThemeIcon, Tooltip } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconRefresh, IconX } from '@tabler/icons-react'
import type React from 'react'

import type { FeatureAvailability, PermissionStatus, PluginMetadata } from '../../types'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'

const formatPermissionDescription = (description: string) => {
  const parts: (string | React.ReactNode)[] = []
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
  metadata: _metadata,
  status,
  features,
  loading,
  error,
}: PermissionCheckModalProps) => {
  const modalTitle = (
    <Group justify="space-between" w="100%">
      <Text fw={600}>Token permissions</Text>
      {status?.metadata?.token_type && (
        <Badge size="md" variant="light" color="gray">
          {status.metadata.token_type === 'classic_pat' ? 'Classic PAT' : 'Fine-grained token'}
        </Badge>
      )}
    </Group>
  )

  return (
    <StandardModal opened={opened} onClose={onClose} title={modalTitle} size="md">
      {loading ? (
        <LoadingState variant="section" message="Checking permissions..." />
      ) : error ? (
        <ErrorState title="Couldn't check permissions" error={error} onRetry={onRetry} />
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
                          {check.granted ? <IconCheck size={14} /> : <IconX size={14} />}
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
                      <Text size="xs" fw={500} mb={4} c="dimmed">
                        {check.granted ? 'With this you can:' : "Missing this means you can't:"}
                      </Text>
                      <Stack gap={4}>
                        {featuresForPermission.map((f) => (
                          <Group key={f.feature.id} gap={6} wrap="nowrap">
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
        <Alert icon={<IconAlertCircle size={16} />} color="gray" variant="light" title="Permission details unavailable">
          <Stack gap="sm" align="flex-start">
            <Text size="sm">
              Permission details could not be retrieved, so the token's permissions are not verified.
            </Text>
            <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} onClick={onRetry}>
              Retry
            </Button>
          </Stack>
        </Alert>
      )}
    </StandardModal>
  )
}
