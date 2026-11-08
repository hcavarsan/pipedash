import { Group, Text, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import type { RunLogNode } from '../../../types/logs'
import { NodeDuration } from '../../logs/components/NodeDuration'

interface LogJobItemProps {
  node: RunLogNode
  selectedId: string | null
  onSelect: (id: string) => void
}

function LogJobSummary({ node }: { node: RunLogNode }): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap">
      <StatusIcon status={node.status} />
      <Text size="sm" truncate flex={1} miw={0}>
        {node.name}
      </Text>
      <Text size="xs" c="dimmed">
        <NodeDuration node={node} />
      </Text>
    </Group>
  )
}

export function LogJobItem({ node, selectedId, onSelect }: LogJobItemProps): ReactNode {
  const paddingLeft = `calc(calc(${node.depth} * var(--mantine-spacing-md)) + var(--mantine-spacing-xs))`

  if (node.log === null) {
    return (
      <Text size="xs" fw={600} c="dimmed" px="xs" py="xs" pl={paddingLeft} truncate>
        {node.name}
      </Text>
    )
  }

  const selected = node.id === selectedId

  return (
    <UnstyledButton
      className="interactive-surface"
      w="100%"
      px="xs"
      py="xs"
      pl={paddingLeft}
      bdrs="sm"
      aria-current={selected ? 'true' : undefined}
      style={selected ? { backgroundColor: 'var(--mantine-color-blue-light)' } : undefined}
      onClick={() => onSelect(node.id)}
    >
      <LogJobSummary node={node} />
    </UnstyledButton>
  )
}
