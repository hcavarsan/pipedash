import { Badge } from '@mantine/core'

import { STATUS_META } from '../../../theme/status'
import type { PipelineStatus } from '../../../types'

interface StatusBadgeProps {
  status: PipelineStatus
  size?: 'sm' | 'md'
  withIcon?: boolean
}

export const StatusBadge = ({ status, size = 'sm', withIcon = false }: StatusBadgeProps) => {
  const { label, color, icon: Icon } = STATUS_META[status]

  return (
    <Badge
      variant="light"
      color={color}
      size={size}
      fw={500}
      leftSection={withIcon ? <Icon size={size === 'sm' ? 12 : 14} aria-hidden /> : undefined}
    >
      {label}
    </Badge>
  )
}
