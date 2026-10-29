import { Group, Text } from '@mantine/core'
import { type ReactNode, useMemo } from 'react'

import type { PipelineRun } from '../../types'
import type { CancelGroup } from '../../utils/cancelActiveRuns'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { StatusBadge } from '../atoms/StatusBadge'
import { VirtualList } from '../common/VirtualList'
import { IosGroup, IosRow } from '../ios/IosList'

type TargetRow = { kind: 'header'; group: CancelGroup } | { kind: 'run'; group: CancelGroup; run: PipelineRun }

interface CancelRunTargetsProps {
  groups: CancelGroup[]
  now: number
  shell: boolean
  where: (group: CancelGroup) => string
}

const rowKey = (row: TargetRow): string =>
  row.kind === 'header' ? `header:${row.group.pipeline.id}` : `run:${row.group.pipeline.id}:${row.run.run_number}`

export function CancelRunTargets({ groups, now, shell, where }: CancelRunTargetsProps): ReactNode {
  const rows = useMemo<TargetRow[]>(
    () =>
      groups.flatMap((group) => [
        { kind: 'header' as const, group },
        ...group.runs.map((run) => ({ kind: 'run' as const, group, run })),
      ]),
    [groups]
  )

  const renderRow = (row: TargetRow): ReactNode => {
    if (row.kind === 'header') {
      return shell ? (
        <IosRow title={`${row.group.pipeline.name} · ${row.group.name}`} subtitle={where(row.group)} />
      ) : (
        <>
          <Group justify="space-between" wrap="nowrap" gap="md" pt="xs">
            <Text size="sm" fw={600} truncate>
              {row.group.pipeline.name}
            </Text>
            <Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
              {row.group.name}
            </Text>
          </Group>
          <Text size="xs" c="dimmed" style={{ overflowWrap: 'anywhere' }}>
            {where(row.group)}
          </Text>
        </>
      )
    }

    const { run } = row

    return shell ? (
      <IosRow
        title={`#${run.run_number}${run.branch ? ` · ${run.branch}` : ''}`}
        subtitle={`Started ${formatRelativeTime(run.started_at, now)}`}
        value={<StatusBadge status={run.status} />}
      />
    ) : (
      <Group justify="space-between" wrap="nowrap" gap="md" py={4}>
        <Group gap="sm" wrap="nowrap" miw={0}>
          <Text size="sm" fw={500}>
            #{run.run_number}
          </Text>
          {run.branch && (
            <Text size="sm" c="dimmed" truncate>
              {run.branch}
            </Text>
          )}
        </Group>
        <Group gap="sm" wrap="nowrap" style={{ flexShrink: 0 }}>
          <StatusBadge status={run.status} />
          <Text size="xs" c="dimmed">
            {formatRelativeTime(run.started_at, now)}
          </Text>
        </Group>
      </Group>
    )
  }

  const list = <VirtualList items={rows} getKey={rowKey} estimateSize={shell ? 56 : 44} renderItem={renderRow} />

  return shell ? <IosGroup>{list}</IosGroup> : list
}
