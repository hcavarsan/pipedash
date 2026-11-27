import { Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import { EmptyState } from '../../../components/common/EmptyState'
import { VirtualList } from '../../../components/common/VirtualList'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import type { RunLogNode } from '../../../types/logs'
import { NodeDuration } from '../../logs/components/NodeDuration'
import { type LogNodeGroup, skippedLabel, stageKey, toggleStage } from '../lib/logTree'

type JobListItem =
  | { kind: 'stage'; stage: string; key: string }
  | { kind: 'job'; node: RunLogNode; key: string }
  | { kind: 'skipped'; stageKey: string; count: number; open: boolean; key: string }

function flattenJobs(groups: LogNodeGroup[], openStages: ReadonlySet<string>): JobListItem[] {
  const staged = groups.some((group) => group.stage !== null)
  const jobs = (nodes: RunLogNode[]) => nodes.map((node) => ({ kind: 'job' as const, node, key: `job:${node.id}` }))

  return groups.flatMap((group): JobListItem[] => {
    const key = stageKey(group.stage)
    const open = openStages.has(key)
    const fold: JobListItem[] = group.skipped.length
      ? [{ kind: 'skipped', stageKey: key, count: group.skipped.length, open, key: `skipped:${key}` }]
      : []

    return [
      ...(staged ? [{ kind: 'stage' as const, stage: group.stage ?? 'Jobs', key: `stage:${key}` }] : []),
      ...jobs(group.nodes),
      ...fold,
      ...(open ? jobs(group.skipped) : []),
    ]
  })
}

interface MobileRunJobsProps {
  groups: LogNodeGroup[]
  onSelect: (nodeId: string) => void
}

export function MobileRunJobs({ groups, onSelect }: MobileRunJobsProps): ReactNode {
  const [openStages, setOpenStages] = useState<ReadonlySet<string>>(new Set())
  const items = flattenJobs(groups, openStages)

  if (items.length === 0) {
    return <EmptyState title="This run has no jobs yet" description="Jobs appear here once the provider starts them." />
  }

  const renderItem = (item: JobListItem): ReactNode => {
    if (item.kind === 'stage') {
      return (
        <Text size="xs" fw={600} c="dimmed" px="md" py="xs">
          {item.stage}
        </Text>
      )
    }

    if (item.kind === 'skipped') {
      return (
        <IosRow
          title={skippedLabel(item.count)}
          expanded={item.open}
          onClick={() => setOpenStages(toggleStage(openStages, item.stageKey))}
        />
      )
    }

    return (
      <IosRow
        icon={<StatusIcon status={item.node.status} />}
        title={item.node.name}
        value={<NodeDuration node={item.node} />}
        chevron
        onClick={() => onSelect(item.node.id)}
      />
    )
  }

  return (
    <IosGroup header={items.some((item) => item.kind === 'stage') ? undefined : 'Jobs'}>
      <VirtualList items={items} getKey={(item) => item.key} estimateSize={52} renderItem={renderItem} />
    </IosGroup>
  )
}
