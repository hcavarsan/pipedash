import { useEffect, useMemo, useState } from 'react'
import type { DataTableColumn } from 'mantine-datatable'

import { useTableSchema } from '../contexts/TableSchemaContext'
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
  const { getTableSchema } = useTableSchema()
  const [baseColumns, setBaseColumns] = useState<DataTableColumn<PipelineRun>[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const stableAdditionalColumns = useMemo(
    () => additionalColumns || [],
    [additionalColumns]
  )

  useEffect(() => {
    let cancelled = false

    if (!providerId) {
      setBaseColumns(stableAdditionalColumns)

return
    }

    const loadColumns = async () => {
      try {
        setLoading(true)
        setError(null)

        const tableSchema = await getTableSchema(providerId, tableId)

        if (cancelled) {
return
}

        if (tableSchema) {
          const visibleCols = filterVisibleColumns(tableSchema.columns)
          const builtColumns = buildColumnsFromSchema(visibleCols, stableAdditionalColumns)

          setBaseColumns(builtColumns)
        } else {
          console.warn(`No schema found for table ${tableId}, using default columns`)
          setBaseColumns(stableAdditionalColumns)
        }
      } catch (err: any) {
        if (cancelled) {
return
}

        console.error('Failed to load table columns:', err)
        setError(err.message || 'Failed to load columns')
        setBaseColumns(stableAdditionalColumns)
      } finally {
        if (!cancelled) {
          setLoading(false)
        }
      }
    }

    loadColumns()

    return () => {
      cancelled = true
    }
  }, [providerId, tableId, stableAdditionalColumns, getTableSchema])

  const effectiveColumns = useMemo(() => {
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
    error,
  }
}
