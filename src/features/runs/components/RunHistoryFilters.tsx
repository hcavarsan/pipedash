import { Button, Group } from '@mantine/core'
import { IconSquare } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { FilterBar, type FilterSheet } from '../../../components/common/FilterBar'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { Pipeline } from '../../../types/domain'
import { getStatusOptions } from '../../../utils/pipelinePresentation'
import type { RunHistoryPageData } from '../hooks/useRunHistoryPageData'
import type { RunPageSelection } from '../lib/runHistoryTypes'
import { useRunActions } from './RunActionsContext'

interface RunHistoryFiltersProps {
  pipeline: Pipeline
  data: RunHistoryPageData
  selection: RunPageSelection
  sheet: FilterSheet
}

export function RunHistoryFilters({ pipeline, data, selection, sheet }: RunHistoryFiltersProps): ReactNode {
  const mobileShell = useMobileShell()
  const { labels } = useRunActions()
  const { setFilter, filters } = data

  return (
    <Group
      justify="space-between"
      align="flex-start"
      wrap="nowrap"
      style={mobileShell ? { display: 'contents' } : undefined}
    >
      <FilterBar
        sheet={sheet}
        onClearAll={data.clearFilters}
        filters={{
          search: {
            value: filters.search,
            onChange: (value) => setFilter('search', value),
            placeholder: `Search ${labels.runs}…`,
          },
          status: {
            value: filters.status,
            onSelect: data.selectStatus,
            onClear: () => setFilter('status', []),
            options: getStatusOptions(pipeline.provider_type),
          },
          branch: {
            value: filters.branch,
            onChange: (value) => setFilter('branch', value),
            options: data.branches,
          },
          actor: {
            value: filters.actor,
            onChange: (value) => setFilter('actor', value),
            options: data.actors,
          },
          dateRange: {
            value: filters.dateRange,
            onChange: (value) => setFilter('dateRange', value),
          },
        }}
      />
      {!mobileShell && selection.selectedRuns.length > 0 && (
        <Button
          type="button"
          size="xs"
          variant="light"
          color="red"
          leftSection={<IconSquare size={14} aria-hidden />}
          loading={selection.isCancelling}
          onClick={selection.cancelSelected}
        >
          Cancel selected
        </Button>
      )}
    </Group>
  )
}
