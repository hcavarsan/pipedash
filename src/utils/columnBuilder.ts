import type { DataTableColumn } from 'mantine-datatable'

import { logger } from '../lib/logger'
import type { ColumnDefinition, PipelineRun } from '../types'

import { renderCellValue } from './dynamicRenderers'
import { getValueByPath } from './objectPath'
import { getStatusLabel } from './pipelinePresentation'

const RIGHT_ALIGNED_TYPES: ColumnDefinition['data_type'][] = ['Number', 'Duration']

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
      accessor: def.id,
      title: def.label,
      sortable: def.sortable,
      ...(def.width == null ? {} : { width: def.width }),
      textAlign: RIGHT_ALIGNED_TYPES.includes(def.data_type) ? 'right' : 'left',
      resizable: true,
      toggleable: false,
      draggable: false,
      render: (record) => {
        const value = getValueByPath(record, def.field_path)

        return renderCellValue(def.renderer, value, getStatusLabel(record))
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
