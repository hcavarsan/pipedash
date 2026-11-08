import { ActionIcon, Group, Tooltip } from '@mantine/core'
import { IconEye, IconEyeOff } from '@tabler/icons-react'
import type { ReactNode } from 'react'

interface ColumnBulkActionsProps {
  onShowAll: () => void
  onHideOptional: () => void
}

export function ColumnBulkActions({ onShowAll, onHideOptional }: ColumnBulkActionsProps): ReactNode {
  return (
    <Group gap="xs">
      <Tooltip label="Show all columns">
        <ActionIcon type="button" size="sm" aria-label="Show all columns" onClick={onShowAll}>
          <IconEye size={14} aria-hidden />
        </ActionIcon>
      </Tooltip>
      <Tooltip label="Hide optional columns">
        <ActionIcon type="button" size="sm" aria-label="Hide optional columns" onClick={onHideOptional}>
          <IconEyeOff size={14} aria-hidden />
        </ActionIcon>
      </Tooltip>
    </Group>
  )
}
