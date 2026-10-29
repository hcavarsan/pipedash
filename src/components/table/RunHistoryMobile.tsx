import { Box, Group, Loader, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { formatCount } from '../../utils/formatCount'
import { VirtualList } from '../common/VirtualList'

import { IosRunList } from './IosRunList'
import { useRunActions } from './RunActionsContext'
import { RunHistoryCard } from './RunHistoryCard'
import type { RunPageSelection } from './runHistoryTypes'
import type { RunHistoryPageData } from './useRunHistoryPageData'

interface RunHistoryMobileProps {
  data: RunHistoryPageData
  selection: RunPageSelection
  shell: boolean
  selecting: boolean
  onStartSelection: () => void
  emptyState: ReactNode
}

export function RunHistoryMobile({
  data,
  selection,
  shell,
  selecting,
  onStartSelection,
  emptyState,
}: RunHistoryMobileProps): ReactNode {
  const { labels } = useRunActions()
  const { runs } = data
  const { selectedRuns, selectRuns, isRunCancellable } = selection
  const isSelected = (runNumber: number) => selectedRuns.some((selected) => selected.run_number === runNumber)

  return (
    <Stack gap="md">
      {shell && runs.length > 0 && (
        <IosRunList
          runs={runs}
          label={formatCount(runs.length, labels.run)}
          onSelect={runs.some(isRunCancellable) ? onStartSelection : undefined}
          selecting={selecting}
          selectedNumbers={selectedRuns.map((run) => run.run_number)}
          isSelectable={isRunCancellable}
          onToggle={(run) =>
            selectRuns(
              isSelected(run.run_number)
                ? selectedRuns.filter((selected) => selected.run_number !== run.run_number)
                : [...selectedRuns, run]
            )
          }
          onEndReached={data.fetchMore}
        />
      )}
      {!shell && runs.length > 0 && (
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
                onSelectedChange={(checked) =>
                  selectRuns(
                    checked
                      ? [...selectedRuns, run]
                      : selectedRuns.filter((selected) => selected.run_number !== run.run_number)
                  )
                }
              />
            </Box>
          )}
        />
      )}
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
