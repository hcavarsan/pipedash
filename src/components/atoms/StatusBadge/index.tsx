import { Box, type MantineColor, Text, Tooltip } from '@mantine/core'
import type { CSSProperties } from 'react'

import { STATUS_META } from '../../../theme/status'
import type { PipelineStatus } from '../../../types'

const toneStyle = (color: MantineColor) => {
  const [light, dark] = color === 'gray' ? [6, 4] : [9, 5]

  return {
    '--status-light': `var(--mantine-color-${color}-${light})`,
    '--status-dark': `var(--mantine-color-${color}-${dark})`,
  } as CSSProperties
}

interface StatusBadgeProps {
  status: PipelineStatus
  size?: 'sm' | 'md'
}

export const StatusBadge = ({ status, size = 'sm' }: StatusBadgeProps) => {
  const { label, color } = STATUS_META[status]

  return (
    <Text component="span" size={size === 'sm' ? 'xs' : 'sm'} fw={600} className="status-tone" style={toneStyle(color)}>
      {label}
    </Text>
  )
}

export const StatusIcon = ({ status }: { status: PipelineStatus }) => {
  const { label, color, icon: Icon } = STATUS_META[status]

  return (
    <Tooltip label={label}>
      <Box
        component="span"
        role="img"
        aria-label={label}
        display="inline-flex"
        className="status-tone"
        style={toneStyle(color)}
      >
        <Icon size={20} aria-hidden />
      </Box>
    </Tooltip>
  )
}
