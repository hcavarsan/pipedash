import { Menu } from '@mantine/core'
import type { ReactNode } from 'react'
import type { WorkflowMenuItem } from '../../types/pipelineRows'

export function RowMenuItems({ items }: { items: WorkflowMenuItem[] }): ReactNode {
  return items.map((item) => (
    <Menu.Item
      key={item.label}
      leftSection={item.icon}
      {...(item.color === undefined ? {} : { color: item.color })}
      disabled={item.disabled ?? false}
      onClick={item.onClick}
    >
      {item.label}
    </Menu.Item>
  ))
}
