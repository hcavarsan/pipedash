import { Box, Text, Tooltip } from '@mantine/core'
import type { ReactNode } from 'react'

import { type StatusKey, statusMeta } from '../../theme/status'

interface StatusBadgeProps {
  status: StatusKey
  label?: string | undefined
  size?: 'sm' | 'md' | undefined
}

export const StatusBadge = ({ status, label: customLabel, size = 'sm' }: StatusBadgeProps): ReactNode => {
  const { label, tone } = statusMeta(status)

  return (
    <Text component="span" size={size === 'sm' ? 'xs' : 'sm'} fw={600} c={`var(--status-${tone})`}>
      {customLabel ?? label}
    </Text>
  )
}

export const StatusIcon = ({ status, label: customLabel }: Omit<StatusBadgeProps, 'size'>): ReactNode => {
  const { label, tone, icon: Icon } = statusMeta(status)

  return (
    <Tooltip label={customLabel ?? label}>
      <Box
        component="span"
        role="img"
        aria-label={customLabel ?? label}
        display="inline-flex"
        c={`var(--status-${tone})`}
      >
        <Icon size={20} aria-hidden />
      </Box>
    </Tooltip>
  )
}

export const StatusGlyph = ({ status, size = '1.1em' }: { status: StatusKey; size?: string }): ReactNode => {
  const { tone, icon: Icon } = statusMeta(status)

  return (
    <Box component="span" display="inline-flex" c={`var(--status-${tone})`} style={{ flexShrink: 0 }} aria-hidden>
      <Icon size={size} />
    </Box>
  )
}
