import { Button, Menu } from '@mantine/core'
import { IconBell, IconBellOff, IconBellPlus, IconBellRinging } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { Pipeline } from '../../../types/domain'
import { type PipelineNotifyState, useNotifyItems } from '../hooks/useNotifyItems'

const STATE_BUTTON: Record<PipelineNotifyState, { label: string; icon: ReactNode }> = {
  watching: { label: 'Watching', icon: <IconBellRinging size={16} aria-hidden /> },
  pending: { label: 'Notify on finish', icon: <IconBellPlus size={16} aria-hidden /> },
  default: { label: 'Notifications', icon: <IconBell size={16} aria-hidden /> },
  muted: { label: 'Muted', icon: <IconBellOff size={16} aria-hidden /> },
}

export function PipelineNotifyMenu({ pipeline }: { pipeline: Pipeline }): ReactNode {
  const { pipelineState, notifyItems } = useNotifyItems()
  const { label, icon } = STATE_BUTTON[pipelineState(pipeline.id)]

  return (
    <Menu position="bottom-end">
      <Menu.Target>
        <Button
          type="button"
          variant="default"
          size="sm"
          leftSection={icon}
          aria-label={`${label}, notification options`}
        >
          {label}
        </Button>
      </Menu.Target>
      <Menu.Dropdown>
        {notifyItems(pipeline).map((item) => (
          <Menu.Item key={item.label} leftSection={item.icon} onClick={item.onClick}>
            {item.label}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}
