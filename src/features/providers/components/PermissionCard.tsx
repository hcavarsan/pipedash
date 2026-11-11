import { Box, Card, Code, Divider, Group, Stack, Text, ThemeIcon, Tooltip } from '@mantine/core'
import { IconCheck, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import type { FeatureAvailability, PermissionCheck } from '../../../types/plugins'

export const formatPermissionDescription = (description: string): ReactNode[] => {
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

function FeatureRow({ name }: { name: string }): ReactNode {
  return (
    <Group gap="2xs" wrap="nowrap">
      <Text size="xs" c="dimmed">
        •
      </Text>
      <Text size="xs" c="dimmed">
        {name}
      </Text>
    </Group>
  )
}

function FeatureList({ granted, features }: { granted: boolean; features: FeatureAvailability[] }): ReactNode {
  return (
    <>
      <Divider />
      <Box>
        <Text size="xs" fw={500} mb="3xs" c="dimmed">
          {granted ? 'With this you can:' : "Missing this means you can't:"}
        </Text>
        <Stack gap="3xs">
          {features.map((f) => (
            <FeatureRow key={f.feature.id} name={f.feature.name} />
          ))}
        </Stack>
      </Box>
    </>
  )
}

function PermissionHeader({ check }: { check: PermissionCheck }): ReactNode {
  const label = check.granted ? 'Permission granted' : 'Permission not granted'

  return (
    <Group justify="space-between" align="flex-start">
      <Group gap="xs">
        <Tooltip label={label}>
          <ThemeIcon size="sm" variant="transparent" color={check.granted ? 'green' : 'red'} aria-label={label}>
            {check.granted ? <IconCheck size={14} aria-hidden /> : <IconX size={14} aria-hidden />}
          </ThemeIcon>
        </Tooltip>
        <Code fz="sm">{check.permission.name}</Code>
      </Group>
      <Text size="sm" c="dimmed">
        {check.permission.required ? 'Required' : 'Optional'}
      </Text>
    </Group>
  )
}

export function PermissionCard({
  check,
  features,
}: {
  check: PermissionCheck
  features: FeatureAvailability[]
}): ReactNode {
  const forPermission = features.filter((f) => f.feature.required_permissions.includes(check.permission.name))

  return (
    <Card p="md">
      <Stack gap="sm">
        <PermissionHeader check={check} />

        <Text size="xs" c="dimmed">
          {formatPermissionDescription(check.permission.description)}
        </Text>

        {forPermission.length > 0 && <FeatureList granted={check.granted} features={forPermission} />}
      </Stack>
    </Card>
  )
}
