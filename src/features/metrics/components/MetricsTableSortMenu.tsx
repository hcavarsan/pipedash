import { Menu, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronUp } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { MetricSortField, MetricTableSort } from '../../../types/metrics'
import classes from './metrics.module.css'

export interface SortColumn {
  field: MetricSortField
  label: string
  numeric: boolean
}

const SortIndicator = ({ sort, field }: { sort: MetricTableSort | undefined; field: MetricSortField }): ReactNode => {
  if (sort?.field !== field) {
    return null
  }

  return sort.direction === 'asc' ? <IconChevronUp size={16} aria-hidden /> : <IconChevronDown size={16} aria-hidden />
}

interface MetricsTableSortMenuProps {
  columns: SortColumn[]
  sort: MetricTableSort | undefined
  onSort: (field: MetricSortField) => void
}

export const MetricsTableSortMenu = ({ columns, sort, onSort }: MetricsTableSortMenuProps): ReactNode => {
  const active = columns.find(({ field }) => field === sort?.field)
  const sortLabel =
    sort && active ? `${active.label} · ${sort.direction === 'asc' ? 'Ascending' : 'Descending'}` : 'Default'

  return (
    <Menu position="bottom" withinPortal>
      <Menu.Target>
        <UnstyledButton className={classes.menuRow}>
          <span>Sort by</span>
          <span className={classes.menuRowValue}>{sortLabel}</span>
        </UnstyledButton>
      </Menu.Target>
      <Menu.Dropdown>
        {columns.map(({ field, label }) => (
          <Menu.Item
            key={field}
            onClick={() => onSort(field)}
            rightSection={<SortIndicator sort={sort} field={field} />}
          >
            {label}
          </Menu.Item>
        ))}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}
