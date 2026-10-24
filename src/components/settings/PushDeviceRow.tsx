import { ActionIcon, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconSend, IconTrash } from '@tabler/icons-react'

interface PushDeviceRowProps {
  title: string
  subtitle: string
  testLabel: string
  removeLabel: string
  testing: boolean
  removing: boolean
  onTest: () => void
  onRemove: () => void
}

export const PushDeviceRow = ({
  title,
  subtitle,
  testLabel,
  removeLabel,
  testing,
  removing,
  onTest,
  onRemove,
}: PushDeviceRowProps) => (
  <Group justify="space-between" wrap="nowrap" gap="sm">
    <Stack gap={0} miw={0}>
      <Text size="sm" fw={500} truncate>
        {title}
      </Text>
      <Text size="xs" c="dimmed" truncate>
        {subtitle}
      </Text>
    </Stack>

    <Group gap={4} wrap="nowrap" style={{ flexShrink: 0 }}>
      <Tooltip label={testLabel}>
        <ActionIcon variant="subtle" color="gray" size="lg" aria-label={testLabel} loading={testing} onClick={onTest}>
          <IconSend size={16} />
        </ActionIcon>
      </Tooltip>
      <Tooltip label={removeLabel}>
        <ActionIcon
          variant="subtle"
          color="gray"
          size="lg"
          aria-label={removeLabel}
          loading={removing}
          onClick={onRemove}
        >
          <IconTrash size={16} />
        </ActionIcon>
      </Tooltip>
    </Group>
  </Group>
)
