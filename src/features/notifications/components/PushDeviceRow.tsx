import { ActionIcon, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconSend, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { PushDeviceActions } from './PushDeviceActions'

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
}: PushDeviceRowProps): ReactNode => {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <IosRow
        title={title}
        subtitle={subtitle}
        longPressMenu
        accessory={<PushDeviceActions title={title} busy={testing || removing} onTest={onTest} onRemove={onRemove} />}
      />
    )
  }

  return (
    <Group justify="space-between" wrap="nowrap" gap="sm">
      <Stack gap={0} miw={0}>
        <Text size="sm" fw={500} truncate>
          {title}
        </Text>
        <Text size="xs" c="dimmed" truncate>
          {subtitle}
        </Text>
      </Stack>

      <Group gap="3xs" wrap="nowrap" style={{ flexShrink: 0 }}>
        <Tooltip label={testLabel}>
          <ActionIcon size="lg" aria-label={testLabel} loading={testing} onClick={onTest}>
            <IconSend size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
        <Tooltip label={removeLabel}>
          <ActionIcon size="lg" aria-label={removeLabel} loading={removing} onClick={onRemove}>
            <IconTrash size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      </Group>
    </Group>
  )
}
