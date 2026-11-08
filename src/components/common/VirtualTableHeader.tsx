import { Checkbox, Group, Table, UnstyledButton } from '@mantine/core'
import { IconArrowDown, IconArrowUp, IconSelector } from '@tabler/icons-react'
import type { ReactNode } from 'react'

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

interface HeaderColumn {
  id: string
  title: ReactNode
  width?: number | string | undefined
  textAlign?: 'left' | 'center' | 'right' | undefined
  sortable?: boolean | undefined
}

interface HeaderSort {
  columnId: string
  direction: 'asc' | 'desc'
}

interface SelectAllState {
  checked: boolean
  indeterminate: boolean
  disabled: boolean
  onToggle: () => void
}

interface VirtualTableHeaderProps {
  columns: readonly HeaderColumn[]
  sort: HeaderSort | undefined
  onSortChange: ((sort: HeaderSort) => void) | undefined
  selectAll: SelectAllState | undefined
}

interface ColumnHeaderCellProps {
  column: HeaderColumn
  sort: HeaderSort | undefined
  onSortChange: ((sort: HeaderSort) => void) | undefined
}

function nextSort(current: HeaderSort | undefined, columnId: string): HeaderSort {
  if (current?.columnId === columnId) {
    return { columnId, direction: current.direction === 'asc' ? 'desc' : 'asc' }
  }

  return { columnId, direction: 'asc' }
}

function ariaSort(sort: HeaderSort | undefined, columnId: string): 'ascending' | 'descending' | 'none' {
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

function ColumnHeaderCell({ column, sort, onSortChange }: ColumnHeaderCellProps): ReactNode {
  let content: ReactNode = column.title

  if (column.sortable && onSortChange) {
    content = (
      <UnstyledButton onClick={() => onSortChange(nextSort(sort, column.id))} fz="sm" fw={600}>
        <Group gap={4} wrap="nowrap">
          {column.title}
          <SortIcon active={sort?.columnId === column.id} direction={sort?.direction ?? 'asc'} />
        </Group>
      </UnstyledButton>
    )
  }

  return (
    <Table.Th
      aria-sort={column.sortable ? ariaSort(sort, column.id) : undefined}
      style={{ ...HEADER_STYLE, width: column.width, textAlign: column.textAlign }}
    >
      {content}
    </Table.Th>
  )
}

export function VirtualTableHeader({ columns, sort, onSortChange, selectAll }: VirtualTableHeaderProps): ReactNode {
  return (
    <Table.Tr aria-rowindex={1}>
      {selectAll && (
        <Table.Th style={{ ...HEADER_STYLE, width: 40 }}>
          <Checkbox
            aria-label="Select all"
            checked={selectAll.checked}
            indeterminate={selectAll.indeterminate}
            disabled={selectAll.disabled}
            onChange={selectAll.onToggle}
          />
        </Table.Th>
      )}
      {columns.map((column) => (
        <ColumnHeaderCell key={column.id} column={column} sort={sort} onSortChange={onSortChange} />
      ))}
    </Table.Tr>
  )
}
