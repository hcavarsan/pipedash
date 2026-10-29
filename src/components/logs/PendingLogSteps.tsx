import { Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import type { RunLogNode } from '../../types/logs'
import { formatDuration } from '../../utils/formatDuration'
import { StatusIcon } from '../atoms/StatusBadge'
import { VirtualList } from '../common/VirtualList'

const STEP_ROW_PX = 32

const stepKey = (step: RunLogNode): string => step.id

export function PendingLogSteps({ steps }: { steps: readonly RunLogNode[] }): ReactNode {
  return (
    <VirtualList
      items={steps}
      getKey={stepKey}
      estimateSize={STEP_ROW_PX}
      renderItem={(step) => (
        <Group gap="xs" wrap="nowrap" justify="space-between" py={4}>
          <Group gap="xs" wrap="nowrap">
            <StatusIcon status={step.status} />
            <Text size="sm">{step.name}</Text>
          </Group>
          <Text size="xs" c="dimmed">
            {formatDuration(step.duration_seconds)}
          </Text>
        </Group>
      )}
    />
  )
}
