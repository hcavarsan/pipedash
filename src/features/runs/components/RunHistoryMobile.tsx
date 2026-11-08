import { Box, Group, Loader, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { IosRunList } from '../../../components/common/IosRunList'
import { VirtualList } from '../../../components/common/VirtualList'
import type { PipelineRun } from '../../../types/pipelines'
import { formatCount } from '../../../utils/formatCount'
import type { RunHistoryPageData } from '../hooks/useRunHistoryPageData'
import type { RunPageSelection } from '../lib/runHistoryTypes'
import { useRunActions } from './RunActionsContext'
import { RunHistoryCard } from './RunHistoryCard'

interface RunHistoryMobileProps {
  data: RunHistoryPageData
  selection: RunPageSelection
  shell: boolean
  selecting: boolean
  emptyState: ReactNode
}

export function RunHistoryMobile({ data, selection, shell, selecting, emptyState }: RunHistoryMobileProps): ReactNode {
  const { labels } = useRunActions()
  const { runs } = data
  const { selectedRuns, selectRuns, isRunCancellable } = selection
  const isSelected = (runNumber: number) => selectedRuns.some((selected) => selected.run_number === runNumber)

  const toggle = (run: PipelineRun, checked: boolean) =>
    selectRuns(
      checked ? [...selectedRuns, run] : selectedRuns.filter((selected) => selected.run_number !== run.run_number)
    )

  const list = shell ? (
    <IosRunList
      runs={runs}
      label={formatCount(runs.length, labels.run)}
      selecting={selecting}
      selectedNumbers={selectedRuns.map((run) => run.run_number)}
      isSelectable={isRunCancellable}
      onToggle={(run) => toggle(run, !isSelected(run.run_number))}
      onEndReached={data.fetchMore}
    />
  ) : (
    <VirtualList
      items={runs}
      getKey={(run) => String(run.run_number)}
      estimateSize={220}
      onEndReached={data.fetchMore}
      renderItem={(run) => (
        <Box pb="md">
          <RunHistoryCard
            run={run}
            selected={isSelected(run.run_number)}
            selectable={isRunCancellable(run)}
            onSelectedChange={(checked) => toggle(run, checked)}
          />
        </Box>
      )}
    />
  )

  return (
    <Stack gap="md">
      {runs.length > 0 && list}
      {emptyState}
      <Box py="md">
        {data.isFetchingMore && (
          <Group justify="center" gap="xs" role="status">
            <Loader size="sm" aria-hidden />
            <Text size="sm" c="dimmed">
              Loading more {labels.runs}…
            </Text>
          </Group>
        )}
        {!data.hasMore && runs.length > 0 && (
          <Text size="sm" c="dimmed" ta="center">
            {data.isComplete
              ? `All available ${formatCount(data.totalCount, labels.run)}`
              : `${data.totalCount}+ ${labels.runs}`}
          </Text>
        )}
      </Box>
    </Stack>
  )
}
