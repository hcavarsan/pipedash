import type { DataTableColumn } from 'mantine-datatable'
import { useMemo } from 'react'

import { useTableDefinition } from '../queries/useTableSchemaQueries'
import type { PipelineRun } from '../types'
import { buildColumnsFromSchema, filterVisibleColumns } from '../utils/columnBuilder'

interface ColumnPreferences {
  columnOrder?: string[] | undefined
  columnVisibility?: Record<string, boolean> | undefined
}

interface TableColumnsResult {
  columns: DataTableColumn<PipelineRun>[]
  allColumns: DataTableColumn<PipelineRun>[]
  loading: boolean
  error: Error | null
  refetch: () => Promise<unknown>
}

export function useTableColumns(
  providerId: number | undefined,
  tableId: string,
  additionalColumns: DataTableColumn<PipelineRun>[],
  preferences?: ColumnPreferences
): TableColumnsResult {
  const {
    data: tableSchema,
    isLoading: loading,
    error: queryError,
    refetch,
  } = useTableDefinition(providerId ?? 0, tableId)

  const error = useMemo(() => {
    if (queryError) {
      return queryError
    }

    if (providerId && !loading && !tableSchema?.columns?.length) {
      return new Error(`No "${tableId}" table schema is available for this provider.`)
    }

    return null
  }, [queryError, providerId, loading, tableSchema, tableId])

  const baseColumns = useMemo(() => {
    if (!providerId || !tableSchema?.columns) {
      return additionalColumns
    }

    return buildColumnsFromSchema(filterVisibleColumns(tableSchema.columns), additionalColumns)
  }, [providerId, tableSchema, additionalColumns])

  const effectiveColumns = useMemo(() => {
    if (!preferences?.columnOrder && !preferences?.columnVisibility) {
      return baseColumns
    }

    let columns = [...baseColumns]

    if (preferences.columnVisibility) {
      columns = columns.filter((col) => {
        const accessor = String(col.accessor)

        return preferences.columnVisibility?.[accessor] !== false
      })
    }

    if (preferences.columnOrder) {
      const ordered: DataTableColumn<PipelineRun>[] = []
      const columnMap = new Map(columns.map((col) => [String(col.accessor), col]))

      for (const accessor of preferences.columnOrder) {
        const column = columnMap.get(accessor)

        if (column) {
          ordered.push(column)
          columnMap.delete(accessor)
        }
      }

      ordered.push(...columnMap.values())
      columns = ordered
    }

    return columns
  }, [baseColumns, preferences?.columnOrder, preferences?.columnVisibility])

  return {
    columns: effectiveColumns,
    allColumns: baseColumns,
    loading,
    error,
    refetch,
  }
}
