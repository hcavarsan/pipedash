import { ScrollArea, Table } from '@mantine/core'
import { useVirtualizer } from '@tanstack/react-virtual'
import { type HTMLAttributes, type MouseEvent, type ReactNode, useEffect, useEffectEvent, useRef } from 'react'
import { FillScrollArea } from './FillScrollArea'
import { TABLE_NUMERIC } from './tableStyles'
import { VirtualTableHeader } from './VirtualTableHeader'
import { VirtualTableRow } from './VirtualTableRow'

const INTERACTIVE_TARGET = 'button, a, input, label, [role="checkbox"]'
const END_THRESHOLD = 5

export interface VirtualTableColumn<T> {
  id: string
  title: ReactNode
  width?: number | string | undefined
  textAlign?: 'left' | 'center' | 'right' | undefined
  sortable?: boolean | undefined
  render: (record: T) => ReactNode
}

export interface VirtualTableSort {
  columnId: string
  direction: 'asc' | 'desc'
}

interface VirtualTableSelection<T> {
  selectedKeys: ReadonlySet<string>
  isSelectable: (record: T) => boolean
  onChange: (keys: Set<string>) => void
  label: (record: T) => string
  hasMore?: boolean | undefined
  onToggleAll?: (() => void) | undefined
}

interface VirtualTableProps<T> {
  records: readonly T[]
  columns: readonly VirtualTableColumn<T>[]
  getKey: (record: T) => string
  height: number | string | 'fill'
  rowHeight: number
  label: string
  sort?: VirtualTableSort | undefined
  onSortChange?: ((sort: VirtualTableSort) => void) | undefined
  onRowClick?: ((record: T) => void) | undefined
  rowAttributes?: ((record: T) => HTMLAttributes<HTMLTableRowElement>) | undefined
  selection?: VirtualTableSelection<T> | undefined
  onEndReached?: (() => void) | undefined
}

export function VirtualTable<T>({
  records,
  columns,
  getKey,
  height,
  rowHeight,
  label,
  sort,
  onSortChange,
  onRowClick,
  rowAttributes,
  selection,
  onEndReached,
}: VirtualTableProps<T>): ReactNode {
  const viewportRef = useRef<HTMLDivElement>(null)
  const virtualizer = useVirtualizer({
    count: records.length,
    getScrollElement: () => viewportRef.current,
    estimateSize: () => rowHeight,
    overscan: 10,
    initialRect: { width: window.innerWidth, height: typeof height === 'number' ? height : window.innerHeight },
    getItemKey: (index) => {
      const record = records[index]

      return record === undefined ? index : getKey(record)
    },
  })

  const rows = virtualizer.getVirtualItems()
  const first = rows[0]
  const last = rows.at(-1)
  const topSpace = first?.start ?? 0
  const bottomSpace = last ? virtualizer.getTotalSize() - last.end : 0
  const selectableRecords = selection ? records.filter(selection.isSelectable) : []
  const selectedCount = selection
    ? selectableRecords.filter((record) => selection.selectedKeys.has(getKey(record))).length
    : 0
  const allSelected = selectableRecords.length > 0 && selectedCount === selectableRecords.length && !selection?.hasMore
  const columnCount = columns.length + (selection ? 1 : 0)
  const lastIndex = last?.index ?? -1
  const reachEnd = useEffectEvent(() => {
    onEndReached?.()
  })

  useEffect(() => {
    if (records.length > 0 && lastIndex >= records.length - END_THRESHOLD) {
      reachEnd()
    }
  })

  const toggleAll = () => {
    if (!selection) {
      return
    }
    if (selection.onToggleAll) {
      selection.onToggleAll()
      return
    }
    selection.onChange(allSelected ? new Set() : new Set(selectableRecords.map(getKey)))
  }

  const toggleOne = (key: string) => {
    if (!selection) {
      return
    }
    const next = new Set(selection.selectedKeys)
    if (next.has(key)) {
      next.delete(key)
    } else {
      next.add(key)
    }
    selection.onChange(next)
  }

  const handleRowClick = (event: MouseEvent<HTMLTableRowElement>, record: T) => {
    if (onRowClick && event.target instanceof Element && !event.target.closest(INTERACTIVE_TARGET)) {
      onRowClick(record)
    }
  }

  const selectAll = selection
    ? {
        checked: allSelected,
        indeterminate: selectedCount > 0 && !allSelected,
        disabled: selectableRecords.length === 0,
        onToggle: toggleAll,
      }
    : undefined

  const rowElements = rows.map((row) => {
    const record = records[row.index]

    if (record === undefined) {
      return null
    }
    const key = getKey(record)
    const rowSelectable = selection?.isSelectable(record) ?? false

    return (
      <VirtualTableRow
        key={row.key}
        record={record}
        columns={columns}
        rowKey={key}
        index={row.index}
        rowHeight={rowHeight}
        clickable={Boolean(onRowClick)}
        attributes={rowAttributes?.(record)}
        measureRef={virtualizer.measureElement}
        onClick={handleRowClick}
        hasSelection={Boolean(selection)}
        selectable={rowSelectable}
        selected={selection?.selectedKeys.has(key) ?? false}
        selectionLabel={selection && rowSelectable ? selection.label(record) : ''}
        onToggle={toggleOne}
      />
    )
  })

  const Scroll = height === 'fill' ? FillScrollArea : ScrollArea

  return (
    <Scroll
      {...(height === 'fill' ? {} : { h: height })}
      viewportRef={viewportRef}
      type="auto"
      scrollbarSize={10}
      scrollHideDelay={0}
    >
      <Table
        stickyHeader
        verticalSpacing="xs"
        horizontalSpacing="md"
        fz="sm"
        style={TABLE_NUMERIC}
        aria-label={label}
        aria-rowcount={records.length + 1}
      >
        <Table.Thead>
          <VirtualTableHeader columns={columns} sort={sort} onSortChange={onSortChange} selectAll={selectAll} />
        </Table.Thead>
        <Table.Tbody>
          {topSpace > 0 && (
            <tr aria-hidden style={{ height: topSpace }}>
              <td colSpan={columnCount} />
            </tr>
          )}
          {rowElements}
          {bottomSpace > 0 && (
            <tr aria-hidden style={{ height: bottomSpace }}>
              <td colSpan={columnCount} />
            </tr>
          )}
        </Table.Tbody>
      </Table>
    </Scroll>
  )
}
