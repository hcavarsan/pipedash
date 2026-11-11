import { Button, Menu } from '@mantine/core'
import { IconBell, IconBellOff, IconBellPlus, IconBellRinging, IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { PipelineNotifyState } from '../../../types/notification_preferences'
import type { Pipeline } from '../../../types/pipelines'
import { useNotifyItems } from '../hooks/useNotifyItems'

const STATE_BUTTON: Record<PipelineNotifyState, { label: string; icon: ReactNode }> = {
  watching: { label: 'Watching', icon: <IconBellRinging size={16} aria-hidden /> },
  pending: { label: 'Notify on finish', icon: <IconBellPlus size={16} aria-hidden /> },
  default: { label: 'Notifications', icon: <IconBell size={16} aria-hidden /> },
  muted: { label: 'Muted', icon: <IconBellOff size={16} aria-hidden /> },
}

export function PipelineNotifyMenu({ pipeline }: { pipeline: Pipeline }): ReactNode {
  const { pipelineState, policyOptions } = useNotifyItems()
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
        {policyOptions(pipeline).map((option) => (
          <Menu.Item
            key={option.value}
            aria-current={option.selected ? 'true' : undefined}
            leftSection={option.icon}
            rightSection={option.selected ? <IconCheck size={14} aria-hidden /> : null}
            onClick={() => void option.select()}
          >
            {option.label}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}
