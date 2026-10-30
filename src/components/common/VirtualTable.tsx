import { Checkbox, Group, ScrollArea, Table, UnstyledButton } from '@mantine/core'
import { IconArrowDown, IconArrowUp, IconSelector } from '@tabler/icons-react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { type HTMLAttributes, type MouseEvent, type ReactNode, useEffect, useEffectEvent, useRef } from 'react'
import { FillScrollArea } from './FillScrollArea'

const INTERACTIVE_TARGET = 'button, a, input, label, [role="checkbox"]'
const END_THRESHOLD = 5

const HEADER_STYLE = {
  fontSize: 'var(--mantine-font-size-sm)',
  fontWeight: 600,
  paddingTop: 'var(--mantine-spacing-sm)',
  paddingBottom: 'var(--mantine-spacing-sm)',
  paddingLeft: 'var(--mantine-spacing-md)',
  paddingRight: 'var(--mantine-spacing-md)',
  whiteSpace: 'nowrap',
  verticalAlign: 'middle',
  borderBottom: '1px solid var(--mantine-color-default-border)',
} as const

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

function nextSort(current: VirtualTableSort | undefined, columnId: string): VirtualTableSort {
  if (current?.columnId === columnId) {
    return { columnId, direction: current.direction === 'asc' ? 'desc' : 'asc' }
  }

  return { columnId, direction: 'asc' }
}

function ariaSort(sort: VirtualTableSort | undefined, columnId: string): 'ascending' | 'descending' | 'none' {
  if (sort?.columnId !== columnId) {
    return 'none'
  }

  return sort.direction === 'asc' ? 'ascending' : 'descending'
}

function SortIcon({ active, direction }: { active: boolean; direction: 'asc' | 'desc' }): ReactNode {
  if (!active) {
    return <IconSelector size={14} aria-hidden />
  }

  return direction === 'asc' ? <IconArrowUp size={14} aria-hidden /> : <IconArrowDown size={14} aria-hidden />
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
  const selectable = selection ? records.filter(selection.isSelectable) : []
  const selectedCount = selection ? selectable.filter((record) => selection.selectedKeys.has(getKey(record))).length : 0
  const allSelected = selectable.length > 0 && selectedCount === selectable.length && !selection?.hasMore
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
    selection.onChange(allSelected ? new Set() : new Set(selectable.map(getKey)))
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
        aria-label={label}
        aria-rowcount={records.length + 1}
      >
        <Table.Thead>
          <Table.Tr aria-rowindex={1}>
            {selection && (
              <Table.Th style={{ ...HEADER_STYLE, width: 40 }}>
                <Checkbox
                  aria-label="Select all"
                  checked={allSelected}
                  indeterminate={selectedCount > 0 && !allSelected}
                  disabled={selectable.length === 0}
                  onChange={toggleAll}
                />
              </Table.Th>
            )}
            {columns.map((column) => (
              <Table.Th
                key={column.id}
                aria-sort={column.sortable ? ariaSort(sort, column.id) : undefined}
                style={{ ...HEADER_STYLE, width: column.width, textAlign: column.textAlign }}
              >
                {column.sortable && onSortChange ? (
                  <UnstyledButton onClick={() => onSortChange(nextSort(sort, column.id))} fz="sm" fw={600}>
                    <Group gap={4} wrap="nowrap">
                      {column.title}
                      <SortIcon active={sort?.columnId === column.id} direction={sort?.direction ?? 'asc'} />
                    </Group>
                  </UnstyledButton>
                ) : (
                  column.title
                )}
              </Table.Th>
            ))}
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {topSpace > 0 && (
            <tr aria-hidden style={{ height: topSpace }}>
              <td colSpan={columnCount} />
            </tr>
          )}
          {rows.map((row) => {
            const record = records[row.index]

            if (record === undefined) {
              return null
            }
            const key = getKey(record)

            return (
              <Table.Tr
                key={row.key}
                ref={virtualizer.measureElement}
                data-index={row.index}
                aria-rowindex={row.index + 2}
                className={onRowClick ? 'interactive-surface row-clickable' : 'interactive-surface'}
                style={{ height: rowHeight, verticalAlign: 'middle' }}
                onClick={(event) => handleRowClick(event, record)}
                {...rowAttributes?.(record)}
              >
                {selection && (
                  <Table.Td>
                    {selection.isSelectable(record) && (
                      <Checkbox
                        aria-label={selection.label(record)}
                        checked={selection.selectedKeys.has(key)}
                        onChange={() => toggleOne(key)}
                      />
                    )}
                  </Table.Td>
                )}
                {columns.map((column) => (
                  <Table.Td key={column.id} style={{ textAlign: column.textAlign }}>
                    {column.render(record)}
                  </Table.Td>
                ))}
              </Table.Tr>
            )
          })}
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
