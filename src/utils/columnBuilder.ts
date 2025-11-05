import type { DataTableColumn } from 'mantine-datatable'

import type { ColumnDefinition, PipelineRun } from '../types'

import { DynamicRenderers } from './dynamicRenderers'

function getValueByPath(record: any, path: string): any {
  const parts = path.split('.')
  let value: any = record

  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined
    }

    if (typeof value !== 'object' || value === null) {
      return undefined
    }

    value = value[part]
  }

  return value
}

/**
 * Builds DataTable columns from schema column definitions
 */
export function buildColumnsFromSchema(
  columnDefs: ColumnDefinition[],
  additionalColumns?: DataTableColumn<PipelineRun>[]
): DataTableColumn<PipelineRun>[] {
  const columns: DataTableColumn<PipelineRun>[] = columnDefs.map((def) => {
    const column: DataTableColumn<PipelineRun> = {
      accessor: def.id as keyof PipelineRun,
      title: def.label,
      sortable: def.sortable,
      width: def.width ?? undefined,
      textAlign: (def.align as 'left' | 'center' | 'right' | undefined) ?? undefined,
      // Enable only column resizing in the table
      // All other customization (reorder, show/hide) done via modal
      resizable: true,
      toggleable: false, // Disable built-in Mantine column toggle UI
      draggable: false, // Disable in-table dragging
      render: (record) => {
        const value = getValueByPath(record, def.field_path)



return DynamicRenderers.render(def.renderer, value)
      },
    }

    return column
  })

  // Add additional columns if provided (e.g., actions column)
  if (additionalColumns) {
    columns.push(...additionalColumns)
  }

  return columns
}

/**
 * Filter columns based on visibility rules
 * For now, only uses 'Always' visibility - more sophisticated filtering can be added later
 */
export function filterVisibleColumns(columnDefs: ColumnDefinition[]): ColumnDefinition[] {
  return columnDefs.filter((def) => {
    // For now, include all 'Always' and 'WhenPresent' columns
    // 'WhenPresent' will be handled per-row in the render function
    if (typeof def.visibility === 'string') {
      return def.visibility === 'Always' || def.visibility === 'WhenPresent'
    }
    // Include conditional and capability-based columns by default

    return true
  })
}
