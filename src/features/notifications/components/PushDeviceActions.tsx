import { ActionIcon, Menu } from '@mantine/core'
import { IconDots, IconSend, IconTrash } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'

interface PushDeviceActionsProps {
  title: string
  busy: boolean
  onTest: () => void
  onRemove: () => void
}

export const PushDeviceActions = ({ title, busy, onTest, onRemove }: PushDeviceActionsProps): ReactNode => (
  <Menu position="bottom-end">
    <Menu.Target>
      <ActionIcon data-row-menu aria-label={`Actions for ${title}`} loading={busy}>
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
)
