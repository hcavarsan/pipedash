import { Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'

const LABEL_WIDTH = 130

interface SummaryRowProps {
  label: string
  children: ReactNode
}

export function SummaryRow({ label, children }: SummaryRowProps): ReactNode {
  return (
    <Group justify="space-between" wrap="nowrap" gap="xs">
      <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
        {label}
      </Text>
      {children}
    </Group>
  )
}
