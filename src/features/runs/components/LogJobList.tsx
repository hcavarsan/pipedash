import { Box, Group, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { FillScrollArea } from '../../../components/common/FillScrollArea'
import { VirtualList } from '../../../components/common/VirtualList'
import type { RunLogTree } from '../../../types/logs'
import { type JobListRow, jobListRows, skippedLabel, toggleStage } from '../lib/logTree'
import { LogJobItem } from './LogJobSummary'

interface LogJobListProps {
  tree: Pick<RunLogTree, 'nodes' | 'groups'>
  selectedId: string | null
  onSelect: (id: string) => void
}

function SkippedJobsRow({ row, onToggle }: { row: Extract<JobListRow, { kind: 'skipped' }>; onToggle: () => void }) {
  const Chevron = row.open ? IconChevronDown : IconChevronRight

  return (
    <UnstyledButton
      className="interactive-surface"
      w="100%"
      px="xs"
      py="xs"
      bdrs="sm"
      aria-expanded={row.open}
      onClick={onToggle}
    >
      <Group gap="xs" wrap="nowrap">
        <Chevron size={16} color="var(--mantine-color-dimmed)" aria-hidden />
        <Text size="sm" c="dimmed">
          {skippedLabel(row.count)}
        </Text>
      </Group>
    </UnstyledButton>
  )
}

export function LogJobList({ tree, selectedId, onSelect }: LogJobListProps): ReactNode {
  const [openStages, setOpenStages] = useState<ReadonlySet<string>>(new Set())

  return (
    <FillScrollArea w={260} style={{ flex: 'none', borderRight: '1px solid var(--mantine-color-default-border)' }}>
      <Box component="nav" aria-label="Jobs" p="xs">
        <VirtualList
          items={jobListRows(tree, openStages, selectedId)}
          getKey={(row) => (row.kind === 'node' ? row.node.id : `skipped:${row.stageKey}`)}
          estimateSize={36}
          renderItem={(row) =>
            row.kind === 'node' ? (
              <LogJobItem node={row.node} selectedId={selectedId} onSelect={onSelect} />
            ) : (
              <SkippedJobsRow row={row} onToggle={() => setOpenStages(toggleStage(openStages, row.stageKey))} />
            )
          }
        />
      </Box>
    </FillScrollArea>
  )
}
