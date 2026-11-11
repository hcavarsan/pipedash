import { Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { StatusGlyph } from '../../../components/atoms/StatusBadge'
import { VirtualList } from '../../../components/common/VirtualList'
import type { RunLogNode } from '../../../types/logs'
import { NodeDuration } from './NodeDuration'

const STEP_ROW_PX = 32

const stepKey = (step: RunLogNode): string => step.id

interface PendingLogStepProps {
  step: RunLogNode
}

function PendingLogStep({ step }: PendingLogStepProps): ReactNode {
  const running = step.status === 'running'

  return (
    <Group
      gap="xs"
      wrap="nowrap"
      justify="space-between"
      py="3xs"
      px="xs"
      bdrs="sm"
      aria-current={running ? 'step' : undefined}
    >
      <Group gap="xs" wrap="nowrap">
        <StatusGlyph status={step.status} />
        <Text size="sm" fw={running ? 600 : 400}>
          {step.name}
        </Text>
      </Group>
      <Text size="xs" c="dimmed">
        <NodeDuration node={step} />
      </Text>
    </Group>
  )
}

export function PendingLogSteps({ steps }: { steps: readonly RunLogNode[] }): ReactNode {
  return (
    <VirtualList
      items={steps}
      getKey={stepKey}
      estimateSize={STEP_ROW_PX}
      renderItem={(step) => <PendingLogStep step={step} />}
    />
  )
}
