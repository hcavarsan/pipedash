import { useMemo } from 'react'
import type { DataTableColumn } from 'mantine-datatable'

import { logger } from '../lib/logger'
import { useTableDefinition } from '../queries/useTableSchemaQueries'
import type { PipelineRun } from '../types'
import { buildColumnsFromSchema, filterVisibleColumns } from '../utils/columnBuilder'

interface ColumnPreferences {
  columnOrder?: string[]
  columnVisibility?: Record<string, boolean>
}

export function useTableColumns(
  providerId: number | undefined,
  tableId: string,
  additionalColumns?: DataTableColumn<PipelineRun>[],
  preferences?: ColumnPreferences
) {
  const {
    data: tableSchema,
    isLoading: loading,
    error: queryError,
  } = useTableDefinition(providerId ?? 0, tableId)

  const stableAdditionalColumns = useMemo(
    () => (additionalColumns && Array.isArray(additionalColumns) ? additionalColumns : []),
    [additionalColumns]
  )

  const baseColumns = useMemo(() => {
    if (!providerId) {
      return stableAdditionalColumns
    }

    if (!tableSchema) {
      if (!loading && queryError) {
        logger.warn('useTableColumns', `No schema found for table ${tableId}, using default columns`)
      }

return stableAdditionalColumns
    }

    if (!tableSchema.columns || !Array.isArray(tableSchema.columns)) {
      logger.warn('useTableColumns', `Schema for table ${tableId} has no columns array, using default columns`)

return stableAdditionalColumns
    }

    const visibleCols = filterVisibleColumns(tableSchema.columns)
    const builtColumns = buildColumnsFromSchema(visibleCols, stableAdditionalColumns)

    return builtColumns
  }, [providerId, tableSchema, loading, queryError, tableId, stableAdditionalColumns])

  const effectiveColumns = useMemo(() => {
    if (!baseColumns || !Array.isArray(baseColumns)) {
      logger.warn('useTableColumns', 'baseColumns is not an array', baseColumns)

return []
    }

    if (!preferences?.columnOrder && !preferences?.columnVisibility) {
      return baseColumns
    }

    let columns = [...baseColumns]

    if (preferences.columnVisibility) {
      columns = columns.filter(col => {
        const accessor = String(col.accessor)



return preferences.columnVisibility?.[accessor] !== false
      })
    }

    if (preferences.columnOrder) {
      const ordered: DataTableColumn<PipelineRun>[] = []
      const columnMap = new Map(columns.map(col => [String(col.accessor), col]))

      for (const accessor of preferences.columnOrder) {
        const column = columnMap.get(accessor)


        if (column) {
          ordered.push(column)
          columnMap.delete(accessor)
        }
      }

      columnMap.forEach(col => ordered.push(col))
      columns = ordered
    }

    return columns
  }, [baseColumns, preferences?.columnOrder, preferences?.columnVisibility])

  return {
    columns: effectiveColumns,
    allColumns: baseColumns,
    loading,
    error: queryError ? (queryError as Error).message : null,
  }
}
