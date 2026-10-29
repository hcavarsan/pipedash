import { Button, Group } from '@mantine/core'
import { IconSquare } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { Pipeline } from '../../types'
import { getStatusOptions } from '../../utils/pipelinePresentation'
import { FilterBar } from '../common/FilterBar'

import { useRunActions } from './RunActionsContext'
import type { RunPageSelection } from './runHistoryTypes'
import type { RunHistoryPageData } from './useRunHistoryPageData'

interface RunHistoryFiltersProps {
  pipeline: Pipeline
  data: RunHistoryPageData
  selection: RunPageSelection
}

export function RunHistoryFilters({ pipeline, data, selection }: RunHistoryFiltersProps): ReactNode {
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
