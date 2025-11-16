import React from 'react'

import {
  Alert,
  Badge,
  Box,
  Code,
  Divider,
  Group,
  Loader,
  Modal,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import {
  IconAlertCircle,
  IconCheck,
  IconX,
} from '@tabler/icons-react'

import type { FeatureAvailability, PermissionStatus, PluginMetadata } from '../../types'

// Helper function to format permission descriptions
// Converts text like: "Some text 'repo' more text 'workflow' end"
// Into: ["Some text ", <Code>repo</Code>, " more text ", <Code>workflow</Code>, " end"]
const formatPermissionDescription = (description: string) => {
  const parts: (string | React.ReactNode)[] = []
  const regex = /'([^']+)'/g
  let lastIndex = 0
  let match

  while ((match = regex.exec(description)) !== null) {
    // Add text before the match
    if (match.index > lastIndex) {
      parts.push(description.substring(lastIndex, match.index))
    }
    // Add the matched text wrapped in Code component
    parts.push(
      <Code key={`code-${match.index}`} fz="xs">
        {match[1]}
      </Code>
    )
    lastIndex = regex.lastIndex
  }

  // Add remaining text after last match
  if (lastIndex < description.length) {
    parts.push(description.substring(lastIndex))
  }

  return parts.length > 0 ? parts : [description]
}

interface PermissionCheckModalProps {
  opened: boolean
  onClose: () => void
  metadata: PluginMetadata | null
  status: PermissionStatus | null
  features: FeatureAvailability[]
  loading: boolean
  error: string | null
}

export const PermissionCheckModal = ({
  opened,
  onClose,
  metadata: _metadata,
  status,
  features,
  loading,
  error,
}: PermissionCheckModalProps) => {
  const modalTitle = (
    <Group justify="space-between" w="100%">
      <Text fw={600}>Token Permissions</Text>
      {status?.metadata?.token_type && (
        <Badge size="md" variant="light" color="blue">
          {status.metadata.token_type === 'classic_pat' ? 'CLASSIC PAT' : 'FINE-GRAINED TOKEN'}
        </Badge>
      )}
    </Group>
  )

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={modalTitle}
      size="lg"
      yOffset="10vh"
      styles={{
        body: {
          maxHeight: '60vh',
          overflowY: 'auto',
        },
      }}
    >
      {loading ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
          <Text size="sm" c="dimmed">
            Checking permissions...
          </Text>
        </Group>
      ) : error ? (
        <Alert icon={<IconAlertCircle size={16} />} color="red" title="Error">
          {error}
        </Alert>
      ) : status ? (
        <Stack gap="md">
          {/* Permission List */}
          <Stack gap="sm">
            {status.permissions.map((check) => {
              // Get features that require this permission
              const featuresForPermission = features.filter((f) =>
                f.feature.required_permissions.includes(check.permission.name)
              )

              return (
                <Box
                  key={check.permission.name}
                  p="md"
                  style={{
                    border: '1px solid var(--mantine-color-dark-5)',
                    borderRadius: '8px',
                    backgroundColor: 'var(--mantine-color-dark-8)',
                  }}
                >
                  <Stack gap="sm">
                    {/* Permission Header */}
                    <Group justify="space-between" align="flex-start">
                      <Group gap="xs">
                        <Tooltip
                          label={check.granted ? 'Permission granted' : 'Permission not granted'}
                          withArrow
                        >
                          <ThemeIcon
                            size="sm"
                            radius="xl"
                            variant="light"
                            color={check.granted ? 'green' : check.permission.required ? 'red' : 'yellow'}
                          >
                            {check.granted ? <IconCheck size={14} /> : <IconX size={14} />}
                          </ThemeIcon>
                        </Tooltip>
                        <Code fz="sm">{check.permission.name}</Code>
                      </Group>
                      <Badge
                        size="xs"
                        variant="light"
                        color="blue"
                      >
                        {check.permission.required ? 'Required' : 'Optional'}
                      </Badge>
                    </Group>

                    {/* Permission Description */}
                    <Text size="xs" c="dimmed">
                      {formatPermissionDescription(check.permission.description)}
                    </Text>

                    {/* Divider before features */}
                    {featuresForPermission.length > 0 && <Divider />}

                    {/* Feature List */}
                    {featuresForPermission.length > 0 && (
                      <Box>
                        <Text size="xs" fw={500} mb={4} c="dimmed">
                          {check.granted ? 'With this you can:' : 'Missing this means you can\'t:'}
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
                </Box>
              )
            })}
          </Stack>

        </Stack>
      ) : !loading && !error ? (
        <Alert icon={<IconAlertCircle size={16} />} color="blue" variant="light">
          <Stack gap="xs">
            <Text size="sm" fw={500}>
              Permission Check Unavailable
            </Text>
            <Text size="xs">
              Unable to check token permissions. This could be due to network connectivity issues or
              GitHub API rate limits. Your token credentials are valid, but permission details cannot
              be verified at this time.
            </Text>
            <Text size="xs" fw={500}>
              You can still add this provider and it should work normally if your token has the
              required permissions.
            </Text>
          </Stack>
        </Alert>
      ) : null}
    </Modal>
  )
}
