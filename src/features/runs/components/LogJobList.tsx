import { Box, Group, Text, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import { FillScrollArea } from '../../../components/common/FillScrollArea'
import { VirtualList } from '../../../components/common/VirtualList'
import type { RunLogNode } from '../../../types/logs'
import { NodeDuration } from '../../logs/components/NodeDuration'

interface LogJobListProps {
  nodes: readonly RunLogNode[]
  selectedId: string | null
  onSelect: (id: string) => void
}

export const LogJobList = ({ nodes, selectedId, onSelect }: LogJobListProps): ReactNode => (
  <FillScrollArea w={260} style={{ flex: 'none', borderRight: '1px solid var(--mantine-color-default-border)' }}>
    <Box component="nav" aria-label="Jobs" p="xs">
      <VirtualList
        items={nodes.filter((node) => node.navigable)}
        getKey={(node) => node.id}
        estimateSize={36}
        renderItem={(node) => {
          const indent = `calc(${node.depth} * var(--mantine-spacing-md))`

          if (node.log === null) {
            return (
              <Text
                size="xs"
                fw={600}
                c="dimmed"
                px="xs"
                py="xs"
                pl={`calc(${indent} + var(--mantine-spacing-xs))`}
                truncate
              >
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
              pl={`calc(${indent} + var(--mantine-spacing-xs))`}
              bdrs="sm"
              aria-current={selected ? 'true' : undefined}
              style={selected ? { backgroundColor: 'var(--mantine-color-blue-light)' } : undefined}
              onClick={() => onSelect(node.id)}
            >
              <Group gap="xs" wrap="nowrap">
                <StatusIcon status={node.status} />
                <Text size="sm" truncate flex={1} miw={0}>
                  {node.name}
                </Text>
                <Text size="xs" c="dimmed">
                  <NodeDuration node={node} />
                </Text>
              </Group>
            </UnstyledButton>
          )
        }}
      />
    </Box>
  </FillScrollArea>
)
