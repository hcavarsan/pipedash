import { Anchor, Badge, Box, Group, Paper, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import { STATUS_META } from '../../../theme/status'
import type { FlowNode } from '../../../types/applications'
import { NODE_HEIGHT, NODE_WIDTH, nodePosition } from '../lib/flowLayout'

interface FlowNodeBoxProps {
  node: FlowNode
  vertical: boolean
}

function NodeTitle({ node }: { node: FlowNode }): ReactNode {
  if (node.pipeline_id === null) {
    return (
      <Text size="sm" fw={600} truncate>
        {node.title}
      </Text>
    )
  }

  return (
    <Anchor component={Link} to={`/pipelines/${encodeURIComponent(node.pipeline_id)}`} size="sm" fw={600} truncate>
      {node.title}
    </Anchor>
  )
}

function NodeStatusDot({ status }: { status: NonNullable<FlowNode['status']> }): ReactNode {
  const { label, tone } = STATUS_META[status]

  return (
    <Box
      component="span"
      role="img"
      aria-label={label}
      w={10}
      h={10}
      bdrs="50%"
      style={{ flexShrink: 0, background: `var(--status-${tone})` }}
    />
  )
}

function NodeHeader({ node }: { node: FlowNode }): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap">
      {node.status !== null && <NodeStatusDot status={node.status} />}
      <Box flex={1} miw={0}>
        <NodeTitle node={node} />
      </Box>
      {node.count > 1 && (
        <Badge size="xs" variant="light" color="gray">
          {node.count}
        </Badge>
      )}
    </Group>
  )
}

export function FlowNodeBox({ node, vertical }: FlowNodeBoxProps): ReactNode {
  const { x, y } = nodePosition(node, vertical)

  return (
    <g data-node-id={node.id} transform={`translate(${x} ${y})`}>
      <foreignObject width={NODE_WIDTH} height={NODE_HEIGHT}>
        <Paper withBorder p="xs" h={NODE_HEIGHT} style={{ overflow: 'hidden' }}>
          <NodeHeader node={node} />
          {node.subtitle !== null && (
            <Text size="xs" c="dimmed" truncate>
              {node.subtitle}
            </Text>
          )}
        </Paper>
      </foreignObject>
    </g>
  )
}
