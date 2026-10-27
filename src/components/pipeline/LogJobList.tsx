import { Box, Group, ScrollArea, Text, UnstyledButton } from '@mantine/core'

import type { RunLogNode } from '../../types/logs'
import { formatDuration } from '../../utils/formatDuration'
import { StatusIcon } from '../atoms/StatusBadge'

interface LogJobListProps {
  nodes: readonly RunLogNode[]
  selectedId: string | null
  onSelect: (id: string) => void
}

export const LogJobList = ({ nodes, selectedId, onSelect }: LogJobListProps) => (
  <ScrollArea w={260} flex="none" style={{ borderRight: '1px solid var(--mantine-color-default-border)' }}>
    <Box component="nav" aria-label="Jobs" p="xs">
      {nodes
        .filter((node) => node.navigable)
        .map((node) => {
          const indent = `calc(${node.depth} * var(--mantine-spacing-md))`

          if (node.log === null) {
            return (
              <Text
                key={node.id}
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
              key={node.id}
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
                  {formatDuration(node.duration_seconds)}
                </Text>
              </Group>
            </UnstyledButton>
          )
        })}
    </Box>
  </ScrollArea>
)
