import { ActionIcon, Group, Menu, Stack, Text, Tooltip } from '@mantine/core'
import { IconDots, IconSend, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosRow } from '../ios/IosList'

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
        accessory={
          <Menu position="bottom-end">
            <Menu.Target>
              <ActionIcon data-row-menu aria-label={`Actions for ${title}`} loading={testing || removing}>
                <IconDots size={20} aria-hidden />
              </ActionIcon>
            </Menu.Target>
            <Menu.Dropdown>
              <Menu.Label>{title}</Menu.Label>
              <Menu.Item leftSection={<IconSend size={18} aria-hidden />} onClick={onTest}>
                Send test push
              </Menu.Item>
              <Menu.Item color="red" leftSection={<IconTrash size={18} aria-hidden />} onClick={onRemove}>
                Remove
              </Menu.Item>
              <ActionSheetCancel />
            </Menu.Dropdown>
          </Menu>
        }
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
