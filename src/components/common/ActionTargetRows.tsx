import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { RerunSummaryField } from '../../utils/rerunSummary'

export const ActionTargetRows = ({ rows }: { rows: RerunSummaryField[] }): ReactNode => (
  <Stack gap={2}>
    {rows.map(({ label, value }) => (
      <Text key={label} size="sm" style={{ overflowWrap: 'anywhere' }}>
        <Text span inherit c="dimmed">
          {label}:
        </Text>{' '}
        {value}
      </Text>
    ))}
  </Stack>
)

export const confirmBody = (message: string, rows: RerunSummaryField[]): ReactNode => (
  <Stack gap="sm">
    <Text size="sm">{message}</Text>
    {rows.length > 0 && <ActionTargetRows rows={rows} />}
  </Stack>
)
