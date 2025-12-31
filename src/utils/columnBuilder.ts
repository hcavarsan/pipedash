import type { DataTableColumn } from 'mantine-datatable'

import { logger } from '../lib/logger'
import type { ColumnDefinition, PipelineRun } from '../types'

import { DynamicRenderers } from './dynamicRenderers'
import { getValueByPath } from './objectPath'

export function buildColumnsFromSchema(
  columnDefs: ColumnDefinition[],
  additionalColumns?: DataTableColumn<PipelineRun>[]
): DataTableColumn<PipelineRun>[] {
  if (!columnDefs || !Array.isArray(columnDefs)) {
    logger.warn('buildColumnsFromSchema', 'columnDefs is not an array', columnDefs)

return additionalColumns || []
  }

  const columns: DataTableColumn<PipelineRun>[] = columnDefs.map((def) => {
    const column: DataTableColumn<PipelineRun> = {
      accessor: def.id as keyof PipelineRun,
      title: def.label,
      sortable: def.sortable,
      width: def.width ?? undefined,
      textAlign: (def.align as 'left' | 'center' | 'right' | undefined) ?? undefined,
      resizable: true,
      toggleable: false,
      draggable: false,
      render: (record) => {
        const value = getValueByPath(record, def.field_path)

        return DynamicRenderers.render(def.renderer, value)
      },
    }

    return column
  })

  if (additionalColumns) {
    columns.push(...additionalColumns)
  }

  return columns
}

export function filterVisibleColumns(columnDefs: ColumnDefinition[]): ColumnDefinition[] {
  if (!columnDefs || !Array.isArray(columnDefs)) {
    logger.warn('filterVisibleColumns', 'columnDefs is not an array', columnDefs)

return []
  }

  return columnDefs.filter((def) => {
    if (typeof def.visibility === 'string') {
      return def.visibility === 'Always' || def.visibility === 'WhenPresent'
    }

    return true
  })
}
