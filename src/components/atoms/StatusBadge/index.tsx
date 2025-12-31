import { Badge, Box } from '@mantine/core'
import {
  IconCheck,
  IconCircleOff,
  IconClock,
  IconLoader,
  IconMinus,
  IconX,
} from '@tabler/icons-react'

import { STATUS_COLORS } from '../../../theme/colors'
import type { PipelineStatus } from '../../../types'

interface StatusBadgeProps {
  status: PipelineStatus;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  withIcon?: boolean;
}

const statusConfig: Record<PipelineStatus, {
  label: string;
  icon: React.ComponentType<any>;
}> = {
  success: {
    label: 'Success',
    icon: IconCheck,
  },
  failed: {
    label: 'Failed',
    icon: IconX,
  },
  running: {
    label: 'Running',
    icon: IconLoader,
  },
  pending: {
    label: 'Pending',
    icon: IconClock,
  },
  cancelled: {
    label: 'Cancelled',
    icon: IconCircleOff,
  },
  skipped: {
    label: 'Skipped',
    icon: IconMinus,
  },
}

export const StatusBadge = ({
  status,
  size = 'md',
  withIcon = false,
}: StatusBadgeProps) => {
  const config = statusConfig[status]
  const Icon = config.icon

  const iconSize = {
    xs: 14,
    sm: 16,
    md: 18,
    lg: 20,
  }[size]

  return (
    <Badge
      variant="light"
      color={STATUS_COLORS[status]}
      size={size}
      fw={500}
      leftSection={
        withIcon ? (
          <Box component="span" style={{ display: 'flex', alignItems: 'center' }}>
            <Icon size={iconSize} style={{ display: 'block' }} />
          </Box>
        ) : undefined
      }
      styles={{
        label: {
          textTransform: 'none',
        },
      }}
    >
      {config.label}
    </Badge>
  )
}
