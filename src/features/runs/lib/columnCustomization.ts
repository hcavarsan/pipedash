import type { DataTableColumn } from 'mantine-datatable'

import type { PipelineRun } from '../../../types/domain'
import { reorderSlots } from '../../../utils/viewTree'

export interface ColumnItem {
  id: string
  title: string
  visible: boolean
  locked: boolean
  moveable: boolean
}

const toItem = (column: DataTableColumn<PipelineRun>, visible: boolean): ColumnItem => {
  const id = String(column.accessor)

  return {
    id,
    title: String(column.title || id),
    visible,
    locked: id === 'status' || id === 'actions',
    moveable: id !== 'actions',
  }
}

export function buildColumnItems(
  columns: DataTableColumn<PipelineRun>[],
  visibleColumns: DataTableColumn<PipelineRun>[],
  order: string[] | undefined,
  visibility: Record<string, boolean> | undefined
): ColumnItem[] {
  const visibleIds = new Set(visibleColumns.map((column) => String(column.accessor)))
  const items = columns.map((column) =>
    toItem(column, visibility?.[String(column.accessor)] ?? visibleIds.has(String(column.accessor)))
  )

  if (!order || order.length === 0) {
    return items
  }

  const byId = new Map(items.map((item) => [item.id, item]))
  const ordered = order.flatMap((id) => {
    const item = byId.get(id)
    byId.delete(id)

    return item ? [item] : []
  })

  return [...ordered, ...byId.values()]
}

export function resetColumnItems(columns: DataTableColumn<PipelineRun>[]): ColumnItem[] {
  return columns.map((column) => toItem(column, true))
}

export function columnSignature(items: ColumnItem[]): string {
  return items.map((item) => `${item.id}:${item.visible ? 1 : 0}`).join(',')
}

export function canMoveColumn(items: ColumnItem[], id: string, direction: -1 | 1): boolean {
  const movable = items.filter((item) => item.moveable).map((item) => item.id)
  const index = movable.indexOf(id)
  const target = index + direction

  return index !== -1 && target >= 0 && target < movable.length
}

export function moveColumn(items: ColumnItem[], id: string, direction: -1 | 1): ColumnItem[] {
  if (!canMoveColumn(items, id, direction)) {
    return items
  }

  const movable = items.filter((item) => item.moveable).map((item) => item.id)
  const index = movable.indexOf(id)
  const swapped = [...movable]
  const other = swapped[index + direction]

  if (other === undefined) {
    return items
  }

  swapped[index + direction] = id
  swapped[index] = other

  const byId = new Map(items.map((item) => [item.id, item]))

  return reorderSlots(
    items.map((item) => item.id),
    swapped
  ).flatMap((slot) => {
    const item = byId.get(slot)

    return item ? [item] : []
  })
}

export function columnPreferencesOf(items: ColumnItem[]): { order: string[]; visibility: Record<string, boolean> } {
  return {
    order: items.map((item) => item.id),
    visibility: Object.fromEntries(items.map((item) => [item.id, item.visible])),
  }
}
