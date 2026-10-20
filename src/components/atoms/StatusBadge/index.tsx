import { Box, Text, Tooltip } from '@mantine/core'

import { STATUS_META } from '../../../theme/status'
import type { PipelineStatus } from '../../../types'

interface StatusBadgeProps {
  status: PipelineStatus
  size?: 'sm' | 'md'
}

export const StatusBadge = ({ status, size = 'sm' }: StatusBadgeProps) => {
  const { label, tone } = STATUS_META[status]

  return (
    <Text component="span" size={size === 'sm' ? 'xs' : 'sm'} fw={600} c={`var(--status-${tone})`}>
      {label}
    </Text>
  )
}

export const StatusIcon = ({ status }: { status: PipelineStatus }) => {
  const { label, tone, icon: Icon } = STATUS_META[status]

  return (
    <Tooltip label={label}>
      <Box component="span" role="img" aria-label={label} display="inline-flex" c={`var(--status-${tone})`}>
        <Icon size={20} aria-hidden />
      </Box>
    </Tooltip>
  )
}
