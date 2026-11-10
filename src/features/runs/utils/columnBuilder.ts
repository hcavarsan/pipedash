import type { DataTableColumn } from 'mantine-datatable'

import { TABLE_NUMERIC } from '../../../components/common/tableStyles'
import { logger } from '../../../lib/logger'
import type { PipelineRun } from '../../../types/pipelines'
import type { ColumnDefinition } from '../../../types/plugins'
import { getRunStatusLabel } from '../../../utils/pipelinePresentation'
import { isLiveRun } from '../../../utils/runTiming'
import { renderCellValue, renderLiveRunDuration } from './dynamicRenderers'
import { getValueByPath } from './objectPath'

const RIGHT_ALIGNED_TYPES: ColumnDefinition['data_type'][] = ['Number', 'Duration']

export const isNumericColumn = (dataType: ColumnDefinition['data_type'] | undefined): boolean =>
  dataType !== undefined && RIGHT_ALIGNED_TYPES.includes(dataType)

export function buildColumnsFromSchema(
  columnDefs: ColumnDefinition[],
  additionalColumns?: DataTableColumn<PipelineRun>[]
): DataTableColumn<PipelineRun>[] {
  if (!columnDefs || !Array.isArray(columnDefs)) {
    logger.warn('buildColumnsFromSchema', 'columnDefs is not an array', columnDefs)

    return additionalColumns || []
  }

  const columns: DataTableColumn<PipelineRun>[] = columnDefs.map((def) => {
    const numeric = isNumericColumn(def.data_type)
    const column: DataTableColumn<PipelineRun> = {
      accessor: def.id,
      title: def.label,
      sortable: def.sort !== 'none',
      ...(def.width == null ? {} : { width: def.width }),
      textAlign: numeric ? 'right' : 'left',
      ...(numeric ? { titleStyle: TABLE_NUMERIC, cellsStyle: () => TABLE_NUMERIC } : {}),
      resizable: true,
      toggleable: false,
      draggable: false,
      render: (record) => {
        if (def.renderer === 'Duration' && def.field_path === 'duration_seconds' && isLiveRun(record)) {
          return renderLiveRunDuration(record)
        }

        const value = getValueByPath(record, def.field_path)

        return renderCellValue(def.renderer, value, getRunStatusLabel(record))
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
