import { Checkbox, Table } from '@mantine/core'
import type { HTMLAttributes, MouseEvent, ReactNode } from 'react'

import { rowClassName } from './tableStyles'

interface RowColumn<T> {
  id: string
  textAlign?: 'left' | 'center' | 'right' | undefined
  render: (record: T) => ReactNode
}

interface VirtualTableRowProps<T> {
  record: T
  columns: readonly RowColumn<T>[]
  rowKey: string
  index: number
  rowHeight: number
  clickable: boolean
  attributes: HTMLAttributes<HTMLTableRowElement> | undefined
  measureRef: (node: HTMLTableRowElement | null) => void
  onClick: (event: MouseEvent<HTMLTableRowElement>, record: T) => void
  hasSelection: boolean
  selectable: boolean
  selected: boolean
  selectionLabel: string
  onToggle: (key: string) => void
}

export function VirtualTableRow<T>({
  record,
  columns,
  rowKey,
  index,
  rowHeight,
  clickable,
  attributes,
  measureRef,
  onClick,
  hasSelection,
  selectable,
  selected,
  selectionLabel,
  onToggle,
}: VirtualTableRowProps<T>): ReactNode {
  return (
    <Table.Tr
      ref={measureRef}
      data-index={index}
      aria-rowindex={index + 2}
      className={rowClassName(clickable)}
      style={{ height: rowHeight, verticalAlign: 'middle' }}
      onClick={(event) => onClick(event, record)}
      {...attributes}
    >
      {hasSelection && (
        <Table.Td>
          {selectable && <Checkbox aria-label={selectionLabel} checked={selected} onChange={() => onToggle(rowKey)} />}
        </Table.Td>
      )}
      {columns.map((column) => (
        <Table.Td key={column.id} style={{ textAlign: column.textAlign }}>
          {column.render(record)}
        </Table.Td>
      ))}
    </Table.Tr>
  )
}
