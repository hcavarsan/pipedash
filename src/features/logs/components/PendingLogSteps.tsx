import { Group, Loader, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import { VirtualList } from '../../../components/common/VirtualList'
import type { RunLogNode } from '../../../types/logs'
import { NodeDuration } from './NodeDuration'

const STEP_ROW_PX = 32

const stepKey = (step: RunLogNode): string => step.id

export function PendingLogSteps({ steps }: { steps: readonly RunLogNode[] }): ReactNode {
  return (
    <VirtualList
      items={steps}
      getKey={stepKey}
      estimateSize={STEP_ROW_PX}
      renderItem={(step) => {
        const running = step.status === 'running'

        return (
          <Group
            gap="xs"
            wrap="nowrap"
            justify="space-between"
            py={4}
            px="xs"
            bdrs="sm"
            aria-current={running ? 'step' : undefined}
            style={running ? { backgroundColor: 'var(--mantine-color-blue-light)' } : undefined}
          >
            <Group gap="xs" wrap="nowrap">
              {running ? <Loader size={14} aria-label="Running" /> : <StatusIcon status={step.status} />}
              <Text size="sm" fw={running ? 600 : 400}>
                {step.name}
              </Text>
            </Group>
            <Text size="xs" c="dimmed">
              <NodeDuration node={step} />
            </Text>
          </Group>
        )
      }}
    />
  )
}
