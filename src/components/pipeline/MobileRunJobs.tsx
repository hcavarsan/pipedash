import { Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { RunLogNode } from '../../types/logs'
import { formatDuration } from '../../utils/formatDuration'
import { StatusIcon } from '../atoms/StatusBadge'
import { VirtualList } from '../common/VirtualList'
import { IosGroup, IosRow } from '../ios/IosList'
import { groupLogNodes } from './logTree'

type JobListItem = { kind: 'stage'; stage: string; key: string } | { kind: 'job'; node: RunLogNode; key: string }

function flattenJobs(nodes: readonly RunLogNode[]): JobListItem[] {
  const groups = groupLogNodes(nodes, true)
  const staged = groups.some((group) => group.stage !== null)

  return groups.flatMap((group) => [
    ...(staged
      ? [{ kind: 'stage' as const, stage: group.stage ?? 'Jobs', key: `stage:${JSON.stringify(group.stage)}` }]
      : []),
    ...group.nodes.map((node) => ({ kind: 'job' as const, node, key: `job:${node.id}` })),
  ])
}

interface MobileRunJobsProps {
  nodes: readonly RunLogNode[]
  onSelect: (nodeId: string) => void
}

export function MobileRunJobs({ nodes, onSelect }: MobileRunJobsProps): ReactNode {
  const items = flattenJobs(nodes)

  if (items.length === 0) {
    return null
  }

  return (
    <IosGroup header={items.some((item) => item.kind === 'stage') ? undefined : 'Jobs'}>
      <VirtualList
        items={items}
        getKey={(item) => item.key}
        estimateSize={52}
        renderItem={(item) =>
          item.kind === 'stage' ? (
            <Text size="xs" fw={600} c="dimmed" px="md" py="xs">
              {item.stage}
            </Text>
          ) : (
            <IosRow
              icon={<StatusIcon status={item.node.status} />}
              title={item.node.name}
              value={formatDuration(item.node.duration_seconds)}
              chevron
              onClick={() => onSelect(item.node.id)}
            />
          )
        }
      />
    </IosGroup>
  )
}
