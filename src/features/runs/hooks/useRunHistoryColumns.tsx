import type { DataTableColumn } from 'mantine-datatable'
import { useMemo } from 'react'
import { useTablePreferences } from '../../../queries/useRunHistoryQueries'
import type { PipelineRun } from '../../../types/domain'
import { RunActionButtons } from '../components/RunActionControls'
import { useTableColumns } from './useTableColumns'

export interface RunHistoryColumns {
  columns: DataTableColumn<PipelineRun>[]
  allColumns: DataTableColumn<PipelineRun>[]
  loading: boolean
  error: Error | null
  refetch: () => Promise<unknown>
  columnOrder: string[] | undefined
  columnVisibility: Record<string, boolean> | undefined
}

export function useRunHistoryColumns(providerId: number | undefined): RunHistoryColumns {
  const { data: preferences, isLoading: preferencesLoading } = useTablePreferences(providerId ?? 0, 'pipeline_runs')
  const columnOrder = preferences?.columnOrder
  const columnVisibility = preferences?.columnVisibility

  const additionalColumns = useMemo<DataTableColumn<PipelineRun>[]>(
    () => [
      {
        accessor: 'actions',
        title: 'Actions',
        width: '120px',
        textAlign: 'right',
        resizable: false,
        toggleable: false,
        draggable: false,
        render: (run) => <RunActionButtons run={run} withDetails />,
      },
    ],
    []
  )

  const table = useTableColumns(providerId, 'pipeline_runs', additionalColumns, { columnOrder, columnVisibility })

  return { ...table, loading: table.loading || preferencesLoading, columnOrder, columnVisibility }
}
